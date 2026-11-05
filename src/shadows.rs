use crate::geometry::Mesh;
use crate::math::{Mat4, Vec3};
use crate::profile::DetailedProfile;
use std::cmp::Ordering;
use std::cell::RefCell;
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
        self.intersection_interval(origin, direction, 0.0, max_distance).is_some()
    }

    fn intersection_interval(&self, origin: Vec3, direction: Vec3, min_distance: f32, max_distance: f32) -> Option<(f32, f32)> {
        let mut near = min_distance;
        let mut far = max_distance;
        for axis in 0..3 {
            let o = component(origin, axis);
            let d = component(direction, axis);
            let min = component(self.min, axis);
            let max = component(self.max, axis);
            if d.abs() <= EPSILON {
                if o < min || o > max {
                    return None;
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
                return None;
            }
        }
        Some((near, far))
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct BvhPrimitive {
    object_index: usize,
    triangle_index: usize,
    bounds: Aabb,
    centroid: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
struct BvhNode {
    bounds: Aabb,
    left: Option<usize>,
    right: Option<usize>,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct Bvh {
    nodes: Vec<BvhNode>,
    primitives: Vec<BvhPrimitive>,
    leaf_size: usize,
    max_depth: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadowComparisonStats {
    pub eligible_queries: u64,
    pub queries_compared: u64,
    pub matches: u64,
    pub discrepancies: u64,
    pub false_positives: u64,
    pub false_negatives: u64,
    pub first_discrepancy: Option<ShadowComparisonDiscrepancy>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowComparisonDiscrepancy {
    pub origin: Vec3,
    pub direction: Vec3,
    pub max_distance: f32,
    pub receiver_id: u32,
    pub brute_occluded: bool,
    pub bvh_occluded: bool,
}

impl Bvh {
    fn build(objects: &[ShadowObject]) -> Option<Self> {
        let mut primitives = Vec::new();
        for (object_index, object) in objects.iter().enumerate() {
            for (triangle_index, triangle) in object.triangles.iter().enumerate() {
                let bounds = Aabb::from_triangle(triangle.vertices);
                let centroid = (bounds.min + bounds.max) * 0.5;
                primitives.push(BvhPrimitive { object_index, triangle_index, bounds, centroid });
            }
        }
        if primitives.is_empty() { return None; }
        let mut bvh = Self { nodes: Vec::new(), primitives, leaf_size: 8, max_depth: 0 };
        let end = bvh.primitives.len();
        bvh.build_range(0, end, 0);
        Some(bvh)
    }

    fn build_range(&mut self, start: usize, end: usize, depth: u32) -> usize {
        self.max_depth = self.max_depth.max(depth);
        let node_index = self.nodes.len();
        self.nodes.push(BvhNode {
            bounds: bounds_for_primitives(&self.primitives[start..end]),
            left: None,
            right: None,
            start,
            end,
        });
        if end - start <= self.leaf_size { return node_index; }
        let bounds = self.nodes[node_index].bounds;
        let extent = bounds.max - bounds.min;
        let axis = if extent.x >= extent.y && extent.x >= extent.z { 0 } else if extent.y >= extent.z { 1 } else { 2 };
        self.primitives[start..end].sort_by(|a, b| {
            component(a.centroid, axis).partial_cmp(&component(b.centroid, axis))
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.object_index.cmp(&b.object_index))
                .then_with(|| a.triangle_index.cmp(&b.triangle_index))
        });
        let middle = start + (end - start) / 2;
        let left = self.build_range(start, middle, depth + 1);
        let right = self.build_range(middle, end, depth + 1);
        self.nodes[node_index].left = Some(left);
        self.nodes[node_index].right = Some(right);
        node_index
    }
}

fn bounds_for_primitives(primitives: &[BvhPrimitive]) -> Aabb {
    let mut bounds = primitives[0].bounds;
    for primitive in &primitives[1..] {
        bounds.include(primitive.bounds.min);
        bounds.include(primitive.bounds.max);
    }
    bounds
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShadowScene {
    objects: Vec<ShadowObject>,
    bvh: Option<Bvh>,
    bvh_build_ns: u128,
    transform_ns: u128,
    compare_algorithms: bool,
    compare_all: bool,
    comparison_queries: RefCell<u64>,
    comparison_stats: RefCell<ShadowComparisonStats>,
}

impl ShadowScene {
    pub fn from_meshes(objects: &[(u32, &Mesh, Mat4)]) -> Self {
        Self::from_meshes_with_bvh(objects, false)
    }

    pub fn from_meshes_with_bvh(objects: &[(u32, &Mesh, Mat4)], use_bvh: bool) -> Self {
        Self::from_meshes_with_options(objects, use_bvh, false)
    }

    pub fn from_meshes_with_options(objects: &[(u32, &Mesh, Mat4)], use_bvh: bool, compare_algorithms: bool) -> Self {
        Self::from_meshes_with_comparison(objects, use_bvh, compare_algorithms, false)
    }

    pub fn from_meshes_with_comparison(objects: &[(u32, &Mesh, Mat4)], use_bvh: bool, compare_algorithms: bool, compare_all: bool) -> Self {
        let transform_started = Instant::now();
        let objects: Vec<ShadowObject> = objects
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
        let transform_ns = transform_started.elapsed().as_nanos();
        let (bvh, bvh_build_ns) = if use_bvh || compare_algorithms {
            let started = Instant::now();
            let bvh = Bvh::build(&objects);
            (bvh, started.elapsed().as_nanos())
        } else {
            (None, 0)
        };
        Self {
            objects,
            bvh,
            bvh_build_ns,
            transform_ns,
            compare_algorithms,
            compare_all,
            comparison_queries: RefCell::new(0),
            comparison_stats: RefCell::new(ShadowComparisonStats::default()),
        }
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
        let use_bvh = self.bvh.is_some();
        let occluded = if use_bvh {
            if let Some(profile) = profile { profile.add_bvh_query(); }
            self.is_occluded_bvh(origin, direction, distance - EPSILON, receiver_id, profile)
        } else {
            if let Some(profile) = profile { profile.add_brute_query(); }
            self.is_occluded_brute(origin, direction, distance - EPSILON, receiver_id, profile)
        };
        if self.compare_algorithms && self.bvh.is_some() {
            let mut stats = self.comparison_stats.borrow_mut();
            stats.eligible_queries += 1;
            drop(stats);
            let mut query_index = self.comparison_queries.borrow_mut();
            let sampled = self.compare_all || (*query_index & 1023) == 0;
            *query_index = query_index.saturating_add(1);
            if sampled {
                let brute_occluded = if use_bvh {
                    self.is_occluded_brute(origin, direction, distance - EPSILON, receiver_id, None)
                } else {
                    occluded
                };
                let bvh_occluded = if use_bvh {
                    occluded
                } else {
                    self.is_occluded_bvh(origin, direction, distance - EPSILON, receiver_id, None)
                };
                let mut stats = self.comparison_stats.borrow_mut();
                stats.queries_compared += 1;
                if brute_occluded == bvh_occluded {
                    stats.matches += 1;
                } else {
                    stats.discrepancies += 1;
                    if bvh_occluded { stats.false_positives += 1; } else { stats.false_negatives += 1; }
                    if stats.first_discrepancy.is_none() {
                        stats.first_discrepancy = Some(ShadowComparisonDiscrepancy {
                            origin,
                            direction,
                            max_distance: distance - EPSILON,
                            receiver_id,
                            brute_occluded,
                            bvh_occluded,
                        });
                    }
                }
            }
        }
        if occluded {
            if let Some(profile) = profile { profile.add_shadow_occluded(); }
        }
        if let (Some(profile), Some(started)) = (profile, started) {
            profile.add_shadow_query(started.elapsed());
        }
        occluded
    }

    fn is_occluded_brute(&self, origin: Vec3, direction: Vec3, max_distance: f32, receiver_id: u32, profile: Option<&DetailedProfile>) -> bool {
        for object in &self.objects {
            if object.object_id == receiver_id
            {
                continue;
            }
            if !object.bounds.intersects_segment(origin, direction, max_distance) {
                if let Some(profile) = profile { profile.add_shadow_aabb_discarded(); }
                continue;
            }
            if let Some(profile) = profile { profile.add_shadow_aabb_passed(); }
            for triangle in &object.triangles {
                if let Some(profile) = profile { profile.add_shadow_triangle_test(); }
                if let Some(profile) = profile { profile.add_brute_triangle_test(); }
                if ray_triangle_intersection(origin, direction, triangle.vertices)
                    .is_some_and(|hit| hit < max_distance)
                {
                    if let Some(profile) = profile { profile.add_brute_occluded(); }
                    return true;
                }
            }
        }
        false
    }

    fn is_occluded_bvh(&self, origin: Vec3, direction: Vec3, max_distance: f32, receiver_id: u32, profile: Option<&DetailedProfile>) -> bool {
        let bvh = self.bvh.as_ref().expect("BVH route requires BVH");
        let Some((_, _)) = bvh.nodes[0].bounds.intersection_interval(origin, direction, 0.0, max_distance) else {
            if let Some(profile) = profile { profile.add_bvh_node_discarded(); }
            return false;
        };
        let mut stack = [(0usize, 0u32); 64];
        let mut stack_len = 1usize;
        stack[0] = (0, 0);
        while stack_len > 0 {
            stack_len -= 1;
            let (node_index, depth) = stack[stack_len];
            if let Some(profile) = profile {
                profile.add_bvh_node_visited();
                profile.observe_bvh_depth(depth);
            }
            let node = &bvh.nodes[node_index];
            if node_index != 0 && node.bounds.intersection_interval(origin, direction, 0.0, max_distance).is_none() {
                if let Some(profile) = profile { profile.add_bvh_node_discarded(); }
                continue;
            }
            if node.left.is_none() && node.right.is_none() {
                if let Some(profile) = profile { profile.add_bvh_leaf(); }
                for primitive in &bvh.primitives[node.start..node.end] {
                    let object = &self.objects[primitive.object_index];
                    if object.object_id == receiver_id { continue; }
                    if primitive.bounds.intersection_interval(origin, direction, 0.0, max_distance).is_none() { continue; }
                    if let Some(profile) = profile {
                        profile.add_shadow_triangle_test();
                        profile.add_bvh_triangle_test();
                    }
                    let triangle = object.triangles[primitive.triangle_index];
                    if ray_triangle_intersection(origin, direction, triangle.vertices).is_some_and(|hit| hit < max_distance) {
                        if let Some(profile) = profile { profile.add_bvh_occluded(); }
                        return true;
                    }
                }
                continue;
            }
            let mut children = [(0usize, 0.0f32); 2];
            let mut count = 0;
            for child in [node.left, node.right].into_iter().flatten() {
                if let Some((near, _)) = bvh.nodes[child].bounds.intersection_interval(origin, direction, 0.0, max_distance) {
                    children[count] = (child, near);
                    count += 1;
                } else if let Some(profile) = profile { profile.add_bvh_node_discarded(); }
            }
            if count == 2 && children[0].1 < children[1].1 { children.swap(0, 1); }
            for (child, _) in children[..count].iter().copied() {
                if stack_len < stack.len() {
                    stack[stack_len] = (child, depth + 1);
                    stack_len += 1;
                } else {
                    if let Some(profile) = profile { profile.add_bvh_stack_fallback(); }
                    return self.is_occluded_brute(origin, direction, max_distance, receiver_id, None);
                }
            }
        }
        false
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.objects.iter().map(|object| object.triangles.len()).sum()
    }

    pub fn uses_bvh(&self) -> bool { self.bvh.is_some() }
    pub fn bvh_node_count(&self) -> usize { self.bvh.as_ref().map_or(0, |bvh| bvh.nodes.len()) }
    pub fn bvh_triangle_count(&self) -> usize { self.bvh.as_ref().map_or(0, |bvh| bvh.primitives.len()) }
    pub fn bvh_max_depth(&self) -> u32 { self.bvh.as_ref().map_or(0, |bvh| bvh.max_depth) }
    pub fn bvh_build_ns(&self) -> u128 { self.bvh_build_ns }
    pub fn transform_ns(&self) -> u128 { self.transform_ns }
    pub fn comparison_stats(&self) -> ShadowComparisonStats { *self.comparison_stats.borrow() }
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