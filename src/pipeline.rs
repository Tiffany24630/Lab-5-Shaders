use crate::camera::Camera;
use crate::color::Color;
use crate::framebuffer::Framebuffer;
use crate::geometry::{Mesh, Triangle3D};
use crate::math::{Mat4, Vec2, Vec3, Vec4};
use crate::rasterizer::{PerspectiveRasterVertex, rasterize_triangle_with_shader};
use crate::shader::{CachedCameraMatrices, ClipBufferStats, FragmentShader, ShaderUniforms, VertexColorShader};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const CLIP_EPSILON: f32 = 1.0e-6;

#[derive(Clone, Debug, Default)]
pub struct ClipStageDiagnosticReport {
    pub cases: u64,
    pub polygons_compared: u64,
    pub planes_compared: u64,
    pub vertices_compared: u64,
    pub vertex_count_differences: u64,
    pub order_differences: u64,
    pub position_differences: u64,
    pub attribute_differences: u64,
    pub triangulation_differences: u64,
    pub original_initial_capacity: usize,
    pub reused_initial_capacity_a: usize,
    pub reused_initial_capacity_b: usize,
    pub original_max_vertices: usize,
    pub reused_max_vertices: usize,
    pub original: ClipBufferStats,
    pub reused: ClipBufferStats,
    pub first_discrepancy: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct ClipVertex {
    position: Vec4,
    color: Color,
    normal: Vec3,
    local_position: Vec3,
    world_position: Vec3,
    uv: Option<Vec2>,
}

pub fn render_mesh(
    framebuffer: &mut Framebuffer,
    mesh: &Mesh,
    model: Mat4,
    camera: &Camera,
) -> usize {
    render_mesh_with_shader(
        framebuffer,
        mesh,
        model,
        camera,
        &VertexColorShader,
        &ShaderUniforms::default(),
    )
}

pub fn render_mesh_with_shader<S: FragmentShader>(
    framebuffer: &mut Framebuffer,
    mesh: &Mesh,
    model: Mat4,
    camera: &Camera,
    shader: &S,
    uniforms: &ShaderUniforms,
) -> usize {
    let mut written = 0;
    let mut clip_context = uniforms.clip_buffers_reused
        .then(|| ClipBufferContext::new(uniforms.clip_buffer_stats.clone()));
    for triangle in mesh.triangles() {
        written += render_triangle_with_shader_internal(
            framebuffer, triangle, model, camera, shader, uniforms, clip_context.as_mut(),
        );
    }
    written
}

pub fn render_triangle(
    framebuffer: &mut Framebuffer,
    triangle: &Triangle3D,
    model: Mat4,
    camera: &Camera,
) -> usize {
    render_triangle_with_shader(
        framebuffer,
        triangle,
        model,
        camera,
        &VertexColorShader,
        &ShaderUniforms::default(),
    )
}

pub fn render_triangle_with_shader<S: FragmentShader>(
    framebuffer: &mut Framebuffer,
    triangle: &Triangle3D,
    model: Mat4,
    camera: &Camera,
    shader: &S,
    uniforms: &ShaderUniforms,
) -> usize {
    render_triangle_with_shader_internal(framebuffer, triangle, model, camera, shader, uniforms, None)
}

struct ClipBufferContext {
    a: Vec<ClipVertex>,
    b: Vec<ClipVertex>,
    input_is_a: bool,
    stats: Option<Arc<Mutex<ClipBufferStats>>>,
}

impl ClipBufferContext {
    fn new(stats: Option<Arc<Mutex<ClipBufferStats>>>) -> Self {
        let context = Self { a: Vec::new(), b: Vec::new(), input_is_a: true, stats };
        if let Some(stats) = &context.stats {
            if let Ok(mut stats) = stats.lock() {
                stats.vec_constructions += 2;
                stats.max_capacity_a = context.a.capacity();
                stats.max_capacity_b = context.b.capacity();
            }
        }
        context
    }

    fn record(&self, update: impl FnOnce(&mut ClipBufferStats)) {
        if let Some(stats) = &self.stats {
            if let Ok(mut stats) = stats.lock() { update(&mut stats); }
        }
    }

    fn current(&self) -> &[ClipVertex] {
        if self.input_is_a { &self.a } else { &self.b }
    }

    fn clip(&mut self, vertices: &[ClipVertex; 3]) -> &[ClipVertex] {
        self.record(|stats| {
            stats.retained_capacity_events += u64::from(self.a.capacity() > 0);
            stats.retained_capacity_events += u64::from(self.b.capacity() > 0);
        });
        self.a.clear();
        self.b.clear();
        let previous_input_capacity = self.a.capacity();
        self.a.extend_from_slice(vertices);
        if self.a.capacity() > previous_input_capacity {
            self.record(|stats| stats.capacity_growth_events += 1);
        }
        self.input_is_a = true;
        self.record(|stats| {
            stats.input_triangles += 1;
            stats.vertices_written += 3;
            stats.vertices_processed += 3;
            if triangle_is_outside(vertices) {
                stats.triangles_outside += 1;
            } else if triangle_crosses_any_plane(vertices) {
                stats.triangles_crossing += 1;
            } else {
                stats.triangles_inside += 1;
            }
            stats.max_capacity_a = stats.max_capacity_a.max(self.a.capacity());
            stats.max_capacity_b = stats.max_capacity_b.max(self.b.capacity());
            record_capacity_bin(stats, self.a.capacity());
            record_capacity_bin(stats, self.b.capacity());
        });
        for (plane_index, plane) in PLANES.into_iter().enumerate() {
            let (input_len, output_len, output_grew) = {
                let (input, output) = if self.input_is_a {
                    (&self.a, &mut self.b)
                } else {
                    (&self.b, &mut self.a)
                };
                let previous_capacity = output.capacity();
                output.clear();
                clip_polygon_against_plane_into(input, plane, output, &self.stats);
                (input.len(), output.len(), output.capacity() > previous_capacity)
            };
            if output_grew {
                self.record(|stats| stats.capacity_growth_events += 1);
            }
            self.record(|stats| {
                stats.planes_processed += 1;
                stats.vertices_processed_by_plane[plane_index] += input_len as u64;
                stats.vertices_written_by_plane[plane_index] += output_len as u64;
                stats.polygon_vertices_by_plane[plane_index] += output_len as u64;
                stats.max_polygon_vertices = stats.max_polygon_vertices.max(output_len);
                stats.max_capacity_a = stats.max_capacity_a.max(self.a.capacity());
                stats.max_capacity_b = stats.max_capacity_b.max(self.b.capacity());
                record_capacity_bin(stats, self.a.capacity());
                record_capacity_bin(stats, self.b.capacity());
            });
            self.input_is_a = !self.input_is_a;
            if self.current().is_empty() {
                self.record(|stats| {
                    stats.empty_polygons += 1;
                    stats.empty_buffer_a_events += u64::from(self.a.is_empty());
                    stats.empty_buffer_b_events += u64::from(self.b.is_empty());
                });
                break;
            }
        }
        self.current()
    }

