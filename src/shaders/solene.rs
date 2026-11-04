use crate::color::Color;
use crate::math::Vec3;
use crate::procedural::{fbm, noise_3d, smoothstep};
use crate::shader::{apply_point_light, FragmentInput, FragmentShader, ShaderUniforms};

#[derive(Clone, Copy, Debug)]
pub struct SoleneShader {
    pub dune_scale: f32,
    pub wave_frequency: f32,
    pub crest_amplitude: f32,
    pub pattern_distortion: f32,
    pub plateau_scale: f32,
    pub mineral_density: f32,
    pub geological_contrast: f32,
    pub light_intensity: f32,
    pub detail: f32,
    pub seed: u32,
}

impl Default for SoleneShader {
    fn default() -> Self {
        Self {
            dune_scale: 2.6,
            wave_frequency: 8.0,
            crest_amplitude: 1.0,
            pattern_distortion: 1.7,
            plateau_scale: 2.1,
            mineral_density: 0.64,
            geological_contrast: 1.0,
            light_intensity: 1.0,
            detail: 1.0,
            seed: 0x50_1E_0001,
        }
    }
}

impl FragmentShader for SoleneShader {
    fn shade(&self, input: &FragmentInput, uniforms: &ShaderUniforms) -> Color {
        let surface = input.local_position.normalized();
        if surface == Vec3::ZERO || !surface.is_finite() {
            return Color::new(0.055, 0.025, 0.012);
        }

        let scale = finite_or(self.dune_scale, 2.6).max(0.01);
        let detail = finite_or(self.detail, 1.0).max(0.0);
        let point = surface * scale;
        let broad = fbm(point * 0.56, self.seed, 5);
        let distortion_noise = fbm(
            point * 1.25 + Vec3::new(2.0, -1.0, 3.0),
            self.seed.wrapping_add(17),
            4,
        );
        let fine = noise_3d(
            point * (7.0 + detail * 2.0) + Vec3::new(-3.0, 4.0, 1.0),
            self.seed.wrapping_add(31),
        );
        let distortion = (distortion_noise - 0.5) * finite_or(self.pattern_distortion, 1.7)
            + (broad - 0.5) * 0.65;
        let frequency = finite_or(self.wave_frequency, 8.0).max(0.1);
        let wave_a = 0.5 + 0.5 * (surface.x * frequency + surface.z * 2.2 + distortion).sin();
        let wave_b = 0.5
            + 0.5
                * ((surface.z - surface.x * 0.35) * frequency * 0.73
                    + distortion * 1.4
                    + distortion_noise)
                    .sin();
        let dune_value = ((wave_a * 0.58 + wave_b * 0.30 + fine * 0.12)
            * finite_or(self.crest_amplitude, 1.0).max(0.0))
        .clamp(0.0, 1.0);

        let plateau_field = fbm(
            surface * finite_or(self.plateau_scale, 2.1).max(0.1) + Vec3::new(-2.0, 1.0, 4.0),
            self.seed.wrapping_add(71),
            5,
        );
        let plateau = smoothstep(0.56, 0.78, plateau_field) * (0.68 + 0.32 * broad);
        let erosion = fbm(
            point * 2.7 + Vec3::new(4.0, -2.0, 1.0),
            self.seed.wrapping_add(89),
            3,
        );
        let geological = ((dune_value * 0.54 + plateau * 0.34 + erosion * 0.12)
            * finite_or(self.geological_contrast, 1.0).max(0.0))
        .clamp(0.0, 1.0);
        let mut color = desert_palette(geological, plateau);

        let minerals = smoothstep(
            finite_or(self.mineral_density, 0.64).clamp(0.30, 0.92),
            0.94,
            fbm(
                point * 3.2 + Vec3::new(1.0, 3.0, -2.0),
                self.seed.wrapping_add(113),
                4,
            ),
        );
        let mineral_color = Color::new_const(0.48, 0.16, 0.075);
        color = lerp_color(color, mineral_color, minerals * (0.28 + 0.20 * plateau));

        let normal = input.normal.normalized();
        let dune_light = 0.88 + 0.12 * dune_value;
        apply_point_light(
            scale_color(color, dune_light),
            Color::BLACK,
            normal,
            input.world_position,
            uniforms,
            self.light_intensity,
        )
    }
}

fn desert_palette(value: f32, plateau: f32) -> Color {
    let dark = Color::new_const(0.055, 0.025, 0.012);
    let mineral = Color::new_const(0.18, 0.075, 0.030);
    let ochre = Color::new_const(0.48, 0.245, 0.075);
    let sand = Color::new_const(0.76, 0.56, 0.255);
    let terracotta = Color::new_const(0.55, 0.16, 0.065);
    let t = value.clamp(0.0, 1.0);
    let a = lerp_color(dark, mineral, smoothstep(0.10, 0.36, t));
    let b = lerp_color(a, ochre, smoothstep(0.32, 0.60, t));
    let c = lerp_color(b, sand, smoothstep(0.56, 0.86, t));
    lerp_color(c, terracotta, plateau * 0.22)
}

#[cfg(test)]
fn dune_pattern(point: Vec3, frequency: f32, distortion: f32, seed: u32) -> f32 {
    let warp = fbm(point * 1.2, seed, 4);
    let value = 0.5 + 0.5 * (point.x * frequency + point.z * 2.0 + warp * distortion).sin();
    value.clamp(0.0, 1.0)
}

#[cfg(test)]
fn plateau_mask(surface: Vec3, scale: f32, seed: u32) -> f32 {
    smoothstep(0.56, 0.78, fbm(surface * scale, seed, 5))
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
    use super::{SoleneShader, desert_palette, dune_pattern, plateau_mask};
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
            screen_position: Vec2::new(20.5, 20.5),
            vertex_color: Color::WHITE,
        }
    }

    #[test]
    fn desert_palette_has_dark_mineral_ochre_sand_and_terracotta_layers() {
        let samples = [
            desert_palette(0.05, 0.0),
            desert_palette(0.32, 0.0),
            desert_palette(0.62, 0.0),
            desert_palette(0.90, 1.0),
        ];
        assert!(samples[0].r < samples[2].r);
        assert!(samples[1] != samples[2]);
        assert!(samples[3].r.is_finite() && samples[3].g.is_finite());
    }

    #[test]
    fn dunes_and_plateaus_are_spatially_varying() {
        let a = dune_pattern(Vec3::new(0.12, 0.53, 0.81), 8.0, 1.7, 7);
        let b = dune_pattern(Vec3::new(-0.72, 0.17, 0.42), 8.0, 1.7, 7);
        let p1 = plateau_mask(Vec3::new(0.12, 0.53, 0.81), 2.1, 9);
        let p2 = plateau_mask(Vec3::new(-0.72, 0.17, 0.42), 2.1, 9);
        assert!(a.is_finite() && b.is_finite() && p1.is_finite() && p2.is_finite());
        assert!((0.0..=1.0).contains(&a) && (0.0..=1.0).contains(&p1));
        assert!((a - b).abs() > 1e-5 || (p1 - p2).abs() > 1e-5);
    }

    #[test]
    fn shader_responds_to_normals_is_deterministic_and_finite() {
        let shader = SoleneShader::default();
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