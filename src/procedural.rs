use crate::math::Vec3;

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