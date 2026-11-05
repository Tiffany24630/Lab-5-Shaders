use crate::geometry::Mesh;
use crate::math::{Mat4, Vec3};
use crate::profile::DetailedProfile;
use std::time::Instant;

const EPSILON: f32 = 1.0e-5;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Aabb {
    min: Vec3,
    max: Vec3,
}

impl Aabb {
    fn from_triangle(triangle: [Vec3; 3]) -> Self {
        let mut bounds = Self {
            min: triangle[0],
            max: triangle[0],
        };
        for point in triangle.into_iter().skip(1) {
            bounds.include(point);
        }
        bounds
    }

    fn include(&mut self, point: Vec3) {
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.min.z = self.min.z.min(point.z);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
        self.max.z = self.max.z.max(point.z);
    }

    fn intersects_segment(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> bool {
        let mut near: f32 = 0.0;
        let mut far: f32 = max_distance;
        for axis in 0..3 {
            let o = component(origin, axis);
            let d = component(direction, axis);
            let min = component(self.min, axis);
            let max = component(self.max, axis);
            if d.abs() <= EPSILON {
                if o < min || o > max {
                    return false;
                }
                continue;
            }
            let inverse = 1.0 / d;
            let mut t0 = (min - o) * inverse;
            let mut t1 = (max - o) * inverse;
            if t0 > t1 {
                std::mem::swap(&mut t0, &mut t1);
            }
            near = near.max(t0);
            far = far.min(t1);
            if near > far {
                return false;
            }
        }
        near <= max_distance && far >= 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WorldTriangle {
    vertices: [Vec3; 3],
}

#[derive(Clone, Debug, PartialEq)]
struct ShadowObject {
    object_id: u32,
    bounds: Aabb,
    triangles: Vec<WorldTriangle>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShadowScene {
    objects: Vec<ShadowObject>,
}

impl ShadowScene {
    pub fn from_meshes(objects: &[(u32, &Mesh, Mat4)]) -> Self {
        let objects = objects
            .iter()
            .filter_map(|(object_id, mesh, model)| {
                let mut triangles = Vec::with_capacity(mesh.triangles().len());
                let mut bounds: Option<Aabb> = None;
                for triangle in mesh.triangles() {
                    let vertices = triangle.vertices.map(|vertex| model.transform_point(vertex.position));
                    if vertices.iter().any(|point| !point.is_finite()) {
                        continue;
                    }
                    let triangle_bounds = Aabb::from_triangle(vertices);
                    if let Some(total_bounds) = &mut bounds {
                        total_bounds.include(triangle_bounds.min);
                        total_bounds.include(triangle_bounds.max);
                    } else {
                        bounds = Some(triangle_bounds);
                    }
                    triangles.push(WorldTriangle { vertices });
                }
                Some(ShadowObject {
                    object_id: *object_id,
                    bounds: bounds?,
                    triangles,
                })
            })
            .collect();
        Self { objects }
    }

    pub fn is_occluded(
        &self,
        point: Vec3,
        light_position: Vec3,
        receiver_id: u32,
        profile: Option<&DetailedProfile>,
    ) -> bool {
        let started = profile.map(|_| Instant::now());
        let to_light = light_position - point;
        let distance = to_light.length();
        if !distance.is_finite() || distance <= EPSILON {
            if let (Some(profile), Some(started)) = (profile, started) { profile.add_shadow_query(started.elapsed()); }
            return false;
        }
        let direction = to_light / distance;
        let origin = point + direction * (EPSILON * 4.0);
        let mut occluded = false;
        for object in &self.objects {
            if object.object_id == receiver_id
            {
                continue;
            }
            if !object.bounds.intersects_segment(origin, direction, distance - EPSILON) {
                if let Some(profile) = profile { profile.add_shadow_aabb_discarded(); }
                continue;
            }
            if let Some(profile) = profile { profile.add_shadow_aabb_passed(); }
            for triangle in &object.triangles {
                if let Some(profile) = profile { profile.add_shadow_triangle_test(); }
                if ray_triangle_intersection(origin, direction, triangle.vertices)
                    .is_some_and(|hit| hit < distance - EPSILON)
                {
                    occluded = true;
                    if let Some(profile) = profile { profile.add_shadow_occluded(); }
                    break;
                }
            }
            if occluded { break; }
        }
        if let (Some(profile), Some(started)) = (profile, started) {
            profile.add_shadow_query(started.elapsed());
        }
        occluded
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.objects.iter().map(|object| object.triangles.len()).sum()
    }
}

pub fn ray_triangle_intersection(
    origin: Vec3,
    direction: Vec3,
    triangle: [Vec3; 3],
) -> Option<f32> {
    if !origin.is_finite() || !direction.is_finite() || triangle.iter().any(|point| !point.is_finite()) {
        return None;
    }
    let edge_a = triangle[1] - triangle[0];
    let edge_b = triangle[2] - triangle[0];
    let cross_direction = direction.cross(edge_b);
    let determinant = edge_a.dot(cross_direction);
    if !determinant.is_finite() || determinant.abs() <= EPSILON {
        return None;
    }
    let inverse = 1.0 / determinant;
    let offset = origin - triangle[0];
    let u = offset.dot(cross_direction) * inverse;
    if !u.is_finite() || !(0.0..=1.0).contains(&u) {
        return None;
    }
    let cross_offset = offset.cross(edge_a);
    let v = direction.dot(cross_offset) * inverse;
    if !v.is_finite() || v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = edge_b.dot(cross_offset) * inverse;
    (distance.is_finite() && distance > EPSILON).then_some(distance)
}

fn component(value: Vec3, axis: usize) -> f32 {
    match axis {
        0 => value.x,
        1 => value.y,
        _ => value.z,
    }
}