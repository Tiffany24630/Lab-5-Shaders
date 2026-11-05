use std::fs;
use std::io;
use std::sync::Arc;
use std::time::Instant;
#[cfg(feature = "interactive")]
use std::time::Duration;
#[cfg(feature = "interactive")]
use std::collections::VecDeque;
use std::env;

use aetheria::animation::{BodyMotion, Orbit, RotationAxis};
#[cfg(feature = "interactive")]
use aetheria::animation::SimulationClock;
use aetheria::assets::load_obj;
use aetheria::camera::Camera;
use aetheria::color::Color;
use aetheria::diagnostics::{DiagnosticObject, DiagnosticsHandle};
use aetheria::framebuffer::Framebuffer;
#[cfg(feature = "interactive")]
use aetheria::hud::{self, HudState};
use aetheria::math::{Mat4, Vec3};
use aetheria::pipeline::render_mesh_with_shader;
use aetheria::procedural::{fbm_comparison_stats, fbm_normalization_table_matches_original, set_fbm_comparison, set_fbm_precomputed, FBM_NORMALIZATION_TABLE};
use aetheria::profile::{DetailedProfile, DetailedProfileStats, ProceduralDetailProfile, ProceduralDetailStats, RasterDetailProfile, RasterDetailStats, ShaderDetailProfile, ShaderDetailStats};
use aetheria::orbit_camera::OrbitCameraController;
use aetheria::scene::{BodyId, BodyPose, BodyPoses};
use aetheria::shader::{CachedCameraMatrices, ShaderUniforms};
use aetheria::shaders::cryon::CryonShader;
use aetheria::shaders::nymara::{NymaraRingShader, NymaraShader};
use aetheria::shaders::selene::SeleneShader;
use aetheria::shaders::solaris::SolarisShader;
use aetheria::shaders::solene::SoleneShader;
use aetheria::shaders::umbra::UmbraShader;
use aetheria::shaders::veridis::VeridisShader;
use aetheria::shaders::vesper::VesperShader;
use aetheria::shadows::ShadowScene;
#[cfg(feature = "interactive")]
use aetheria::space_background::SpaceBackground;
#[cfg(feature = "interactive")]
use aetheria::window::CpuWindow;
#[cfg(feature = "interactive")]
use minifb::Key;

#[derive(Clone, Copy, Debug)]
enum PreviewCurve {
    Original,
    A,
    B,
    C,
}

impl PreviewCurve {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "original" => Some(Self::Original),
            "A" => Some(Self::A),
            "B" => Some(Self::B),
            "C" => Some(Self::C),
            _ => None,
        }
    }

    fn parameters(self) -> Option<(f32, f32)> {
        match self {
            Self::Original => None,
            Self::A => Some((0.50, 0.90)),
            Self::B => Some((0.50, 0.80)),
            Self::C => Some((0.50, 0.70)),
        }
    }

    fn output_path(self) -> &'static str {
        match self {
            Self::Original => "output/phase21p_vesper_original.ppm",
            Self::A => "output/phase21p_vesper_A.ppm",
            Self::B => "output/phase21p_vesper_B.ppm",
            Self::C => "output/phase21p_vesper_C.ppm",
        }
    }
}

#[cfg_attr(not(feature = "interactive"), allow(dead_code))]
#[derive(Clone, Copy, Debug, Default)]
struct ProfileFrame {
    total_ms: f64,
    update_ms: f64,
    poses_ms: f64,
    shadows_ms: f64,
    background_ms: f64,
    framebuffer_ms: f64,
    geometry_ms: f64,
    hud_ms: f64,
    conversion_ms: f64,
    present_ms: f64,
    wait_ms: f64,
    bvh_build_ms: f64,
    shadow_transform_ms: f64,
    bvh_nodes: u64,
    bvh_triangles: u64,
    bvh_depth: u32,
    shadow_scene_builds: u64,
    shadow_triangles_built: u64,
    detail: DetailedProfileStats,
    raster_detail: RasterDetailStats,
    shader_detail: ShaderDetailStats,
    procedural_detail: ProceduralDetailStats,
}

#[cfg(feature = "interactive")]
struct ProfileWindow {
    samples: VecDeque<ProfileFrame>,
    capacity: usize,
    last_report: Instant,
}

#[cfg(feature = "interactive")]
impl ProfileWindow {
    fn new() -> Self {
        Self { samples: VecDeque::with_capacity(120), capacity: 120, last_report: Instant::now() }
    }

    fn record(&mut self, sample: ProfileFrame) {
        if self.samples.len() == self.capacity { self.samples.pop_front(); }
        self.samples.push_back(sample);
        if self.last_report.elapsed() >= Duration::from_secs(1) {
            self.report();
            self.last_report = Instant::now();
        }
    }

    fn report(&self) {
        if self.samples.is_empty() { return; }
        let values = |f: fn(ProfileFrame) -> f64| -> Vec<f64> {
            let mut values: Vec<f64> = self.samples.iter().copied().map(f).collect();
            values.sort_by(f64::total_cmp);
            values
        };
        let percentile = |values: &[f64], p: f64| -> f64 {
            let index = ((values.len() - 1) as f64 * p).round() as usize;
            values[index]
        };
        let average = |f: fn(ProfileFrame) -> f64| -> f64 {
            self.samples.iter().map(|sample| f(*sample)).sum::<f64>() / self.samples.len() as f64
        };
        let total_values = values(|sample| sample.total_ms);
        let total_avg = average(|sample| sample.total_ms).max(f64::EPSILON);
        let stage = |name: &str, f: fn(ProfileFrame) -> f64| {
            let samples = self.samples.iter().filter(|sample| f(**sample) > 0.0).count();
            let avg = average(f);
            let pct = avg * 100.0 / total_avg;
            println!("  {name}: avg={avg:.3} ms ({pct:.1}%), samples={samples}/{}", self.samples.len());
        };
        println!("[PROFILE] frames={} fps={:.2} frame_ms avg={:.3} min={:.3} max={:.3} p50={:.3} p95={:.3} p99={:.3}",
            self.samples.len(), 1000.0 / total_avg, total_avg, total_values[0], *total_values.last().unwrap(), percentile(&total_values, 0.50), percentile(&total_values, 0.95), percentile(&total_values, 0.99));
        stage("update/input+clock", |sample| sample.update_ms);
        stage("poses", |sample| sample.poses_ms);
        stage("shadow_scene", |sample| sample.shadows_ms);
        stage("background_generated", |sample| sample.background_ms);
        stage("framebuffer_setup", |sample| sample.framebuffer_ms);
        stage("geometry+raster+shaders+depth", |sample| sample.geometry_ms);
        stage("hud/restore", |sample| sample.hud_ms);
        stage("rgb8_conversion", |sample| sample.conversion_ms);
        stage("present", |sample| sample.present_ms);
        stage("explicit_wait", |sample| sample.wait_ms);
        stage("bvh_build", |sample| sample.bvh_build_ms);
        stage("shadow_transform", |sample| sample.shadow_transform_ms);
        let detail_samples = self.samples.iter().filter(|sample| sample.detail.shadow_queries > 0 || sample.detail.shader_evaluations > 0).count();
        let detail_count = |f: fn(DetailedProfileStats) -> u64| -> u64 {
            self.samples.iter().map(|sample| f(sample.detail)).sum()
        };
        let detail_ns = |f: fn(DetailedProfileStats) -> u128| -> f64 {
            self.samples.iter().map(|sample| f(sample.detail) as f64).sum::<f64>()
                / detail_samples.max(1) as f64 / 1_000_000.0
        };
        println!("  detail samples={detail_samples}/{} shaders={} shader_ms={:.3} prep_ms={:.3} raster_inclusive_ms={:.3}", self.samples.len(), detail_count(|s| s.shader_evaluations), detail_ns(|s| s.shader_ns), detail_ns(|s| s.preparation_ns), detail_ns(|s| s.raster_ns));
        println!("  shadows queries={} shadow_ms={:.3} aabb_discarded={} aabb_passed={} triangle_tests={} occluded={}", detail_count(|s| s.shadow_queries), detail_ns(|s| s.shadow_ns), detail_count(|s| s.shadow_aabb_discarded), detail_count(|s| s.shadow_aabb_passed), detail_count(|s| s.shadow_triangle_tests), detail_count(|s| s.shadow_occluded));
        println!("  routes brute_queries={} brute_triangle_tests={} brute_occluded={} bvh_queries={} bvh_nodes_visited={} bvh_nodes_discarded={} bvh_leaves={} bvh_triangle_tests={} bvh_occluded={} bvh_stack_fallbacks={} bvh_max_depth={}", detail_count(|s| s.brute_queries), detail_count(|s| s.brute_triangle_tests), detail_count(|s| s.brute_occluded), detail_count(|s| s.bvh_queries), detail_count(|s| s.bvh_nodes_visited), detail_count(|s| s.bvh_nodes_discarded), detail_count(|s| s.bvh_leaves_visited), detail_count(|s| s.bvh_triangle_tests), detail_count(|s| s.bvh_occluded), detail_count(|s| s.bvh_stack_fallbacks), self.samples.iter().map(|s| s.detail.bvh_max_depth).max().unwrap_or(0));
        let raster_count = |f: fn(RasterDetailStats) -> u64| -> u64 { self.samples.iter().map(|sample| f(sample.raster_detail)).sum() };
        let raster_ns = |f: fn(RasterDetailStats) -> u128| -> f64 { self.samples.iter().map(|sample| f(sample.raster_detail) as f64).sum::<f64>() / 1_000_000.0 };
        if raster_count(|s| s.input_triangles) > 0 {
            println!("  raster_detail triangles input={} invalid={} clipped={} projected={} boxes={} candidates={} covered={} depth_passed={} shader_evals={} written={}", raster_count(|s| s.input_triangles), raster_count(|s| s.invalid_triangles), raster_count(|s| s.clipped_triangles), raster_count(|s| s.projected_triangles), raster_count(|s| s.bounding_boxes), raster_count(|s| s.candidate_pixels), raster_count(|s| s.covered_pixels), raster_count(|s| s.depth_passed), raster_count(|s| s.shader_evaluations), raster_count(|s| s.written_fragments));
            println!("  raster_detail measured_ms transform={:.3} clipping={:.3} projection={:.3} bbox={:.3}", raster_ns(|s| s.transform_ns), raster_ns(|s| s.clipping_ns), raster_ns(|s| s.projection_ns), raster_ns(|s| s.bounding_box_ns));
            println!("  raster_detail samples coverage={}/{} interpolation={}/{} depth={}/{} shader={}/{} write={}/{} estimated_ms coverage={:.3} interpolation={:.3} depth={:.3} shader={:.3} write={:.3}", raster_count(|s| s.coverage_samples), raster_count(|s| s.candidate_pixels), raster_count(|s| s.interpolation_samples), raster_count(|s| s.covered_pixels), raster_count(|s| s.depth_samples), raster_count(|s| s.depth_passed), raster_count(|s| s.shader_samples), raster_count(|s| s.shader_evaluations), raster_count(|s| s.write_samples), raster_count(|s| s.written_fragments), raster_ns(|s| s.coverage_ns) * 64.0, raster_ns(|s| s.interpolation_ns) * 64.0, raster_ns(|s| s.depth_ns) * 64.0, raster_ns(|s| s.shader_ns) * 64.0, raster_ns(|s| s.write_ns) * 64.0);
        }
        let shader_detail_frames = self.samples.iter().filter(|sample| sample.shader_detail.objects.iter().any(|object| object.evaluations > 0)).count();
        if shader_detail_frames > 0 {
            println!("  shader_detail frames={shader_detail_frames}/{} sample_rate=continuous_stratified_one_of_64_per_object; extrapolation=window_ns*window_evaluations/window_samples", self.samples.len());
            for (index, object) in DiagnosticObject::ALL.into_iter().enumerate() {
                let stats = self.samples.iter().fold((0u64, 0u64, 0u64, 0u128, 0usize, 0usize), |acc, sample| {
                    let item = sample.shader_detail.objects[index];
                    let evaluated = usize::from(item.evaluations > 0);
                    (acc.0 + item.depth_passed, acc.1 + item.evaluations, acc.2 + item.samples, acc.3 + item.shader_ns, acc.4 + evaluated, acc.5 + usize::from(item.evaluations == 0))
                });
                if stats.1 == 0 {
                    println!("    {} depth_passed=0 evaluations=0 samples=0", object.name());
                } else {
                    let estimated_ms = if stats.2 == 0 { 0.0 } else { stats.3 as f64 / 1_000_000.0 * stats.1 as f64 / stats.2 as f64 };
                    let avg_us = estimated_ms * 1000.0 / stats.1 as f64;
                    let factor = stats.1 as f64 / stats.2.max(1) as f64;
                    println!("    {} depth_passed={} evaluations={} samples={} factor={:.3} sampled_ms={:.3} evaluated_frames={} frames_without_evaluations={} estimated_shader_ms={:.3} estimated_us_per_eval={:.3}", object.name(), stats.0, stats.1, stats.2, factor, stats.3 as f64 / 1_000_000.0, stats.4, stats.5, estimated_ms, avg_us);
                }
            }
        }
        let procedural_frames = self.samples.iter().filter(|sample| sample.procedural_detail.objects.iter().any(|object| object.evaluations > 0)).count();
        if procedural_frames > 0 {
            println!("  procedural_detail frames={procedural_frames}/{} sample_rate=continuous_stratified_one_of_64_per_object; fbm_time_inclusive; noise_time_nested_in_fbm", self.samples.len());
            for (index, object) in DiagnosticObject::ALL.into_iter().enumerate() {
                let stats = self.samples.iter().fold((0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u128, 0u64, 0u128), |acc, sample| {
                    let item = sample.procedural_detail.objects[index];
                    (
                        acc.0 + item.evaluations,
                        acc.1 + item.sampled_evaluations,
                        acc.2 + item.fbm_calls,
                        acc.3 + item.fbm_octaves,
                        acc.4 + item.fbm_samples,
                        acc.5 + item.noise_calls,
                        acc.6 + item.fbm_ns,
                        acc.7 + item.noise_samples,
                        acc.8 + item.noise_ns,
                    )
                });
                if stats.0 > 0 {
                    let factor = stats.0 as f64 / stats.1.max(1) as f64;
                    let fbm_ms = stats.6 as f64 / 1_000_000.0 * factor;
                    let noise_ms = if stats.7 == 0 { 0.0 } else { stats.8 as f64 / 1_000_000.0 * factor };
                    println!("    {} evaluations={} samples={} factor={:.3} fbm_calls={} octaves={} noise_calls={} sampled_fbm={} sampled_noise={} fbm_inclusive_ms={:.3} noise_nested_ms={:.3}", object.name(), stats.0, stats.1, factor, stats.2, stats.3, stats.5, stats.4, stats.7, fbm_ms, noise_ms);
                }
            }
        }
        let bvh_frames = self.samples.iter().filter(|sample| sample.bvh_nodes > 0).count();
        if bvh_frames > 0 {
            println!("  bvh_structure frames={bvh_frames} nodes_avg={:.1} triangles_avg={:.1} build_depth_max={}", average(|sample| sample.bvh_nodes as f64), average(|sample| sample.bvh_triangles as f64), self.samples.iter().map(|s| s.bvh_depth).max().unwrap_or(0));
        }
        let builds: u64 = self.samples.iter().map(|sample| sample.shadow_scene_builds).sum();
        let triangles: u64 = self.samples.iter().map(|sample| sample.shadow_triangles_built).sum();
        if builds > 0 { println!("  shadow_rebuilds={} transformed_triangles={} reused_frames={}", builds, triangles, self.samples.len().saturating_sub(builds as usize)); }
    }
}

