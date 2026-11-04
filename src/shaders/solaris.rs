use crate::color::Color;
use crate::math::Vec3;
use crate::procedural::{fbm, noise_3d, smoothstep};
use crate::shader::{FragmentInput, FragmentShader, ShaderUniforms};

#[derive(Clone, Copy, Debug)]
pub struct SolarisShader {
    pub pattern_scale: f32,
    pub emission: f32,
    pub turbulence: f32,
    pub contrast: f32,
    pub detail: f32,
    pub seed: u32,
}

impl Default for SolarisShader {
    fn default() -> Self {
        Self {
            pattern_scale: 2.4,
            emission: 1.25,
            turbulence: 1.0,
            contrast: 1.0,
            detail: 1.0,
            seed: 0x51_0A_71_5,
        }
    }
}

impl FragmentShader for SolarisShader {
    fn shade(&self, input: &FragmentInput, uniforms: &ShaderUniforms) -> Color {
        let surface = input.local_position.normalized();
        if surface == Vec3::ZERO || !surface.is_finite() {
            return Color::new(0.22, 0.015, 0.002);
        }

        let time = if uniforms.time.is_finite() {
            uniforms.time
        } else {
            0.0
        };
        let scale = finite_or(self.pattern_scale, 2.4).max(0.01);
        let animated = surface * scale + Vec3::new(time * 0.025, -time * 0.018, time * 0.012);
        let low = fbm(animated * 0.72, self.seed, 4);
        let medium = fbm(
            animated * 1.9 + Vec3::new(3.1, -1.7, 2.4),
            self.seed.wrapping_add(17),
            4,
        );
        let high = noise_3d(
            animated * 7.5 + Vec3::new(-2.0, 4.0, 1.0),
            self.seed.wrapping_add(31),
        );
        let detail = finite_or(self.detail, 1.0).max(0.0);
        let filaments = smoothstep(0.56, 0.86, high) * (0.35 + 0.65 * medium) * detail;
        let turbulence = (0.5 + 0.5 * low) * 0.58 + (0.5 + 0.5 * medium) * 0.27 + filaments * 0.15;
        let turbulence_strength = finite_or(self.turbulence, 1.0).max(0.0);
        let contrast = finite_or(self.contrast, 1.0).max(0.0);
        let temperature = ((0.12 + turbulence * turbulence_strength) * contrast).clamp(0.0, 1.0);
        let base = palette(temperature);

        let view_direction = (uniforms.camera_position - input.world_position).normalized();
        let facing = input
            .normal
            .normalized()
            .dot(view_direction)
            .clamp(0.0, 1.0);
        let contour = (1.0 - facing).powf(1.7);
        let intensity = (finite_or(self.emission, 1.25).max(0.0)
            * (0.72 + 0.38 * temperature + 0.42 * filaments + 0.08 * contour))
            .max(0.0);

        let r = 1.0 - (-base.r * intensity).exp();
        let g = 1.0 - (-base.g * intensity).exp();
        let b = 1.0 - (-base.b * intensity).exp();
        Color::new(r, g, b)
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn palette(temperature: f32) -> Color {
    let deep = Color::new_const(0.22, 0.012, 0.002);
    let hot = Color::new_const(0.72, 0.075, 0.004);
    let main = Color::new_const(1.0, 0.42, 0.018);
    let extreme = Color::new_const(1.0, 0.88, 0.42);
    let t = temperature.clamp(0.0, 1.0);
    let deep_to_hot = smoothstep(0.16, 0.38, t);
    let hot_to_main = smoothstep(0.36, 0.64, t);
    let main_to_extreme = smoothstep(0.62, 0.86, t);
    let first = lerp_color(deep, hot, deep_to_hot);
    let second = lerp_color(first, main, hot_to_main);
    lerp_color(second, extreme, main_to_extreme)
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::new_const(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}

#[cfg(test)]
mod tests {
    use super::{SolarisShader, palette};
    use crate::color::Color;
    use crate::math::{Vec2, Vec3};
    use crate::shader::{FragmentInput, FragmentShader, ShaderUniforms};

    fn input(position: Vec3, normal: Vec3) -> FragmentInput {
        FragmentInput {
            world_position: position,
            local_position: position,
            normal,
            uv: None,
            depth: 0.5,
            screen_position: Vec2::new(1.5, 1.5),
            vertex_color: Color::WHITE,
        }
    }

    #[test]
    fn palette_exposes_four_distinct_temperature_regions() {
        let samples = [palette(0.08), palette(0.32), palette(0.58), palette(0.96)];
        assert!(
            samples
                .windows(2)
                .all(|pair| (pair[0].g - pair[1].g).abs() > 0.01)
        );
        assert!(samples[0].r < samples[3].r || samples[0].g < samples[3].g);
        assert!(
            samples
                .iter()
                .all(|color| color.r.is_finite() && color.g.is_finite() && color.b.is_finite())
        );
    }

    #[test]
    fn shader_is_deterministic_and_uses_surface_position() {
        let shader = SolarisShader::default();
        let uniforms = ShaderUniforms {
            time: 0.0,
            camera_position: Vec3::new(0.0, 0.0, 5.0),
            ..ShaderUniforms::default()
        };
        let a = shader.shade(&input(Vec3::new(0.2, 0.4, 0.8), Vec3::Z), &uniforms);
        let b = shader.shade(&input(Vec3::new(0.2, 0.4, 0.8), Vec3::Z), &uniforms);
        let c = shader.shade(&input(Vec3::new(-0.7, 0.1, 0.7), Vec3::Z), &uniforms);
        assert_eq!(a, b);
        assert!(a.r.is_finite() && a.g.is_finite() && a.b.is_finite());
        assert!(a != c);
    }

    #[test]
    fn shader_responds_to_normal_and_time_without_invalid_output() {
        let shader = SolarisShader::default();
        let front = shader.shade(&input(Vec3::Z, Vec3::Z), &ShaderUniforms::default());
        let edge = shader.shade(
            &input(Vec3::Z, Vec3::X),
            &ShaderUniforms {
                time: 4.0,
                ..ShaderUniforms::default()
            },
        );
        assert!(front != edge);
        for color in [front, edge] {
            assert!(color.r.is_finite() && color.g.is_finite() && color.b.is_finite());
            assert!((0.0..=1.0).contains(&color.r));
            assert!((0.0..=1.0).contains(&color.g));
            assert!((0.0..=1.0).contains(&color.b));
        }
    }
}