use crate::math::Vec3;
use crate::diagnostics::DiagnosticObject;
use crate::profile::ProceduralDetailProfile;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[derive(Clone)]
struct ProceduralScope {
    profile: ProceduralDetailProfile,
    object: DiagnosticObject,
    sampled: bool,
}

thread_local! {
    static CURRENT_SCOPE: RefCell<Option<ProceduralScope>> = const { RefCell::new(None) };
}

static PROCEDURAL_PROFILE_ACTIVE: AtomicBool = AtomicBool::new(false);
static FBM_PRECOMPUTED_ACTIVE: AtomicBool = AtomicBool::new(false);
static FBM_COMPARE_ACTIVE: AtomicBool = AtomicBool::new(false);
static FBM_COMPARE_CALLS: AtomicU64 = AtomicU64::new(0);
static FBM_COMPARE_MISMATCHES: AtomicU64 = AtomicU64::new(0);

const fn build_fbm_normalization_table() -> [f32; 17] {
    let mut table = [0.0; 17];
    let mut octaves = 0;
    while octaves <= 16 {
        let mut amplitude: f32 = 0.5;
        let mut normalization: f32 = 0.0;
        let mut index = 0;
        while index < octaves {
            normalization += amplitude;
            amplitude *= 0.5;
            index += 1;
        }
        table[octaves] = normalization;
        octaves += 1;
    }
    table
}

pub const FBM_NORMALIZATION_TABLE: [f32; 17] = build_fbm_normalization_table();

pub fn fbm_normalization_table_matches_original() -> bool {
    for octaves in 0..=16 {
        let mut amplitude: f32 = 0.5;
        let mut normalization: f32 = 0.0;
        for _ in 0..octaves {
            normalization += amplitude;
            amplitude *= 0.5;
        }
        if FBM_NORMALIZATION_TABLE[octaves].to_bits() != normalization.to_bits() {
            return false;
        }
    }
    true
}

pub fn set_fbm_precomputed(enabled: bool) {
    FBM_PRECOMPUTED_ACTIVE.store(enabled, Ordering::Relaxed);
}

pub fn set_fbm_comparison(enabled: bool) {
    FBM_COMPARE_CALLS.store(0, Ordering::Relaxed);
    FBM_COMPARE_MISMATCHES.store(0, Ordering::Relaxed);
    FBM_COMPARE_ACTIVE.store(enabled, Ordering::Relaxed);
}

pub fn fbm_comparison_stats() -> (u64, u64) {
    (
        FBM_COMPARE_CALLS.load(Ordering::Relaxed),
        FBM_COMPARE_MISMATCHES.load(Ordering::Relaxed),
    )
}

pub(crate) struct ProceduralScopeGuard {
    previous: Option<ProceduralScope>,
}

pub(crate) fn enter_scope(profile: ProceduralDetailProfile, object: DiagnosticObject) -> ProceduralScopeGuard {
    let sampled = profile.begin_evaluation(object);
    let previous = CURRENT_SCOPE.with(|scope| scope.replace(Some(ProceduralScope { profile, object, sampled })));
    PROCEDURAL_PROFILE_ACTIVE.store(true, Ordering::Relaxed);
    ProceduralScopeGuard { previous }
}

impl Drop for ProceduralScopeGuard {
    fn drop(&mut self) {
        CURRENT_SCOPE.with(|scope| { scope.replace(self.previous.take()); });
        PROCEDURAL_PROFILE_ACTIVE.store(false, Ordering::Relaxed);
    }
}

pub fn seeded_value(seed: u32, index: u32) -> f32 {
    let mut value = seed.wrapping_add(index.wrapping_mul(0x9E37_79B9));
    value ^= value >> 16;
    value = value.wrapping_mul(0x85EB_CA6B);
    value ^= value >> 13;
    value = value.wrapping_mul(0xC2B2_AE35);
    value ^= value >> 16;
    value as f32 / u32::MAX as f32
}

