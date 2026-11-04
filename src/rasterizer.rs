use crate::color::Color;
use crate::framebuffer::Framebuffer;

#[derive(Clone, Copy, Debug)]
pub struct RasterVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub color: Color,
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

fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (px - ax) * (by - ay) - (py - ay) * (bx - ax)
}

#[cfg(test)]
mod tests {
    use super::{RasterVertex, rasterize_triangle};
    use crate::color::Color;
    use crate::framebuffer::Framebuffer;

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
}