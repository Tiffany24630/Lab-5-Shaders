use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

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
}

#[derive(Clone, Debug)]
pub struct DetailedProfile(Rc<RefCell<DetailedProfileStats>>);

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
}