    fn diagnostic_stages(&mut self, vertices: &[ClipVertex; 3]) -> Vec<Vec<ClipVertex>> {
        self.record(|stats| {
            stats.retained_capacity_events += u64::from(self.a.capacity() > 0);
            stats.retained_capacity_events += u64::from(self.b.capacity() > 0);
        });
        self.a.clear();
        self.b.clear();
        let previous_input_capacity = self.a.capacity();
        self.a.extend_from_slice(vertices);
        if self.a.capacity() > previous_input_capacity {
            self.record(|stats| stats.capacity_growth_events += 1);
        }
        self.input_is_a = true;
        self.record(|stats| {
            stats.input_triangles += 1;
            stats.vertices_written += 3;
            stats.vertices_processed += 3;
            if triangle_is_outside(vertices) { stats.triangles_outside += 1; }
            else if triangle_crosses_any_plane(vertices) { stats.triangles_crossing += 1; }
            else { stats.triangles_inside += 1; }
            stats.max_capacity_a = stats.max_capacity_a.max(self.a.capacity());
            stats.max_capacity_b = stats.max_capacity_b.max(self.b.capacity());
            record_capacity_bin(stats, self.a.capacity());
            record_capacity_bin(stats, self.b.capacity());
        });
        let mut stages = vec![self.current().to_vec()];
        for (plane_index, plane) in PLANES.into_iter().enumerate() {
            let (output_grew, input_len, output_len) = {
                let (input, output) = if self.input_is_a {
                    (&self.a, &mut self.b)
                } else {
                    (&self.b, &mut self.a)
                };
                let previous_capacity = output.capacity();
                output.clear();
                clip_polygon_against_plane_into(input, plane, output, &self.stats);
                (output.capacity() > previous_capacity, input.len(), output.len())
            };
            if output_grew {
                self.record(|stats| stats.capacity_growth_events += 1);
            }
            self.record(|stats| {
                stats.planes_processed += 1;
                stats.vertices_processed_by_plane[plane_index] += input_len as u64;
                stats.vertices_written_by_plane[plane_index] += output_len as u64;
                stats.polygon_vertices_by_plane[plane_index] += output_len as u64;
                stats.max_polygon_vertices = stats.max_polygon_vertices.max(output_len);
                stats.max_capacity_a = stats.max_capacity_a.max(self.a.capacity());
                stats.max_capacity_b = stats.max_capacity_b.max(self.b.capacity());
                record_capacity_bin(stats, self.a.capacity());
                record_capacity_bin(stats, self.b.capacity());
            });
            self.input_is_a = !self.input_is_a;
            stages.push(self.current().to_vec());
            if self.current().is_empty() {
                self.record(|stats| {
                    stats.empty_polygons += 1;
                    stats.empty_buffer_a_events += u64::from(self.a.is_empty());
                    stats.empty_buffer_b_events += u64::from(self.b.is_empty());
                });
                while stages.len() < PLANES.len() + 1 { stages.push(Vec::new()); }
                break;
            }
        }
        stages
    }
}

fn render_triangle_with_shader_internal<S: FragmentShader>(
    framebuffer: &mut Framebuffer,
    triangle: &Triangle3D,
    model: Mat4,
    camera: &Camera,
    shader: &S,
    uniforms: &ShaderUniforms,
    mut clip_context: Option<&mut ClipBufferContext>,
) -> usize {
    if let Some(profile) = &uniforms.raster_detail_profile { profile.add_input(); }
    let transform_started = uniforms.raster_detail_profile.as_ref().map(|_| Instant::now());
    let preparation_started = uniforms.detail_profile.as_ref().map(|_| Instant::now());
    let (view, projection) = uniforms
        .cached_camera_matrices
        .map(|matrices: CachedCameraMatrices| (matrices.view, matrices.projection))
        .unwrap_or_else(|| (camera.view_matrix(), camera.projection_matrix()));
    let transform = projection * view * model;
    let vertices = triangle.vertices.map(|vertex| ClipVertex {
        position: transform
            * Vec4::new(vertex.position.x, vertex.position.y, vertex.position.z, 1.0),
        color: vertex.color,
        normal: uniforms
            .cached_normal_transform
            .map(|transform| transform.transform_normal(vertex.normal))
            .unwrap_or_else(|| model.transform_normal(vertex.normal)),
        local_position: vertex.position,
        world_position: model.transform_point(vertex.position),
        uv: vertex.texcoord,
    });
    if let (Some(profile), Some(started)) = (&uniforms.raster_detail_profile, transform_started) {
        profile.add_transform(started.elapsed());
    }
    if vertices.iter().any(|vertex| !finite_clip_vertex(*vertex)) {
        if let Some(profile) = &uniforms.raster_detail_profile { profile.add_invalid(); }
        return 0;
    }

    let clipping_started = uniforms.raster_detail_profile.as_ref().map(|_| Instant::now());
    let clipped = if let Some(context) = clip_context.as_deref_mut() {
        let polygon = clip_frustum_reused(&vertices, context);
        let clipped = triangulate(polygon);
        if let Some(stats) = &uniforms.clip_buffer_stats {
            if let Ok(mut stats) = stats.lock() {
                stats.final_triangles += clipped.len() as u64;
            }
        }
        clipped
    } else if let Some(stats) = &uniforms.clip_buffer_stats {
        let polygon = clip_frustum_with_stats(&vertices, stats);
        let clipped = triangulate(&polygon);
        if let Ok(mut stats) = stats.lock() {
            stats.final_triangles += clipped.len() as u64;
            stats.vertices_processed += polygon.len() as u64;
        }
        clipped
    } else {
        let polygon = clip_frustum(&vertices);
        triangulate(&polygon)
    };
    if let (Some(profile), Some(started)) = (&uniforms.raster_detail_profile, clipping_started) {
        profile.add_clipping(started.elapsed());
    }
    if let Some(profile) = &uniforms.raster_detail_profile { profile.add_clipped(clipped.len()); }
    let mut written = 0;
    for triangle in clipped {
        let projection_started = uniforms.raster_detail_profile.as_ref().map(|_| Instant::now());
        let Some(projected) = project_triangle(triangle, framebuffer.width(), framebuffer.height())
        else {
            if let (Some(profile), Some(started)) = (&uniforms.raster_detail_profile, projection_started) {
                profile.add_projection(started.elapsed());
            }
            continue;
        };
        if let (Some(profile), Some(started)) = (&uniforms.raster_detail_profile, projection_started) {
            profile.add_projection(started.elapsed());
            profile.add_projected();
        }
        if let (Some(profile), Some(preparation_started)) = (&uniforms.detail_profile, preparation_started.as_ref()) {
            profile.add_preparation(preparation_started.elapsed());
        }
        let raster_started = uniforms.detail_profile.as_ref().map(|_| Instant::now());
        written += rasterize_triangle_with_shader(framebuffer, projected, shader, uniforms);
        if let (Some(profile), Some(raster_started)) = (&uniforms.detail_profile, raster_started) {
            profile.add_raster(raster_started.elapsed());
        }
    }
    written
}

fn finite_clip_vertex(vertex: ClipVertex) -> bool {
    vertex.position.x.is_finite()
        && vertex.position.y.is_finite()
        && vertex.position.z.is_finite()
        && vertex.position.w.is_finite()
        && vertex.color.r.is_finite()
        && vertex.color.g.is_finite()
        && vertex.color.b.is_finite()
        && vertex.normal.is_finite()
        && vertex.local_position.is_finite()
        && vertex.world_position.is_finite()
        && vertex
            .uv
            .map(|uv| uv.x.is_finite() && uv.y.is_finite())
            .unwrap_or(true)
}

pub fn run_clip_buffers_stage_diagnostic() -> ClipStageDiagnosticReport {
    let cases = diagnostic_clip_cases();
    let mut report = ClipStageDiagnosticReport {
        cases: cases.len() as u64,
        original_initial_capacity: 3,
        reused_initial_capacity_a: 0,
        reused_initial_capacity_b: 0,
        ..ClipStageDiagnosticReport::default()
    };
    let reused_stats = Arc::new(Mutex::new(ClipBufferStats::default()));
    let mut reused_context = ClipBufferContext::new(Some(reused_stats.clone()));

    for (case_name, vertices) in &cases {
        let mut original_stats = ClipBufferStats::default();
        let original_stages = clip_original_diagnostic_stages(vertices, &mut original_stats);
        let reused_stages = reused_context.diagnostic_stages(vertices);
        report.original_max_vertices = report.original_max_vertices.max(original_stages.iter().map(Vec::len).max().unwrap_or(0));
        report.reused_max_vertices = report.reused_max_vertices.max(reused_stages.iter().map(Vec::len).max().unwrap_or(0));
        compare_clip_stages(&mut report, case_name, &original_stages, &reused_stages);

        let original_triangles = triangulate(original_stages.last().map(Vec::as_slice).unwrap_or(&[]));
        let reused_triangles = triangulate(reused_stages.last().map(Vec::as_slice).unwrap_or(&[]));
        if original_triangles.len() != reused_triangles.len()
            || original_triangles.iter().zip(&reused_triangles).any(|(left, right)| {
                left.iter().zip(right).any(|(a, b)| !same_clip_vertex(*a, *b))
            })
        {
            report.triangulation_differences += 1;
            record_first_discrepancy(&mut report, format!("caso {case_name}: triangulación distinta"));
        }
    }

    report.original = cases.iter().fold(ClipBufferStats::default(), |mut total, (_, vertices)| {
        let mut stats = ClipBufferStats::default();
        let stages = clip_original_diagnostic_stages(vertices, &mut stats);
        stats.final_triangles = triangulate(stages.last().map(Vec::as_slice).unwrap_or(&[])).len() as u64;
        accumulate_clip_stats(&mut total, &stats);
        total
    });
    report.reused = reused_stats.lock().map(|stats| stats.clone()).unwrap_or_default();
    report.reused.final_triangles = cases.iter().map(|(_, vertices)| {
        let stages = reused_context.diagnostic_stages(vertices);
        triangulate(stages.last().map(Vec::as_slice).unwrap_or(&[])).len() as u64
    }).sum();
    report
}

pub fn compare_mesh_clipping(
    mesh: &Mesh,
    model: Mat4,
    camera: &Camera,
) -> ClipStageDiagnosticReport {
    let mut report = ClipStageDiagnosticReport::default();
    let reused_stats = Arc::new(Mutex::new(ClipBufferStats::default()));
    let mut reused_context = ClipBufferContext::new(Some(reused_stats.clone()));
    let transform = camera.projection_matrix() * camera.view_matrix() * model;
    for (triangle_index, triangle) in mesh.triangles().into_iter().enumerate() {
        let vertices = triangle.vertices.map(|vertex| ClipVertex {
            position: transform * Vec4::new(vertex.position.x, vertex.position.y, vertex.position.z, 1.0),
            color: vertex.color,
            normal: model.transform_normal(vertex.normal),
            local_position: vertex.position,
            world_position: model.transform_point(vertex.position),
            uv: vertex.texcoord,
        });
        if vertices.iter().any(|vertex| !finite_clip_vertex(*vertex)) { continue; }
        report.cases += 1;
        let mut original_stats = ClipBufferStats::default();
        let original_stages = clip_original_diagnostic_stages(&vertices, &mut original_stats);
        let reused_stages = reused_context.diagnostic_stages(&vertices);
        report.original_max_vertices = report.original_max_vertices.max(original_stages.iter().map(Vec::len).max().unwrap_or(0));
        report.reused_max_vertices = report.reused_max_vertices.max(reused_stages.iter().map(Vec::len).max().unwrap_or(0));
        compare_clip_stages(&mut report, &format!("triángulo {triangle_index}"), &original_stages, &reused_stages);
        let original_triangles = triangulate(original_stages.last().map(Vec::as_slice).unwrap_or(&[]));
        let reused_triangles = triangulate(reused_stages.last().map(Vec::as_slice).unwrap_or(&[]));
        if original_triangles.len() != reused_triangles.len()
            || original_triangles.iter().zip(&reused_triangles).any(|(left, right)| {
                left.iter().zip(right).any(|(a, b)| !same_clip_vertex(*a, *b))
            })
        {
            report.triangulation_differences += 1;
            record_first_discrepancy(&mut report, format!("triángulo {triangle_index}: triangulación distinta"));
        }
        let mut original_stats = original_stats;
        original_stats.final_triangles = original_triangles.len() as u64;
        accumulate_clip_stats(&mut report.original, &original_stats);
    }
    report.reused = reused_stats.lock().map(|stats| stats.clone()).unwrap_or_default();
    report.reused.final_triangles = report.original.final_triangles;
    report
}

fn record_first_discrepancy(report: &mut ClipStageDiagnosticReport, message: String) {
    if report.first_discrepancy.is_none() { report.first_discrepancy = Some(message); }
}

fn compare_clip_stages(
    report: &mut ClipStageDiagnosticReport,
    case_name: &str,
    original: &[Vec<ClipVertex>],
    reused: &[Vec<ClipVertex>],
) {
    for plane_index in 0..PLANES.len() + 1 {
        let left = original.get(plane_index).map(Vec::as_slice).unwrap_or(&[]);
        let right = reused.get(plane_index).map(Vec::as_slice).unwrap_or(&[]);
        report.polygons_compared += 1;
        report.planes_compared += u64::from(plane_index > 0);
        if left.len() != right.len() {
            report.vertex_count_differences += 1;
            record_first_discrepancy(report, format!("caso {case_name}, etapa {plane_index}: cantidad {} != {}", left.len(), right.len()));
            continue;
        }
        report.vertices_compared += left.len() as u64;
        let mut same_order = true;
        let mut position_difference = false;
        let mut attribute_difference = false;
        for (left_vertex, right_vertex) in left.iter().zip(right) {
            if !same_clip_vertex(*left_vertex, *right_vertex) { same_order = false; }
            if !same_position(*left_vertex, *right_vertex) { position_difference = true; }
            if !same_attributes(*left_vertex, *right_vertex) { attribute_difference = true; }
        }
        if !same_order {
            let same_multiset = left.iter().all(|vertex| right.iter().any(|other| same_clip_vertex(*vertex, *other)));
            if same_multiset { report.order_differences += 1; }
            if position_difference { report.position_differences += 1; }
            if attribute_difference { report.attribute_differences += 1; }
            record_first_discrepancy(report, format!("caso {case_name}, etapa {plane_index}: atributos u orden distintos"));
        }
    }
}

fn same_f32(a: f32, b: f32) -> bool { a.to_bits() == b.to_bits() }

fn same_position(a: ClipVertex, b: ClipVertex) -> bool {
    same_f32(a.position.x, b.position.x) && same_f32(a.position.y, b.position.y)
        && same_f32(a.position.z, b.position.z) && same_f32(a.position.w, b.position.w)
}

fn same_attributes(a: ClipVertex, b: ClipVertex) -> bool {
    same_f32(a.color.r, b.color.r) && same_f32(a.color.g, b.color.g) && same_f32(a.color.b, b.color.b)
        && same_f32(a.normal.x, b.normal.x) && same_f32(a.normal.y, b.normal.y) && same_f32(a.normal.z, b.normal.z)
        && same_f32(a.local_position.x, b.local_position.x) && same_f32(a.local_position.y, b.local_position.y) && same_f32(a.local_position.z, b.local_position.z)
        && same_f32(a.world_position.x, b.world_position.x) && same_f32(a.world_position.y, b.world_position.y) && same_f32(a.world_position.z, b.world_position.z)
        && match (a.uv, b.uv) {
            (None, None) => true,
            (Some(a), Some(b)) => same_f32(a.x, b.x) && same_f32(a.y, b.y),
            _ => false,
        }
}

fn same_clip_vertex(a: ClipVertex, b: ClipVertex) -> bool { same_position(a, b) && same_attributes(a, b) }

fn clip_original_diagnostic_stages(vertices: &[ClipVertex; 3], stats: &mut ClipBufferStats) -> Vec<Vec<ClipVertex>> {
    let mut polygon = vertices.to_vec();
    stats.input_triangles += 1;
    stats.vec_constructions += 1;
    stats.vertices_written += 3;
    stats.vertices_processed += 3;
    if triangle_is_outside(vertices) { stats.triangles_outside += 1; }
    else if triangle_crosses_any_plane(vertices) { stats.triangles_crossing += 1; }
    else { stats.triangles_inside += 1; }
    stats.max_capacity_a = stats.max_capacity_a.max(polygon.capacity());
    record_capacity_bin(stats, polygon.capacity());
    let mut stages = vec![polygon.clone()];
    for (plane_index, plane) in PLANES.into_iter().enumerate() {
        stats.planes_processed += 1;
        stats.vec_constructions += 1;
        stats.vertices_processed += polygon.len() as u64;
        stats.vertices_processed_by_plane[plane_index] += polygon.len() as u64;
        record_capacity_bin(stats, polygon.capacity());
        let next = clip_polygon_against_plane(&polygon, plane);
        stats.vertices_written += next.len() as u64;
        stats.vertices_written_by_plane[plane_index] += next.len() as u64;
        stats.polygon_vertices_by_plane[plane_index] += next.len() as u64;
        stats.max_polygon_vertices = stats.max_polygon_vertices.max(next.len());
        stats.max_capacity_a = stats.max_capacity_a.max(next.capacity());
        record_capacity_bin(stats, next.capacity());
        if next.capacity() != polygon.capacity() { stats.capacity_growth_events += 1; }
        polygon = next;
        stages.push(polygon.clone());
        if polygon.is_empty() {
            stats.empty_polygons += 1;
            while stages.len() < PLANES.len() + 1 { stages.push(Vec::new()); }
            break;
        }
    }
    stages
}

fn accumulate_clip_stats(total: &mut ClipBufferStats, value: &ClipBufferStats) {
    total.input_triangles += value.input_triangles;
    total.planes_processed += value.planes_processed;
    total.vec_constructions += value.vec_constructions;
    total.capacity_growth_events += value.capacity_growth_events;
    total.max_capacity_a = total.max_capacity_a.max(value.max_capacity_a);
    total.max_capacity_b = total.max_capacity_b.max(value.max_capacity_b);
    total.vertices_written += value.vertices_written;
    total.vertices_processed += value.vertices_processed;
    total.empty_polygons += value.empty_polygons;
    total.final_triangles += value.final_triangles;
    total.triangles_inside += value.triangles_inside;
    total.triangles_outside += value.triangles_outside;
    total.triangles_crossing += value.triangles_crossing;
    for index in 0..6 {
        total.vertices_written_by_plane[index] += value.vertices_written_by_plane[index];
        total.vertices_processed_by_plane[index] += value.vertices_processed_by_plane[index];
        total.polygon_vertices_by_plane[index] += value.polygon_vertices_by_plane[index];
    }
    total.max_polygon_vertices = total.max_polygon_vertices.max(value.max_polygon_vertices);
    total.retained_capacity_events += value.retained_capacity_events;
    total.empty_buffer_a_events += value.empty_buffer_a_events;
    total.empty_buffer_b_events += value.empty_buffer_b_events;
    for index in 0..total.capacity_bins.len() {
        total.capacity_bins[index] += value.capacity_bins[index];
    }
}

fn diagnostic_clip_vertex(x: f32, y: f32, z: f32, seed: f32) -> ClipVertex {
    ClipVertex {
        position: Vec4::new(x, y, z, 1.0),
        color: Color::new(seed, seed + 0.1, seed + 0.2),
        normal: Vec3::new(seed + 0.3, seed + 0.4, seed + 0.5),
        local_position: Vec3::new(x + seed, y - seed, z + seed),
        world_position: Vec3::new(x - seed, y + seed, z - seed),
        uv: Some(Vec2::new(seed + 0.6, seed + 0.7)),
    }
}

fn diagnostic_clip_cases() -> Vec<(&'static str, [ClipVertex; 3])> {
    let e = CLIP_EPSILON;
    vec![
        ("inside", [diagnostic_clip_vertex(-0.5, -0.5, 0.0, 0.1), diagnostic_clip_vertex(0.5, -0.5, 0.0, 0.2), diagnostic_clip_vertex(0.0, 0.5, 0.0, 0.3)]),
        ("outside_left", [diagnostic_clip_vertex(-2.0, 0.0, 0.0, 1.0), diagnostic_clip_vertex(-2.0, 0.5, 0.0, 1.1), diagnostic_clip_vertex(-2.0, -0.5, 0.0, 1.2)]),
        ("cross_left", [diagnostic_clip_vertex(-1.5, 0.0, 0.0, 2.0), diagnostic_clip_vertex(0.5, -0.5, 0.0, 2.1), diagnostic_clip_vertex(0.5, 0.5, 0.0, 2.2)]),
        ("cross_right", [diagnostic_clip_vertex(1.5, 0.0, 0.0, 3.0), diagnostic_clip_vertex(-0.5, -0.5, 0.0, 3.1), diagnostic_clip_vertex(-0.5, 0.5, 0.0, 3.2)]),
        ("cross_bottom", [diagnostic_clip_vertex(-0.5, -1.5, 0.0, 4.0), diagnostic_clip_vertex(0.5, 0.5, 0.0, 4.1), diagnostic_clip_vertex(-0.5, 0.5, 0.0, 4.2)]),
        ("cross_top", [diagnostic_clip_vertex(-0.5, 1.5, 0.0, 5.0), diagnostic_clip_vertex(0.5, -0.5, 0.0, 5.1), diagnostic_clip_vertex(-0.5, -0.5, 0.0, 5.2)]),
        ("cross_near", [diagnostic_clip_vertex(-0.5, -0.5, -1.5, 6.0), diagnostic_clip_vertex(0.5, -0.5, 0.0, 6.1), diagnostic_clip_vertex(0.0, 0.5, 0.0, 6.2)]),
        ("cross_far", [diagnostic_clip_vertex(-0.5, -0.5, 1.5, 7.0), diagnostic_clip_vertex(0.5, -0.5, 0.0, 7.1), diagnostic_clip_vertex(0.0, 0.5, 0.0, 7.2)]),
        ("multi_plane", [diagnostic_clip_vertex(-3.0, -3.0, -3.0, 8.0), diagnostic_clip_vertex(3.0, -3.0, 3.0, 8.1), diagnostic_clip_vertex(0.0, 3.0, 0.0, 8.2)]),
        ("hexagon_a", [diagnostic_clip_vertex(-4.0, -4.0, 0.0, 9.0), diagnostic_clip_vertex(4.0, -4.0, 0.0, 9.1), diagnostic_clip_vertex(0.0, 4.0, 0.0, 9.2)]),
        ("hexagon_b", [diagnostic_clip_vertex(-4.0, 0.0, -4.0, 10.0), diagnostic_clip_vertex(4.0, 0.0, -4.0, 10.1), diagnostic_clip_vertex(0.0, 0.0, 4.0, 10.2)]),
        ("epsilon", [diagnostic_clip_vertex(-1.0 - e * 0.5, -0.2, 0.0, 11.0), diagnostic_clip_vertex(0.8, -0.2, 0.0, 11.1), diagnostic_clip_vertex(0.0, 0.8, 0.0, 11.2)]),
        ("negative_small", [diagnostic_clip_vertex(-e, -e, -e, 12.0), diagnostic_clip_vertex(e, -e, -e, 12.1), diagnostic_clip_vertex(0.0, e, -e, 12.2)]),
        ("degenerate", [diagnostic_clip_vertex(0.0, 0.0, 0.0, 13.0), diagnostic_clip_vertex(0.0, 0.0, 0.0, 13.1), diagnostic_clip_vertex(0.0, 0.0, 0.0, 13.2)]),
    ]
}

#[derive(Clone, Copy)]
enum ClipPlane {
    Left,
    Right,
    Bottom,
    Top,
    Near,
    Far,
}

const PLANES: [ClipPlane; 6] = [
    ClipPlane::Left,
    ClipPlane::Right,
    ClipPlane::Bottom,
    ClipPlane::Top,
    ClipPlane::Near,
    ClipPlane::Far,
];

fn triangle_crosses_any_plane(vertices: &[ClipVertex; 3]) -> bool {
    PLANES.into_iter().any(|plane| {
        let inside = vertices.map(|vertex| plane_distance(vertex, plane) >= -CLIP_EPSILON);
        inside.iter().any(|value| *value) && inside.iter().any(|value| !*value)
    })
}

fn triangle_is_outside(vertices: &[ClipVertex; 3]) -> bool {
    PLANES.into_iter().any(|plane| {
        vertices.iter().all(|vertex| plane_distance(*vertex, plane) < -CLIP_EPSILON)
    })
}

fn record_capacity_bin(stats: &mut ClipBufferStats, capacity: usize) {
    stats.capacity_bins[capacity.min(stats.capacity_bins.len() - 1)] += 1;
}

fn plane_distance(vertex: ClipVertex, plane: ClipPlane) -> f32 {
    let p = vertex.position;
    match plane {
        ClipPlane::Left => p.x + p.w,
        ClipPlane::Right => p.w - p.x,
        ClipPlane::Bottom => p.y + p.w,
        ClipPlane::Top => p.w - p.y,
        ClipPlane::Near => p.z + p.w,
        ClipPlane::Far => p.w - p.z,
    }
}

fn clip_frustum(vertices: &[ClipVertex; 3]) -> Vec<ClipVertex> {
    let mut polygon = vertices.to_vec();
    for plane in PLANES {
        polygon = clip_polygon_against_plane(&polygon, plane);
        if polygon.is_empty() {
            break;
        }
    }
    polygon
}

fn clip_frustum_with_stats(
    vertices: &[ClipVertex; 3],
    stats: &Arc<Mutex<ClipBufferStats>>,
) -> Vec<ClipVertex> {
    if let Ok(mut stats) = stats.lock() {
        stats.input_triangles += 1;
        stats.vec_constructions += 1;
        stats.vertices_written += 3;
        if triangle_is_outside(vertices) { stats.triangles_outside += 1; }
        else if triangle_crosses_any_plane(vertices) { stats.triangles_crossing += 1; }
        else { stats.triangles_inside += 1; }
        record_capacity_bin(&mut stats, 3);
    }
    let mut polygon = vertices.to_vec();
    for (plane_index, plane) in PLANES.into_iter().enumerate() {
        if let Ok(mut stats) = stats.lock() {
            stats.planes_processed += 1;
            stats.vec_constructions += 1;
            stats.vertices_processed += polygon.len() as u64;
            stats.vertices_processed_by_plane[plane_index] += polygon.len() as u64;
            record_capacity_bin(&mut stats, polygon.capacity());
        }
        polygon = clip_polygon_against_plane(&polygon, plane);
        if let Ok(mut stats) = stats.lock() {
            stats.vertices_written += polygon.len() as u64;
            stats.vertices_written_by_plane[plane_index] += polygon.len() as u64;
            stats.polygon_vertices_by_plane[plane_index] += polygon.len() as u64;
            stats.max_polygon_vertices = stats.max_polygon_vertices.max(polygon.len());
            stats.max_capacity_a = stats.max_capacity_a.max(polygon.capacity());
            record_capacity_bin(&mut stats, polygon.capacity());
        }
        if polygon.is_empty() {
            if let Ok(mut stats) = stats.lock() {
                stats.empty_polygons += 1;
            }
            break;
        }
    }
    polygon
}

fn clip_frustum_reused<'a>(
    vertices: &[ClipVertex; 3],
    context: &'a mut ClipBufferContext,
) -> &'a [ClipVertex] {
    context.clip(vertices)
}

