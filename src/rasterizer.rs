use crate::color::Color;
use crate::framebuffer::Framebuffer;
use crate::math::Vec3;
use crate::shader::{FragmentInput, FragmentShader, ShaderUniforms, VertexColorShader};
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub struct RasterVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub color: Color,
}

#[derive(Clone, Copy, Debug)]
pub struct PerspectiveRasterVertex {
    pub x: f32,
    pub y: f32,
    pub ndc_depth: f32,
    pub inv_w: f32,
    pub color_over_w: Color,
    pub normal_over_w: Vec3,
    pub local_position_over_w: Vec3,
    pub world_position_over_w: Vec3,
    pub uv_over_w: Option<crate::math::Vec2>,
}

pub fn rasterize_triangle(framebuffer: &mut Framebuffer, vertices: [RasterVertex; 3]) -> usize {
    if vertices
        .iter()
        .any(|vertex| !vertex.x.is_finite() || !vertex.y.is_finite() || !vertex.z.is_finite())
    {
        return 0;
    }

    let [v0, v1, v2] = vertices;
    let area = edge(v0.x, v0.y, v1.x, v1.y, v2.x, v2.y);
    if !area.is_finite() || area.abs() <= f32::EPSILON {
        return 0;
    }

    let min_x = v0.x.min(v1.x).min(v2.x).floor().max(0.0) as i32;
    let max_x =
        v0.x.max(v1.x)
            .max(v2.x)
            .ceil()
            .min(framebuffer.width() as f32) as i32
            - 1;
    let min_y = v0.y.min(v1.y).min(v2.y).floor().max(0.0) as i32;
    let max_y =
        v0.y.max(v1.y)
            .max(v2.y)
            .ceil()
            .min(framebuffer.height() as f32) as i32
            - 1;

    if min_x > max_x || min_y > max_y {
        return 0;
    }

    let mut written = 0;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let lambda0 = edge(v1.x, v1.y, v2.x, v2.y, px, py) / area;
            let lambda1 = edge(v2.x, v2.y, v0.x, v0.y, px, py) / area;
            let lambda2 = edge(v0.x, v0.y, v1.x, v1.y, px, py) / area;
            let epsilon = 1.0e-6;
            if lambda0 < -epsilon || lambda1 < -epsilon || lambda2 < -epsilon {
                continue;
            }

            let depth = v0.z * lambda0 + v1.z * lambda1 + v2.z * lambda2;
            let color = Color::new(
                v0.color.r * lambda0 + v1.color.r * lambda1 + v2.color.r * lambda2,
                v0.color.g * lambda0 + v1.color.g * lambda1 + v2.color.g * lambda2,
                v0.color.b * lambda0 + v1.color.b * lambda1 + v2.color.b * lambda2,
            );
            if framebuffer.write_pixel(x as usize, y as usize, color, depth) {
                written += 1;
            }
        }
    }
    written
}

pub fn rasterize_triangle_perspective(
    framebuffer: &mut Framebuffer,
    vertices: [PerspectiveRasterVertex; 3],
) -> usize {
    rasterize_triangle_with_shader(
        framebuffer,
        vertices,
        &VertexColorShader,
        &ShaderUniforms::default(),
    )
}

