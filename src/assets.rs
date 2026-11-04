use std::fmt;
use std::fs;
use std::path::Path;

use crate::color::Color;
use crate::geometry::{Mesh, Triangle3D, Vertex3D};
use crate::math::{Vec2, Vec3};

#[derive(Clone, Debug, PartialEq)]
pub struct ObjAsset {
    pub mesh: Mesh,
    pub positions: Vec<Vec3>,
    pub texcoords: Vec<Vec2>,
    pub normals: Vec<Vec3>,
    pub object_name: Option<String>,
    pub groups: Vec<String>,
    pub source: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjError {
    pub source: String,
    pub line: Option<usize>,
    pub message: String,
}

impl ObjError {
    fn line(source: &str, line: usize, message: impl Into<String>) -> Self {
        Self {
            source: source.to_string(),
            line: Some(line),
            message: message.into(),
        }
    }
    fn file(source: &str, message: impl Into<String>) -> Self {
        Self {
            source: source.to_string(),
            line: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for ObjError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(formatter, "{}:{}: {}", self.source, line, self.message),
            None => write!(formatter, "{}: {}", self.source, self.message),
        }
    }
}

impl std::error::Error for ObjError {}

#[derive(Clone, Copy, Debug)]
struct FaceCorner {
    position: usize,
    texcoord: Option<usize>,
    normal: Option<usize>,
}

pub fn load_obj<P: AsRef<Path>>(path: P) -> Result<ObjAsset, ObjError> {
    let path_string = path.as_ref().display().to_string();
    let text = fs::read_to_string(path.as_ref())
        .map_err(|error| ObjError::file(&path_string, format!("cannot read file: {error}")))?;
    parse_obj(&path_string, &text)
}

pub fn parse_obj(source: &str, text: &str) -> Result<ObjAsset, ObjError> {
    let mut positions = Vec::new();
    let mut texcoords = Vec::new();
    let mut normals = Vec::new();
    let mut triangles = Vec::new();
    let mut object_name = None;
    let mut groups = Vec::new();
    let mut warnings = Vec::new();

    for (line_index, raw_line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line
            .split_once('#')
            .map_or(raw_line, |(content, _)| content)
            .trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let keyword = fields.next().unwrap_or_default();
        let values: Vec<&str> = fields.collect();
        match keyword {
            "v" => positions.push(parse_vec3(source, line_number, "position", &values)?),
            "vt" => {
                if values.len() < 2 {
                    return Err(ObjError::line(
                        source,
                        line_number,
                        "texture coordinate requires u and v",
                    ));
                }
                texcoords.push(Vec2::new(
                    parse_f32(source, line_number, values[0], "u")?,
                    parse_f32(source, line_number, values[1], "v")?,
                ));
            }
            "vn" => {
                let normal = parse_vec3(source, line_number, "normal", &values)?;
                normals.push(normal);
            }
            "f" => {
                if values.len() < 3 {
                    return Err(ObjError::line(
                        source,
                        line_number,
                        "face requires at least three vertices",
                    ));
                }
                let corners: Vec<FaceCorner> = values
                    .iter()
                    .map(|value| {
                        parse_face_corner(
                            source,
                            line_number,
                            value,
                            positions.len(),
                            texcoords.len(),
                            normals.len(),
                        )
                    })
                    .collect::<Result<_, _>>()?;
                let points: Vec<Vec3> = corners
                    .iter()
                    .map(|corner| positions[corner.position])
                    .collect();
                let face_normal = polygon_normal(&points).ok_or_else(|| {
                    ObjError::line(
                        source,
                        line_number,
                        "degenerate polygon has no valid normal",
                    )
                })?;
                for [a, b, c] in triangulate_polygon(&points)
                    .map_err(|message| ObjError::line(source, line_number, message))?
                {
                    let vertices = [a, b, c].map(|index| {
                        let corner = corners[index];
                        let normal = corner
                            .normal
                            .map(|normal_index| normals[normal_index])
                            .filter(|normal| normal.is_finite() && normal.length() > f32::EPSILON)
                            .map(Vec3::normalized)
                            .unwrap_or(face_normal);
                        Vertex3D {
                            position: positions[corner.position],
                            color: Color::new(0.72, 0.72, 0.72),
                            normal,
                            texcoord: corner.texcoord.map(|uv| texcoords[uv]),
                        }
                    });
                    triangles.push(Triangle3D { vertices });
                }
            }
            "o" => {
                if !values.is_empty() {
                    object_name = Some(values.join(" "));
                }
            }
            "g" => {
                for group in values {
                    if !groups.iter().any(|known| known == group) {
                        groups.push(group.to_string());
                    }
                }
            }
            "s" | "mtllib" | "usemtl" => {
                if keyword == "mtllib" || keyword == "usemtl" {
                    warnings.push(format!("line {line_number}: ignored {keyword} reference"));
                }
            }
            _ => {}
        }
    }

    if positions.is_empty() {
        return Err(ObjError::file(source, "OBJ contains no positions"));
    }
    if triangles.is_empty() {
        return Err(ObjError::file(source, "OBJ contains no faces"));
    }
    let mesh = Mesh::new(triangles).map_err(|message| ObjError::file(source, message))?;
    Ok(ObjAsset {
        mesh,
        positions,
        texcoords,
        normals,
        object_name,
        groups,
        source: source.to_string(),
        warnings,
    })
}

fn parse_vec3(source: &str, line: usize, label: &str, values: &[&str]) -> Result<Vec3, ObjError> {
    if values.len() < 3 {
        return Err(ObjError::line(
            source,
            line,
            format!("{label} requires three components"),
        ));
    }
    let result = Vec3::new(
        parse_f32(source, line, values[0], "x")?,
        parse_f32(source, line, values[1], "y")?,
        parse_f32(source, line, values[2], "z")?,
    );
    if !result.is_finite() {
        return Err(ObjError::line(
            source,
            line,
            format!("{label} contains non-finite coordinates"),
        ));
    }
    Ok(result)
}

fn parse_f32(source: &str, line: usize, value: &str, label: &str) -> Result<f32, ObjError> {
    let parsed = value
        .parse::<f32>()
        .map_err(|_| ObjError::line(source, line, format!("invalid {label} value '{value}'")))?;
    if !parsed.is_finite() {
        return Err(ObjError::line(
            source,
            line,
            format!("{label} value is not finite"),
        ));
    }
    Ok(parsed)
}

fn parse_face_corner(
    source: &str,
    line: usize,
    text: &str,
    position_count: usize,
    texcoord_count: usize,
    normal_count: usize,
) -> Result<FaceCorner, ObjError> {
    let parts: Vec<&str> = text.split('/').collect();
    if parts.is_empty()
        || parts.len() > 3
        || parts[0].is_empty()
        || (parts.len() > 1 && parts[1].is_empty() && parts.len() == 2)
        || (parts.len() == 3 && parts[2].is_empty())
    {
        return Err(ObjError::line(
            source,
            line,
            format!("malformed face vertex '{text}'"),
        ));
    }
    let position = resolve_index(source, line, parts[0], position_count, "position")?;
    let texcoord = if parts.len() >= 2 && !parts[1].is_empty() {
        Some(resolve_index(
            source,
            line,
            parts[1],
            texcoord_count,
            "texture coordinate",
        )?)
    } else {
        None
    };
    let normal = if parts.len() == 3 {
        Some(resolve_index(
            source,
            line,
            parts[2],
            normal_count,
            "normal",
        )?)
    } else {
        None
    };
    Ok(FaceCorner {
        position,
        texcoord,
        normal,
    })
}

fn resolve_index(
    source: &str,
    line: usize,
    text: &str,
    count: usize,
    kind: &str,
) -> Result<usize, ObjError> {
    let raw = text
        .parse::<i64>()
        .map_err(|_| ObjError::line(source, line, format!("invalid {kind} index '{text}'")))?;
    if raw == 0 {
        return Err(ObjError::line(
            source,
            line,
            format!("{kind} index cannot be zero"),
        ));
    }
    let resolved = if raw > 0 { raw - 1 } else { count as i64 + raw };
    if resolved < 0 || resolved >= count as i64 {
        return Err(ObjError::line(
            source,
            line,
            format!("{kind} index '{text}' is out of range"),
        ));
    }
    Ok(resolved as usize)
}

fn polygon_normal(points: &[Vec3]) -> Option<Vec3> {
    let mut normal = Vec3::ZERO;
    for index in 0..points.len() {
        let current = points[index];
        let next = points[(index + 1) % points.len()];
        normal = normal
            + Vec3::new(
                (current.y - next.y) * (current.z + next.z),
                (current.z - next.z) * (current.x + next.x),
                (current.x - next.x) * (current.y + next.y),
            );
    }
    let normal = normal.normalized();
    (normal != Vec3::ZERO).then_some(normal)
}

fn triangulate_polygon(points: &[Vec3]) -> Result<Vec<[usize; 3]>, String> {
    if points.len() < 3 {
        return Err("polygon has fewer than three vertices".into());
    }
    if points.len() == 3 {
        return Ok(vec![[0, 1, 2]]);
    }
    let normal = polygon_normal(points).ok_or("polygon is degenerate")?;
    let axis = [normal.x.abs(), normal.y.abs(), normal.z.abs()]
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(index, _)| index)
        .unwrap_or(2);
    let projected: Vec<Vec2> = points
        .iter()
        .map(|point| match axis {
            0 => Vec2::new(point.y, point.z),
            1 => Vec2::new(point.x, point.z),
            _ => Vec2::new(point.x, point.y),
        })
        .collect();
    let area = signed_area(&projected);
    if area.abs() <= f32::EPSILON {
        return Err("polygon projection is degenerate".into());
    }
    let orientation = area.signum();
    let mut remaining: Vec<usize> = (0..points.len()).collect();
    let mut result = Vec::with_capacity(points.len() - 2);
    let mut guard = 0;
    while remaining.len() > 3 {
        let mut found = false;
        for i in 0..remaining.len() {
            let previous = remaining[(i + remaining.len() - 1) % remaining.len()];
            let current = remaining[i];
            let next = remaining[(i + 1) % remaining.len()];
            if cross2(projected[previous], projected[current], projected[next]) * orientation
                <= f32::EPSILON
            {
                continue;
            }
            if remaining.iter().any(|candidate| {
                *candidate != previous
                    && *candidate != current
                    && *candidate != next
                    && point_in_triangle(
                        projected[*candidate],
                        projected[previous],
                        projected[current],
                        projected[next],
                    )
            }) {
                continue;
            }
            result.push([previous, current, next]);
            remaining.remove(i);
            found = true;
            break;
        }
        guard += 1;
        if !found || guard > points.len() * points.len() {
            return Err("polygon is not triangulable by ear clipping".into());
        }
    }
    result.push([remaining[0], remaining[1], remaining[2]]);
    Ok(result)
}

