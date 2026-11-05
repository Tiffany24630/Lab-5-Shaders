use crate::color::Color;
use crate::diagnostics::{DiagnosticObject, DiagnosticSample, DiagnosticsHandle, LavaDiagnostic};
use crate::math::{Mat4, NormalTransform, Vec2, Vec3};
use crate::profile::{DetailedProfile, ProceduralDetailProfile, RasterDetailProfile, ShaderDetailProfile};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FragmentInput {
    pub world_position: Vec3,
    pub local_position: Vec3,
    pub normal: Vec3,
    pub uv: Option<Vec2>,
    pub depth: f32,
    pub screen_position: Vec2,
    pub vertex_color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CachedCameraMatrices {
    pub view: Mat4,
    pub projection: Mat4,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShaderUniforms {
    pub time: f32,
    pub camera_position: Vec3,
    pub light_position: Vec3,
    pub light_color: Color,
    pub light_intensity: f32,
    pub light_attenuation: f32,
    pub ambient_light: f32,
    pub shadow_scene: Option<Arc<crate::shadows::ShadowScene>>,
    pub shadow_receiver: u32,
    pub diagnostics: Option<DiagnosticsHandle>,
    pub diagnostic_object: Option<DiagnosticObject>,
    pub vesper_emission_curve: Option<(f32, f32)>,
    pub detail_profile: Option<DetailedProfile>,
    pub raster_detail_profile: Option<RasterDetailProfile>,
    pub shader_detail_profile: Option<ShaderDetailProfile>,
    pub procedural_detail_profile: Option<ProceduralDetailProfile>,
    pub solene_normalized_light: bool,
    pub framebuffer_fused_write: bool,
    pub raster_incremental_coverage: bool,
    pub cached_camera_matrices: Option<CachedCameraMatrices>,
    pub cached_normal_transform: Option<NormalTransform>,
    pub parameters: [f32; 8],
}

impl Default for ShaderUniforms {
    fn default() -> Self {
        Self {
            time: 0.0,
            camera_position: Vec3::ZERO,
            light_position: Vec3::new(0.0, 0.0, 1.0),
            light_color: Color::WHITE,
            light_intensity: 1.8,
            light_attenuation: 0.04,
            ambient_light: 0.10,
            shadow_scene: None,
            shadow_receiver: u32::MAX,
            diagnostics: None,
            diagnostic_object: None,
            vesper_emission_curve: None,
            detail_profile: None,
            raster_detail_profile: None,
            shader_detail_profile: None,
            procedural_detail_profile: None,
            solene_normalized_light: false,
            framebuffer_fused_write: false,
            raster_incremental_coverage: false,
            cached_camera_matrices: None,
            cached_normal_transform: None,
            parameters: [0.0; 8],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    pub name: &'static str,
    pub position: Vec3,
    pub color: Color,
    pub intensity: f32,
    pub attenuation: f32,
}

impl PointLight {
    pub fn from_uniforms(uniforms: &ShaderUniforms) -> Self {
        Self {
            name: "Solaris",
            position: uniforms.light_position,
            color: uniforms.light_color,
            intensity: uniforms.light_intensity,
            attenuation: uniforms.light_attenuation,
        }
    }
}

pub fn apply_point_light(
    base: Color,
    emission: Color,
    normal: Vec3,
    world_position: Vec3,
    uniforms: &ShaderUniforms,
    material_intensity: f32,
) -> Color {
    apply_point_light_internal(base, emission, normal, world_position, uniforms, material_intensity, false, None)
}

pub(crate) fn apply_point_light_with_normalized_normal(
    base: Color,
    emission: Color,
    normal: Vec3,
    world_position: Vec3,
    uniforms: &ShaderUniforms,
    material_intensity: f32,
) -> Color {
    apply_point_light_internal(base, emission, normal, world_position, uniforms, material_intensity, true, None)
}

pub fn apply_point_light_with_lava(
    base: Color,
    emission: Color,
    normal: Vec3,
    world_position: Vec3,
    uniforms: &ShaderUniforms,
    material_intensity: f32,
    lava: LavaDiagnostic,
) -> Color {
    apply_point_light_internal(base, emission, normal, world_position, uniforms, material_intensity, false, Some(lava))
}

fn apply_point_light_internal(
    base: Color,
    emission: Color,
    normal: Vec3,
    world_position: Vec3,
    uniforms: &ShaderUniforms,
    material_intensity: f32,
    normal_is_normalized: bool,
    lava: Option<LavaDiagnostic>,
) -> Color {
    let light = PointLight::from_uniforms(uniforms);
    let offset = light.position - world_position;
    let distance_squared = offset.length_squared();
    let light_direction = offset.normalized();
    let normal = if normal_is_normalized { normal } else { normal.normalized() };
    let diffuse = normal.dot(light_direction).clamp(0.0, 1.0);
    let intensity = finite_or(light.intensity, 0.0).max(0.0);
    let attenuation = finite_or(light.attenuation, 0.04).max(0.0);
    let distance_factor = if distance_squared.is_finite() {
        1.0 / (1.0 + attenuation * distance_squared.max(0.0))
    } else {
        0.0
    };
    let material_intensity = finite_or(material_intensity, 1.0).max(0.0);
    let ambient = finite_or(uniforms.ambient_light, 0.10).clamp(0.0, 1.0);
    let visibility = uniforms
        .shadow_scene
        .as_ref()
        .map(|scene| {
            if scene.is_occluded(
                world_position,
                light.position,
                uniforms.shadow_receiver,
                uniforms.detail_profile.as_ref(),
            ) {
                0.0
            } else {
                1.0
            }
        })
        .unwrap_or(1.0);
    let diffuse_factor = diffuse * intensity * distance_factor * material_intensity * visibility;
    let light_color = light.color;
    let Some(handle) = uniforms.diagnostics.as_ref() else {
        return Color::new(
            base.r * (ambient + diffuse_factor * light_color.r) + emission.r,
            base.g * (ambient + diffuse_factor * light_color.g) + emission.g,
            base.b * (ambient + diffuse_factor * light_color.b) + emission.b,
        );
    };
    let Some(object) = uniforms.diagnostic_object else {
        return Color::new(
            base.r * (ambient + diffuse_factor * light_color.r) + emission.r,
            base.g * (ambient + diffuse_factor * light_color.g) + emission.g,
            base.b * (ambient + diffuse_factor * light_color.b) + emission.b,
        );
    };
    let ambient_component = Color::new_const(
        base.r * ambient,
        base.g * ambient,
        base.b * ambient,
    );
    let direct_component = Color::new_const(
        base.r * diffuse_factor * light_color.r,
        base.g * diffuse_factor * light_color.g,
        base.b * diffuse_factor * light_color.b,
    );
    let material_pre_emission = Color::new_const(
        base.r * (ambient + diffuse_factor * light_color.r),
        base.g * (ambient + diffuse_factor * light_color.g),
        base.b * (ambient + diffuse_factor * light_color.b),
    );
    let combined_pre_clamp = Color::new_const(
        material_pre_emission.r + emission.r,
        material_pre_emission.g + emission.g,
        material_pre_emission.b + emission.b,
    );
    let display_color = Color::new(
        combined_pre_clamp.r,
        combined_pre_clamp.g,
        combined_pre_clamp.b,
    );
    handle.record(object, DiagnosticSample {
        albedo: base,
        emission,
        ambient_component,
        direct_component,
        material_pre_emission,
        combined_pre_clamp,
        display_color,
        shadow_visibility: Some(visibility),
        material_intensity,
        lava,
    });
    display_color
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

pub trait FragmentShader {
    fn shade(&self, input: &FragmentInput, uniforms: &ShaderUniforms) -> Color;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VertexColorShader;

impl FragmentShader for VertexColorShader {
    fn shade(&self, input: &FragmentInput, _uniforms: &ShaderUniforms) -> Color {
        input.vertex_color
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NormalDiagnosticShader;

impl FragmentShader for NormalDiagnosticShader {
    fn shade(&self, input: &FragmentInput, _uniforms: &ShaderUniforms) -> Color {
        let normal = input.normal.normalized();
        Color::new(
            0.5 + normal.x * 0.5,
            0.5 + normal.y * 0.5,
            0.5 + normal.z * 0.5,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FragmentInput, FragmentShader, NormalDiagnosticShader, ShaderUniforms, VertexColorShader,
    };
    use crate::color::Color;
    use crate::math::{Vec2, Vec3};

    fn input() -> FragmentInput {
        FragmentInput {
            world_position: Vec3::new(1.0, 2.0, 3.0),
            local_position: Vec3::new(0.0, 1.0, 0.0),
            normal: Vec3::Z,
            uv: Some(Vec2::new(0.25, 0.75)),
            depth: 0.5,
            screen_position: Vec2::new(10.5, 20.5),
            vertex_color: Color::new(0.2, 0.4, 0.6),
        }
    }

    #[test]
    fn shaders_receive_input_and_uniforms() {
        let input = input();
        let uniforms = ShaderUniforms {
            time: 3.0,
            ..ShaderUniforms::default()
        };
        assert_eq!(
            VertexColorShader.shade(&input, &uniforms),
            input.vertex_color
        );
        assert_eq!(
            NormalDiagnosticShader.shade(&input, &uniforms),
            Color::new(0.5, 0.5, 1.0)
        );
    }

    #[test]
    fn diagnostic_shader_handles_non_unit_normals() {
        let mut input = input();
        input.normal = Vec3::new(0.0, 0.0, 4.0);
        assert_eq!(
            NormalDiagnosticShader.shade(&input, &ShaderUniforms::default()),
            Color::new(0.5, 0.5, 1.0)
        );
    }
}