fn clip_polygon_against_plane_into(
    polygon: &[ClipVertex],
    plane: ClipPlane,
    result: &mut Vec<ClipVertex>,
    stats: &Option<Arc<Mutex<ClipBufferStats>>>,
) {
    if polygon.is_empty() { return; }
    if let Some(stats) = stats {
        if let Ok(mut stats) = stats.lock() { stats.vertices_processed += polygon.len() as u64; }
    }
    for index in 0..polygon.len() {
        let current = polygon[index];
        let next = polygon[(index + 1) % polygon.len()];
        let current_distance = plane_distance(current, plane);
        let next_distance = plane_distance(next, plane);
        let current_inside = current_distance >= -CLIP_EPSILON;
        let next_inside = next_distance >= -CLIP_EPSILON;

        if current_inside != next_inside {
            let denominator = current_distance - next_distance;
            if denominator.abs() > f32::EPSILON {
                let t = current_distance / denominator;
                if t.is_finite() && (-CLIP_EPSILON..=1.0 + CLIP_EPSILON).contains(&t) {
                    let intersection = ClipVertex {
                        position: lerp_vec4(current.position, next.position, t),
                        color: current.color.lerp(next.color, t),
                        normal: current.normal.lerp(next.normal, t),
                        local_position: current.local_position.lerp(next.local_position, t),
                        world_position: current.world_position.lerp(next.world_position, t),
                        uv: lerp_uv(current.uv, next.uv, t),
                    };
                    if finite_clip_vertex(intersection) {
                        result.push(intersection);
                        if let Some(stats) = stats {
                            if let Ok(mut stats) = stats.lock() { stats.vertices_written += 1; }
                        }
                    }
                }
            }
        }
        if next_inside && finite_clip_vertex(next) {
            result.push(next);
            if let Some(stats) = stats {
                if let Ok(mut stats) = stats.lock() { stats.vertices_written += 1; }
            }
        }
    }
}

