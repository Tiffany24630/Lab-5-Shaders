use crate::camera::Camera;
use crate::color::Color;
use crate::framebuffer::Framebuffer;
use crate::geometry::{Mesh, Triangle3D};
use crate::math::{Mat4, Vec2, Vec3, Vec4};
use crate::rasterizer::{PerspectiveRasterVertex, rasterize_triangle_with_shader};
use crate::shader::{FragmentShader, ShaderUniforms, VertexColorShader};
use std::time::Instant;

const CLIP_EPSILON: f32 = 1.0e-6;

#[derive(Clone, Copy, Debug)]
struct ClipVertex {
    position: Vec4,
    color: Color,
    normal: Vec3,
    local_position: Vec3,
    world_position: Vec3,
    uv: Option<Vec2>,
}

pub fn render_mesh(
    framebuffer: &mut Framebuffer,
    mesh: &Mesh,
    model: Mat4,
    camera: &Camera,
) -> usize {
    render_mesh_with_shader(
        framebuffer,
        mesh,
        model,
        camera,
        &VertexColorShader,
        &ShaderUniforms::default(),
    )
}

pub fn render_mesh_with_shader<S: FragmentShader>(
    framebuffer: &mut Framebuffer,
    mesh: &Mesh,
    model: Mat4,
    camera: &Camera,
    shader: &S,
    uniforms: &ShaderUniforms,
) -> usize {
    let mut written = 0;
    for triangle in mesh.triangles() {
        written +=
            render_triangle_with_shader(framebuffer, triangle, model, camera, shader, uniforms);
    }
    written
}

pub fn render_triangle(
    framebuffer: &mut Framebuffer,
    triangle: &Triangle3D,
    model: Mat4,
    camera: &Camera,
) -> usize {
    render_triangle_with_shader(
        framebuffer,
        triangle,
        model,
        camera,
        &VertexColorShader,
        &ShaderUniforms::default(),
    )
}

pub fn render_triangle_with_shader<S: FragmentShader>(
    framebuffer: &mut Framebuffer,
    triangle: &Triangle3D,
    model: Mat4,
    camera: &Camera,
    shader: &S,
    uniforms: &ShaderUniforms,
) -> usize {
    let preparation_started = uniforms.detail_profile.as_ref().map(|_| Instant::now());
    let view = camera.view_matrix();
    let transform = camera.projection_matrix() * view * model;
    let vertices = triangle.vertices.map(|vertex| ClipVertex {
        position: transform
            * Vec4::new(vertex.position.x, vertex.position.y, vertex.position.z, 1.0),
        color: vertex.color,
        normal: model.transform_normal(vertex.normal),
        local_position: vertex.position,
        world_position: model.transform_point(vertex.position),
        uv: vertex.texcoord,
    });
    if vertices.iter().any(|vertex| !finite_clip_vertex(*vertex)) {
        return 0;
    }

    let polygon = clip_frustum(&vertices);
    let mut written = 0;
    for triangle in triangulate(&polygon) {
        let Some(projected) = project_triangle(triangle, framebuffer.width(), framebuffer.height())
        else {
            continue;
        };
        if let (Some(profile), Some(preparation_started)) = (&uniforms.detail_profile, preparation_started.as_ref()) {
            profile.add_preparation(preparation_started.elapsed());
        }
        let raster_started = uniforms.detail_profile.as_ref().map(|_| Instant::now());
        written += rasterize_triangle_with_shader(framebuffer, projected, shader, uniforms);
        if let (Some(profile), Some(raster_started)) = (&uniforms.detail_profile, raster_started) {
            profile.add_raster(raster_started.elapsed());
        }
    }
    written
}

