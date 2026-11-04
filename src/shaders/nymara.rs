use crate::color::Color;
use crate::math::Vec3;
use crate::procedural::{fbm, noise_3d, smoothstep};
use crate::shader::{FragmentInput, FragmentShader, ShaderUniforms};

#[derive(Clone, Copy, Debug)]
pub struct NymaraShader {
    pub band_frequency: f32,
    pub band_distortion: f32,
    pub atmospheric_contrast: f32,
    pub storm_intensity: f32,
    pub turbulence_scale: f32,
    pub light_intensity: f32,
    pub detail: f32,
    pub seed: u32,
}

impl Default for NymaraShader {
    fn default() -> Self {
        Self {
            band_frequency: 7.0,
            band_distortion: 1.8,
            atmospheric_contrast: 1.0,
            storm_intensity: 1.0,
            turbulence_scale: 3.5,
            light_intensity: 1.0,
            detail: 1.0,
            seed: 0x4E_59_4D_01,
        }
    }
}

impl FragmentShader for NymaraShader {
    fn shade(&self, input: &FragmentInput, uniforms: &ShaderUniforms) -> Color {
        let surface = input.local_position.normalized();
        if surface == Vec3::ZERO || !surface.is_finite() {
            return Color::new(0.01, 0.04, 0.06);
        }

        let time = finite_or(uniforms.time, 0.0);
        let detail = finite_or(self.detail, 1.0).max(0.0);
        let turbulence_scale = finite_or(self.turbulence_scale, 3.5).max(0.1);
        let drift = Vec3::new(time * 0.006, 0.0, -time * 0.004);
        let point = surface * turbulence_scale + drift;

        let broad = fbm(point * 0.55, self.seed, 4);
        let medium = fbm(
            point * 1.55 + Vec3::new(2.7, -3.1, 1.4),
            self.seed.wrapping_add(17),
            4,
        );
        let fine = noise_3d(
            point * (5.5 + detail * 2.0) + Vec3::new(-1.0, 4.0, 2.0),
            self.seed.wrapping_add(31),
        );
        let distortion =
            (broad - 0.5) * finite_or(self.band_distortion, 1.8) + (medium - 0.5) * 0.55;
        let frequency = finite_or(self.band_frequency, 7.0).max(0.1);
        let band_wave = 0.5 + 0.5 * (surface.y * frequency + distortion).sin();
        let band_value = (band_wave * 0.72 + broad * 0.18 + fine * 0.10).clamp(0.0, 1.0);
        let contrast = finite_or(self.atmospheric_contrast, 1.0).max(0.0);
        let atmospheric_value = ((band_value - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
        let mut color = atmospheric_palette(atmospheric_value);

        let storm_field = fbm(
            surface * 4.6 + Vec3::new(-2.0, 1.5, 4.0) + drift * 1.8,
            self.seed.wrapping_add(71),
            5,
        );
        let storm_detail = noise_3d(
            surface * 12.0 + Vec3::new(5.0, -1.0, 3.0) + drift * 3.0,
            self.seed.wrapping_add(89),
        );
        let storm = smoothstep(0.60, 0.80, storm_field)
            * (0.55 + 0.45 * storm_detail)
            * finite_or(self.storm_intensity, 1.0).max(0.0);
        let storm_color = Color::new_const(0.70, 0.86, 0.92);
        color = lerp_color(color, storm_color, (storm * 0.58).clamp(0.0, 0.68));

        let normal = input.normal.normalized();
        let light_direction = (uniforms.light_position - input.world_position).normalized();
        let view_direction = (uniforms.camera_position - input.world_position).normalized();
        let diffuse = normal.dot(light_direction).clamp(0.0, 1.0);
        let rim = (1.0 - normal.dot(view_direction).clamp(0.0, 1.0)).powf(1.8);
        let illumination =
            (0.54 + 0.36 * diffuse + 0.10 * rim) * finite_or(self.light_intensity, 1.0).max(0.0);
        scale_color(color, illumination)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NymaraRingShader {
    pub tint: Color,
    pub brightness: f32,
    pub seed: u32,
}

impl NymaraRingShader {
    pub const fn new(tint: Color, brightness: f32, seed: u32) -> Self {
        Self {
            tint,
            brightness,
            seed,
        }
    }
}

impl FragmentShader for NymaraRingShader {
    fn shade(&self, input: &FragmentInput, _uniforms: &ShaderUniforms) -> Color {
        let normal = input.normal.normalized();
        let variation = 0.86 + 0.14 * noise_3d(input.local_position * 5.0, self.seed);
        let face = 0.72 + 0.28 * normal.z.abs();
        scale_color(
            self.tint,
            finite_or(self.brightness, 0.75).max(0.0) * variation * face,
        )
    }
}

fn atmospheric_palette(value: f32) -> Color {
    let deep = Color::new_const(0.015, 0.090, 0.120);
    let turquoise = Color::new_const(0.035, 0.390, 0.420);
    let cyan = Color::new_const(0.180, 0.760, 0.760);
    let violet = Color::new_const(0.290, 0.120, 0.410);
    let t = value.clamp(0.0, 1.0);
    let a = lerp_color(deep, turquoise, smoothstep(0.12, 0.40, t));
    let b = lerp_color(a, cyan, smoothstep(0.36, 0.66, t));
    lerp_color(b, violet, smoothstep(0.62, 0.92, t))
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::new_const(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}

fn scale_color(color: Color, factor: f32) -> Color {
    Color::new(color.r * factor, color.g * factor, color.b * factor)
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::{NymaraShader, atmospheric_palette};
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
            screen_position: Vec2::new(12.5, 12.5),
            vertex_color: Color::WHITE,
        }
    }

    #[test]
    fn palette_contains_distinct_atmospheric_layers() {
        let samples = [
            atmospheric_palette(0.08),
            atmospheric_palette(0.32),
            atmospheric_palette(0.58),
            atmospheric_palette(0.88),
        ];
        assert!(samples.windows(2).any(|pair| pair[0] != pair[1]));
        assert!(
            samples
                .iter()
                .all(|color| { color.r.is_finite() && color.g.is_finite() && color.b.is_finite() })
        );
    }

    #[test]
    fn shader_uses_position_and_has_turbulent_variation() {
        let shader = NymaraShader::default();
        let uniforms = ShaderUniforms {
            camera_position: Vec3::new(0.0, 0.0, 5.0),
            light_position: Vec3::new(3.0, 2.0, 4.0),
            ..ShaderUniforms::default()
        };
        let a = shader.shade(&input(Vec3::new(0.1, 0.5, 0.8), Vec3::Z), &uniforms);
        let b = shader.shade(&input(Vec3::new(-0.7, 0.2, 0.4), Vec3::Z), &uniforms);
        assert_ne!(a, b);
        assert!(fracture_samples(Vec3::new(0.1, 0.5, 0.8), 7).0.is_finite());
    }

    #[test]
    fn shader_responds_to_normal_and_time_deterministically() {
        let shader = NymaraShader::default();
        let base = ShaderUniforms::default();
        let a = shader.shade(&input(Vec3::Z, Vec3::Z), &base);
        let b = shader.shade(
            &input(Vec3::Z, Vec3::X),
            &ShaderUniforms { time: 6.0, ..base },
        );
        assert_ne!(a, b);
        assert_eq!(a, shader.shade(&input(Vec3::Z, Vec3::Z), &base));
        for color in [a, b] {
            assert!(color.r.is_finite() && color.g.is_finite() && color.b.is_finite());
            assert!((0.0..=1.0).contains(&color.r));
            assert!((0.0..=1.0).contains(&color.g));
            assert!((0.0..=1.0).contains(&color.b));
        }
    }

    fn fracture_samples(point: Vec3, seed: u32) -> (f32, f32) {
        let broad = crate::procedural::fbm(point * 2.0, seed, 4);
        let fine = crate::procedural::noise_3d(point * 8.0, seed.wrapping_add(1));
        (broad, fine)
    }
}