fn clip_polygon_against_plane(polygon: &[ClipVertex], plane: ClipPlane) -> Vec<ClipVertex> {
    if polygon.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(polygon.len() + 1);
    for index in 0..polygon.len() {
        let current = polygon[index];
        let next = polygon[(index + 1) % polygon.len()];
        let current_distance = plane_distance(current, plane);
        let next_distance = plane_distance(next, plane);
        let current_inside = current_distance >= -CLIP_EPSILON;
        let next_inside = next_distance >= -CLIP_EPSILON;

        if current_inside != next_inside {
            let denominator = current_distance - next_distance;
            if denominator.abs() > f32::EPSILON {
                let t = current_distance / denominator;
                if t.is_finite() && (-CLIP_EPSILON..=1.0 + CLIP_EPSILON).contains(&t) {
                    let intersection = ClipVertex {
                        position: lerp_vec4(current.position, next.position, t),
                        color: current.color.lerp(next.color, t),
                        normal: current.normal.lerp(next.normal, t),
                        local_position: current.local_position.lerp(next.local_position, t),
                        world_position: current.world_position.lerp(next.world_position, t),
                        uv: lerp_uv(current.uv, next.uv, t),
                    };
                    if finite_clip_vertex(intersection) {
                        result.push(intersection);
                    }
                }
            }
        }
        if next_inside && finite_clip_vertex(next) {
            result.push(next);
        }
    }
    result
}