pub fn rasterize_triangle_with_shader<S: FragmentShader>(
    framebuffer: &mut Framebuffer,
    vertices: [PerspectiveRasterVertex; 3],
    shader: &S,
    uniforms: &ShaderUniforms,
) -> usize {
    if vertices.iter().any(|vertex| {
        !vertex.x.is_finite()
            || !vertex.y.is_finite()
            || !vertex.ndc_depth.is_finite()
            || !vertex.inv_w.is_finite()
            || vertex.inv_w <= 0.0
            || !vertex.color_over_w.r.is_finite()
            || !vertex.color_over_w.g.is_finite()
            || !vertex.color_over_w.b.is_finite()
            || !vertex.normal_over_w.is_finite()
            || !vertex.local_position_over_w.is_finite()
            || !vertex.world_position_over_w.is_finite()
            || vertex
                .uv_over_w
                .map(|uv| !uv.x.is_finite() || !uv.y.is_finite())
                .unwrap_or(false)
    }) {
        return 0;
    }

    let [v0, v1, v2] = vertices;
    let area = edge(v0.x, v0.y, v1.x, v1.y, v2.x, v2.y);
    if !area.is_finite() || area.abs() <= f32::EPSILON {
        return 0;
    }

    let min_x = v0.x.min(v1.x).min(v2.x).floor().max(0.0) as i32;
    let max_x =
        v0.x.max(v1.x)
            .max(v2.x)
            .ceil()
            .min(framebuffer.width() as f32) as i32
            - 1;
    let min_y = v0.y.min(v1.y).min(v2.y).floor().max(0.0) as i32;
    let max_y =
        v0.y.max(v1.y)
            .max(v2.y)
            .ceil()
            .min(framebuffer.height() as f32) as i32
            - 1;
    if min_x > max_x || min_y > max_y {
        return 0;
    }

    let mut written = 0;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let lambda0 = edge(v1.x, v1.y, v2.x, v2.y, px, py) / area;
            let lambda1 = edge(v2.x, v2.y, v0.x, v0.y, px, py) / area;
            let lambda2 = edge(v0.x, v0.y, v1.x, v1.y, px, py) / area;
            if lambda0 < -1.0e-6 || lambda1 < -1.0e-6 || lambda2 < -1.0e-6 {
                continue;
            }

            let inv_w = lambda0 * v0.inv_w + lambda1 * v1.inv_w + lambda2 * v2.inv_w;
            if !inv_w.is_finite() || inv_w <= f32::EPSILON {
                continue;
            }
            // Projected depth is already affine in screen-space barycentrics.
            // Color needs perspective correction; depth must not be divided by inv_w again.
            let depth_ndc =
                lambda0 * v0.ndc_depth + lambda1 * v1.ndc_depth + lambda2 * v2.ndc_depth;
            if !depth_ndc.is_finite() || !(-1.0 - 1.0e-5..=1.0 + 1.0e-5).contains(&depth_ndc) {
                continue;
            }
            let depth = ((depth_ndc + 1.0) * 0.5).clamp(0.0, 1.0);
            let color_over_w = Color::new_const(
                lambda0 * v0.color_over_w.r
                    + lambda1 * v1.color_over_w.r
                    + lambda2 * v2.color_over_w.r,
                lambda0 * v0.color_over_w.g
                    + lambda1 * v1.color_over_w.g
                    + lambda2 * v2.color_over_w.g,
                lambda0 * v0.color_over_w.b
                    + lambda1 * v1.color_over_w.b
                    + lambda2 * v2.color_over_w.b,
            );
            let color = Color::new(
                color_over_w.r / inv_w,
                color_over_w.g / inv_w,
                color_over_w.b / inv_w,
            );
            if !framebuffer.depth_test(x as usize, y as usize, depth) {
                continue;
            }
            let normal_over_w = v0.normal_over_w * lambda0
                + v1.normal_over_w * lambda1
                + v2.normal_over_w * lambda2;
            let local_over_w = v0.local_position_over_w * lambda0
                + v1.local_position_over_w * lambda1
                + v2.local_position_over_w * lambda2;
            let world_over_w = v0.world_position_over_w * lambda0
                + v1.world_position_over_w * lambda1
                + v2.world_position_over_w * lambda2;
            let uv = interpolate_uv(&v0, &v1, &v2, lambda0, lambda1, lambda2, inv_w);
            let screen_position = crate::math::Vec2::new(px, py);
            let input = FragmentInput {
                world_position: world_over_w / inv_w,
                local_position: local_over_w / inv_w,
                normal: (normal_over_w / inv_w).normalized(),
                uv,
                depth,
                screen_position,
                vertex_color: color,
            };
            let shader_started = uniforms.detail_profile.as_ref().map(|_| Instant::now());
            let shaded = shader.shade(&input, uniforms);
            if let (Some(profile), Some(shader_started)) = (&uniforms.detail_profile, shader_started) {
                profile.add_shader(shader_started.elapsed());
            }
            if !shaded.r.is_finite() || !shaded.g.is_finite() || !shaded.b.is_finite() {
                continue;
            }
            if framebuffer.write_pixel(
                x as usize,
                y as usize,
                Color::new(shaded.r, shaded.g, shaded.b),
                depth,
            ) {
                written += 1;
            }
        }
    }
    written
}

fn interpolate_uv(
    v0: &PerspectiveRasterVertex,
    v1: &PerspectiveRasterVertex,
    v2: &PerspectiveRasterVertex,
    lambda0: f32,
    lambda1: f32,
    lambda2: f32,
    inv_w: f32,
) -> Option<crate::math::Vec2> {
    let a = v0.uv_over_w?;
    let b = v1.uv_over_w?;
    let c = v2.uv_over_w?;
    Some((a * lambda0 + b * lambda1 + c * lambda2) / inv_w)
}

fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (px - ax) * (by - ay) - (py - ay) * (bx - ax)
}

#[cfg(test)]
mod tests {
    use super::{
        PerspectiveRasterVertex, RasterVertex, rasterize_triangle, rasterize_triangle_perspective,
    };
    use crate::color::Color;
    use crate::framebuffer::Framebuffer;
    use crate::math::Vec3;

    fn vertex(x: f32, y: f32, z: f32, color: Color) -> RasterVertex {
        RasterVertex { x, y, z, color }
    }

    fn triangle(z: f32) -> [RasterVertex; 3] {
        [
            vertex(1.0, 1.0, z, Color::new(1.0, 0.0, 0.0)),
            vertex(7.0, 1.0, z, Color::new(0.0, 1.0, 0.0)),
            vertex(1.0, 7.0, z, Color::new(0.0, 0.0, 1.0)),
        ]
    }