fn signed_area(points: &[Vec2]) -> f32 {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let next = points[(i + 1) % points.len()];
            p.x * next.y - next.x * p.y
        })
        .sum::<f32>()
        * 0.5
}
fn cross2(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
fn point_in_triangle(point: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let ab = cross2(a, b, point);
    let bc = cross2(b, c, point);
    let ca = cross2(c, a, point);
    (ab >= -f32::EPSILON && bc >= -f32::EPSILON && ca >= -f32::EPSILON)
        || (ab <= f32::EPSILON && bc <= f32::EPSILON && ca <= f32::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::{load_obj, parse_obj, triangulate_polygon};
    use crate::math::Vec3;

    #[test]
    fn parses_all_face_forms_and_metadata() {
        let asset = parse_obj("triangle.obj", "# comment\no Demo\ng body\nv 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nvn 0 0 1\nf 1/1/1 2/2/1 3/3/1\n").unwrap();
        assert_eq!(asset.mesh.triangles().len(), 1);
        assert_eq!(asset.object_name.as_deref(), Some("Demo"));
        assert_eq!(asset.groups, vec!["body"]);
        assert_eq!(
            asset.mesh.triangles()[0].vertices[0].texcoord.unwrap().x,
            0.0
        );
        assert_eq!(asset.mesh.triangles()[0].vertices[0].normal, Vec3::Z);
    }

    #[test]
    fn supports_negative_indices_and_convex_quad() {
        let asset = parse_obj(
            "quad.obj",
            "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf -4 -3 -2 -1\n",
        )
        .unwrap();
        assert_eq!(asset.mesh.triangles().len(), 2);
    }

    #[test]
    fn ear_clipping_handles_concave_polygon() {
        let points = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(2.0, 2.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ];
        assert_eq!(triangulate_polygon(&points).unwrap().len(), 3);
    }

    #[test]
    fn calculates_missing_normals_and_reports_errors() {
        let asset = parse_obj("missing.obj", "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        assert_eq!(asset.mesh.triangles()[0].vertices[0].normal, Vec3::Z);
        let error = parse_obj("bad.obj", "v 0 0 0\nf 1 2 3\n").unwrap_err();
        assert!(error.to_string().contains("bad.obj"));
    }

    #[test]
    fn rejects_zero_and_out_of_range_indices() {
        assert!(parse_obj("zero.obj", "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 0 1 2\n").is_err());
        assert!(parse_obj("range.obj", "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 4\n").is_err());
    }

    #[test]
    fn invalid_explicit_normal_falls_back_to_geometric_normal() {
        let asset = parse_obj(
            "invalid-normal.obj",
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 0\nf 1//1 2//1 3//1\n",
        )
        .unwrap();
        assert_eq!(asset.mesh.triangles()[0].vertices[0].normal, Vec3::Z);
    }

    #[test]
    fn generated_blender_assets_load_individually() {
        let names = [
            "solaris",
            "vesper",
            "nymara",
            "nymara_ring_inner",
            "nymara_ring_middle",
            "nymara_ring_outer",
            "cryon",
            "veridis",
            "umbra",
            "solene",
            "selene",
        ];
        for name in names {
            let path = format!("assets/models/obj/{name}.obj");
            let asset = load_obj(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
            assert!(!asset.positions.is_empty(), "{path} has no positions");
            assert!(
                !asset.mesh.triangles().is_empty(),
                "{path} has no triangles"
            );
            assert!(asset.mesh.validate().is_ok(), "{path} is invalid");
            assert!(
                asset
                    .mesh
                    .triangles()
                    .iter()
                    .flat_map(|triangle| triangle.vertices)
                    .all(|vertex| vertex.position.is_finite()
                        && vertex.normal.is_finite()
                        && vertex.normal.length() > 0.9)
            );
            let min = asset.positions.iter().fold(
                Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
                |value, point| {
                    Vec3::new(
                        value.x.min(point.x),
                        value.y.min(point.y),
                        value.z.min(point.z),
                    )
                },
            );
            let max = asset.positions.iter().fold(
                Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
                |value, point| {
                    Vec3::new(
                        value.x.max(point.x),
                        value.y.max(point.y),
                        value.z.max(point.z),
                    )
                },
            );
            let center = (min + max) * 0.5;
            assert!(center.length() < 0.2, "{path} is not centered: {center:?}");
            assert!(
                min.is_finite() && max.is_finite(),
                "{path} has invalid bounds"
            );
        }
    }

    #[test]
    fn generated_rings_are_separate_and_have_thin_depth() {
        for name in [
            "nymara_ring_inner",
            "nymara_ring_middle",
            "nymara_ring_outer",
        ] {
            let asset = load_obj(format!("assets/models/obj/{name}.obj")).unwrap();
            let min = asset.positions.iter().fold(
                Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
                |value, point| {
                    Vec3::new(
                        value.x.min(point.x),
                        value.y.min(point.y),
                        value.z.min(point.z),
                    )
                },
            );
            let max = asset.positions.iter().fold(
                Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
                |value, point| {
                    Vec3::new(
                        value.x.max(point.x),
                        value.y.max(point.y),
                        value.z.max(point.z),
                    )
                },
            );
            let spans = [max.x - min.x, max.y - min.y, max.z - min.z];
            assert!(spans.iter().filter(|span| **span > 2.0).count() >= 2);
            assert!(spans.iter().any(|span| *span > 0.0 && *span < 0.4));
        }
    }
}