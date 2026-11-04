use crate::color::Color;
use crate::math::Vec3;
use crate::procedural::{fbm, noise_3d, smoothstep};
use crate::shader::{FragmentInput, FragmentShader, ShaderUniforms};

#[derive(Clone, Copy, Debug)]
pub struct SeleneShader {
    pub crater_density: f32,
    pub small_crater_scale: f32,
    pub medium_crater_scale: f32,
    pub large_crater_scale: f32,
    pub apparent_depth: f32,
    pub rim_width: f32,
    pub contour_irregularity: f32,
    pub erosion: f32,
    pub mineral_variation: f32,
    pub contrast: f32,
    pub light_intensity: f32,
    pub detail: f32,
    pub seed: u32,
}

impl Default for SeleneShader {
    fn default() -> Self {
        Self {
            crater_density: 1.0,
            small_crater_scale: 8.0,
            medium_crater_scale: 4.8,
            large_crater_scale: 2.2,
            apparent_depth: 1.0,
            rim_width: 1.0,
            contour_irregularity: 0.16,
            erosion: 1.0,
            mineral_variation: 1.0,
            contrast: 1.0,
            light_intensity: 1.0,
            detail: 1.0,
            seed: 0x5E_1E_0001,
        }
    }
}

impl FragmentShader for SeleneShader {
    fn shade(&self, input: &FragmentInput, uniforms: &ShaderUniforms) -> Color {
        let surface = input.local_position.normalized();
        if surface == Vec3::ZERO || !surface.is_finite() {
            return Color::new(0.045, 0.055, 0.075);
        }

        let detail = finite_or(self.detail, 1.0).max(0.0);
        let terrain_point = surface * (2.2 + detail * 0.35);
        let ancient = fbm(terrain_point * 0.72, self.seed, 5);
        let mineral_noise = fbm(
            terrain_point * (2.1 + detail * 0.3) + Vec3::new(2.0, -3.0, 1.0),
            self.seed.wrapping_add(17),
            4,
        );
        let fine = noise_3d(
            terrain_point * (7.0 + detail * 2.0) + Vec3::new(-1.0, 4.0, 3.0),
            self.seed.wrapping_add(31),
        );
        let terrain = ((ancient * 0.62 + mineral_noise * 0.28 + fine * 0.10)
            * finite_or(self.contrast, 1.0).max(0.0))
        .clamp(0.0, 1.0);
        let mineral =
            (mineral_noise * 0.7 + fine * 0.3) * finite_or(self.mineral_variation, 1.0).max(0.0);
        let mut color = lunar_palette(terrain, mineral);

        let (rim, interior, transition, material) = crater_layers(
            surface,
            finite_or(self.crater_density, 1.0).clamp(0.0, 2.0),
            finite_or(self.small_crater_scale, 8.0).max(0.1),
            finite_or(self.medium_crater_scale, 4.8).max(0.1),
            finite_or(self.large_crater_scale, 2.2).max(0.1),
            finite_or(self.rim_width, 1.0).clamp(0.1, 2.0),
            finite_or(self.contour_irregularity, 0.16).clamp(0.0, 0.6),
            self.seed.wrapping_add(101),
        );
        let depth = finite_or(self.apparent_depth, 1.0).max(0.0);
        let dark_interior = Color::new_const(0.035, 0.050, 0.075);
        let raised_rim = Color::new_const(0.62, 0.67, 0.73);
        let ejecta = Color::new_const(0.42, 0.46, 0.51);
        color = lerp_color(
            color,
            dark_interior,
            (interior * depth * 0.72).clamp(0.0, 0.82),
        );
        color = lerp_color(
            color,
            ejecta,
            (transition * material * 0.32).clamp(0.0, 0.42),
        );
        color = lerp_color(color, raised_rim, (rim * depth * 0.58).clamp(0.0, 0.70));

        let normal = input.normal.normalized();
        let light_direction = (uniforms.light_position - input.world_position).normalized();
        let view_direction = (uniforms.camera_position - input.world_position).normalized();
        let diffuse = normal.dot(light_direction).clamp(0.0, 1.0);
        let rim_light = (1.0 - normal.dot(view_direction).clamp(0.0, 1.0)).powf(1.8);
        let crater_light = (0.78 + 0.22 * rim - 0.18 * interior).clamp(0.45, 1.15);
        let illumination = (0.48 + 0.40 * diffuse + 0.12 * rim_light)
            * crater_light
            * finite_or(self.light_intensity, 1.0).max(0.0);
        scale_color(color, illumination)
    }
}

fn lunar_palette(value: f32, mineral: f32) -> Color {
    let dark = Color::new_const(0.075, 0.085, 0.105);
    let mineral_gray = Color::new_const(0.24, 0.26, 0.29);
    let silver = Color::new_const(0.52, 0.55, 0.59);
    let blue_gray = Color::new_const(0.38, 0.45, 0.53);
    let t = value.clamp(0.0, 1.0);
    let a = lerp_color(dark, mineral_gray, smoothstep(0.12, 0.40, t));
    let b = lerp_color(a, silver, smoothstep(0.38, 0.70, t));
    let c = lerp_color(b, blue_gray, smoothstep(0.68, 0.92, t));
    lerp_color(
        c,
        Color::new_const(0.66, 0.69, 0.74),
        mineral.clamp(0.0, 1.0) * 0.16,
    )
}