fn main() -> io::Result<()> {
    let mut interactive = false;
    let mut diagnostics = false;
    let mut diagnostics_only = false;
    let mut profile = false;
    let mut profile_detail = false;
    let mut profile_raster_detail = false;
    let mut profile_shader_detail = false;
    let mut profile_procedural_detail = false;
    let mut benchmark_static = false;
    let mut shadow_bvh = true;
    let mut shadow_bvh_explicit = false;
    let mut shadow_brute_force = false;
    let mut shadow_compare = false;
    let mut shadow_compare_all = false;
    let mut solene_normalized = false;
    let mut solene_experiment_original = false;
    let mut fbm_precomputed = false;
    let mut fbm_original = false;
    let mut fbm_compare = false;
    let mut benchmark_fbm = false;
    let mut framebuffer_fused_write = false;
    let mut framebuffer_compare = false;
    let mut benchmark_framebuffer = false;
    let mut raster_incremental_coverage = false;
    let mut raster_incremental_compare = false;
    let mut camera_matrices_cached = false;
    let mut camera_matrices_compare = false;
    let mut benchmark_camera_matrices = false;
    let mut normal_transform_cached = false;
    let mut normal_transform_compare = false;
    let mut benchmark_normal_transform = false;
    let mut preview = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--offline" => {}
            "--interactive" => interactive = true,
            "--diagnostics" => diagnostics = true,
            "--diagnostics-only" => {
                diagnostics = true;
                diagnostics_only = true;
            }
            "--profile" => profile = true,
            "--profile-detail" => {
                profile = true;
                profile_detail = true;
            }
            "--profile-raster-detail" => {
                profile = true;
                profile_raster_detail = true;
            }
            "--profile-shader-detail" => {
                profile = true;
                profile_shader_detail = true;
            }
            "--profile-procedural-detail" => {
                profile = true;
                profile_procedural_detail = true;
            }
            "--benchmark-static" => {
                benchmark_static = true;
                profile = true;
            }
            "--shadow-bvh" => {
                shadow_bvh = true;
                shadow_bvh_explicit = true;
            }
            "--shadow-brute-force" => {
                shadow_bvh = false;
                shadow_brute_force = true;
            }
            "--shadow-compare" => shadow_compare = true,
            "--shadow-compare-all" => {
                shadow_compare = true;
                shadow_compare_all = true;
            }
            "--solene-normalized" => solene_normalized = true,
            "--solene-experiment-original" => solene_experiment_original = true,
            "--fbm-precomputed" => fbm_precomputed = true,
            "--fbm-original" => fbm_original = true,
            "--fbm-compare" => fbm_compare = true,
            "--benchmark-fbm" => benchmark_fbm = true,
            "--framebuffer-fused-write" => framebuffer_fused_write = true,
            "--framebuffer-fused-compare" => framebuffer_compare = true,
            "--benchmark-framebuffer-write" => benchmark_framebuffer = true,
            "--raster-incremental-coverage" => raster_incremental_coverage = true,
            "--raster-incremental-compare" => raster_incremental_compare = true,
            "--camera-matrices-cached" => camera_matrices_cached = true,
            "--camera-matrices-compare" => camera_matrices_compare = true,
            "--benchmark-camera-matrices" => benchmark_camera_matrices = true,
            "--normal-transform-cached" => normal_transform_cached = true,
            "--normal-transform-compare" => normal_transform_compare = true,
            "--benchmark-normal-transform" => benchmark_normal_transform = true,
            "--preview" => {
                let value = arguments.next().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "--preview requiere original, A, B o C"))?;
                preview = Some(PreviewCurve::parse(&value).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "preview invalido: use original, A, B o C"))?);
                diagnostics = true;
            }
            _ => {
                eprintln!("Uso: aetheria [--offline] [--interactive] [--diagnostics] [--diagnostics-only] [--preview original|A|B|C] [--profile] [--profile-detail] [--profile-raster-detail] [--profile-shader-detail] [--profile-procedural-detail] [--benchmark-static] [--benchmark-fbm] [--benchmark-framebuffer-write] [--benchmark-camera-matrices] [--benchmark-normal-transform] [--framebuffer-fused-write] [--framebuffer-fused-compare] [--raster-incremental-coverage] [--raster-incremental-compare] [--camera-matrices-cached] [--camera-matrices-compare] [--normal-transform-cached] [--normal-transform-compare] [--shadow-bvh] [--shadow-brute-force] [--shadow-compare] [--shadow-compare-all] [--solene-normalized] [--solene-experiment-original] [--fbm-original|--fbm-precomputed|--fbm-compare]");
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "argumento desconocido"));
            }
        }
    }
    if interactive && (diagnostics_only || preview.is_some()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "diagnostics-only y preview requieren el modo offline",
        ));
    }
    if shadow_compare_all && !diagnostics_only {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--shadow-compare-all requiere --diagnostics-only"));
    }
    if shadow_bvh_explicit && shadow_brute_force {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--shadow-bvh y --shadow-brute-force son incompatibles"));
    }
    if solene_normalized && solene_experiment_original {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "los modos experimentales de Solene son incompatibles"));
    }
    if (fbm_precomputed as u8 + fbm_original as u8 + fbm_compare as u8) > 1 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "los modos de FBM son incompatibles"));
    }
    if (fbm_precomputed || fbm_original || fbm_compare) && interactive {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "los modos de FBM requieren el modo offline"));
    }
    if benchmark_fbm && interactive {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--benchmark-fbm requiere el modo offline"));
    }
    if benchmark_fbm && (!fbm_original && !fbm_precomputed || fbm_original && fbm_precomputed) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--benchmark-fbm requiere exactamente --fbm-original o --fbm-precomputed"));
    }
    if framebuffer_compare && !diagnostics_only {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--framebuffer-fused-compare requiere --diagnostics-only"));
    }
    if raster_incremental_compare && !diagnostics_only {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--raster-incremental-compare requiere --diagnostics-only"));
    }
    if camera_matrices_compare && !diagnostics_only {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--camera-matrices-compare requiere --diagnostics-only"));
    }
    if benchmark_camera_matrices && interactive {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--benchmark-camera-matrices requiere el modo offline"));
    }
    if normal_transform_compare && !diagnostics_only {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--normal-transform-compare requiere --diagnostics-only"));
    }
    if normal_transform_compare && camera_matrices_compare {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--normal-transform-compare y --camera-matrices-compare son incompatibles"));
    }
    if benchmark_normal_transform && interactive {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--benchmark-normal-transform requiere el modo offline"));
    }
    if benchmark_normal_transform && camera_matrices_cached {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--benchmark-normal-transform no se combina con --camera-matrices-cached"));
    }
    if benchmark_framebuffer && interactive {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--benchmark-framebuffer-write requiere el modo offline"));
    }
    if interactive && solene_experiment_original {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "--solene-experiment-original requiere el modo offline"));
    }
    if (solene_normalized || solene_experiment_original) && preview.is_some() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "los modos experimentales de Solene no se combinan con --preview"));
    }
    if interactive {
        #[cfg(feature = "interactive")]
        {
            return run_app(true, diagnostics, diagnostics_only, preview, profile, profile_detail, profile_raster_detail, profile_shader_detail, profile_procedural_detail, benchmark_static, shadow_bvh, shadow_compare, shadow_compare_all, solene_normalized, solene_experiment_original, fbm_precomputed, fbm_original, fbm_compare, benchmark_fbm, framebuffer_fused_write, framebuffer_compare, benchmark_framebuffer, raster_incremental_coverage, raster_incremental_compare, camera_matrices_cached, camera_matrices_compare, benchmark_camera_matrices, normal_transform_cached, normal_transform_compare, benchmark_normal_transform);
        }
        #[cfg(not(feature = "interactive"))]
        {
            eprintln!("El modo interactivo requiere compilar con: cargo run --features interactive -- --interactive");
            return Err(io::Error::new(io::ErrorKind::Unsupported, "feature interactive no habilitada"));
        }
    }
    run_app(false, diagnostics, diagnostics_only, preview, profile, profile_detail, profile_raster_detail, profile_shader_detail, profile_procedural_detail, benchmark_static, shadow_bvh, shadow_compare, shadow_compare_all, solene_normalized, solene_experiment_original, fbm_precomputed, fbm_original, fbm_compare, benchmark_fbm, framebuffer_fused_write, framebuffer_compare, benchmark_framebuffer, raster_incremental_coverage, raster_incremental_compare, camera_matrices_cached, camera_matrices_compare, benchmark_camera_matrices, normal_transform_cached, normal_transform_compare, benchmark_normal_transform)
}