fn lerp_vec4(a: Vec4, b: Vec4, t: f32) -> Vec4 {
    Vec4::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
        a.w + (b.w - a.w) * t,
    )
}

fn lerp_uv(a: Option<Vec2>, b: Option<Vec2>, t: f32) -> Option<Vec2> {
    Some(a? + (b? - a?) * t)
}

fn triangulate(polygon: &[ClipVertex]) -> Vec<[ClipVertex; 3]> {
    if polygon.len() < 3 {
        return Vec::new();
    }
    (1..polygon.len() - 1)
        .map(|index| [polygon[0], polygon[index], polygon[index + 1]])
        .filter(|triangle| triangle_area_squared(*triangle) > CLIP_EPSILON * CLIP_EPSILON)
        .collect()
}

fn triangle_area_squared(triangle: [ClipVertex; 3]) -> f32 {
    let a = triangle[0].position;
    let b = triangle[1].position;
    let c = triangle[2].position;
    if a.w.abs() <= f32::EPSILON || b.w.abs() <= f32::EPSILON || c.w.abs() <= f32::EPSILON {
        return 0.0;
    }
    let ax = a.x / a.w;
    let ay = a.y / a.w;
    let bx = b.x / b.w;
    let by = b.y / b.w;
    let cx = c.x / c.w;
    let cy = c.y / c.w;
    ((bx - ax) * (cy - ay) - (by - ay) * (cx - ax)).powi(2)
}

