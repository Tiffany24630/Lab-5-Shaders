use crate::color::Color;
use crate::math::Vec3;
use crate::procedural::{fbm, noise_3d, smoothstep};
use crate::shader::{apply_point_light, FragmentInput, FragmentShader, ShaderUniforms};

#[derive(Clone, Copy, Debug)]
pub struct CryonShader {
    pub ice_scale: f32,
    pub crest_scale: f32,
    pub apparent_relief: f32,
    pub fracture_scale: f32,
    pub fracture_density: f32,
    pub fracture_width: f32,
    pub crystal_density: f32,
    pub ice_contrast: f32,
    pub light_intensity: f32,
    pub detail: f32,
    pub seed: u32,
}

impl Default for CryonShader {
    fn default() -> Self {
        Self {
            ice_scale: 2.3,
            crest_scale: 5.2,
            apparent_relief: 1.0,
            fracture_scale: 4.4,
            fracture_density: 0.52,
            fracture_width: 0.16,
            crystal_density: 0.68,
            ice_contrast: 1.0,
            light_intensity: 1.0,
            detail: 1.0,
            seed: 0xC2_9F_0101,
        }
    }
}

impl FragmentShader for CryonShader {
    fn shade(&self, input: &FragmentInput, uniforms: &ShaderUniforms) -> Color {
        let surface = input.local_position.normalized();
        if surface == Vec3::ZERO || !surface.is_finite() {
            return Color::new(0.015, 0.045, 0.090);
        }

        let scale = finite_or(self.ice_scale, 2.3).max(0.01);
        let detail = finite_or(self.detail, 1.0).max(0.0);
        let point = surface * scale;

        let plates = fbm(point * 0.62, self.seed, 5);
        let crest_noise = fbm(
            point * (finite_or(self.crest_scale, 5.2) * 0.34).max(0.1) + Vec3::new(1.7, -3.0, 2.4),
            self.seed.wrapping_add(17),
            4,
        );
        let fine = noise_3d(
            point * (7.0 + detail * 2.5) + Vec3::new(-4.0, 1.0, 3.0),
            self.seed.wrapping_add(31),
        );
        let ridge = (1.0 - (crest_noise * 2.0 - 1.0).abs()).clamp(0.0, 1.0);
        let apparent_relief = finite_or(self.apparent_relief, 1.0).max(0.0);
        let elevation =
            ((plates * 0.52 + ridge * 0.38 + fine * 0.10) * apparent_relief).clamp(0.0, 1.0);
        let contrast = finite_or(self.ice_contrast, 1.0).max(0.0);
        let ice_value = ((elevation - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
        let mut color = ice_palette(ice_value);

        let fracture = fracture_mask(
            point,
            finite_or(self.fracture_scale, 4.4).max(0.1),
            finite_or(self.fracture_density, 0.52).clamp(0.05, 0.95),
            finite_or(self.fracture_width, 0.16).clamp(0.02, 0.40),
            self.seed.wrapping_add(101),
        );
        let fracture_color = Color::new_const(0.12, 0.72, 0.90);
        color = lerp_color(color, fracture_color, fracture * 0.62);

        let crystal_noise = fbm(
            point * (3.1 + detail) + Vec3::new(3.0, 2.0, -1.0),
            self.seed.wrapping_add(211),
            3,
        );
        let crystal = smoothstep(
            finite_or(self.crystal_density, 0.68).clamp(0.30, 0.92),
            0.94,
            crystal_noise,
        ) * smoothstep(0.38, 0.78, ridge);
        let crystal_color = Color::new_const(0.78, 0.94, 1.0);
        color = lerp_color(color, crystal_color, crystal * 0.72);

        let normal = input.normal.normalized();
        let crystal_glint = crystal * 0.24;
        let crystal_emission = Color::new_const(
            crystal_glint * 0.35,
            crystal_glint * 0.45,
            crystal_glint * 0.55,
        );
        apply_point_light(
            color,
            crystal_emission,
            normal,
            input.world_position,
            uniforms,
            self.light_intensity,
        )
    }
}

fn ice_palette(value: f32) -> Color {
    let deep = Color::new_const(0.015, 0.045, 0.105);
    let cold = Color::new_const(0.070, 0.230, 0.420);
    let crest = Color::new_const(0.270, 0.650, 0.820);
    let crystal = Color::new_const(0.760, 0.900, 0.980);
    let t = value.clamp(0.0, 1.0);
    let a = lerp_color(deep, cold, smoothstep(0.12, 0.40, t));
    let b = lerp_color(a, crest, smoothstep(0.38, 0.70, t));
    lerp_color(b, crystal, smoothstep(0.68, 0.94, t))
}

fn fracture_mask(point: Vec3, scale: f32, density: f32, width: f32, seed: u32) -> f32 {
    let first = noise_3d(point * scale, seed);
    let second = noise_3d(
        point * (scale * 1.41) + Vec3::new(6.0, -2.5, 3.7),
        seed.wrapping_add(1),
    );
    let line_a = (1.0 - (first - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    let line_b = (1.0 - (second - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    let network = (line_a * 0.64 + line_b * 0.36).clamp(0.0, 1.0);
    let threshold = 0.72 + (1.0 - density) * 0.16;
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
    use super::{CryonShader, fracture_mask, ice_palette};
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
            screen_position: Vec2::new(14.5, 14.5),
            vertex_color: Color::WHITE,
        }
    }

    #[test]
    fn ice_palette_has_deep_cold_crest_and_crystal_layers() {
        let samples = [
            ice_palette(0.05),
            ice_palette(0.32),
            ice_palette(0.62),
            ice_palette(0.96),
        ];
        assert!(samples[0].b > samples[0].r);
        assert!(samples[3].r > samples[0].r && samples[3].g > samples[0].g);
        assert!(samples.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn shader_uses_position_and_produces_fracture_variation() {
        let shader = CryonShader::default();
        let uniforms = ShaderUniforms {
            camera_position: Vec3::new(0.0, 0.0, 5.0),
            light_position: Vec3::new(3.0, 2.0, 4.0),
            ..ShaderUniforms::default()
        };
        let a = shader.shade(&input(Vec3::new(0.12, 0.54, 0.82), Vec3::Z), &uniforms);
        let b = shader.shade(&input(Vec3::new(-0.71, 0.18, 0.42), Vec3::Z), &uniforms);
        assert_ne!(a, b);
        let f1 = fracture_mask(Vec3::new(0.12, 0.54, 0.82), 4.4, 0.52, 0.16, 3);
        let f2 = fracture_mask(Vec3::new(-0.71, 0.18, 0.42), 4.4, 0.52, 0.16, 3);
        assert!((f1 - f2).abs() > 1e-5 || f1 > 0.0 || f2 > 0.0);
    }

    #[test]
    fn shader_responds_to_normals_is_deterministic_and_finite() {
        let shader = CryonShader::default();
        let uniforms = ShaderUniforms {
            camera_position: Vec3::new(0.0, 0.0, 5.0),
            light_position: Vec3::new(3.0, 2.0, 4.0),
            ..ShaderUniforms::default()
        };
        let a = shader.shade(&input(Vec3::Z, Vec3::Z), &uniforms);
        let b = shader.shade(&input(Vec3::Z, Vec3::X), &uniforms);
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