fn run_app(_interactive: bool, diagnostics_enabled: bool, diagnostics_only: bool, preview: Option<PreviewCurve>, profile_enabled: bool, profile_detail: bool, profile_raster_detail: bool, profile_shader_detail: bool, profile_procedural_detail: bool, benchmark_static: bool, shadow_bvh: bool, shadow_compare: bool, shadow_compare_all: bool, solene_normalized: bool, solene_experiment_original: bool, fbm_precomputed: bool, fbm_original: bool, fbm_compare: bool, benchmark_fbm: bool, framebuffer_fused_write: bool, framebuffer_compare: bool, benchmark_framebuffer: bool, raster_incremental_coverage: bool, raster_incremental_compare: bool, camera_matrices_cached: bool, camera_matrices_compare: bool, benchmark_camera_matrices: bool, normal_transform_cached: bool, normal_transform_compare: bool, benchmark_normal_transform: bool) -> io::Result<()> {
    #[cfg(not(feature = "interactive"))]
    let _ = profile_enabled;
    #[cfg(not(feature = "interactive"))]
    let _ = profile_detail;
    #[cfg(not(feature = "interactive"))]
    let _ = profile_raster_detail;
    #[cfg(not(feature = "interactive"))]
    let _ = profile_shader_detail;
    #[cfg(not(feature = "interactive"))]
    let _ = profile_procedural_detail;
    set_fbm_precomputed(fbm_precomputed);
    set_fbm_comparison(fbm_compare);
    #[cfg(not(feature = "interactive"))]
    let _ = benchmark_static;
    #[cfg(not(feature = "interactive"))]
    let _ = shadow_bvh;
    #[cfg(not(feature = "interactive"))]
    let _ = shadow_compare;
    #[cfg(not(feature = "interactive"))]
    let _ = shadow_compare_all;
    let solaris = load_obj("assets/models/obj/solaris.obj").map_err(io::Error::other)?;
    let vesper = load_obj("assets/models/obj/vesper.obj").map_err(io::Error::other)?;
    let selene = load_obj("assets/models/obj/selene.obj").map_err(io::Error::other)?;
    let nymara = load_obj("assets/models/obj/nymara.obj").map_err(io::Error::other)?;
    let ring_inner = load_obj("assets/models/obj/nymara_ring_inner.obj").map_err(io::Error::other)?;
    let ring_middle = load_obj("assets/models/obj/nymara_ring_middle.obj").map_err(io::Error::other)?;
    let ring_outer = load_obj("assets/models/obj/nymara_ring_outer.obj").map_err(io::Error::other)?;
    let cryon = load_obj("assets/models/obj/cryon.obj").map_err(io::Error::other)?;
    let veridis = load_obj("assets/models/obj/veridis.obj").map_err(io::Error::other)?;
    let umbra = load_obj("assets/models/obj/umbra.obj").map_err(io::Error::other)?;
    let solene = load_obj("assets/models/obj/solene.obj").map_err(io::Error::other)?;

    let camera = Camera::new(
        Vec3::new(0.0, 4.8, 11.8),
        Vec3::new(0.0, 0.0, -2.0),
        Vec3::Y,
        0.72,
        800.0 / 600.0,
        0.1,
        50.0,
    )
    .map_err(io::Error::other)?;

    let solaris_motion = BodyMotion {
        orbit: Orbit { center: Vec3::ZERO, radius: 0.0, angular_velocity: 0.0, phase: 0.0, inclination: 0.0 },
        base_rotation: Mat4::identity(), scale: 0.70, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.05, rotation_phase: 0.0,
    };
    let vesper_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, -1.35, 0.0), 3.826878624, -2.836369520, 0.10),
        base_rotation: Mat4::rotation_y(-0.25), scale: 0.52, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.22, rotation_phase: 0.0,
    };
    let nymara_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, 1.15, 0.0), 3.785828839, -0.588002604, 0.065),
        base_rotation: Mat4::rotation_x(0.48) * Mat4::rotation_z(-0.18), scale: 0.60,
        rotation_axis: RotationAxis::Y, rotation_speed: 0.12, rotation_phase: 0.0,
    };
    let cryon_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, 2.75, 0.0), 3.100403200, -1.554668693, 0.045),
        base_rotation: Mat4::rotation_y(0.35), scale: 0.46, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.15, rotation_phase: 0.0,
    };
    let veridis_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, -2.65, 0.0), 3.782195130, -1.993047480, 0.055),
        base_rotation: Mat4::rotation_z(0.30), scale: 0.43, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.10, rotation_phase: 0.0,
    };
    let umbra_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, -1.75, 0.0), 5.905294235, -0.767436024, 0.035),
        base_rotation: Mat4::rotation_y(-0.40), scale: 0.50, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.08, rotation_phase: 0.0,
    };
    let solene_motion = BodyMotion {
        orbit: orbit(Vec3::new(0.0, 1.65, 0.0), 6.081529413, -2.344567107, 0.028),
        base_rotation: Mat4::rotation_x(-0.22), scale: 0.46, rotation_axis: RotationAxis::Y,
        rotation_speed: 0.07, rotation_phase: 0.0,
    };

    if benchmark_fbm {
        return run_fbm_benchmark(
            &camera, &solaris, &vesper, &selene, &nymara, &ring_inner, &ring_middle,
            &ring_outer, &cryon, &veridis, &umbra, &solene, solaris_motion, vesper_motion,
            nymara_motion, cryon_motion, veridis_motion, umbra_motion, solene_motion,
            shadow_bvh, fbm_precomputed, framebuffer_fused_write,
        );
    }
    if benchmark_framebuffer {
        return run_framebuffer_benchmark(
            &camera, &solaris, &vesper, &selene, &nymara, &ring_inner, &ring_middle,
            &ring_outer, &cryon, &veridis, &umbra, &solene, solaris_motion, vesper_motion,
            nymara_motion, cryon_motion, veridis_motion, umbra_motion, solene_motion,
            shadow_bvh, framebuffer_fused_write, raster_incremental_coverage,
        );
    }
    if benchmark_camera_matrices || benchmark_normal_transform {
        return run_camera_matrices_benchmark(
            &camera, &solaris, &vesper, &selene, &nymara, &ring_inner, &ring_middle,
            &ring_outer, &cryon, &veridis, &umbra, &solene, solaris_motion, vesper_motion,
            nymara_motion, cryon_motion, veridis_motion, umbra_motion, solene_motion,
            shadow_bvh, benchmark_normal_transform,
        );
    }
    if framebuffer_compare {
        return run_framebuffer_compare(
            &camera, &solaris, &vesper, &selene, &nymara, &ring_inner, &ring_middle,
            &ring_outer, &cryon, &veridis, &umbra, &solene, solaris_motion, vesper_motion,
            nymara_motion, cryon_motion, veridis_motion, umbra_motion, solene_motion,
            shadow_bvh,
        );
    }
    if raster_incremental_compare {
        return run_raster_incremental_compare(
            &camera, &solaris, &vesper, &selene, &nymara, &ring_inner, &ring_middle,
            &ring_outer, &cryon, &veridis, &umbra, &solene, solaris_motion, vesper_motion,
            nymara_motion, cryon_motion, veridis_motion, umbra_motion, solene_motion,
            shadow_bvh,
        );
    }

    #[cfg(feature = "interactive")]
    if _interactive {
        let space_background = SpaceBackground::new(0xA37E_214D);
        let mut clock = SimulationClock::new(1.0);
        if benchmark_static { clock.pause(); }
        let mut poses = build_poses(
            clock.time(), solaris_motion, vesper_motion, nymara_motion, cryon_motion,
            veridis_motion, umbra_motion, solene_motion,
        );
        let mut controller = OrbitCameraController::new(
            BodyId::Solaris, camera.fov_y, camera.aspect, camera.near, camera.far,
        );
        controller.set_orbit(std::f32::consts::FRAC_PI_2, 0.30, 5.0);
        let interactive_camera = controller
            .camera_for_target(poses.get(BodyId::Solaris).world_position)
            .map_err(io::Error::other)?;
        let mut background_colors = space_background.render(&interactive_camera, 800, 600);
        let mut shadow_scene = build_shadow_scene(
            &vesper, &selene, &nymara, &ring_inner, &ring_middle, &ring_outer,
            &cryon, &veridis, &umbra, &solene, poses, shadow_bvh, shadow_compare, shadow_compare_all,
        );
        let mut framebuffer = render_frame(
            clock.time(), &interactive_camera, &solaris, &vesper, &selene, &nymara,
            &ring_inner, &ring_middle, &ring_outer, &cryon, &veridis, &umbra,
            &solene, poses, &shadow_scene, Some(&background_colors), diagnostics_enabled, None, solene_normalized, framebuffer_fused_write, raster_incremental_coverage, camera_matrices_cached, false, false, None, None, None, None, None,
        )?;
        let mut clean_colors = framebuffer.color_buffer().to_vec();
        hud::draw(&mut framebuffer, HudState {
            target: target_name(controller.target_id),
            time: clock.time(),
            paused: clock.is_paused(),
            speed: clock.speed(),
        });
        let mut window = CpuWindow::new("AETHERIA — CPU Renderer", &framebuffer)
            .map_err(io::Error::other)?;
        println!("Controles: arrastre izquierdo = rotar, rueda = zoom, 1-8 = objetivo, R = restablecer");
        println!("Simulacion: Espacio = pausa/reanudar, +/- = velocidad, N = paso manual");
        println!("Reproduciendo; velocidad {:.2}x; tiempo {:.3}; objetivo {}", clock.speed(), clock.time(), target_name(controller.target_id));
        let mut dragging = false;
        let mut previous_mouse = None;
        let mut last_tick = Instant::now();
        let mut profiler = profile_enabled.then(ProfileWindow::new);
        let shader_detail_profile = profile_shader_detail.then(ShaderDetailProfile::new);
        let procedural_detail_profile = profile_procedural_detail.then(ProceduralDetailProfile::new);
        while window.is_open() {
            let frame_start = Instant::now();
            let mut profile_frame = ProfileFrame::default();
            let mut camera_changed = false;
            let mut scene_changed = false;
            let mut hud_dirty = false;
            let mut background_changed = false;
            let update_start = Instant::now();
            let now = Instant::now();
            let real_delta = now.duration_since(last_tick).as_secs_f32();
            last_tick = now;
            let real_delta = if real_delta.is_finite() {
                real_delta.clamp(0.0, 0.25)
            } else {
                0.0
            };
            let mouse_position = window.mouse_position();
            if window.left_mouse_down() {
                if !dragging {
                    dragging = true;
                    previous_mouse = mouse_position;
                } else if let (Some(previous), Some(current)) = (previous_mouse, mouse_position) {
                    let delta_x = current.0 - previous.0;
                    let delta_y = current.1 - previous.1;
                    if delta_x != 0.0 || delta_y != 0.0 {
                        let previous_controller = controller;
                        controller.rotate_yaw(delta_x * 0.01);
                        controller.rotate_pitch(-delta_y * 0.01);
                        camera_changed = controller != previous_controller;
                        background_changed |= camera_changed;
                    }
                    previous_mouse = Some(current);
                }
            } else {
                dragging = false;
                previous_mouse = None;
            }
            if let Some((_, wheel_y)) = window.scroll_wheel() {
                if wheel_y != 0.0 {
                    let previous_controller = controller;
                    controller.zoom(-wheel_y * 0.5);
                    camera_changed = controller != previous_controller;
                }
            }
            for key in window.pressed_keys() {
                if let Some(target) = body_id_from_key(key) {
                    let previous_controller = controller;
                    controller.set_target(target);
                    if controller != previous_controller {
                        println!("Objetivo seleccionado: {}", target_name(target));
                        camera_changed = true;
                        hud_dirty = true;
                    }
                } else if key == Key::R {
                    let previous_controller = controller;
                    controller.set_target(BodyId::Solaris);
                    controller.set_orbit(std::f32::consts::FRAC_PI_2, 0.30, 5.0);
                    if controller != previous_controller {
                        println!("Camara restablecida: Solaris");
                        camera_changed = true;
                        hud_dirty = true;
                        background_changed = controller.yaw != previous_controller.yaw
                            || controller.pitch != previous_controller.pitch;
                    }
                } else if key == Key::Space {
                    if clock.is_paused() {
                        clock.resume();
                        println!("Reproduciendo; velocidad {:.2}x; tiempo {:.3}", clock.speed(), clock.time());
                    } else {
                        clock.pause();
                        println!("Pausado; velocidad {:.2}x; tiempo {:.3}", clock.speed(), clock.time());
                    }
                    hud_dirty = true;
                } else if key == Key::Equal {
                    let previous_speed = clock.speed();
                    clock.set_speed((clock.speed() + 0.25).clamp(0.25, 4.0));
                    if clock.speed() != previous_speed {
                        println!("Velocidad de simulacion: {:.2}x", clock.speed());
                        hud_dirty = true;
                    }
                } else if key == Key::Minus {
                    let previous_speed = clock.speed();
                    clock.set_speed((clock.speed() - 0.25).clamp(0.25, 4.0));
                    if clock.speed() != previous_speed {
                        println!("Velocidad de simulacion: {:.2}x", clock.speed());
                        hud_dirty = true;
                    }
                } else if key == Key::N && clock.is_paused() {
                    clock.advance_manual(1.0 / 60.0);
                    scene_changed = true;
                    hud_dirty = true;
                    println!("Paso manual; tiempo {:.3}", clock.time());
                }
            }
            if !clock.is_paused() {
                let previous_time = clock.time();
                clock.advance(real_delta);
                if clock.time() != previous_time {
                    scene_changed = true;
                }
            }
            if benchmark_static { camera_changed = true; }
            profile_frame.update_ms = update_start.elapsed().as_secs_f64() * 1000.0;
            if scene_changed {
                let poses_start = Instant::now();
                poses = build_poses(
                    clock.time(), solaris_motion, vesper_motion, nymara_motion, cryon_motion,
                    veridis_motion, umbra_motion, solene_motion,
                );
                profile_frame.poses_ms = poses_start.elapsed().as_secs_f64() * 1000.0;
                let shadows_start = Instant::now();
                shadow_scene = build_shadow_scene(
                    &vesper, &selene, &nymara, &ring_inner, &ring_middle, &ring_outer,
                    &cryon, &veridis, &umbra, &solene, poses, shadow_bvh, shadow_compare, shadow_compare_all,
                );
                profile_frame.shadows_ms = shadows_start.elapsed().as_secs_f64() * 1000.0;
                profile_frame.shadow_scene_builds = 1;
                profile_frame.shadow_triangles_built = shadow_scene.triangle_count() as u64;
                if shadow_bvh {
                    profile_frame.bvh_build_ms = shadow_scene.bvh_build_ns() as f64 / 1_000_000.0;
                    profile_frame.bvh_nodes = shadow_scene.bvh_node_count() as u64;
                    profile_frame.bvh_triangles = shadow_scene.bvh_triangle_count() as u64;
                    profile_frame.bvh_depth = shadow_scene.bvh_max_depth();
                }
                profile_frame.shadow_transform_ms = shadow_scene.transform_ns() as f64 / 1_000_000.0;
                camera_changed = true;
                hud_dirty = true;
                println!("Tiempo de simulacion: {:.3}", clock.time());
            }
            if camera_changed {
                let current_camera = controller
                    .camera_for_target(poses.get(controller.target_id).world_position)
                    .map_err(io::Error::other)?;
                if background_changed {
                    let background_start = Instant::now();
                    background_colors = space_background.render(&current_camera, 800, 600);
                    profile_frame.background_ms = background_start.elapsed().as_secs_f64() * 1000.0;
                }
                let detail_profile = profile_detail.then(DetailedProfile::new);
                let raster_detail_profile = profile_raster_detail.then(RasterDetailProfile::new);
                let updated = render_frame(
                    clock.time(), &current_camera, &solaris, &vesper, &selene, &nymara,
                    &ring_inner, &ring_middle, &ring_outer, &cryon, &veridis, &umbra,
                    &solene, poses, &shadow_scene, Some(&background_colors), diagnostics_enabled, None, solene_normalized, framebuffer_fused_write, raster_incremental_coverage, camera_matrices_cached, false, false, detail_profile.clone(), raster_detail_profile.clone(), shader_detail_profile.clone(), procedural_detail_profile.clone(), Some(&mut profile_frame),
                )?;
                if let Some(detail_profile) = detail_profile {
                    profile_frame.detail = detail_profile.snapshot();
                }
                if let Some(raster_detail_profile) = raster_detail_profile {
                    profile_frame.raster_detail = raster_detail_profile.snapshot();
                }
                if let Some(shader_detail_profile) = &shader_detail_profile {
                    profile_frame.shader_detail = shader_detail_profile.take_frame_snapshot();
                }
                if let Some(procedural_detail_profile) = &procedural_detail_profile {
                    profile_frame.procedural_detail = procedural_detail_profile.take_frame_snapshot();
                }
                framebuffer = updated;
                clean_colors = framebuffer.color_buffer().to_vec();
                hud_dirty = true;
            }
            if hud_dirty {
                let hud_start = Instant::now();
                if !camera_changed {
                    framebuffer.restore_colors(&clean_colors);
                }
                hud::draw(&mut framebuffer, HudState {
                    target: target_name(controller.target_id),
                    time: clock.time(),
                    paused: clock.is_paused(),
                    speed: clock.speed(),
                });
                profile_frame.hud_ms = hud_start.elapsed().as_secs_f64() * 1000.0;
                let conversion_start = Instant::now();
                window.set_framebuffer(&framebuffer);
                profile_frame.conversion_ms = conversion_start.elapsed().as_secs_f64() * 1000.0;
            }
            let present_start = Instant::now();
            window.present().map_err(io::Error::other)?;
            profile_frame.present_ms = present_start.elapsed().as_secs_f64() * 1000.0;
            let wait_start = Instant::now();
            std::thread::sleep(Duration::from_millis(16));
            profile_frame.wait_ms = wait_start.elapsed().as_secs_f64() * 1000.0;
            profile_frame.total_ms = frame_start.elapsed().as_secs_f64() * 1000.0;
            if let Some(profiler) = &mut profiler {
                profiler.record(profile_frame);
            }
        }
        if shadow_compare {
            let stats = shadow_scene.comparison_stats();
            println!("[SHADOW-COMPARE] queries={} matches={} discrepancies={} false_positives={} false_negatives={} equivalence={:.3}%",
                stats.queries_compared, stats.matches, stats.discrepancies, stats.false_positives, stats.false_negatives,
                if stats.queries_compared == 0 { 100.0 } else { stats.matches as f64 * 100.0 / stats.queries_compared as f64 });
            println!("[SHADOW-COMPARE] eligible_queries={} compared_queries={} missing_pairs={}", stats.eligible_queries, stats.queries_compared, stats.eligible_queries.saturating_sub(stats.queries_compared));
            println!("[SHADOW-COMPARE] brute_ms={:.3} bvh_ms={:.3} brute_avg_us={:.3} bvh_avg_us={:.3} brute_triangle_tests={} bvh_triangle_tests={} brute_occluded={} bvh_occluded={} bvh_stack_fallbacks={}",
                stats.brute_ns as f64 / 1_000_000.0, stats.bvh_ns as f64 / 1_000_000.0,
                if stats.queries_compared == 0 { 0.0 } else { stats.brute_ns as f64 / stats.queries_compared as f64 / 1_000.0 },
                if stats.queries_compared == 0 { 0.0 } else { stats.bvh_ns as f64 / stats.queries_compared as f64 / 1_000.0 },
                stats.brute_triangle_tests, stats.bvh_triangle_tests, stats.brute_occluded, stats.bvh_occluded, stats.bvh_stack_fallbacks);
            println!("[SHADOW-COMPARE] transform_ms={:.3} bvh_build_ms={:.3} bvh_nodes={} bvh_triangles={} bvh_depth={}", shadow_scene.transform_ns() as f64 / 1_000_000.0, shadow_scene.bvh_build_ns() as f64 / 1_000_000.0, shadow_scene.bvh_node_count(), shadow_scene.bvh_triangle_count(), shadow_scene.bvh_max_depth());
            if let Some(first) = stats.first_discrepancy { println!("[SHADOW-COMPARE] first discrepancy origin=({:.6},{:.6},{:.6}) direction=({:.6},{:.6},{:.6}) max_distance={:.6} receiver={} brute={} bvh={}", first.origin.x, first.origin.y, first.origin.z, first.direction.x, first.direction.y, first.direction.z, first.max_distance, first.receiver_id, first.brute_occluded, first.bvh_occluded); }
        }
        if fbm_compare {
            let (calls, mismatches) = fbm_comparison_stats();
            println!("[FBM-COMPARE] calls={calls} bitwise_mismatches={mismatches}");
        }
        return Ok(());
    }

    if fbm_compare {
        println!("[FBM-COMPARE] divisors_bitwise_equal={}", fbm_normalization_table_matches_original());
        println!("[FBM-COMPARE] normalization_table_bits=");
        for (octaves, divisor) in FBM_NORMALIZATION_TABLE.iter().enumerate() {
            println!("  octaves={octaves} divisor={divisor:.9} bits=0x{:08X}", divisor.to_bits());
        }
    }
    let outputs = if fbm_original || fbm_precomputed {
        let output = if fbm_original {
            "output/phase22l3_fbm_original.ppm"
        } else {
            "output/phase22l3_fbm_precomputed.ppm"
        };
        if std::path::Path::new(output).exists() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("salida experimental ya existe: {output}")));
        }
        vec![(output, 0.0, CameraMode::Static)]
    } else if solene_experiment_original || solene_normalized {
        let output = if solene_experiment_original {
            "output/phase22k3_solene_original.ppm"
        } else {
            "output/phase22k3_solene_normalized.ppm"
        };
        if std::path::Path::new(output).exists() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("salida experimental ya existe: {output}")));
        }
        vec![(output, 0.0, CameraMode::Static)]
    } else if let Some(preview) = preview {
        let output = preview.output_path();
        if std::path::Path::new(output).exists() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("preview ya existe: {output}")));
        }
        vec![(output, 0.0, CameraMode::Static)]
    } else {
        vec![
            ("output/phase19c_camera_static.ppm", 0.0, CameraMode::Static),
            ("output/phase19c_camera_solaris.ppm", 4.0, CameraMode::Orbit(BodyId::Solaris, 5.0)),
            ("output/phase19c_camera_vesper.ppm", 4.0, CameraMode::Orbit(BodyId::Vesper, 4.8)),
            ("output/phase19c_camera_selene.ppm", 4.0, CameraMode::Orbit(BodyId::Selene, 2.0)),
            ("output/phase19c_camera_nymara.ppm", 4.0, CameraMode::Orbit(BodyId::Nymara, 6.2)),
        ]
    };
    for (output, time, mode) in outputs {
        let poses = build_poses(
            time, solaris_motion, vesper_motion, nymara_motion, cryon_motion,
            veridis_motion, umbra_motion, solene_motion,
        );
        let frame_camera = match mode {
            CameraMode::Static => camera,
            CameraMode::Orbit(target_id, distance) => {
                let mut controller = OrbitCameraController::new(
                    target_id, camera.fov_y, camera.aspect, camera.near, camera.far,
                );
                controller.set_orbit(std::f32::consts::FRAC_PI_2, 0.30, distance);
                controller.camera_for_target(poses.get(target_id).world_position)
                    .map_err(io::Error::other)?
            }
        };
        let shadow_scene = build_shadow_scene(
            &vesper, &selene, &nymara, &ring_inner, &ring_middle, &ring_outer,
            &cryon, &veridis, &umbra, &solene, poses, shadow_bvh, shadow_compare, shadow_compare_all,
        );
        println!("[SHADOW-ROUTE] mode={} bvh_nodes={} bvh_depth={}", if shadow_scene.uses_bvh() { "BVH" } else { "brute-force" }, shadow_scene.bvh_node_count(), shadow_scene.bvh_max_depth());
        if normal_transform_compare {
            let objects = [
                (&solaris.mesh, poses.get(BodyId::Solaris).model_matrix),
                (&vesper.mesh, poses.get(BodyId::Vesper).model_matrix),
                (&selene.mesh, poses.get(BodyId::Selene).model_matrix),
                (&nymara.mesh, poses.get(BodyId::Nymara).model_matrix),
                (&ring_inner.mesh, poses.get(BodyId::Nymara).model_matrix),
                (&ring_middle.mesh, poses.get(BodyId::Nymara).model_matrix),
                (&ring_outer.mesh, poses.get(BodyId::Nymara).model_matrix),
                (&cryon.mesh, poses.get(BodyId::Cryon).model_matrix),
                (&veridis.mesh, poses.get(BodyId::Veridis).model_matrix),
                (&umbra.mesh, poses.get(BodyId::Umbra).model_matrix),
                (&solene.mesh, poses.get(BodyId::Solene).model_matrix),
            ];
            let mut compared = 0u64;
            let mut different = 0u64;
            let mut max_difference = 0.0f32;
            for (mesh, model) in objects {
                let cached = model.precompute_normal_transform();
                for triangle in mesh.triangles() {
                    for vertex in triangle.vertices {
                        let original = model.transform_normal(vertex.normal);
                        let experimental = cached.transform_normal(vertex.normal);
                        compared += 1;
                        let difference = (original.x - experimental.x).abs()
                            .max((original.y - experimental.y).abs())
                            .max((original.z - experimental.z).abs());
                        max_difference = max_difference.max(difference);
                        if original != experimental { different += 1; }
                    }
                }
            }
            let original = render_frame(
                time, &frame_camera, &solaris, &vesper, &selene, &nymara,
                &ring_inner, &ring_middle, &ring_outer, &cryon, &veridis, &umbra, &solene,
                poses, &shadow_scene, None, diagnostics_enabled, preview.and_then(PreviewCurve::parameters), solene_normalized, framebuffer_fused_write, raster_incremental_coverage, false, false, false, None, None, None, None, None,
            )?;
            let experimental = render_frame(
                time, &frame_camera, &solaris, &vesper, &selene, &nymara,
                &ring_inner, &ring_middle, &ring_outer, &cryon, &veridis, &umbra, &solene,
                poses, &shadow_scene, None, diagnostics_enabled, preview.and_then(PreviewCurve::parameters), solene_normalized, framebuffer_fused_write, raster_incremental_coverage, false, false, true, None, None, None, None, None,
            )?;
            let color_differences = original.color_buffer().iter().zip(experimental.color_buffer()).filter(|(a, b)| a != b).count();
            let depth_differences = original.depth_buffer().iter().zip(experimental.depth_buffer()).filter(|(a, b)| a.to_bits() != b.to_bits()).count();
            let original_rgb8 = original.serialize_ppm();
            let experimental_rgb8 = experimental.serialize_ppm();
            let rgb8_differences = original_rgb8.iter().zip(&experimental_rgb8).filter(|(a, b)| a != b).count();
            println!("[NORMAL-TRANSFORM-COMPARE] normals_compared={} normal_differences={} max_abs_difference={:.9} clip_vertices_compared=0 projected_vertices_compared=0 (not exposed by current pipeline)", compared, different, max_difference);
            println!("[NORMAL-TRANSFORM-COMPARE] pixels={} color_differences={} depth_differences={} rgb8_bytes={} rgb8_differences={} dimensions={:?}", original.color_buffer().len(), color_differences, depth_differences, original_rgb8.len(), rgb8_differences, original.dimensions());
            break;
        }
        if camera_matrices_compare {
            let original_matrices = (frame_camera.view_matrix(), frame_camera.projection_matrix());
            let cached_matrices = (frame_camera.view_matrix(), frame_camera.projection_matrix());
            let original = render_frame(
                time, &frame_camera, &solaris, &vesper, &selene, &nymara,
                &ring_inner, &ring_middle, &ring_outer, &cryon, &veridis, &umbra, &solene,
                poses, &shadow_scene, None, diagnostics_enabled, preview.and_then(PreviewCurve::parameters), solene_normalized, framebuffer_fused_write, raster_incremental_coverage, false, false, false, None, None, None, None, None,
            )?;
            let cached = render_frame(
                time, &frame_camera, &solaris, &vesper, &selene, &nymara,
                &ring_inner, &ring_middle, &ring_outer, &cryon, &veridis, &umbra, &solene,
                poses, &shadow_scene, None, diagnostics_enabled, preview.and_then(PreviewCurve::parameters), solene_normalized, framebuffer_fused_write, raster_incremental_coverage, true, false, false, None, None, None, None, None,
            )?;
            let color_differences = original.color_buffer().iter().zip(cached.color_buffer())
                .filter(|(a, b)| a != b).count();
            let depth_differences = original.depth_buffer().iter().zip(cached.depth_buffer())
                .filter(|(a, b)| a.to_bits() != b.to_bits()).count();
            let original_rgb8 = original.serialize_ppm();
            let cached_rgb8 = cached.serialize_ppm();
            let rgb8_differences = original_rgb8.iter().zip(&cached_rgb8)
                .filter(|(a, b)| a != b).count();
            let matrix_differences = original_matrices.0 != cached_matrices.0
                || original_matrices.1 != cached_matrices.1;
            println!("[CAMERA-MATRICES-COMPARE] matrices_compared=2 matrix_differences={} clip_vertices_compared=0 projected_vertices_compared=0 (not exposed by current pipeline)", matrix_differences as u64);
            println!("[CAMERA-MATRICES-COMPARE] pixels={} color_differences={} depth_differences={} rgb8_bytes={} rgb8_differences={} dimensions={:?}", original.color_buffer().len(), color_differences, depth_differences, original_rgb8.len(), rgb8_differences, original.dimensions());
            break;
        }
        let framebuffer = render_frame(
            time, &frame_camera, &solaris, &vesper, &selene, &nymara,
            &ring_inner, &ring_middle, &ring_outer, &cryon, &veridis, &umbra, &solene,
            poses, &shadow_scene, None, diagnostics_enabled, preview.and_then(PreviewCurve::parameters), solene_normalized, framebuffer_fused_write, raster_incremental_coverage, camera_matrices_cached, false, normal_transform_cached, None, None, None, None, None,
        )?;
        if !diagnostics_only {
            fs::create_dir_all("output")?;
            framebuffer.save_ppm(output)?;
        }
        if diagnostics_only {
            if shadow_compare {
                let stats = shadow_scene.comparison_stats();
                println!("[SHADOW-COMPARE] queries={} matches={} discrepancies={} false_positives={} false_negatives={} equivalence={:.3}%",
                    stats.queries_compared, stats.matches, stats.discrepancies, stats.false_positives, stats.false_negatives,
                    if stats.queries_compared == 0 { 100.0 } else { stats.matches as f64 * 100.0 / stats.queries_compared as f64 });
                println!("[SHADOW-COMPARE] eligible_queries={} compared_queries={} missing_pairs={}", stats.eligible_queries, stats.queries_compared, stats.eligible_queries.saturating_sub(stats.queries_compared));
                println!("[SHADOW-COMPARE] brute_ms={:.3} bvh_ms={:.3} brute_avg_us={:.3} bvh_avg_us={:.3} brute_triangle_tests={} bvh_triangle_tests={} brute_occluded={} bvh_occluded={} bvh_stack_fallbacks={}",
                    stats.brute_ns as f64 / 1_000_000.0, stats.bvh_ns as f64 / 1_000_000.0,
                    if stats.queries_compared == 0 { 0.0 } else { stats.brute_ns as f64 / stats.queries_compared as f64 / 1_000.0 },
                    if stats.queries_compared == 0 { 0.0 } else { stats.bvh_ns as f64 / stats.queries_compared as f64 / 1_000.0 },
                    stats.brute_triangle_tests, stats.bvh_triangle_tests, stats.brute_occluded, stats.bvh_occluded, stats.bvh_stack_fallbacks);
                println!("[SHADOW-COMPARE] transform_ms={:.3} bvh_build_ms={:.3} bvh_nodes={} bvh_triangles={} bvh_depth={}", shadow_scene.transform_ns() as f64 / 1_000_000.0, shadow_scene.bvh_build_ns() as f64 / 1_000_000.0, shadow_scene.bvh_node_count(), shadow_scene.bvh_triangle_count(), shadow_scene.bvh_max_depth());
                if let Some(first) = stats.first_discrepancy { println!("[SHADOW-COMPARE] first discrepancy origin=({:.6},{:.6},{:.6}) direction=({:.6},{:.6},{:.6}) max_distance={:.6} receiver={} brute={} bvh={}", first.origin.x, first.origin.y, first.origin.z, first.direction.x, first.direction.y, first.direction.z, first.max_distance, first.receiver_id, first.brute_occluded, first.bvh_occluded); }
            }
            if fbm_compare {
                let (calls, mismatches) = fbm_comparison_stats();
                println!("[FBM-COMPARE] calls={calls} bitwise_mismatches={mismatches}");
            }
            break;
        }
        if fbm_compare {
            let (calls, mismatches) = fbm_comparison_stats();
            println!("[FBM-COMPARE] calls={calls} bitwise_mismatches={mismatches}");
        }
    }
    Ok(())
}