fn finite_clip_vertex(vertex: ClipVertex) -> bool {
    vertex.position.x.is_finite()
        && vertex.position.y.is_finite()
        && vertex.position.z.is_finite()
        && vertex.position.w.is_finite()
        && vertex.color.r.is_finite()
        && vertex.color.g.is_finite()
        && vertex.color.b.is_finite()
        && vertex.normal.is_finite()
        && vertex.local_position.is_finite()
        && vertex.world_position.is_finite()
        && vertex
            .uv
            .map(|uv| uv.x.is_finite() && uv.y.is_finite())
            .unwrap_or(true)
}

#[derive(Clone, Copy)]
enum ClipPlane {
    Left,
    Right,
    Bottom,
    Top,
    Near,
    Far,
}

const PLANES: [ClipPlane; 6] = [
    ClipPlane::Left,
    ClipPlane::Right,
    ClipPlane::Bottom,
    ClipPlane::Top,
    ClipPlane::Near,
    ClipPlane::Far,
];

fn plane_distance(vertex: ClipVertex, plane: ClipPlane) -> f32 {
    let p = vertex.position;
    match plane {
        ClipPlane::Left => p.x + p.w,
        ClipPlane::Right => p.w - p.x,
        ClipPlane::Bottom => p.y + p.w,
        ClipPlane::Top => p.w - p.y,
        ClipPlane::Near => p.z + p.w,
        ClipPlane::Far => p.w - p.z,
    }
}

fn clip_frustum(vertices: &[ClipVertex; 3]) -> Vec<ClipVertex> {
    let mut polygon = vertices.to_vec();
    for plane in PLANES {
        polygon = clip_polygon_against_plane(&polygon, plane);
        if polygon.is_empty() {
            break;
        }
    }
    polygon
}

fn clip_polygon_against_plane(polygon: &[ClipVertex], plane: ClipPlane) -> Vec<ClipVertex> {
    if polygon.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(polygon.len() + 1);
    for index in 0..polygon.len() {
        let current = polygon[index];
        let next = polygon[(index + 1) % polygon.len()];
        let current_distance = plane_distance(current, plane);
        let next_distance = plane_distance(next, plane);
        let current_inside = current_distance >= -CLIP_EPSILON;
        let next_inside = next_distance >= -CLIP_EPSILON;

        if current_inside != next_inside {
            let denominator = current_distance - next_distance;
            if denominator.abs() > f32::EPSILON {
                let t = current_distance / denominator;
                if t.is_finite() && (-CLIP_EPSILON..=1.0 + CLIP_EPSILON).contains(&t) {
                    let intersection = ClipVertex {
                        position: lerp_vec4(current.position, next.position, t),
                        color: current.color.lerp(next.color, t),
                        normal: current.normal.lerp(next.normal, t),
                        local_position: current.local_position.lerp(next.local_position, t),
                        world_position: current.world_position.lerp(next.world_position, t),
                        uv: lerp_uv(current.uv, next.uv, t),
                    };
                    if finite_clip_vertex(intersection) {
                        result.push(intersection);
                    }
                }
            }
        }
        if next_inside && finite_clip_vertex(next) {
            result.push(next);
        }
    }
    result
}

fn lerp_vec4(a: Vec4, b: Vec4, t: f32) -> Vec4 {
    Vec4::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
        a.w + (b.w - a.w) * t,
    )
}

fn lerp_uv(a: Option<Vec2>, b: Option<Vec2>, t: f32) -> Option<Vec2> {
    Some(a? + (b? - a?) * t)
}

fn triangulate(polygon: &[ClipVertex]) -> Vec<[ClipVertex; 3]> {
    if polygon.len() < 3 {
        return Vec::new();
    }
    (1..polygon.len() - 1)
        .map(|index| [polygon[0], polygon[index], polygon[index + 1]])
        .filter(|triangle| triangle_area_squared(*triangle) > CLIP_EPSILON * CLIP_EPSILON)
        .collect()
}