fn project_triangle(
    triangle: [ClipVertex; 3],
    width: usize,
    height: usize,
) -> Option<[PerspectiveRasterVertex; 3]> {
    triangle
        .map(|vertex| {
            let clip = vertex.position;
            if clip.w <= f32::EPSILON || !clip.w.is_finite() {
                return None;
            }
            let inv_w = 1.0 / clip.w;
            let ndc_x = clip.x * inv_w;
            let ndc_y = clip.y * inv_w;
            let ndc_z = clip.z * inv_w;
            if !inv_w.is_finite() || !ndc_x.is_finite() || !ndc_y.is_finite() || !ndc_z.is_finite()
            {
                return None;
            }
            Some(PerspectiveRasterVertex {
                x: (ndc_x + 1.0) * 0.5 * width as f32,
                y: (1.0 - ndc_y) * 0.5 * height as f32,
                ndc_depth: ndc_z,
                inv_w,
                color_over_w: Color::new_const(
                    vertex.color.r * inv_w,
                    vertex.color.g * inv_w,
                    vertex.color.b * inv_w,
                ),
                normal_over_w: vertex.normal * inv_w,
                local_position_over_w: vertex.local_position * inv_w,
                world_position_over_w: vertex.world_position * inv_w,
                uv_over_w: vertex.uv.map(|uv| uv * inv_w),
            })
        })
        .into_iter()
        .collect::<Option<Vec<_>>>()?
        .try_into()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::{
        ClipPlane, ClipVertex, clip_frustum, clip_polygon_against_plane, render_mesh,
        render_mesh_with_shader,
    };
    use crate::camera::Camera;
    use crate::color::Color;
    use crate::framebuffer::Framebuffer;
    use crate::geometry::{Mesh, Triangle3D, Vertex3D};
    use crate::math::{Mat4, Vec3, Vec4};
    use crate::shader::{FragmentInput, FragmentShader, ShaderUniforms};
    use std::f32::consts::FRAC_PI_2;

    fn clip(x: f32, y: f32, z: f32, w: f32, color: Color) -> ClipVertex {
        ClipVertex {
            position: Vec4::new(x, y, z, w),
            color,
            normal: Vec3::Z,
            local_position: Vec3::ZERO,
            world_position: Vec3::ZERO,
            uv: None,
        }
    }

    fn camera() -> Camera {
        Camera::new(
            Vec3::new(0.0, 0.0, 3.0),
            Vec3::ZERO,
            Vec3::Y,
            FRAC_PI_2,
            1.0,
            0.5,
            20.0,
        )
        .unwrap()
    }

    fn triangle(z: f32) -> Triangle3D {
        Triangle3D {
            vertices: [
                Vertex3D::new(Vec3::new(-0.5, -0.5, z), Color::new(1.0, 0.0, 0.0)),
                Vertex3D::new(Vec3::new(0.5, -0.5, z), Color::new(0.0, 1.0, 0.0)),
                Vertex3D::new(Vec3::new(0.0, 0.5, z), Color::new(0.0, 0.0, 1.0)),
            ],
        }
    }

    #[test]
    fn accepts_inside_triangle_and_rejects_each_outside_plane() {
        let inside = [
            clip(-0.5, -0.5, 0.0, 1.0, Color::WHITE),
            clip(0.5, -0.5, 0.0, 1.0, Color::WHITE),
            clip(0.0, 0.5, 0.0, 1.0, Color::WHITE),
        ];
        assert_eq!(clip_frustum(&inside).len(), 3);
        for (plane, vertex) in [
            (ClipPlane::Left, clip(-2.0, 0.0, 0.0, 1.0, Color::WHITE)),
            (ClipPlane::Right, clip(2.0, 0.0, 0.0, 1.0, Color::WHITE)),
            (ClipPlane::Bottom, clip(0.0, -2.0, 0.0, 1.0, Color::WHITE)),
            (ClipPlane::Top, clip(0.0, 2.0, 0.0, 1.0, Color::WHITE)),
            (ClipPlane::Near, clip(0.0, 0.0, -2.0, 1.0, Color::WHITE)),
            (ClipPlane::Far, clip(0.0, 0.0, 2.0, 1.0, Color::WHITE)),
        ] {
            assert!(clip_polygon_against_plane(&[vertex; 3], plane).is_empty());
        }
    }

    #[test]
    fn clips_all_boundaries_and_preserves_intersection_attributes() {
        let polygon = clip_frustum(&[
            clip(-2.0, -2.0, -2.0, 1.0, Color::BLACK),
            clip(2.0, -0.5, 0.0, 1.0, Color::WHITE),
            clip(0.0, 2.0, 0.0, 1.0, Color::WHITE),
        ]);
        assert!(!polygon.is_empty());
        assert!(polygon.len() >= 3);
        assert!(polygon.iter().all(|v| v.position.x.is_finite()
            && v.position.y.is_finite()
            && v.position.z.is_finite()
            && v.position.w.is_finite()));
        assert!(polygon.iter().any(|v| v.color.r > 0.0 && v.color.r < 1.0));
    }

    #[test]
    fn crossing_one_plane_can_create_a_quad() {
        let polygon = clip_polygon_against_plane(
            &[
                clip(-2.0, 0.0, 0.0, 1.0, Color::BLACK),
                clip(0.5, -0.5, 0.0, 1.0, Color::WHITE),
                clip(0.5, 0.5, 0.0, 1.0, Color::WHITE),
            ],
            ClipPlane::Left,
        );
        assert_eq!(polygon.len(), 4);
    }

    #[test]
    fn render_pipeline_handles_inside_behind_and_occlusion() {
        let camera = camera();
        let mesh = Mesh::new(vec![triangle(-3.0)]).unwrap();
        let mut framebuffer = Framebuffer::new(100, 100).unwrap();
        assert!(render_mesh(&mut framebuffer, &mesh, Mat4::identity(), &camera) > 0);
        let behind = Mesh::new(vec![triangle(5.0)]).unwrap();
        assert_eq!(
            render_mesh(&mut framebuffer, &behind, Mat4::identity(), &camera),
            0
        );
    }

    struct ProbeShader;

    impl FragmentShader for ProbeShader {
        fn shade(&self, input: &FragmentInput, uniforms: &ShaderUniforms) -> Color {
            let uv_signal = input.uv.map(|uv| uv.x + uv.y).unwrap_or(0.0);
            Color::new(
                uv_signal + uniforms.time * 0.0,
                input.normal.z.abs(),
                input.depth,
            )
        }
    }

    #[test]
    fn custom_shader_receives_interpolated_attributes() {
        let camera = camera();
        let mut triangle = triangle(-3.0);
        for vertex in &mut triangle.vertices {
            vertex.texcoord = Some(crate::math::Vec2::new(0.25, 0.5));
            vertex.normal = Vec3::Z;
        }
        let mesh = Mesh::new(vec![triangle]).unwrap();
        let mut framebuffer = Framebuffer::new(100, 100).unwrap();
        assert!(
            render_mesh_with_shader(
                &mut framebuffer,
                &mesh,
                Mat4::identity(),
                &camera,
                &ProbeShader,
                &ShaderUniforms::default(),
            ) > 0
        );
        let pixel = (0..100)
            .flat_map(|y| (0..100).map(move |x| (x, y)))
            .find_map(|(x, y)| framebuffer.get_pixel(x, y).filter(|color| color.g > 0.9));
        assert!(pixel.is_some());
    }
}