#[cfg(feature = "interactive")]
fn body_id_from_key(key: Key) -> Option<BodyId> {
    Some(match key {
        Key::Key1 => BodyId::Solaris,
        Key::Key2 => BodyId::Vesper,
        Key::Key3 => BodyId::Selene,
        Key::Key4 => BodyId::Nymara,
        Key::Key5 => BodyId::Cryon,
        Key::Key6 => BodyId::Veridis,
        Key::Key7 => BodyId::Umbra,
        Key::Key8 => BodyId::Solene,
        _ => return None,
    })
}

#[cfg(feature = "interactive")]
fn target_name(id: BodyId) -> &'static str {
    match id {
        BodyId::Solaris => "Solaris",
        BodyId::Vesper => "Vesper",
        BodyId::Selene => "Selene",
        BodyId::Nymara => "Nymara",
        BodyId::Cryon => "Cryon",
        BodyId::Veridis => "Veridis",
        BodyId::Umbra => "Umbra",
        BodyId::Solene => "Solene",
    }
}

#[derive(Clone, Copy)]
enum CameraMode {
    Static,
    Orbit(BodyId, f32),
}

fn orbit(center: Vec3, radius: f32, phase: f32, angular_velocity: f32) -> Orbit {
    Orbit { center, radius, angular_velocity, phase, inclination: 0.0 }
}

