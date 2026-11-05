use crate::color::Color;
use crate::diagnostics::LavaDiagnostic;
use crate::math::Vec3;
use crate::procedural::{fbm, noise_3d, smoothstep};
use crate::shader::{apply_point_light_with_lava, FragmentInput, FragmentShader, ShaderUniforms};

#[derive(Clone, Copy, Debug)]
pub struct VesperShader {
    pub geological_scale: f32,
    pub crack_scale: f32,
    pub crack_density: f32,
    pub crack_width: f32,
    pub lava_intensity: f32,
    pub emission: f32,
    pub rock_contrast: f32,
    pub detail: f32,
    pub seed: u32,
}

impl Default for VesperShader {
    fn default() -> Self {
        Self {
            geological_scale: 2.2,
            crack_scale: 4.7,
            crack_density: 0.58,
            crack_width: 0.18,
            lava_intensity: 1.0,
            emission: 1.15,
            rock_contrast: 1.0,
            detail: 1.0,
            seed: 0x5E5E_7001,
        }
    }
}

impl FragmentShader for VesperShader {
    fn shade(&self, input: &FragmentInput, uniforms: &ShaderUniforms) -> Color {
        let surface = input.local_position.normalized();
        if surface == Vec3::ZERO || !surface.is_finite() {
            return Color::new(0.025, 0.012, 0.045);
        }

        let time = finite_or(uniforms.time, 0.0);
        let geology = finite_or(self.geological_scale, 2.2).max(0.01);
        let detail_strength = finite_or(self.detail, 1.0).max(0.0);
        let point = surface * geology + Vec3::new(time * 0.004, -time * 0.002, time * 0.003);

        let continental = fbm(point * 0.72, self.seed, 5);
        let erosion = fbm(
            point * 2.15 + Vec3::new(4.2, -1.4, 2.7),
            self.seed.wrapping_add(19),
            4,
        );
        let mineral = noise_3d(
            point * (6.5 + detail_strength * 2.0) + Vec3::new(-3.0, 2.0, 5.0),
            self.seed.wrapping_add(37),
        );
        let elevation = ((continental * 0.62 + erosion * 0.30 + mineral * 0.08)
            * finite_or(self.rock_contrast, 1.0).max(0.0))
        .clamp(0.0, 1.0);

        let normal = input.normal.normalized();
        let light_direction = (uniforms.light_position - input.world_position).normalized();
        let orientation = normal.dot(light_direction).clamp(0.0, 1.0);
        let rock_color = rock_palette((elevation * 0.75 + orientation * 0.12).clamp(0.0, 1.0));

        let (edge, core) = fracture_masks(
            point,
            finite_or(self.crack_scale, 4.7).max(0.1),
            finite_or(self.crack_density, 0.58).clamp(0.05, 0.95),
            finite_or(self.crack_width, 0.18).clamp(0.02, 0.45),
            self.seed.wrapping_add(101),
        );
        let thermal_variation = 0.72 + 0.28 * fbm(point * 3.0, self.seed.wrapping_add(211), 3);
        let activity = (core * 0.85 + edge * 0.22) * thermal_variation;
        let lava = lava_palette(core);
        let lava_intensity = finite_or(self.lava_intensity, 1.0).max(0.0);
        let lava_strength = activity * lava_intensity;

        let emitted = scale_color(
            lava,
            finite_or(self.emission, 1.15).max(0.0)
                * lava_strength
                * uniforms.vesper_emission_curve
                    .map(|(threshold, minimum)| {
                        if lava_strength <= threshold {
                            1.0
                        } else {
                            1.0 - (1.0 - minimum) * ((lava_strength - threshold) / (1.0 - threshold))
                        }
                    })
                    .unwrap_or(1.0),
        );
        apply_point_light_with_lava(
            rock_color,
            emitted,
            normal,
            input.world_position,
            uniforms,
            1.0,
            LavaDiagnostic {
                activity,
                lava_intensity,
                lava_strength,
            },
        )
    }
}

fn rock_palette(value: f32) -> Color {
    let deep = Color::new_const(0.025, 0.012, 0.060);
    let main = Color::new_const(0.105, 0.035, 0.170);
    let elevated = Color::new_const(0.300, 0.135, 0.390);
    let mineral = Color::new_const(0.480, 0.300, 0.620);
    let t = value.clamp(0.0, 1.0);
    let a = lerp_color(deep, main, smoothstep(0.18, 0.40, t));
    let b = lerp_color(a, elevated, smoothstep(0.38, 0.66, t));
    lerp_color(b, mineral, smoothstep(0.62, 0.90, t))
}

