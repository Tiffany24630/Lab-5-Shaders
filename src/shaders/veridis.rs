use crate::color::Color;
use crate::math::Vec3;
use crate::procedural::{fbm, noise_3d, smoothstep};
use crate::shader::{apply_point_light, FragmentInput, FragmentShader, ShaderUniforms};

#[derive(Clone, Copy, Debug)]
pub struct VeridisShader {
    pub organic_scale: f32,
    pub structure_density: f32,
    pub branch_scale: f32,
    pub network_width: f32,
    pub glow_density: f32,
    pub biolum_intensity: f32,
    pub dark_contrast: f32,
    pub light_intensity: f32,
    pub detail: f32,
    pub seed: u32,
}

impl Default for VeridisShader {
    fn default() -> Self {
        Self {
            organic_scale: 2.4,
            structure_density: 0.58,
            branch_scale: 5.0,
            network_width: 0.17,
            glow_density: 0.72,
            biolum_intensity: 1.25,
            dark_contrast: 1.0,
            light_intensity: 1.0,
            detail: 1.0,
            seed: 0x7E_71_0101,
        }
    }
}

impl FragmentShader for VeridisShader {
    fn shade(&self, input: &FragmentInput, uniforms: &ShaderUniforms) -> Color {
        let surface = input.local_position.normalized();
        if surface == Vec3::ZERO || !surface.is_finite() {
            return Color::new(0.008, 0.030, 0.018);
        }

        let scale = finite_or(self.organic_scale, 2.4).max(0.01);
        let detail = finite_or(self.detail, 1.0).max(0.0);
        let point = surface * scale;
        let growth = fbm(point * 0.68, self.seed, 5);
        let canopy = fbm(
            point * (1.75 + detail * 0.25) + Vec3::new(2.0, -1.5, 3.4),
            self.seed.wrapping_add(17),
            4,
        );
        let micro = noise_3d(
            point * (7.0 + detail * 2.0) + Vec3::new(-2.0, 4.0, 1.0),
            self.seed.wrapping_add(31),
        );
        let density = ((growth * 0.58 + canopy * 0.34 + micro * 0.08)
            * finite_or(self.dark_contrast, 1.0).max(0.0))
        .clamp(0.0, 1.0);
        let mut color = organic_palette(density);

        let network = organic_network(
            point,
            finite_or(self.branch_scale, 5.0).max(0.1),
            finite_or(self.structure_density, 0.58).clamp(0.05, 0.95),
            finite_or(self.network_width, 0.17).clamp(0.02, 0.42),
            self.seed.wrapping_add(101),
        );
        let network_color = Color::new_const(0.035, 0.46, 0.30);
        color = lerp_color(color, network_color, network * 0.46);

        let glow_field = fbm(
            point * 3.5 + Vec3::new(-3.0, 1.0, 2.0),
            self.seed.wrapping_add(211),
            4,
        );
        let glow = smoothstep(
            finite_or(self.glow_density, 0.72).clamp(0.35, 0.94),
            0.96,
            glow_field,
        ) * (0.55 + 0.45 * network);
        let glow_color = Color::new_const(0.06, 0.78, 0.62);
        color = lerp_color(color, glow_color, glow * 0.72);

        let normal = input.normal.normalized();
        // Time changes only intensity, keeping the living networks attached to the surface.
        let time = finite_or(uniforms.time, 0.0);
        let pulse = 0.88 + 0.12 * (time * 0.8 + growth * 6.0).sin();
        let emission = glow * finite_or(self.biolum_intensity, 1.25).max(0.0) * pulse;
        let emission_color = Color::new_const(
            emission * 0.025,
            emission * 0.34,
            emission * 0.25,
        );
        apply_point_light(
            color,
            emission_color,
            normal,
            input.world_position,
            uniforms,
            self.light_intensity,
        )
    }
}

fn organic_palette(value: f32) -> Color {
    let deep = Color::new_const(0.008, 0.035, 0.018);
    let natural = Color::new_const(0.025, 0.230, 0.090);
    let emerald = Color::new_const(0.040, 0.520, 0.230);
    let turquoise = Color::new_const(0.100, 0.720, 0.500);
    let t = value.clamp(0.0, 1.0);
    let a = lerp_color(deep, natural, smoothstep(0.12, 0.40, t));
    let b = lerp_color(a, emerald, smoothstep(0.36, 0.70, t));
    lerp_color(b, turquoise, smoothstep(0.68, 0.94, t))
}

fn organic_network(point: Vec3, scale: f32, density: f32, width: f32, seed: u32) -> f32 {
    let first = noise_3d(point * scale, seed);
    let second = noise_3d(
        point * (scale * 1.43) + Vec3::new(5.0, -3.0, 2.5),
        seed.wrapping_add(1),
    );
    let branch_a = (1.0 - (first - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    let branch_b = (1.0 - (second - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    let network = (branch_a * 0.62 + branch_b * 0.38).clamp(0.0, 1.0);
    let threshold = 0.73 + (1.0 - density) * 0.15;
    smoothstep(threshold - width * 0.30, threshold + width * 0.22, network)
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::new_const(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::{VeridisShader, organic_network, organic_palette};
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
            screen_position: Vec2::new(16.5, 16.5),
            vertex_color: Color::WHITE,
        }
    }

    #[test]
    fn organic_palette_has_dark_natural_emerald_and_turquoise_layers() {
        let samples = [
            organic_palette(0.05),
            organic_palette(0.32),
            organic_palette(0.62),
            organic_palette(0.96),
        ];
        assert!(samples[0].g > samples[0].r);
        assert!(samples[3].g > samples[0].g);
        assert!(samples.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn network_is_localized_and_position_dependent() {
        let points = [
            Vec3::new(0.14, 0.52, 0.81),
            Vec3::new(-0.71, 0.18, 0.42),
            Vec3::new(0.33, -0.62, 0.41),
            Vec3::new(-0.86, -0.24, -0.17),
        ];
        let values: Vec<_> = points
            .into_iter()
            .map(|point| organic_network(point, 5.0, 0.58, 0.17, 7))
            .collect();
        assert!(
            values
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        );
        assert!(values.iter().any(|value| *value > 0.0));
        assert!(
            values
                .windows(2)
                .any(|pair| (pair[0] - pair[1]).abs() > 1e-5)
        );
    }

    #[test]
    fn shader_responds_to_normals_and_time_deterministically() {
        let shader = VeridisShader::default();
        let uniforms = ShaderUniforms {
            camera_position: Vec3::new(0.0, 0.0, 5.0),
            light_position: Vec3::new(3.0, 2.0, 4.0),
            ..ShaderUniforms::default()
        };
        let a = shader.shade(&input(Vec3::Z, Vec3::Z), &uniforms);
        let b = shader.shade(
            &input(Vec3::Z, Vec3::X),
            &ShaderUniforms {
                time: 5.0,
                ..uniforms
            },
        );
        assert_ne!(a, b);
        assert_eq!(a, shader.shade(&input(Vec3::Z, Vec3::Z), &uniforms));
        for color in [a, b] {
            assert!(color.r.is_finite() && color.g.is_finite() && color.b.is_finite());
            assert!((0.0..=1.0).contains(&color.r));
            assert!((0.0..=1.0).contains(&color.g));
            assert!((0.0..=1.0).contains(&color.b));
        }
    }
}