fn crater_layers(
    surface: Vec3,
    density: f32,
    small_scale: f32,
    medium_scale: f32,
    large_scale: f32,
    rim_width: f32,
    irregularity: f32,
    seed: u32,
) -> (f32, f32, f32, f32) {
    let sites = [
        (Vec3::new(0.30, 0.18, 0.94), 0.40, large_scale),
        (Vec3::new(-0.48, 0.25, 0.84), 0.26, medium_scale),
        (Vec3::new(0.65, -0.18, 0.74), 0.18, medium_scale),
        (Vec3::new(-0.15, -0.55, 0.82), 0.12, small_scale),
        (Vec3::new(0.18, 0.65, 0.74), 0.09, small_scale),
    ];
    let mut rim: f32 = 0.0;
    let mut interior: f32 = 0.0;
    let mut transition: f32 = 0.0;
    let mut material: f32 = 0.0;
    for (index, (center, radius, scale)) in sites.into_iter().enumerate() {
        let center = center.normalized();
        let deformation = (noise_3d(
            surface * scale + Vec3::new(index as f32, -2.0, 3.0),
            seed.wrapping_add(index as u32),
        ) - 0.5)
            * irregularity;
        let normalized_distance = (surface - center).length() / (radius * (1.0 + deformation));
        let weight = density.clamp(0.0, 2.0) * (1.0 / (index as f32 + 1.0).sqrt());
        let inner = (1.0 - smoothstep(0.30, 0.88, normalized_distance)) * weight;
        let ring = smoothstep(0.62, 0.90, normalized_distance)
            * (1.0 - smoothstep(0.90, 1.22 + rim_width * 0.12, normalized_distance))
            * weight;
        let band = smoothstep(0.24, 0.70, normalized_distance)
            * (1.0 - smoothstep(0.70, 1.16, normalized_distance))
            * weight;
        rim = rim.max(ring);
        interior = interior.max(inner);
        transition = transition.max(band);
        material = material
            .max((band * (0.55 + 0.45 * noise_3d(surface * scale, seed + 31))).clamp(0.0, 1.0));
    }
    (
        rim.clamp(0.0, 1.0),
        interior.clamp(0.0, 1.0),
        transition.clamp(0.0, 1.0),
        material,
    )
}

#[cfg(test)]
fn crater_profile(distance: f32) -> (f32, f32, f32) {
    let d = distance.max(0.0);
    let interior = 1.0 - smoothstep(0.30, 0.88, d);
    let rim = smoothstep(0.62, 0.90, d) * (1.0 - smoothstep(0.90, 1.22, d));
    let transition = smoothstep(0.24, 0.70, d) * (1.0 - smoothstep(0.70, 1.16, d));
    (rim, interior, transition)
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
    use super::{SeleneShader, crater_layers, crater_profile, lunar_palette};
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
            screen_position: Vec2::new(22.5, 22.5),
            vertex_color: Color::WHITE,
        }
    }

    #[test]
    fn lunar_palette_has_gray_silver_and_cold_blue_layers() {
        let dark = lunar_palette(0.05, 0.0);
        let silver = lunar_palette(0.72, 0.0);
        let blue = lunar_palette(0.90, 1.0);
        assert!(dark.r < silver.r && dark.g < silver.g);
        assert!(blue.b > dark.b);
        assert!(dark.r < 0.25 && silver.r < 0.8);
    }

    #[test]
    fn crater_profile_distinguishes_interior_rim_and_transition() {
        let center = crater_profile(0.05);
        let edge = crater_profile(0.78);
        let outside = crater_profile(1.5);
        assert!(center.1 > edge.1);
        assert!(edge.0 > outside.0);
        assert!(edge.2 > 0.0 && outside.0 == 0.0);
    }

    #[test]
    fn crater_distribution_is_deterministic_and_has_multiple_scales() {
        let surface = Vec3::new(0.30, 0.18, 0.94).normalized();
        let a = crater_layers(surface, 1.0, 8.0, 4.8, 2.2, 1.0, 0.16, 7);
        let b = crater_layers(
            Vec3::new(-0.48, 0.25, 0.84).normalized(),
            1.0,
            8.0,
            4.8,
            2.2,
            1.0,
            0.16,
            7,
        );
        assert_eq!(a, crater_layers(surface, 1.0, 8.0, 4.8, 2.2, 1.0, 0.16, 7));
        assert!(a.0.is_finite() && a.1.is_finite() && b.0.is_finite());
        assert!(a != b);
    }

    #[test]
    fn shader_responds_to_normals_is_stable_and_finite() {
        let shader = SeleneShader::default();
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