fn lava_palette(value: f32) -> Color {
    let warm = Color::new_const(0.72, 0.025, 0.180);
    let orange = Color::new_const(1.0, 0.180, 0.025);
    let core = Color::new_const(1.0, 0.760, 0.180);
    lerp_color(
        lerp_color(warm, orange, smoothstep(0.18, 0.60, value)),
        core,
        smoothstep(0.58, 0.96, value),
    )
}

fn fracture_masks(point: Vec3, scale: f32, density: f32, width: f32, seed: u32) -> (f32, f32) {
    let a = noise_3d(point * scale, seed);
    let b = noise_3d(
        point * (scale * 1.37) + Vec3::new(7.1, -4.3, 2.8),
        seed.wrapping_add(1),
    );
    let line_a = (1.0 - (a - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    let line_b = (1.0 - (b - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    let network = (line_a * 0.68 + line_b * 0.32).clamp(0.0, 1.0);
    let threshold = 0.72 + (1.0 - density) * 0.18;
    let edge = smoothstep(threshold - width * 0.35, threshold + width * 0.15, network);
    let core = smoothstep(threshold + width * 0.15, threshold + width * 1.2, network);
    (edge, core)
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::new_const(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}

fn scale_color(color: Color, factor: f32) -> Color {
    Color::new_const(color.r * factor, color.g * factor, color.b * factor)
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::{VesperShader, fracture_masks, lava_palette, rock_palette};
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
    fn rock_and_lava_palettes_have_distinct_layers() {
        let rocks = [rock_palette(0.05), rock_palette(0.50), rock_palette(0.95)];
        assert!(rocks[0].b > rocks[0].r);
        assert!(rocks[2].r > rocks[0].r && rocks[2].g > rocks[0].g);
        let lava_edge = lava_palette(0.20);
        let lava_core = lava_palette(0.98);
        assert!(lava_core.g > lava_edge.g);
        assert!(lava_core.r.is_finite() && lava_core.g.is_finite() && lava_core.b.is_finite());
    }

    #[test]
    fn fractures_have_edges_and_hotter_cores() {
        let points = [
            Vec3::new(0.37, 0.91, -0.42),
            Vec3::new(-0.61, 0.12, 0.83),
            Vec3::new(0.14, -0.77, 0.48),
            Vec3::new(-0.88, -0.21, -0.31),
        ];
        let masks: Vec<_> = points
            .into_iter()
            .map(|point| fracture_masks(point, 4.7, 0.58, 0.18, 7))
            .collect();
        assert!(
            masks
                .iter()
                .all(|(edge, core)| edge >= core && edge.is_finite() && core.is_finite())
        );
        assert!(
            masks
                .windows(2)
                .any(|pair| (pair[0].1 - pair[1].1).abs() > 1e-5)
        );
    }

    #[test]
    fn shader_is_deterministic_and_uses_local_position() {
        let shader = VesperShader::default();
        let uniforms = ShaderUniforms {
            camera_position: Vec3::new(0.0, 0.0, 5.0),
            light_position: Vec3::new(2.0, 3.0, 4.0),
            ..ShaderUniforms::default()
        };
        let a = shader.shade(&input(Vec3::new(0.17, 0.53, 0.82), Vec3::Z), &uniforms);
        let b = shader.shade(&input(Vec3::new(0.17, 0.53, 0.82), Vec3::Z), &uniforms);
        let c = shader.shade(&input(Vec3::new(-0.68, 0.22, 0.41), Vec3::Z), &uniforms);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn shader_responds_to_normal_time_and_keeps_output_valid() {
        let shader = VesperShader::default();
        let normal = shader.shade(&input(Vec3::Z, Vec3::Z), &ShaderUniforms::default());
        let changed = shader.shade(
            &input(Vec3::Z, Vec3::X),
            &ShaderUniforms {
                time: 8.0,
                ..ShaderUniforms::default()
            },
        );
        assert_ne!(normal, changed);
        for color in [normal, changed] {
            assert!(color.r.is_finite() && color.g.is_finite() && color.b.is_finite());
            assert!((0.0..=1.0).contains(&color.r));
            assert!((0.0..=1.0).contains(&color.g));
            assert!((0.0..=1.0).contains(&color.b));
        }
    }
}