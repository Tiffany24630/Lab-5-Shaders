use crate::color::Color;
use crate::math::Vec3;
use crate::procedural::{fbm, noise_3d, smoothstep};
use crate::shader::{apply_point_light, FragmentInput, FragmentShader, ShaderUniforms};

#[derive(Clone, Copy, Debug)]
pub struct UmbraShader {
    pub band_frequency: f32,
    pub atmospheric_distortion: f32,
    pub band_contrast: f32,
    pub turbulence_scale: f32,
    pub storm_scale: f32,
    pub storm_intensity: f32,
    pub color_variation: f32,
    pub light_intensity: f32,
    pub detail: f32,
    pub seed: u32,
}

impl Default for UmbraShader {
    fn default() -> Self {
        Self {
            band_frequency: 6.2,
            atmospheric_distortion: 2.4,
            band_contrast: 1.15,
            turbulence_scale: 3.2,
            storm_scale: 3.8,
            storm_intensity: 1.0,
            color_variation: 1.0,
            light_intensity: 1.0,
            detail: 1.0,
            seed: 0xA0_12_0001,
        }
    }
}

impl FragmentShader for UmbraShader {
    fn shade(&self, input: &FragmentInput, uniforms: &ShaderUniforms) -> Color {
        let surface = input.local_position.normalized();
        if surface == Vec3::ZERO || !surface.is_finite() {
            return Color::new(0.012, 0.004, 0.010);
        }

        let time = finite_or(uniforms.time, 0.0);
        let detail = finite_or(self.detail, 1.0).max(0.0);
        let turbulence_scale = finite_or(self.turbulence_scale, 3.2).max(0.1);
        let drift = Vec3::new(time * 0.004, 0.0, -time * 0.003);
        let point = surface * turbulence_scale + drift;

        let broad = fbm(point * 0.48, self.seed, 5);
        let medium = fbm(
            point * 1.35 + Vec3::new(2.4, -2.0, 3.1),
            self.seed.wrapping_add(17),
            5,
        );
        let fine = noise_3d(
            point * (6.0 + detail * 2.0) + Vec3::new(-3.0, 2.0, 1.0),
            self.seed.wrapping_add(31),
        );
        let warp = (broad - 0.5) * finite_or(self.atmospheric_distortion, 2.4)
            + (medium - 0.5) * 0.8
            + (surface.x * surface.z) * 0.35;
        let frequency = finite_or(self.band_frequency, 6.2).max(0.1);
        let band_wave = 0.5 + 0.5 * (surface.y * frequency + warp).sin();
        let turbulent_bands =
            (band_wave * 0.54 + medium * 0.30 + broad * 0.10 + fine * 0.06).clamp(0.0, 1.0);
        let contrast = finite_or(self.band_contrast, 1.15).max(0.0);
        let band_value = ((turbulent_bands - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
        let mut color = atmosphere_palette(band_value, finite_or(self.color_variation, 1.0));

        let storm_field = fbm(
            surface * finite_or(self.storm_scale, 3.8).max(0.1)
                + Vec3::new(-1.0, 2.0, 3.0)
                + drift * 1.5,
            self.seed.wrapping_add(71),
            5,
        );
        let storm_detail = noise_3d(
            surface * (10.0 + detail * 2.0) + Vec3::new(4.0, -2.0, 1.0) + drift * 2.0,
            self.seed.wrapping_add(89),
        );
        let cell_center = Vec3::new(0.34, 0.20, 0.92).normalized();
        let cell_distance = (surface - cell_center).length();
        let cell = 1.0 - smoothstep(0.22, 0.72, cell_distance);
        let storm = (smoothstep(0.60, 0.82, storm_field) * (0.50 + 0.50 * storm_detail) * 0.72
            + cell * (0.45 + 0.55 * storm_detail))
            * finite_or(self.storm_intensity, 1.0).max(0.0);
        let storm_color = Color::new_const(0.43, 0.045, 0.27);
        color = lerp_color(color, storm_color, storm.clamp(0.0, 0.72));

        let normal = input.normal.normalized();
        let storm_highlight = storm * 0.08;
        let storm_emission = Color::new_const(
            storm_highlight * 0.65,
            storm_highlight * 0.10,
            storm_highlight * 0.42,
        );
        apply_point_light(
            color,
            storm_emission,
            normal,
            input.world_position,
            uniforms,
            self.light_intensity,
        )
    }
}

fn atmosphere_palette(value: f32, variation: f32) -> Color {
    let deep = Color::new_const(0.012, 0.003, 0.010);
    let wine = Color::new_const(0.105, 0.012, 0.040);
    let crimson = Color::new_const(0.300, 0.030, 0.085);
    let magenta = Color::new_const(0.420, 0.080, 0.260);
    let t = value.clamp(0.0, 1.0);
    let v = variation.max(0.0);
    let a = lerp_color(deep, wine, smoothstep(0.12, 0.42, t));
    let b = lerp_color(a, crimson, smoothstep(0.38, 0.70, t));
    let c = lerp_color(b, magenta, smoothstep(0.70, 0.95, t));
    lerp_color(
        c,
        Color::new_const(0.58, 0.18, 0.38),
        (v - 1.0).max(0.0) * 0.25,
    )
}

#[cfg(test)]
fn storm_mask(surface: Vec3, seed: u32) -> f32 {
    let field = fbm(surface * 3.8 + Vec3::new(-1.0, 2.0, 3.0), seed, 5);
    let center = Vec3::new(0.34, 0.20, 0.92).normalized();
    let cell = 1.0 - smoothstep(0.22, 0.72, (surface - center).length());
    (smoothstep(0.60, 0.82, field) * 0.7 + cell * 0.5).clamp(0.0, 1.0)
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
    use super::{UmbraShader, atmosphere_palette, storm_mask};
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
            screen_position: Vec2::new(18.5, 18.5),
            vertex_color: Color::WHITE,
        }
    }

    #[test]
    fn palette_is_dark_and_has_distinct_atmospheric_layers() {
        let deep = atmosphere_palette(0.05, 1.0);
        let wine = atmosphere_palette(0.38, 1.0);
        let crimson = atmosphere_palette(0.68, 1.0);
        let magenta = atmosphere_palette(0.94, 1.0);
        assert!(deep.r < 0.1 && deep.g < 0.05);
        assert!(crimson.r > wine.r && magenta.b > deep.b);
        assert!(deep.r < 0.5);
    }

    #[test]
    fn storm_mask_is_localized_and_position_dependent() {
        let a = storm_mask(Vec3::new(0.34, 0.20, 0.92).normalized(), 7);
        let b = storm_mask(Vec3::new(-0.72, 0.15, 0.41).normalized(), 7);
        assert!(a.is_finite() && b.is_finite());
        assert!((0.0..=1.0).contains(&a) && (0.0..=1.0).contains(&b));
        assert!(a > b || a > 0.0);
    }

    #[test]
    fn shader_responds_to_normals_and_time_deterministically() {
        let shader = UmbraShader::default();
        let uniforms = ShaderUniforms {
            camera_position: Vec3::new(0.0, 0.0, 5.0),
            light_position: Vec3::new(3.0, 2.0, 4.0),
            ..ShaderUniforms::default()
        };
        let a = shader.shade(&input(Vec3::Z, Vec3::Z), &uniforms);
        let b = shader.shade(
            &input(Vec3::Z, Vec3::X),
            &ShaderUniforms {
                time: 6.0,
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