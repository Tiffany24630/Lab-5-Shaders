use crate::color::Color;
use crate::math::{Vec2, Vec3};

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
pub struct ShaderUniforms {
    pub time: f32,
    pub camera_position: Vec3,
    pub light_position: Vec3,
    pub parameters: [f32; 8],
}

impl Default for ShaderUniforms {
    fn default() -> Self {
        Self {
            time: 0.0,
            camera_position: Vec3::ZERO,
            light_position: Vec3::new(0.0, 0.0, 1.0),
            parameters: [0.0; 8],
        }
    }
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