pub fn remap(value: f32, from_min: f32, from_max: f32, to_min: f32, to_max: f32) -> f32 {
    if !value.is_finite()
        || !from_min.is_finite()
        || !from_max.is_finite()
        || !to_min.is_finite()
        || !to_max.is_finite()
        || (from_max - from_min).abs() <= f32::EPSILON
    {
        return to_min;
    }
    to_min + (value - from_min) / (from_max - from_min) * (to_max - to_min)
}

pub fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if !edge0.is_finite()
        || !edge1.is_finite()
        || !value.is_finite()
        || (edge1 - edge0).abs() <= f32::EPSILON
    {
        return 0.0;
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lattice_hash(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut value = seed
        .wrapping_add((x as u32).wrapping_mul(0x8DA6_B343))
        .wrapping_add((y as u32).wrapping_mul(0xD816_3841))
        .wrapping_add((z as u32).wrapping_mul(0xCB1A_B31F));
    value ^= value >> 15;
    value = value.wrapping_mul(0x2C1B_3C6D);
    value ^= value >> 12;
    value as f32 / u32::MAX as f32
}

pub fn noise_3d(point: Vec3, seed: u32) -> f32 {
    if !PROCEDURAL_PROFILE_ACTIVE.load(Ordering::Relaxed) {
        return noise_3d_impl(point, seed);
    }
    let scope = CURRENT_SCOPE.with(|current| current.borrow().clone());
    let started = scope.as_ref().filter(|scope| scope.sampled).map(|_| Instant::now());
    let result = noise_3d_impl(point, seed);
    if let Some(scope) = scope {
        scope.profile.add_noise(
            scope.object,
            scope.sampled,
            started.map(|start| start.elapsed().as_nanos()).unwrap_or(0),
        );
    }
    result
}

fn noise_3d_impl(point: Vec3, seed: u32) -> f32 {
    if !point.is_finite() {
        return 0.0;
    }
    let cell = Vec3::new(point.x.floor(), point.y.floor(), point.z.floor());
    let local = point - cell;
    let fade = Vec3::new(
        smoothstep(0.0, 1.0, local.x),
        smoothstep(0.0, 1.0, local.y),
        smoothstep(0.0, 1.0, local.z),
    );
    let x = cell.x as i32;
    let y = cell.y as i32;
    let z = cell.z as i32;
    let c000 = lattice_hash(x, y, z, seed);
    let xp = x.wrapping_add(1);
    let yp = y.wrapping_add(1);
    let zp = z.wrapping_add(1);
    let c100 = lattice_hash(xp, y, z, seed);
    let c010 = lattice_hash(x, yp, z, seed);
    let c110 = lattice_hash(xp, yp, z, seed);
    let c001 = lattice_hash(x, y, zp, seed);
    let c101 = lattice_hash(xp, y, zp, seed);
    let c011 = lattice_hash(x, yp, zp, seed);
    let c111 = lattice_hash(xp, yp, zp, seed);
    let x00 = c000 + (c100 - c000) * fade.x;
    let x10 = c010 + (c110 - c010) * fade.x;
    let x01 = c001 + (c101 - c001) * fade.x;
    let x11 = c011 + (c111 - c011) * fade.x;
    let y0 = x00 + (x10 - x00) * fade.y;
    let y1 = x01 + (x11 - x01) * fade.y;
    y0 + (y1 - y0) * fade.z
}

pub fn fbm(point: Vec3, seed: u32, octaves: u32) -> f32 {
    let profiling = PROCEDURAL_PROFILE_ACTIVE.load(Ordering::Relaxed);
    let comparing = FBM_COMPARE_ACTIVE.load(Ordering::Relaxed);
    let precomputed = FBM_PRECOMPUTED_ACTIVE.load(Ordering::Relaxed);
    if !profiling && !comparing && !precomputed {
        return fbm_impl(point, seed, octaves);
    }
    let scope = CURRENT_SCOPE.with(|current| current.borrow().clone());
    let started = scope.as_ref().filter(|scope| scope.sampled).map(|_| Instant::now());
    let result = if FBM_COMPARE_ACTIVE.load(Ordering::Relaxed) {
        let original = fbm_impl(point, seed, octaves);
        let precomputed = fbm_precomputed_impl(point, seed, octaves);
        FBM_COMPARE_CALLS.fetch_add(1, Ordering::Relaxed);
        if original.to_bits() != precomputed.to_bits() {
            FBM_COMPARE_MISMATCHES.fetch_add(1, Ordering::Relaxed);
        }
        original
    } else if FBM_PRECOMPUTED_ACTIVE.load(Ordering::Relaxed) {
        fbm_precomputed_impl(point, seed, octaves)
    } else {
        fbm_impl(point, seed, octaves)
    };
    if let Some(scope) = scope {
        scope.profile.add_fbm(
            scope.object,
            octaves.min(16) as u64,
            scope.sampled,
            started.map(|start| start.elapsed().as_nanos()).unwrap_or(0),
        );
    }
    result
}

fn fbm_impl(point: Vec3, seed: u32, octaves: u32) -> f32 {
    if !point.is_finite() {
        return 0.0;
    }
    let mut frequency = 1.0;
    let mut amplitude = 0.5;
    let mut total = 0.0;
    let mut normalization = 0.0;
    for octave in 0..octaves.min(16) {
        total += noise_3d(point * frequency, seed.wrapping_add(octave)) * amplitude;
        normalization += amplitude;
        frequency *= 2.0;
        amplitude *= 0.5;
    }
    if normalization <= f32::EPSILON {
        0.0
    } else {
        total / normalization
    }
}

fn fbm_precomputed_impl(point: Vec3, seed: u32, octaves: u32) -> f32 {
    if !point.is_finite() {
        return 0.0;
    }
    let effective_octaves = octaves.min(16);
    let mut frequency = 1.0;
    let mut amplitude = 0.5;
    let mut total = 0.0;
    for octave in 0..effective_octaves {
        total += noise_3d(point * frequency, seed.wrapping_add(octave)) * amplitude;
        frequency *= 2.0;
        amplitude *= 0.5;
    }
    let normalization = FBM_NORMALIZATION_TABLE[effective_octaves as usize];
    if normalization <= f32::EPSILON {
        0.0
    } else {
        total / normalization
    }
}

#[cfg(test)]
mod tests {
    use super::{fbm, lattice_hash, noise_3d, remap, seeded_value, smoothstep};
    use crate::math::Vec3;

    #[test]
    fn seeded_values_and_noise_are_deterministic() {
        assert_eq!(seeded_value(7, 3), seeded_value(7, 3));
        assert_eq!(
            noise_3d(Vec3::new(1.25, -2.5, 3.75), 42),
            noise_3d(Vec3::new(1.25, -2.5, 3.75), 42)
        );
        assert_eq!(
            fbm(Vec3::new(1.25, -2.5, 3.75), 42, 5),
            fbm(Vec3::new(1.25, -2.5, 3.75), 42, 5)
        );
    }

    #[test]
    fn interpolated_noise_is_continuous_near_cell_boundaries() {
        let left = noise_3d(Vec3::new(0.99999, 0.25, 0.75), 4);
        let right = noise_3d(Vec3::new(1.00001, 0.25, 0.75), 4);
        assert!((left - right).abs() < 0.001);
        assert!(lattice_hash(1, 2, 3, 4).is_finite());
    }

    #[test]
    fn utility_functions_are_bounded_and_finite() {
        assert!((remap(0.5, 0.0, 1.0, 10.0, 20.0) - 15.0).abs() < 1e-6);
        assert_eq!(smoothstep(0.0, 1.0, 0.0), 0.0);
        assert_eq!(smoothstep(0.0, 1.0, 1.0), 1.0);
        assert!(noise_3d(Vec3::new(f32::MAX / 4.0, 0.0, 0.0), 1).is_finite());
        assert!(fbm(Vec3::new(0.3, 0.7, 1.1), 1, 8).is_finite());
    }
}