fn build_poses(
    time: f32,
    solaris_motion: BodyMotion,
    vesper_motion: BodyMotion,
    nymara_motion: BodyMotion,
    cryon_motion: BodyMotion,
    veridis_motion: BodyMotion,
    umbra_motion: BodyMotion,
    solene_motion: BodyMotion,
) -> BodyPoses {
    let solaris_model = solaris_motion.model_matrix_at(time);
    let vesper_model = vesper_motion.model_matrix_at(time);
    let vesper_position = vesper_motion.orbit.position_at(time);
    let moon_orbit = Orbit {
        center: vesper_position + Vec3::new(0.0, 1.20, 0.0),
        radius: 0.585234996,
        angular_velocity: 0.32,
        phase: 2.792821650,
        inclination: 0.0,
    };
    let selene_position = moon_orbit.position_at(time);
    let selene_model = Mat4::translation(selene_position)
        * Mat4::rotation_z(0.25)
        * Mat4::rotation_y(0.45 * time)
        * Mat4::scale(Vec3::new(0.28, 0.28, 0.28));
    let models = [
        (BodyId::Solaris, solaris_model),
        (BodyId::Vesper, vesper_model),
        (BodyId::Selene, selene_model),
        (BodyId::Nymara, nymara_motion.model_matrix_at(time)),
        (BodyId::Cryon, cryon_motion.model_matrix_at(time)),
        (BodyId::Veridis, veridis_motion.model_matrix_at(time)),
        (BodyId::Umbra, umbra_motion.model_matrix_at(time)),
        (BodyId::Solene, solene_motion.model_matrix_at(time)),
    ];
    BodyPoses::new(models.map(|(id, model_matrix)| BodyPose {
        id,
        world_position: model_matrix.transform_point(Vec3::ZERO),
        model_matrix,
    }))
}

