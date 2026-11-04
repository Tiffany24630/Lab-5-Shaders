use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}
impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}
impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}
impl Div<f32> for Vec2 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    pub fn normalized(self) -> Self {
        let length = self.length();
        if !length.is_finite() || length <= f32::EPSILON {
            Self::ZERO
        } else {
            self / length
        }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        self + (rhs - self) * t
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}
impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}
impl Div<f32> for Vec3 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}
impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub m: [[f32; 4]; 4],
}

impl Mat4 {
    pub const fn identity() -> Self {
        Self {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn translation(offset: Vec3) -> Self {
        let mut result = Self::identity();
        result.m[0][3] = offset.x;
        result.m[1][3] = offset.y;
        result.m[2][3] = offset.z;
        result
    }

    pub fn scale(value: Vec3) -> Self {
        let mut result = Self::identity();
        result.m[0][0] = value.x;
        result.m[1][1] = value.y;
        result.m[2][2] = value.z;
        result
    }

    pub fn rotation_x(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, c, -s, 0.0],
                [0.0, s, c, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn rotation_y(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            m: [
                [c, 0.0, s, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [-s, 0.0, c, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn rotation_z(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            m: [
                [c, -s, 0.0, 0.0],
                [s, c, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn transform_point(self, point: Vec3) -> Vec3 {
        let result = self * Vec4::new(point.x, point.y, point.z, 1.0);
        if result.w.is_finite() && result.w.abs() > f32::EPSILON {
            Vec3::new(
                result.x / result.w,
                result.y / result.w,
                result.z / result.w,
            )
        } else {
            Vec3::new(f32::NAN, f32::NAN, f32::NAN)
        }
    }

    pub fn transform_direction(self, direction: Vec3) -> Vec3 {
        let result = self * Vec4::new(direction.x, direction.y, direction.z, 0.0);
        Vec3::new(result.x, result.y, result.z)
    }

    pub fn transform_normal(self, normal: Vec3) -> Vec3 {
        let m = self.m;
        let c00 = m[1][1] * m[2][2] - m[1][2] * m[2][1];
        let c01 = m[1][2] * m[2][0] - m[1][0] * m[2][2];
        let c02 = m[1][0] * m[2][1] - m[1][1] * m[2][0];
        let c10 = m[0][2] * m[2][1] - m[0][1] * m[2][2];
        let c11 = m[0][0] * m[2][2] - m[0][2] * m[2][0];
        let c12 = m[0][1] * m[2][0] - m[0][0] * m[2][1];
        let c20 = m[0][1] * m[1][2] - m[0][2] * m[1][1];
        let c21 = m[0][2] * m[1][0] - m[0][0] * m[1][2];
        let c22 = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        let determinant = m[0][0] * c00 + m[0][1] * c01 + m[0][2] * c02;
        if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
            return Vec3::ZERO;
        }
        Vec3::new(
            (c00 * normal.x + c01 * normal.y + c02 * normal.z) / determinant,
            (c10 * normal.x + c11 * normal.y + c12 * normal.z) / determinant,
            (c20 * normal.x + c21 * normal.y + c22 * normal.z) / determinant,
        )
        .normalized()
    }
}

impl Mul for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let mut result = [[0.0; 4]; 4];
        for (row, result_row) in result.iter_mut().enumerate() {
            for (column, value) in result_row.iter_mut().enumerate() {
                *value = (0..4)
                    .map(|index| self.m[row][index] * rhs.m[index][column])
                    .sum();
            }
        }
        Self { m: result }
    }
}

impl Mul<Vec4> for Mat4 {
    type Output = Vec4;
    fn mul(self, rhs: Vec4) -> Vec4 {
        let values = [rhs.x, rhs.y, rhs.z, rhs.w];
        Vec4::new(
            self.m[0].iter().zip(values).map(|(a, b)| a * b).sum(),
            self.m[1].iter().zip(values).map(|(a, b)| a * b).sum(),
            self.m[2].iter().zip(values).map(|(a, b)| a * b).sum(),
            self.m[3].iter().zip(values).map(|(a, b)| a * b).sum(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Mat4, Vec2, Vec3, Vec4};
    use std::f32::consts::{FRAC_PI_2, PI};

    #[test]
    fn vector_operations_and_safe_normalization() {
        assert_eq!(
            Vec2::new(1.0, 2.0) + Vec2::new(3.0, 4.0),
            Vec2::new(4.0, 6.0)
        );
        assert_eq!(Vec3::X.cross(Vec3::Y), Vec3::Z);
        assert_eq!(Vec3::X.dot(Vec3::Y), 0.0);
        assert_eq!(Vec3::new(3.0, 0.0, 0.0).normalized(), Vec3::X);
        assert_eq!(Vec3::ZERO.normalized(), Vec3::ZERO);
        assert_eq!(Vec3::new(f32::NAN, 0.0, 0.0).normalized(), Vec3::ZERO);
    }

    #[test]
    fn transforms_use_column_vectors() {
        let point = Mat4::translation(Vec3::new(2.0, 3.0, 4.0)).transform_point(Vec3::ZERO);
        assert_eq!(point, Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(
            Mat4::scale(Vec3::new(2.0, 3.0, 4.0)).transform_direction(Vec3::X),
            Vec3::new(2.0, 0.0, 0.0)
        );
        assert!((Mat4::rotation_z(FRAC_PI_2).transform_direction(Vec3::X).y - 1.0).abs() < 1e-5);
        assert!((Mat4::rotation_y(PI).transform_direction(Vec3::X).x + 1.0).abs() < 1e-5);
    }

    #[test]
    fn normals_use_inverse_transpose_for_non_uniform_scale() {
        let normal =
            Mat4::scale(Vec3::new(2.0, 3.0, 4.0)).transform_normal(Vec3::new(1.0, 1.0, 1.0));
        let expected = Vec3::new(0.5, 1.0 / 3.0, 0.25).normalized();
        assert!((normal.x - expected.x).abs() < 1e-5);
        assert!((normal.y - expected.y).abs() < 1e-5);
        assert!((normal.z - expected.z).abs() < 1e-5);
        assert_eq!(
            Mat4::scale(Vec3::new(0.0, 1.0, 1.0)).transform_normal(Vec3::Z),
            Vec3::ZERO
        );
    }

    #[test]
    fn identity_and_matrix_multiplication() {
        let transform =
            Mat4::translation(Vec3::new(1.0, 2.0, 3.0)) * Mat4::scale(Vec3::new(2.0, 2.0, 2.0));
        assert_eq!(Mat4::identity() * transform, transform);
        assert_eq!(
            transform * Vec4::new(1.0, 1.0, 1.0, 1.0),
            Vec4::new(3.0, 4.0, 5.0, 1.0)
        );
    }
}