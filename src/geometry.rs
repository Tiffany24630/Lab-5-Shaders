use crate::color::Color;
use crate::math::{Mat4, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex3D { pub position: Vec3, pub color: Color }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle3D { pub vertices: [Vertex3D; 3] }

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh { triangles: Vec<Triangle3D> }

impl Mesh {
    pub fn new(triangles: Vec<Triangle3D>) -> Result<Self, String> {
        let mesh = Self { triangles };
        mesh.validate()?;
        Ok(mesh)
    }
    pub fn triangles(&self) -> &[Triangle3D] { &self.triangles }
    pub fn validate(&self) -> Result<(), String> {
        if self.triangles.iter().any(|triangle| triangle.vertices.iter().any(|v| !v.position.is_finite() || ![v.color.r, v.color.g, v.color.b].iter().all(|c| c.is_finite()))) { Err("Mesh contains non-finite vertex data".into()) } else { Ok(()) }
    }
    pub fn transformed(&self, matrix: Mat4) -> Result<Self, String> {
        Self::new(self.triangles.iter().map(|triangle| Triangle3D { vertices: triangle.vertices.map(|vertex| Vertex3D { position: matrix.transform_point(vertex.position), color: vertex.color }) }).collect())
    }
}

fn v(position: Vec3, color: Color) -> Vertex3D { Vertex3D { position, color } }
fn tri(a: Vertex3D, b: Vertex3D, c: Vertex3D) -> Triangle3D { Triangle3D { vertices: [a, b, c] } }

pub fn cube() -> Mesh {
    let p = [
        Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, -1.0), Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0), Vec3::new(1.0, -1.0, 1.0), Vec3::new(1.0, 1.0, 1.0), Vec3::new(-1.0, 1.0, 1.0),
    ];
    let colors = [Color::new(0.9, 0.2, 0.2), Color::new(0.2, 0.9, 0.3), Color::new(0.2, 0.4, 1.0), Color::new(0.95, 0.8, 0.1), Color::new(0.8, 0.2, 0.9), Color::new(0.1, 0.9, 0.9)];
    Mesh::new(vec![
        tri(v(p[0], colors[0]), v(p[1], colors[0]), v(p[2], colors[0])), tri(v(p[0], colors[0]), v(p[2], colors[0]), v(p[3], colors[0])),
        tri(v(p[4], colors[1]), v(p[6], colors[1]), v(p[5], colors[1])), tri(v(p[4], colors[1]), v(p[7], colors[1]), v(p[6], colors[1])),
        tri(v(p[0], colors[2]), v(p[4], colors[2]), v(p[5], colors[2])), tri(v(p[0], colors[2]), v(p[5], colors[2]), v(p[1], colors[2])),
        tri(v(p[3], colors[3]), v(p[2], colors[3]), v(p[6], colors[3])), tri(v(p[3], colors[3]), v(p[6], colors[3]), v(p[7], colors[3])),
        tri(v(p[0], colors[4]), v(p[3], colors[4]), v(p[7], colors[4])), tri(v(p[0], colors[4]), v(p[7], colors[4]), v(p[4], colors[4])),
        tri(v(p[1], colors[5]), v(p[5], colors[5]), v(p[6], colors[5])), tri(v(p[1], colors[5]), v(p[6], colors[5]), v(p[2], colors[5])),
    ]).expect("cube primitive is valid")
}

pub fn pyramid() -> Mesh {
    let top = v(Vec3::new(0.0, 1.2, 0.0), Color::new(1.0, 0.8, 0.1));
    let a = v(Vec3::new(-1.0, -1.0, -1.0), Color::new(1.0, 0.1, 0.1));
    let b = v(Vec3::new(1.0, -1.0, -1.0), Color::new(0.1, 1.0, 0.2));
    let c = v(Vec3::new(1.0, -1.0, 1.0), Color::new(0.1, 0.4, 1.0));
    let d = v(Vec3::new(-1.0, -1.0, 1.0), Color::new(0.8, 0.1, 1.0));
    Mesh::new(vec![tri(top, a, b), tri(top, b, c), tri(top, c, d), tri(top, d, a), tri(a, d, c), tri(a, c, b)]).expect("pyramid primitive is valid")
}

pub fn plane() -> Mesh {
    let a = v(Vec3::new(-1.0, 0.0, -1.0), Color::new(0.15, 0.3, 0.2));
    let b = v(Vec3::new(1.0, 0.0, -1.0), Color::new(0.2, 0.5, 0.25));
    let c = v(Vec3::new(1.0, 0.0, 1.0), Color::new(0.35, 0.55, 0.2));
    let d = v(Vec3::new(-1.0, 0.0, 1.0), Color::new(0.1, 0.35, 0.2));
    Mesh::new(vec![tri(a, b, c), tri(a, c, d)]).expect("plane primitive is valid")
}

#[cfg(test)]
mod tests {
    use super::{cube, plane, pyramid};
    use crate::math::{Mat4, Vec3};
    #[test] fn primitives_have_expected_triangle_counts() { assert_eq!(cube().triangles().len(), 12); assert_eq!(pyramid().triangles().len(), 6); assert_eq!(plane().triangles().len(), 2); }
    #[test] fn primitives_are_valid_and_transformable() { let mesh = cube(); assert!(mesh.validate().is_ok()); let moved = mesh.transformed(Mat4::translation(Vec3::Z)).unwrap(); assert_eq!(moved.triangles()[0].vertices[0].position.z, mesh.triangles()[0].vertices[0].position.z + 1.0); }
}