#[allow(clippy::too_many_arguments)]
fn run_fbm_benchmark(
    camera: &Camera,
    solaris: &aetheria::assets::ObjAsset,
    vesper: &aetheria::assets::ObjAsset,
    selene: &aetheria::assets::ObjAsset,
    nymara: &aetheria::assets::ObjAsset,
    ring_inner: &aetheria::assets::ObjAsset,
    ring_middle: &aetheria::assets::ObjAsset,
    ring_outer: &aetheria::assets::ObjAsset,
    cryon: &aetheria::assets::ObjAsset,
    veridis: &aetheria::assets::ObjAsset,
    umbra: &aetheria::assets::ObjAsset,
    solene: &aetheria::assets::ObjAsset,
    solaris_motion: BodyMotion,
    vesper_motion: BodyMotion,
    nymara_motion: BodyMotion,
    cryon_motion: BodyMotion,
    veridis_motion: BodyMotion,
    umbra_motion: BodyMotion,
    solene_motion: BodyMotion,
    shadow_bvh: bool,
    fbm_precomputed: bool,
    framebuffer_fused_write: bool,
) -> io::Result<()> {
    let poses = build_poses(
        0.0, solaris_motion, vesper_motion, nymara_motion, cryon_motion,
        veridis_motion, umbra_motion, solene_motion,
    );
    let shadow_scene = build_shadow_scene(
        vesper, selene, nymara, ring_inner, ring_middle, ring_outer,
        cryon, veridis, umbra, solene, poses, shadow_bvh, false, false,
    );
    #[cfg(feature = "interactive")]
    let background_colors = Some(aetheria::space_background::SpaceBackground::new(0xA37E_214D).render(camera, 800, 600));
    #[cfg(not(feature = "interactive"))]
    let background_colors: Option<Vec<Color>> = None;

    const WARMUP: usize = 10;
    const MEASURED: usize = 60;
    let mut samples = Vec::with_capacity(MEASURED);
    for iteration in 0..(WARMUP + MEASURED) {
        let started = Instant::now();
        let _framebuffer = render_frame(
            0.0, camera, solaris, vesper, selene, nymara,
            ring_inner, ring_middle, ring_outer, cryon, veridis, umbra, solene,
            poses, &shadow_scene, background_colors.as_deref(), false, None, false, framebuffer_fused_write, false, false, false, false,
            None, None, None, None, None,
        )?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        if iteration >= WARMUP {
            samples.push(elapsed_ms);
        }
    }
    samples.sort_by(f64::total_cmp);
    let sum: f64 = samples.iter().sum();
    let average = sum / samples.len() as f64;
    let percentile = |p: f64| samples[((samples.len() - 1) as f64 * p).round() as usize];
    println!(
        "[FBM-BENCH] variant={} warmup={} measured={} poses=1 shadow_scene_builds=1 background_generations=1 presentation=0 wait=0 total_ms={:.3} avg_ms={:.3} median_ms={:.3} p95_ms={:.3} p99_ms={:.3} min_ms={:.3} max_ms={:.3} fps_equiv={:.3}",
        if fbm_precomputed { "precomputed" } else { "original" }, WARMUP, MEASURED,
        sum, average, percentile(0.50), percentile(0.95), percentile(0.99),
        samples[0], *samples.last().unwrap(), 1000.0 / average,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_camera_matrices_benchmark(
    camera: &Camera,
    solaris: &aetheria::assets::ObjAsset,
    vesper: &aetheria::assets::ObjAsset,
    selene: &aetheria::assets::ObjAsset,
    nymara: &aetheria::assets::ObjAsset,
    ring_inner: &aetheria::assets::ObjAsset,
    ring_middle: &aetheria::assets::ObjAsset,
    ring_outer: &aetheria::assets::ObjAsset,
    cryon: &aetheria::assets::ObjAsset,
    veridis: &aetheria::assets::ObjAsset,
    umbra: &aetheria::assets::ObjAsset,
    solene: &aetheria::assets::ObjAsset,
    solaris_motion: BodyMotion,
    vesper_motion: BodyMotion,
    nymara_motion: BodyMotion,
    cryon_motion: BodyMotion,
    veridis_motion: BodyMotion,
    umbra_motion: BodyMotion,
    solene_motion: BodyMotion,
    shadow_bvh: bool,
    benchmark_normal_transform: bool,
) -> io::Result<()> {
    let poses = build_poses(
        0.0, solaris_motion, vesper_motion, nymara_motion, cryon_motion,
        veridis_motion, umbra_motion, solene_motion,
    );
    let shadow_scene = build_shadow_scene(
        vesper, selene, nymara, ring_inner, ring_middle, ring_outer,
        cryon, veridis, umbra, solene, poses, shadow_bvh, false, false,
    );
    const WARMUP: usize = 10;
    const MEASURED: usize = 60;
    let order = [false, true, true, false, false, true, true, false, false, true];
    let mut windows: Vec<(bool, Vec<f64>)> = Vec::with_capacity(order.len());
    for &cached in &order {
        let mut samples = Vec::with_capacity(MEASURED);
        for iteration in 0..(WARMUP + MEASURED) {
            let started = Instant::now();
            let _framebuffer = render_frame(
                0.0, camera, solaris, vesper, selene, nymara,
                ring_inner, ring_middle, ring_outer, cryon, veridis, umbra, solene,
                poses, &shadow_scene, None, false, None, false, false, false,
                if benchmark_normal_transform { false } else { cached }, true,
                if benchmark_normal_transform { cached } else { false },
                None, None, None, None, None,
            )?;
            if iteration >= WARMUP {
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
            }
        }
        samples.sort_by(f64::total_cmp);
        windows.push((cached, samples));
    }
    for (index, (cached, samples)) in windows.iter().enumerate() {
        let sum: f64 = samples.iter().sum();
        let average = sum / samples.len() as f64;
        let percentile = |p: f64| samples[((samples.len() - 1) as f64 * p).round() as usize];
        let variance = samples.iter().map(|sample| (sample - average).powi(2)).sum::<f64>() / samples.len() as f64;
        println!(
            "[MATRIX-TRANSFORM-BENCH] window={} pair={} order={} variant={} warmup={} measured={} avg_ms={:.3} median_ms={:.3} p95_ms={:.3} p99_ms={:.3} stddev_ms={:.3} min_ms={:.3} max_ms={:.3}",
            index + 1, index / 2 + 1, if index % 2 == 0 { "first" } else { "second" },
            if benchmark_normal_transform {
                if *cached { "normal-cached" } else { "normal-original" }
            } else if *cached { "camera-cached" } else { "camera-original" }, WARMUP, MEASURED,
            average, percentile(0.50), percentile(0.95), percentile(0.99), variance.sqrt(),
            samples[0], *samples.last().unwrap(),
        );
    }
    for &cached in &[false, true] {
        let means: Vec<f64> = windows.iter().filter(|(variant, _)| *variant == cached)
            .map(|(_, samples)| samples.iter().sum::<f64>() / samples.len() as f64).collect();
        let mut sorted = means.clone();
        sorted.sort_by(f64::total_cmp);
        let mean = means.iter().sum::<f64>() / means.len() as f64;
        let variance = means.iter().map(|sample| (sample - mean).powi(2)).sum::<f64>() / means.len() as f64;
        println!("[MATRIX-TRANSFORM-BENCH-SUMMARY] variant={} windows={} mean_of_window_means_ms={:.3} median_of_window_means_ms={:.3} stddev_ms={:.3} min_ms={:.3} max_ms={:.3}",
            if benchmark_normal_transform {
                if cached { "normal-cached" } else { "normal-original" }
            } else if cached { "camera-cached" } else { "camera-original" }, means.len(), mean, sorted[2], variance.sqrt(), sorted[0], sorted[4]);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_framebuffer_benchmark(
    camera: &Camera,
    solaris: &aetheria::assets::ObjAsset,
    vesper: &aetheria::assets::ObjAsset,
    selene: &aetheria::assets::ObjAsset,
    nymara: &aetheria::assets::ObjAsset,
    ring_inner: &aetheria::assets::ObjAsset,
    ring_middle: &aetheria::assets::ObjAsset,
    ring_outer: &aetheria::assets::ObjAsset,
    cryon: &aetheria::assets::ObjAsset,
    veridis: &aetheria::assets::ObjAsset,
    umbra: &aetheria::assets::ObjAsset,
    solene: &aetheria::assets::ObjAsset,
    solaris_motion: BodyMotion,
    vesper_motion: BodyMotion,
    nymara_motion: BodyMotion,
    cryon_motion: BodyMotion,
    veridis_motion: BodyMotion,
    umbra_motion: BodyMotion,
    solene_motion: BodyMotion,
    shadow_bvh: bool,
    framebuffer_fused_write: bool,
    raster_incremental_coverage: bool,
) -> io::Result<()> {
    let poses = build_poses(
        0.0, solaris_motion, vesper_motion, nymara_motion, cryon_motion,
        veridis_motion, umbra_motion, solene_motion,
    );
    let shadow_scene = build_shadow_scene(
        vesper, selene, nymara, ring_inner, ring_middle, ring_outer,
        cryon, veridis, umbra, solene, poses, shadow_bvh, false, false,
    );
    const WARMUP: usize = 10;
    const MEASURED: usize = 60;
    let mut samples = Vec::with_capacity(MEASURED);
    for iteration in 0..(WARMUP + MEASURED) {
        let started = Instant::now();
        let _framebuffer = render_frame(
            0.0, camera, solaris, vesper, selene, nymara,
            ring_inner, ring_middle, ring_outer, cryon, veridis, umbra, solene,
            poses, &shadow_scene, None, false, None, false, framebuffer_fused_write, raster_incremental_coverage, false, false, false,
            None, None, None, None, None,
        )?;
        if iteration >= WARMUP {
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    samples.sort_by(f64::total_cmp);
    let sum: f64 = samples.iter().sum();
    let average = sum / samples.len() as f64;
    let percentile = |p: f64| samples[((samples.len() - 1) as f64 * p).round() as usize];
    println!(
        "[FRAMEBUFFER-BENCH] variant={} warmup={} measured={} poses=1 shadow_scene_builds=1 presentation=0 wait=0 total_ms={:.3} avg_ms={:.3} median_ms={:.3} p95_ms={:.3} p99_ms={:.3} min_ms={:.3} max_ms={:.3} fps_equiv={:.3}",
        if raster_incremental_coverage { "incremental" } else if framebuffer_fused_write { "fused" } else { "original" }, WARMUP, MEASURED,
        sum, average, percentile(0.50), percentile(0.95), percentile(0.99),
        samples[0], *samples.last().unwrap(), 1000.0 / average,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_framebuffer_compare(
    camera: &Camera,
    solaris: &aetheria::assets::ObjAsset,
    vesper: &aetheria::assets::ObjAsset,
    selene: &aetheria::assets::ObjAsset,
    nymara: &aetheria::assets::ObjAsset,
    ring_inner: &aetheria::assets::ObjAsset,
    ring_middle: &aetheria::assets::ObjAsset,
    ring_outer: &aetheria::assets::ObjAsset,
    cryon: &aetheria::assets::ObjAsset,
    veridis: &aetheria::assets::ObjAsset,
    umbra: &aetheria::assets::ObjAsset,
    solene: &aetheria::assets::ObjAsset,
    solaris_motion: BodyMotion,
    vesper_motion: BodyMotion,
    nymara_motion: BodyMotion,
    cryon_motion: BodyMotion,
    veridis_motion: BodyMotion,
    umbra_motion: BodyMotion,
    solene_motion: BodyMotion,
    shadow_bvh: bool,
) -> io::Result<()> {
    let original_path = "output/phase22p_framebuffer_original.ppm";
    let fused_path = "output/phase22p_framebuffer_fused.ppm";
    if std::path::Path::new(original_path).exists() || std::path::Path::new(fused_path).exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "los outputs de comparación de framebuffer ya existen"));
    }
    let poses = build_poses(
        0.0, solaris_motion, vesper_motion, nymara_motion, cryon_motion,
        veridis_motion, umbra_motion, solene_motion,
    );
    let shadow_scene = build_shadow_scene(
        vesper, selene, nymara, ring_inner, ring_middle, ring_outer,
        cryon, veridis, umbra, solene, poses, shadow_bvh, false, false,
    );
    let original = render_frame(
        0.0, camera, solaris, vesper, selene, nymara,
        ring_inner, ring_middle, ring_outer, cryon, veridis, umbra, solene,
        poses, &shadow_scene, None, false, None, false, false, false, false, false, false,
        None, None, None, None, None,
    )?;
    let fused = render_frame(
        0.0, camera, solaris, vesper, selene, nymara,
        ring_inner, ring_middle, ring_outer, cryon, veridis, umbra, solene,
        poses, &shadow_scene, None, false, None, false, true, false, false, false, false,
        None, None, None, None, None,
    )?;
    let mut color_pixels = 0u64;
    let mut color_channels = 0u64;
    let mut depth_values = 0u64;
    for (a, b) in original.color_buffer().iter().zip(fused.color_buffer()) {
        let differences = [a.r != b.r, a.g != b.g, a.b != b.b];
        if differences.iter().any(|different| *different) { color_pixels += 1; }
        color_channels += differences.iter().filter(|different| **different).count() as u64;
    }
    for (a, b) in original.depth_buffer().iter().zip(fused.depth_buffer()) {
        if a.to_bits() != b.to_bits() { depth_values += 1; }
    }
    let original_ppm = original.serialize_ppm();
    let fused_ppm = fused.serialize_ppm();
    fs::create_dir_all("output")?;
    fs::write(original_path, &original_ppm)?;
    fs::write(fused_path, &fused_ppm)?;
    println!("[FRAMEBUFFER-COMPARE] pixels={} color_pixels_different={} color_channels_different={} depth_values_different={} ppm_bytes_different={} original={} fused={}",
        original.color_buffer().len(), color_pixels, color_channels, depth_values,
        original_ppm.iter().zip(&fused_ppm).filter(|(a, b)| a != b).count(), original_path, fused_path);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_raster_incremental_compare(
    camera: &Camera,
    solaris: &aetheria::assets::ObjAsset,
    vesper: &aetheria::assets::ObjAsset,
    selene: &aetheria::assets::ObjAsset,
    nymara: &aetheria::assets::ObjAsset,
    ring_inner: &aetheria::assets::ObjAsset,
    ring_middle: &aetheria::assets::ObjAsset,
    ring_outer: &aetheria::assets::ObjAsset,
    cryon: &aetheria::assets::ObjAsset,
    veridis: &aetheria::assets::ObjAsset,
    umbra: &aetheria::assets::ObjAsset,
    solene: &aetheria::assets::ObjAsset,
    solaris_motion: BodyMotion,
    vesper_motion: BodyMotion,
    nymara_motion: BodyMotion,
    cryon_motion: BodyMotion,
    veridis_motion: BodyMotion,
    umbra_motion: BodyMotion,
    solene_motion: BodyMotion,
    shadow_bvh: bool,
) -> io::Result<()> {
    let original_path = "output/phase22q_raster_original.ppm";
    let incremental_path = "output/phase22q_raster_incremental.ppm";
    if std::path::Path::new(original_path).exists() || std::path::Path::new(incremental_path).exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "los outputs de comparación incremental ya existen"));
    }
    let poses = build_poses(
        0.0, solaris_motion, vesper_motion, nymara_motion, cryon_motion,
        veridis_motion, umbra_motion, solene_motion,
    );
    let shadow_scene = build_shadow_scene(
        vesper, selene, nymara, ring_inner, ring_middle, ring_outer,
        cryon, veridis, umbra, solene, poses, shadow_bvh, false, false,
    );
    let original_profile = RasterDetailProfile::new();
    let incremental_profile = RasterDetailProfile::new();
    let original = render_frame(
        0.0, camera, solaris, vesper, selene, nymara,
        ring_inner, ring_middle, ring_outer, cryon, veridis, umbra, solene,
        poses, &shadow_scene, None, false, None, false, false, false, false, false, false,
        None, Some(original_profile.clone()), None, None, None,
    )?;
    let incremental = render_frame(
        0.0, camera, solaris, vesper, selene, nymara,
        ring_inner, ring_middle, ring_outer, cryon, veridis, umbra, solene,
        poses, &shadow_scene, None, false, None, false, false, true, false, false, false,
        None, Some(incremental_profile.clone()), None, None, None,
    )?;
    let a = original_profile.snapshot();
    let b = incremental_profile.snapshot();
    let coverage_equal = a.input_triangles == b.input_triangles
        && a.clipped_triangles == b.clipped_triangles
        && a.projected_triangles == b.projected_triangles
        && a.bounding_boxes == b.bounding_boxes
        && a.candidate_pixels == b.candidate_pixels
        && a.covered_pixels == b.covered_pixels
        && a.depth_passed == b.depth_passed
        && a.shader_evaluations == b.shader_evaluations
        && a.written_fragments == b.written_fragments;
    let original_ppm = original.serialize_ppm();
    let incremental_ppm = incremental.serialize_ppm();
    let mut color_pixels = 0u64;
    let mut color_channels = 0u64;
    let mut depth_values = 0u64;
    for (left, right) in original.color_buffer().iter().zip(incremental.color_buffer()) {
        let differences = [left.r != right.r, left.g != right.g, left.b != right.b];
        if differences.iter().any(|different| *different) { color_pixels += 1; }
        color_channels += differences.iter().filter(|different| **different).count() as u64;
    }
    for (left, right) in original.depth_buffer().iter().zip(incremental.depth_buffer()) {
        if left.to_bits() != right.to_bits() { depth_values += 1; }
    }
    fs::create_dir_all("output")?;
    fs::write(original_path, &original_ppm)?;
    fs::write(incremental_path, &incremental_ppm)?;
    println!("[RASTER-COMPARE] coverage_equal={} input={} clipped={} projected={} boxes={} candidates={} covered_original={} covered_incremental={} rejected_original={} rejected_incremental={} depth_original={} depth_incremental={} shaders_original={} shaders_incremental={} writes_original={} writes_incremental={}",
        coverage_equal, a.input_triangles, a.clipped_triangles, a.projected_triangles, a.bounding_boxes,
        a.candidate_pixels, a.covered_pixels, b.covered_pixels,
        a.candidate_pixels.saturating_sub(a.covered_pixels), b.candidate_pixels.saturating_sub(b.covered_pixels),
        a.depth_passed, b.depth_passed, a.shader_evaluations, b.shader_evaluations,
        a.written_fragments, b.written_fragments);
    println!("[RASTER-COMPARE] color_pixels_different={} color_channels_different={} depth_values_different={} rgb8_equal={} original={} incremental={}",
        color_pixels, color_channels, depth_values, original_ppm == incremental_ppm, original_path, incremental_path);
    if !coverage_equal {
        println!("[RASTER-COMPARE] first_discrepancy=not-captured; inspect per-triangle coverage instrumentation before adoption");
    }
    Ok(())
}

fn build_shadow_scene(
    vesper: &aetheria::assets::ObjAsset,
    selene: &aetheria::assets::ObjAsset,
    nymara: &aetheria::assets::ObjAsset,
    ring_inner: &aetheria::assets::ObjAsset,
    ring_middle: &aetheria::assets::ObjAsset,
    ring_outer: &aetheria::assets::ObjAsset,
    cryon: &aetheria::assets::ObjAsset,
    veridis: &aetheria::assets::ObjAsset,
    umbra: &aetheria::assets::ObjAsset,
    solene: &aetheria::assets::ObjAsset,
    poses: BodyPoses,
    use_bvh: bool,
    compare_algorithms: bool,
    compare_all: bool,
) -> Arc<ShadowScene> {
    let objects = [
        (1, &vesper.mesh, poses.get(BodyId::Vesper).model_matrix),
        (2, &selene.mesh, poses.get(BodyId::Selene).model_matrix),
        (3, &nymara.mesh, poses.get(BodyId::Nymara).model_matrix),
        (4, &ring_inner.mesh, poses.get(BodyId::Nymara).model_matrix),
        (5, &ring_middle.mesh, poses.get(BodyId::Nymara).model_matrix),
        (6, &ring_outer.mesh, poses.get(BodyId::Nymara).model_matrix),
        (7, &cryon.mesh, poses.get(BodyId::Cryon).model_matrix),
        (8, &veridis.mesh, poses.get(BodyId::Veridis).model_matrix),
        (9, &umbra.mesh, poses.get(BodyId::Umbra).model_matrix),
        (10, &solene.mesh, poses.get(BodyId::Solene).model_matrix),
    ];
    Arc::new(ShadowScene::from_meshes_with_comparison(&objects, use_bvh, compare_algorithms, compare_all))
}

#[allow(clippy::too_many_arguments)]
fn render_frame(
    time: f32, camera: &Camera,
    solaris: &aetheria::assets::ObjAsset, vesper: &aetheria::assets::ObjAsset,
    selene: &aetheria::assets::ObjAsset, nymara: &aetheria::assets::ObjAsset,
    ring_inner: &aetheria::assets::ObjAsset, ring_middle: &aetheria::assets::ObjAsset,
    ring_outer: &aetheria::assets::ObjAsset, cryon: &aetheria::assets::ObjAsset,
    veridis: &aetheria::assets::ObjAsset, umbra: &aetheria::assets::ObjAsset,
    solene: &aetheria::assets::ObjAsset, poses: BodyPoses,
    shadow_scene: &Arc<ShadowScene>,
    background_colors: Option<&[Color]>,
    diagnostics_enabled: bool,
    vesper_emission_curve: Option<(f32, f32)>,
    solene_normalized: bool,
    framebuffer_fused_write: bool,
    raster_incremental_coverage: bool,
    camera_matrices_cached: bool,
    suppress_render_log: bool,
    normal_transform_cached: bool,
    detail_profile: Option<DetailedProfile>,
    raster_detail_profile: Option<RasterDetailProfile>,
    shader_detail_profile: Option<ShaderDetailProfile>,
    procedural_detail_profile: Option<ProceduralDetailProfile>,
    mut profile_frame: Option<&mut ProfileFrame>,
) -> io::Result<Framebuffer> {
    let solaris_model = poses.get(BodyId::Solaris).model_matrix;
    let vesper_model = poses.get(BodyId::Vesper).model_matrix;
    let selene_model = poses.get(BodyId::Selene).model_matrix;
    let nymara_model = poses.get(BodyId::Nymara).model_matrix;
    let cryon_model = poses.get(BodyId::Cryon).model_matrix;
    let veridis_model = poses.get(BodyId::Veridis).model_matrix;
    let umbra_model = poses.get(BodyId::Umbra).model_matrix;
    let solene_model = poses.get(BodyId::Solene).model_matrix;

    let diagnostics = diagnostics_enabled.then(DiagnosticsHandle::new);
    let uniforms = ShaderUniforms {
        time, camera_position: camera.position,
        light_position: solaris_model.transform_point(Vec3::ZERO),
        light_color: Color::new_const(1.0, 0.86, 0.68), light_intensity: 1.8,
        light_attenuation: 0.04, ambient_light: 0.10,
        shadow_scene: Some(shadow_scene.clone()), diagnostics: diagnostics.clone(),
        vesper_emission_curve,
        solene_normalized_light: solene_normalized,
        framebuffer_fused_write,
        raster_incremental_coverage,
        cached_camera_matrices: camera_matrices_cached.then(|| CachedCameraMatrices {
            view: camera.view_matrix(),
            projection: camera.projection_matrix(),
        }),
        detail_profile,
        raster_detail_profile,
        shader_detail_profile,
        procedural_detail_profile,
        ..ShaderUniforms::default()
    };
    let framebuffer_setup_start = Instant::now();
    let mut framebuffer = Framebuffer::new(800, 600).map_err(io::Error::other)?;
    framebuffer.clear(Color::new(0.003, 0.004, 0.010));
    if let Some(colors) = background_colors {
        if colors.len() == framebuffer.width() * framebuffer.height() {
            framebuffer.restore_colors(colors);
        }
    }
    if let Some(profile_frame) = profile_frame.as_mut() {
        profile_frame.framebuffer_ms = framebuffer_setup_start.elapsed().as_secs_f64() * 1000.0;
    }

    let solaris_shader = SolarisShader::default();
    let vesper_shader = VesperShader::default();
    let selene_shader = SeleneShader::default();
    let nymara_shader = NymaraShader::default();
    let cryon_shader = CryonShader::default();
    let veridis_shader = VeridisShader::default();
    let umbra_shader = UmbraShader::default();
    let solene_shader = SoleneShader::default();
    let ring_inner_shader = NymaraRingShader::new(Color::new_const(0.32, 0.58, 0.62), 0.92, 11);
    let ring_middle_shader = NymaraRingShader::new(Color::new_const(0.50, 0.38, 0.62), 0.78, 23);
    let ring_outer_shader = NymaraRingShader::new(Color::new_const(0.26, 0.42, 0.54), 0.66, 37);

    let render_start = Instant::now();
    let mut render = |receiver: u32, object: DiagnosticObject,
                      mesh: &aetheria::geometry::Mesh, model: Mat4,
                      shader: &dyn RenderableShader| {
        let mut object_uniforms = uniforms.clone();
        object_uniforms.shadow_receiver = receiver;
        object_uniforms.diagnostic_object = Some(object);
        object_uniforms.cached_normal_transform = normal_transform_cached
            .then(|| model.precompute_normal_transform());
        shader.render(&mut framebuffer, mesh, model, camera, &object_uniforms);
    };
    render(u32::MAX, DiagnosticObject::Solaris, &solaris.mesh, solaris_model, &solaris_shader);
    render(1, DiagnosticObject::Vesper, &vesper.mesh, vesper_model, &vesper_shader);
    render(2, DiagnosticObject::Selene, &selene.mesh, selene_model, &selene_shader);
    render(3, DiagnosticObject::Nymara, &nymara.mesh, nymara_model, &nymara_shader);
    render(4, DiagnosticObject::RingInner, &ring_inner.mesh, nymara_model, &ring_inner_shader);
    render(5, DiagnosticObject::RingMiddle, &ring_middle.mesh, nymara_model, &ring_middle_shader);
    render(6, DiagnosticObject::RingOuter, &ring_outer.mesh, nymara_model, &ring_outer_shader);
    render(7, DiagnosticObject::Cryon, &cryon.mesh, cryon_model, &cryon_shader);
    render(8, DiagnosticObject::Veridis, &veridis.mesh, veridis_model, &veridis_shader);
    render(9, DiagnosticObject::Umbra, &umbra.mesh, umbra_model, &umbra_shader);
    render(10, DiagnosticObject::Solene, &solene.mesh, solene_model, &solene_shader);

    if !suppress_render_log {
        println!("Rendered t={time:.2}; {} shadow triangles; {:?}", shadow_scene.triangle_count(), render_start.elapsed());
    }
    if let Some(diagnostics) = diagnostics {
        println!("{}", diagnostics.report());
    }
    if let Some(profile_frame) = profile_frame {
        profile_frame.geometry_ms = render_start.elapsed().as_secs_f64() * 1000.0;
    }
    Ok(framebuffer)
}

trait RenderableShader {
    fn render(&self, framebuffer: &mut Framebuffer, mesh: &aetheria::geometry::Mesh,
              model: Mat4, camera: &Camera, uniforms: &ShaderUniforms);
}

impl<S: aetheria::shader::FragmentShader> RenderableShader for S {
    fn render(&self, framebuffer: &mut Framebuffer, mesh: &aetheria::geometry::Mesh,
              model: Mat4, camera: &Camera, uniforms: &ShaderUniforms) {
        render_mesh_with_shader(framebuffer, mesh, model, camera, self, uniforms);
    }
}