    #[test]
    fn rasterizes_both_orientations_and_interpolates() {
        let mut framebuffer = Framebuffer::new(8, 8).unwrap();
        assert!(rasterize_triangle(&mut framebuffer, triangle(0.5)) > 0);
        let color = framebuffer.get_pixel(2, 2).unwrap();
        assert!(color.r > 0.0 && color.g > 0.0 && color.b > 0.0);

        let mut reversed = Framebuffer::new(8, 8).unwrap();
        let t = triangle(0.5);
        assert!(rasterize_triangle(&mut reversed, [t[0], t[2], t[1]]) > 0);
    }

    #[test]
    fn ignores_degenerate_non_finite_and_offscreen_triangles() {
        let mut framebuffer = Framebuffer::new(8, 8).unwrap();
        assert_eq!(
            rasterize_triangle(&mut framebuffer, [triangle(0.5)[0]; 3]),
            0
        );
        let mut non_finite = triangle(0.5);
        non_finite[0].x = f32::NAN;
        assert_eq!(rasterize_triangle(&mut framebuffer, non_finite), 0);
        let offscreen = [
            vertex(-10.0, -10.0, 0.5, Color::WHITE),
            vertex(-5.0, -10.0, 0.5, Color::WHITE),
            vertex(-10.0, -5.0, 0.5, Color::WHITE),
        ];
        assert_eq!(rasterize_triangle(&mut framebuffer, offscreen), 0);
    }

    #[test]
    fn clips_partial_triangles_and_respects_depth() {
        let mut framebuffer = Framebuffer::new(8, 8).unwrap();
        let mut partial = triangle(0.5);
        partial[0].x = -3.0;
        assert!(rasterize_triangle(&mut framebuffer, partial) > 0);
        let mut depth_buffer = Framebuffer::new(8, 8).unwrap();
        assert!(rasterize_triangle(&mut depth_buffer, triangle(0.8)) > 0);
        assert_eq!(depth_buffer.get_depth(2, 2), Some(0.8));
        let mut near = triangle(0.2);
        near[0].color = Color::WHITE;
        near[1].color = Color::WHITE;
        near[2].color = Color::WHITE;
        assert!(rasterize_triangle(&mut depth_buffer, near) > 0);
        assert_eq!(depth_buffer.get_depth(2, 2), Some(0.2));
        assert_eq!(depth_buffer.get_pixel(2, 2), Some(Color::WHITE));
        assert_eq!(framebuffer.get_depth(2, 2), Some(0.5));
    }

    #[test]
    fn perspective_interpolation_recovers_color() {
        let mut framebuffer = Framebuffer::new(8, 8).unwrap();
        let vertices = [
            PerspectiveRasterVertex {
                x: 1.0,
                y: 1.0,
                ndc_depth: -0.8,
                inv_w: 1.0,
                color_over_w: Color::new_const(1.0, 0.0, 0.0),
                normal_over_w: Vec3::Z,
                local_position_over_w: Vec3::ZERO,
                world_position_over_w: Vec3::ZERO,
                uv_over_w: None,
            },
            PerspectiveRasterVertex {
                x: 7.0,
                y: 1.0,
                ndc_depth: 0.4,
                inv_w: 0.5,
                color_over_w: Color::new_const(0.0, 0.5, 0.0),
                normal_over_w: Vec3::Z,
                local_position_over_w: Vec3::ZERO,
                world_position_over_w: Vec3::ZERO,
                uv_over_w: None,
            },
            PerspectiveRasterVertex {
                x: 1.0,
                y: 7.0,
                ndc_depth: 0.8,
                inv_w: 1.0,
                color_over_w: Color::new_const(0.0, 0.0, 1.0),
                normal_over_w: Vec3::Z,
                local_position_over_w: Vec3::ZERO,
                world_position_over_w: Vec3::ZERO,
                uv_over_w: None,
            },
        ];
        assert!(rasterize_triangle_perspective(&mut framebuffer, vertices) > 0);
        let color = framebuffer.get_pixel(2, 2).unwrap();
        assert!(color.r.is_finite() && color.g.is_finite() && color.b.is_finite());
        assert!((color.r - 4.0 / 7.0).abs() < 1.0e-5);
        assert!((color.g - 1.0 / 7.0).abs() < 1.0e-5);
        assert!((color.b - 2.0 / 7.0).abs() < 1.0e-5);
        assert!((framebuffer.get_depth(2, 2).unwrap() - 0.45).abs() < 1.0e-5);
    }

    #[test]
    fn perspective_rasterizer_rejects_invalid_reciprocal_depth() {
        let mut framebuffer = Framebuffer::new(8, 8).unwrap();
        let vertex = PerspectiveRasterVertex {
            x: 1.0,
            y: 1.0,
            ndc_depth: 0.0,
            inv_w: 0.0,
            color_over_w: Color::WHITE,
            normal_over_w: Vec3::Z,
            local_position_over_w: Vec3::ZERO,
            world_position_over_w: Vec3::ZERO,
            uv_over_w: None,
        };
        assert_eq!(
            rasterize_triangle_perspective(&mut framebuffer, [vertex; 3]),
            0
        );
    }
}