use std::f32::consts::PI;

use crate::camera::Camera;
use crate::color::Color;
use crate::math::Vec3;

const BASE: Color = Color::new_const(0.003, 0.004, 0.010);
const STAR_COUNT: usize = 6000;

#[derive(Clone, Copy)]
struct Star {
    direction: Vec3,
    intensity: f32,
    size: usize,
    color: Color,
}

pub struct SpaceBackground {
    stars: Vec<Star>,
}

impl SpaceBackground {
    pub fn new(seed: u32) -> Self {
        let mut state = seed;
        let mut stars = Vec::with_capacity(STAR_COUNT);
        for index in 0..STAR_COUNT {
            let u = next_unit(&mut state);
            let v = next_unit(&mut state);
            let y = v * 2.0 - 1.0;
            let phi = u * 2.0 * PI;
            let radius = (1.0 - y * y).max(0.0).sqrt();
            let direction = Vec3::new(radius * phi.cos(), y, radius * phi.sin());
            let intensity = 0.42 + next_unit(&mut state) * 0.58;
            let size = if index % 37 == 0 { 2 } else { 1 };
            let color = match index % 19 {
                0 => Color::new_const(0.78, 0.86, 1.0),
                1 => Color::new_const(1.0, 0.92, 0.78),
                _ => Color::new_const(0.90, 0.94, 1.0),
            };
            stars.push(Star { direction, intensity, size, color });
        }
        Self { stars }
    }

    pub fn render(&self, camera: &Camera, width: usize, height: usize) -> Vec<Color> {
        let mut colors = vec![BASE; width.saturating_mul(height)];
        let forward = (camera.target - camera.position).normalized();
        let right = forward.cross(camera.up).normalized();
        let up = right.cross(forward).normalized();
        if forward == Vec3::ZERO || right == Vec3::ZERO || up == Vec3::ZERO {
            return colors;
        }

        self.draw_nebula(&mut colors, width, height, forward, right, up, camera);
        let tangent = (camera.fov_y * 0.5).tan();
        for star in &self.stars {
            let forward_component = star.direction.dot(forward);
            if forward_component <= 0.0 {
                continue;
            }
            let ndc_x = star.direction.dot(right) / (forward_component * tangent * camera.aspect);
            let ndc_y = star.direction.dot(up) / (forward_component * tangent);
            if !ndc_x.is_finite() || !ndc_y.is_finite() || ndc_x.abs() > 1.0 || ndc_y.abs() > 1.0 {
                continue;
            }
            let x = ((ndc_x + 1.0) * 0.5 * width as f32) as isize;
            let y = ((1.0 - ndc_y) * 0.5 * height as f32) as isize;
            let color = Color::new(
                star.color.r * star.intensity,
                star.color.g * star.intensity,
                star.color.b * star.intensity,
            );
            let radius = star.size as isize / 2;
            for dy in 0..star.size as isize {
                for dx in 0..star.size as isize {
                    let px = x + dx - radius;
                    let py = y + dy - radius;
                    if px >= 0 && py >= 0 && (px as usize) < width && (py as usize) < height {
                        colors[py as usize * width + px as usize] = color;
                    }
                }
            }
        }
        colors
    }

    fn draw_nebula(
        &self,
        colors: &mut [Color],
        width: usize,
        height: usize,
        forward: Vec3,
        right: Vec3,
        up: Vec3,
        camera: &Camera,
    ) {
        let tangent = (camera.fov_y * 0.5).tan();
        for y in 0..height {
            for x in 0..width {
                let sx = ((x as f32 + 0.5) / width as f32 * 2.0 - 1.0)
                    * tangent * camera.aspect;
                let sy = (1.0 - (y as f32 + 0.5) / height as f32 * 2.0) * tangent;
                let direction = (forward + right * sx + up * sy).normalized();
                let wave = (direction.x * 4.7 + direction.y * 2.3).sin()
                    * (direction.z * 5.1 - direction.x * 1.7).cos();
                let mask = ((wave + 1.0) * 0.5).powi(5) * 0.012;
                let index = y * width + x;
                colors[index] = Color::new(BASE.r + mask * 0.25, BASE.g + mask * 0.35, BASE.b + mask);
            }
        }
    }
}

fn next_unit(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    (*state as f32) / (u32::MAX as f32)
}