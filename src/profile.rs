use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use crate::diagnostics::DiagnosticObject;

#[derive(Clone, Copy, Debug, Default)]
pub struct ShaderObjectProfileStats {
    pub depth_passed: u64,
    pub evaluations: u64,
    pub samples: u64,
    pub shader_ns: u128,
    pub estimated_shader_ns: u128,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ShaderDetailStats {
    pub objects: [ShaderObjectProfileStats; 11],
}

#[derive(Clone, Copy, Debug, Default)]
struct ShaderDetailState {
    stats: ShaderDetailStats,
    sample_counters: [u64; 11],
}

#[derive(Clone, Debug)]
pub struct ShaderDetailProfile(Rc<RefCell<ShaderDetailState>>);

impl PartialEq for ShaderDetailProfile {
    fn eq(&self, other: &Self) -> bool { Rc::ptr_eq(&self.0, &other.0) }
}

impl ShaderDetailProfile {
    pub fn new() -> Self { Self(Rc::new(RefCell::new(ShaderDetailState::default()))) }
    pub fn snapshot(&self) -> ShaderDetailStats {
        let mut stats = self.0.borrow().stats;
        for object in &mut stats.objects {
            object.estimated_shader_ns = if object.samples == 0 {
                0
            } else {
                object.shader_ns.saturating_mul(object.evaluations as u128) / object.samples as u128
            };
        }
        stats
    }
    pub fn take_frame_snapshot(&self) -> ShaderDetailStats {
        let mut state = self.0.borrow_mut();
        let mut stats = state.stats;
        state.stats = ShaderDetailStats::default();
        for object in &mut stats.objects {
            object.estimated_shader_ns = if object.samples == 0 {
                0
            } else {
                object.shader_ns.saturating_mul(object.evaluations as u128) / object.samples as u128
            };
        }
        stats
    }
    pub fn should_sample(&self, object: DiagnosticObject) -> bool {
        let state = &mut *self.0.borrow_mut();
        let counter = &mut state.sample_counters[object.index()];
        let ordinal = *counter;
        *counter += 1;
        let block = ordinal / 64;
        let position = ordinal % 64;
        let mut hash = block
            .wrapping_add((object.index() as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        hash ^= hash >> 30;
        hash = hash.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        hash ^= hash >> 27;
        hash = hash.wrapping_mul(0x94D0_49BB_1331_11EB);
        hash ^= hash >> 31;
        position == hash % 64
    }
    pub fn add(&self, object: DiagnosticObject, depth_passed: u64, evaluations: u64, samples: u64, shader_ns: u128) {
        let stats = &mut self.0.borrow_mut().stats.objects[object.index()];
        stats.depth_passed += depth_passed;
        stats.evaluations += evaluations;
        stats.samples += samples;
        stats.shader_ns += shader_ns;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProceduralObjectProfileStats {
    pub evaluations: u64,
    pub sampled_evaluations: u64,
    pub fbm_calls: u64,
    pub fbm_octaves: u64,
    pub fbm_samples: u64,
    pub fbm_ns: u128,
    pub noise_calls: u64,
    pub noise_samples: u64,
    pub noise_ns: u128,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProceduralDetailStats {
    pub objects: [ProceduralObjectProfileStats; 11],
}

#[derive(Clone, Copy, Debug, Default)]
struct ProceduralDetailState {
    stats: ProceduralDetailStats,
    sample_counters: [u64; 11],
}

#[derive(Clone, Debug)]
pub struct ProceduralDetailProfile(Rc<RefCell<ProceduralDetailState>>);

impl PartialEq for ProceduralDetailProfile {
    fn eq(&self, other: &Self) -> bool { Rc::ptr_eq(&self.0, &other.0) }
}

impl ProceduralDetailProfile {
    pub fn new() -> Self { Self(Rc::new(RefCell::new(ProceduralDetailState::default()))) }

    pub fn take_frame_snapshot(&self) -> ProceduralDetailStats {
        let mut state = self.0.borrow_mut();
        let stats = state.stats;
        state.stats = ProceduralDetailStats::default();
        stats
    }

    pub(crate) fn begin_evaluation(&self, object: DiagnosticObject) -> bool {
        let mut state = self.0.borrow_mut();
        let index = object.index();
        let ordinal = state.sample_counters[index];
        state.sample_counters[index] = ordinal + 1;
        let stats = &mut state.stats.objects[index];
        stats.evaluations += 1;
        let block = ordinal / 64;
        let position = ordinal % 64;
        let mut hash = block
            .wrapping_add((index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        hash ^= hash >> 30;
        hash = hash.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        hash ^= hash >> 27;
        hash = hash.wrapping_mul(0x94D0_49BB_1331_11EB);
        hash ^= hash >> 31;
        let sampled = position == hash % 64;
        if sampled { stats.sampled_evaluations += 1; }
        sampled
    }

    pub(crate) fn add_fbm(&self, object: DiagnosticObject, octaves: u64, sampled: bool, elapsed_ns: u128) {
        let stats = &mut self.0.borrow_mut().stats.objects[object.index()];
        stats.fbm_calls += 1;
        stats.fbm_octaves += octaves;
        if sampled {
            stats.fbm_samples += 1;
            stats.fbm_ns += elapsed_ns;
        }
    }

    pub(crate) fn add_noise(&self, object: DiagnosticObject, sampled: bool, elapsed_ns: u128) {
        let stats = &mut self.0.borrow_mut().stats.objects[object.index()];
        stats.noise_calls += 1;
        if sampled {
            stats.noise_samples += 1;
            stats.noise_ns += elapsed_ns;
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DetailedProfileStats {
    pub preparation_ns: u128,
    pub preparation_samples: u64,
    pub raster_ns: u128,
    pub raster_samples: u64,
    pub shader_ns: u128,
    pub shader_evaluations: u64,
    pub shadow_ns: u128,
    pub shadow_queries: u64,
    pub shadow_aabb_discarded: u64,
    pub shadow_aabb_passed: u64,
    pub shadow_triangle_tests: u64,
    pub shadow_occluded: u64,
    pub brute_queries: u64,
    pub brute_triangle_tests: u64,
    pub brute_occluded: u64,
    pub bvh_queries: u64,
    pub bvh_nodes_visited: u64,
    pub bvh_nodes_discarded: u64,
    pub bvh_leaves_visited: u64,
    pub bvh_triangle_tests: u64,
    pub bvh_occluded: u64,
    pub bvh_max_depth: u32,
    pub bvh_stack_fallbacks: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RasterDetailStats {
    pub input_triangles: u64,
    pub invalid_triangles: u64,
    pub clipped_triangles: u64,
    pub projected_triangles: u64,
    pub transform_ns: u128,
    pub clipping_ns: u128,
    pub projection_ns: u128,
    pub bounding_boxes: u64,
    pub bounding_box_ns: u128,
    pub candidate_pixels: u64,
    pub covered_pixels: u64,
    pub depth_passed: u64,
    pub shader_evaluations: u64,
    pub written_fragments: u64,
    pub coverage_samples: u64,
    pub coverage_ns: u128,
    pub interpolation_samples: u64,
    pub interpolation_ns: u128,
    pub depth_samples: u64,
    pub depth_ns: u128,
    pub shader_samples: u64,
    pub shader_ns: u128,
    pub write_samples: u64,
    pub write_ns: u128,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RasterTriangleCounters {
    pub bounding_boxes: u64,
    pub bounding_box_ns: u128,
    pub candidate_pixels: u64,
    pub covered_pixels: u64,
    pub depth_passed: u64,
    pub shader_evaluations: u64,
    pub written_fragments: u64,
    pub coverage_samples: u64,
    pub coverage_ns: u128,
    pub interpolation_samples: u64,
    pub interpolation_ns: u128,
    pub depth_samples: u64,
    pub depth_ns: u128,
    pub shader_samples: u64,
    pub shader_ns: u128,
    pub write_samples: u64,
    pub write_ns: u128,
}

#[derive(Clone, Debug)]
pub struct RasterDetailProfile(Rc<RefCell<RasterDetailStats>>);

impl PartialEq for RasterDetailProfile {
    fn eq(&self, other: &Self) -> bool { Rc::ptr_eq(&self.0, &other.0) }
}

impl RasterDetailProfile {
    pub fn new() -> Self { Self(Rc::new(RefCell::new(RasterDetailStats::default()))) }
    pub fn snapshot(&self) -> RasterDetailStats { *self.0.borrow() }
    pub fn add_input(&self) { self.0.borrow_mut().input_triangles += 1; }
    pub fn add_invalid(&self) { self.0.borrow_mut().invalid_triangles += 1; }
    pub fn add_clipped(&self, count: usize) { self.0.borrow_mut().clipped_triangles += count as u64; }
    pub fn add_projected(&self) { self.0.borrow_mut().projected_triangles += 1; }
    pub fn add_transform(&self, duration: Duration) { self.0.borrow_mut().transform_ns += duration.as_nanos(); }
    pub fn add_clipping(&self, duration: Duration) { self.0.borrow_mut().clipping_ns += duration.as_nanos(); }
    pub fn add_projection(&self, duration: Duration) { self.0.borrow_mut().projection_ns += duration.as_nanos(); }
    pub fn add_raster(&self, counters: RasterTriangleCounters) {
        let mut stats = self.0.borrow_mut();
        stats.bounding_boxes += counters.bounding_boxes;
        stats.bounding_box_ns += counters.bounding_box_ns;
        stats.candidate_pixels += counters.candidate_pixels;
        stats.covered_pixels += counters.covered_pixels;
        stats.depth_passed += counters.depth_passed;
        stats.shader_evaluations += counters.shader_evaluations;
        stats.written_fragments += counters.written_fragments;
        stats.coverage_samples += counters.coverage_samples;
        stats.coverage_ns += counters.coverage_ns;
        stats.interpolation_samples += counters.interpolation_samples;
        stats.interpolation_ns += counters.interpolation_ns;
        stats.depth_samples += counters.depth_samples;
        stats.depth_ns += counters.depth_ns;
        stats.shader_samples += counters.shader_samples;
        stats.shader_ns += counters.shader_ns;
        stats.write_samples += counters.write_samples;
        stats.write_ns += counters.write_ns;
    }
}

#[derive(Clone, Debug)]
pub struct DetailedProfile(Rc<RefCell<DetailedProfileStats>>);

impl PartialEq for DetailedProfile {
    fn eq(&self, other: &Self) -> bool { Rc::ptr_eq(&self.0, &other.0) }
}

impl DetailedProfile {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(DetailedProfileStats::default())))
    }

    pub fn snapshot(&self) -> DetailedProfileStats {
        *self.0.borrow()
    }

    pub fn add_preparation(&self, duration: Duration) {
        let mut stats = self.0.borrow_mut();
        stats.preparation_ns += duration.as_nanos();
        stats.preparation_samples += 1;
    }

    pub fn add_raster(&self, duration: Duration) {
        let mut stats = self.0.borrow_mut();
        stats.raster_ns += duration.as_nanos();
        stats.raster_samples += 1;
    }

    pub fn add_shader(&self, duration: Duration) {
        let mut stats = self.0.borrow_mut();
        stats.shader_ns += duration.as_nanos();
        stats.shader_evaluations += 1;
    }

    pub fn add_shadow_query(&self, duration: Duration) {
        let mut stats = self.0.borrow_mut();
        stats.shadow_ns += duration.as_nanos();
        stats.shadow_queries += 1;
    }

    pub fn add_shadow_aabb_discarded(&self) { self.0.borrow_mut().shadow_aabb_discarded += 1; }
    pub fn add_shadow_aabb_passed(&self) { self.0.borrow_mut().shadow_aabb_passed += 1; }
    pub fn add_shadow_triangle_test(&self) { self.0.borrow_mut().shadow_triangle_tests += 1; }
    pub fn add_shadow_occluded(&self) { self.0.borrow_mut().shadow_occluded += 1; }
    pub fn add_brute_query(&self) { self.0.borrow_mut().brute_queries += 1; }
    pub fn add_brute_triangle_test(&self) { self.0.borrow_mut().brute_triangle_tests += 1; }
    pub fn add_brute_occluded(&self) { self.0.borrow_mut().brute_occluded += 1; }
    pub fn add_bvh_query(&self) { self.0.borrow_mut().bvh_queries += 1; }
    pub fn add_bvh_node_visited(&self) { self.0.borrow_mut().bvh_nodes_visited += 1; }
    pub fn add_bvh_node_discarded(&self) { self.0.borrow_mut().bvh_nodes_discarded += 1; }
    pub fn add_bvh_leaf(&self) { self.0.borrow_mut().bvh_leaves_visited += 1; }
    pub fn add_bvh_triangle_test(&self) { self.0.borrow_mut().bvh_triangle_tests += 1; }
    pub fn add_bvh_occluded(&self) { self.0.borrow_mut().bvh_occluded += 1; }
    pub fn observe_bvh_depth(&self, depth: u32) {
        let mut stats = self.0.borrow_mut();
        stats.bvh_max_depth = stats.bvh_max_depth.max(depth);
    }
    pub fn add_bvh_stack_fallback(&self) { self.0.borrow_mut().bvh_stack_fallbacks += 1; }
}