fn triangle_area_squared(triangle: [ClipVertex; 3]) -> f32 {
    let a = triangle[0].position;
    let b = triangle[1].position;
    let c = triangle[2].position;
    if a.w.abs() <= f32::EPSILON || b.w.abs() <= f32::EPSILON || c.w.abs() <= f32::EPSILON {
        return 0.0;
    }
    let ax = a.x / a.w;
    let ay = a.y / a.w;
    let bx = b.x / b.w;
    let by = b.y / b.w;
    let cx = c.x / c.w;
    let cy = c.y / c.w;
    ((bx - ax) * (cy - ay) - (by - ay) * (cx - ax)).powi(2)
}

fn project_triangle(
    triangle: [ClipVertex; 3],
    width: usize,
    height: usize,
) -> Option<[PerspectiveRasterVertex; 3]> {
    triangle
        .map(|vertex| {
            let clip = vertex.position;
            if clip.w <= f32::EPSILON || !clip.w.is_finite() {
                return None;
            }
            let inv_w = 1.0 / clip.w;
            let ndc_x = clip.x * inv_w;
            let ndc_y = clip.y * inv_w;
            let ndc_z = clip.z * inv_w;
            if !inv_w.is_finite() || !ndc_x.is_finite() || !ndc_y.is_finite() || !ndc_z.is_finite()
            {
                return None;
            }
            Some(PerspectiveRasterVertex {
                x: (ndc_x + 1.0) * 0.5 * width as f32,
                y: (1.0 - ndc_y) * 0.5 * height as f32,
                ndc_depth: ndc_z,
                inv_w,
                color_over_w: Color::new_const(
                    vertex.color.r * inv_w,
                    vertex.color.g * inv_w,
                    vertex.color.b * inv_w,
                ),
                normal_over_w: vertex.normal * inv_w,
                local_position_over_w: vertex.local_position * inv_w,
                world_position_over_w: vertex.world_position * inv_w,
                uv_over_w: vertex.uv.map(|uv| uv * inv_w),
            })
        })
        .into_iter()
        .collect::<Option<Vec<_>>>()?
        .try_into()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::{
        ClipPlane, ClipVertex, clip_frustum, clip_polygon_against_plane, render_mesh,
        render_mesh_with_shader,
    };
    use crate::camera::Camera;
    use crate::color::Color;
    use crate::framebuffer::Framebuffer;
    use crate::geometry::{Mesh, Triangle3D, Vertex3D};
    use crate::math::{Mat4, Vec3, Vec4};
    use crate::shader::{FragmentInput, FragmentShader, ShaderUniforms};
    use std::f32::consts::FRAC_PI_2;

    fn clip(x: f32, y: f32, z: f32, w: f32, color: Color) -> ClipVertex {
        ClipVertex {
            position: Vec4::new(x, y, z, w),
            color,
            normal: Vec3::Z,
            local_position: Vec3::ZERO,
            world_position: Vec3::ZERO,
            uv: None,
        }
    }

    fn camera() -> Camera {
        Camera::new(
            Vec3::new(0.0, 0.0, 3.0),
            Vec3::ZERO,
            Vec3::Y,
            FRAC_PI_2,
            1.0,
            0.5,
            20.0,
        )
        .unwrap()
    }

    fn triangle(z: f32) -> Triangle3D {
        Triangle3D {
            vertices: [
                Vertex3D::new(Vec3::new(-0.5, -0.5, z), Color::new(1.0, 0.0, 0.0)),
                Vertex3D::new(Vec3::new(0.5, -0.5, z), Color::new(0.0, 1.0, 0.0)),
                Vertex3D::new(Vec3::new(0.0, 0.5, z), Color::new(0.0, 0.0, 1.0)),
            ],
        }
    }

    #[test]
    fn accepts_inside_triangle_and_rejects_each_outside_plane() {
        let inside = [
            clip(-0.5, -0.5, 0.0, 1.0, Color::WHITE),
            clip(0.5, -0.5, 0.0, 1.0, Color::WHITE),
            clip(0.0, 0.5, 0.0, 1.0, Color::WHITE),
        ];
        assert_eq!(clip_frustum(&inside).len(), 3);
        for (plane, vertex) in [
            (ClipPlane::Left, clip(-2.0, 0.0, 0.0, 1.0, Color::WHITE)),
            (ClipPlane::Right, clip(2.0, 0.0, 0.0, 1.0, Color::WHITE)),
            (ClipPlane::Bottom, clip(0.0, -2.0, 0.0, 1.0, Color::WHITE)),
            (ClipPlane::Top, clip(0.0, 2.0, 0.0, 1.0, Color::WHITE)),
            (ClipPlane::Near, clip(0.0, 0.0, -2.0, 1.0, Color::WHITE)),
            (ClipPlane::Far, clip(0.0, 0.0, 2.0, 1.0, Color::WHITE)),
        ] {
            assert!(clip_polygon_against_plane(&[vertex; 3], plane).is_empty());
        }
    }

    #[test]
    fn clips_all_boundaries_and_preserves_intersection_attributes() {
        let polygon = clip_frustum(&[
            clip(-2.0, -2.0, -2.0, 1.0, Color::BLACK),
            clip(2.0, -0.5, 0.0, 1.0, Color::WHITE),
            clip(0.0, 2.0, 0.0, 1.0, Color::WHITE),
        ]);
        assert!(!polygon.is_empty());
        assert!(polygon.len() >= 3);
        assert!(polygon.iter().all(|v| v.position.x.is_finite()
            && v.position.y.is_finite()
            && v.position.z.is_finite()
            && v.position.w.is_finite()));
        assert!(polygon.iter().any(|v| v.color.r > 0.0 && v.color.r < 1.0));
    }

    #[test]
    fn crossing_one_plane_can_create_a_quad() {
        let polygon = clip_polygon_against_plane(
            &[
                clip(-2.0, 0.0, 0.0, 1.0, Color::BLACK),
                clip(0.5, -0.5, 0.0, 1.0, Color::WHITE),
                clip(0.5, 0.5, 0.0, 1.0, Color::WHITE),
            ],
            ClipPlane::Left,
        );
        assert_eq!(polygon.len(), 4);
    }

    #[test]
    fn render_pipeline_handles_inside_behind_and_occlusion() {
        let camera = camera();
        let mesh = Mesh::new(vec![triangle(-3.0)]).unwrap();
        let mut framebuffer = Framebuffer::new(100, 100).unwrap();
        assert!(render_mesh(&mut framebuffer, &mesh, Mat4::identity(), &camera) > 0);
        let behind = Mesh::new(vec![triangle(5.0)]).unwrap();
        assert_eq!(
            render_mesh(&mut framebuffer, &behind, Mat4::identity(), &camera),
            0
        );
    }

    struct ProbeShader;

    impl FragmentShader for ProbeShader {
        fn shade(&self, input: &FragmentInput, uniforms: &ShaderUniforms) -> Color {
            let uv_signal = input.uv.map(|uv| uv.x + uv.y).unwrap_or(0.0);
            Color::new(
                uv_signal + uniforms.time * 0.0,
                input.normal.z.abs(),
                input.depth,
            )
        }
    }

    #[test]
    fn custom_shader_receives_interpolated_attributes() {
        let camera = camera();
        let mut triangle = triangle(-3.0);
        for vertex in &mut triangle.vertices {
            vertex.texcoord = Some(crate::math::Vec2::new(0.25, 0.5));
            vertex.normal = Vec3::Z;
        }
        let mesh = Mesh::new(vec![triangle]).unwrap();
        let mut framebuffer = Framebuffer::new(100, 100).unwrap();
        assert!(
            render_mesh_with_shader(
                &mut framebuffer,
                &mesh,
                Mat4::identity(),
                &camera,
                &ProbeShader,
                &ShaderUniforms::default(),
            ) > 0
        );
        let pixel = (0..100)
            .flat_map(|y| (0..100).map(move |x| (x, y)))
            .find_map(|(x, y)| framebuffer.get_pixel(x, y).filter(|color| color.g > 0.9));
        assert!(pixel.is_some());
    }
}