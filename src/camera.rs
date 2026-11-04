use std::f32::consts::PI;

use crate::math::{Mat4, Vec3, Vec4};

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn new(
        position: Vec3,
        target: Vec3,
        up: Vec3,
        fov_y: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> Result<Self, String> {
        let camera = Self {
            position,
            target,
            up,
            fov_y,
            aspect,
            near,
            far,
        };
        camera.validate()?;
        Ok(camera)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.position.is_finite()
            || !self.target.is_finite()
            || !self.up.is_finite()
            || !self.fov_y.is_finite()
            || !self.aspect.is_finite()
            || !self.near.is_finite()
            || !self.far.is_finite()
        {
            return Err("Camera parameters must be finite".into());
        }
        if self.fov_y <= 0.0
            || self.fov_y >= PI
            || self.aspect <= 0.0
            || self.near <= 0.0
            || self.far <= self.near
        {
            return Err("Camera perspective parameters are invalid".into());
        }
        let forward = (self.target - self.position).normalized();
        if forward == Vec3::ZERO || forward.cross(self.up).length() <= f32::EPSILON {
            return Err("Camera direction and up vector must not be parallel".into());
        }
        Ok(())
    }

    pub fn view_matrix(&self) -> Mat4 {
        let forward = (self.target - self.position).normalized();
        let right = forward.cross(self.up).normalized();
        let true_up = right.cross(forward).normalized();
        Mat4 {
            m: [
                [right.x, right.y, right.z, -right.dot(self.position)],
                [true_up.x, true_up.y, true_up.z, -true_up.dot(self.position)],
                [
                    -forward.x,
                    -forward.y,
                    -forward.z,
                    forward.dot(self.position),
                ],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    // Right-handed camera: it looks down -Z and maps near/far to NDC z -1/+1.
    pub fn projection_matrix(&self) -> Mat4 {
        let focal = 1.0 / (self.fov_y * 0.5).tan();
        let range = self.near - self.far;
        Mat4 {
            m: [
                [focal / self.aspect, 0.0, 0.0, 0.0],
                [0.0, focal, 0.0, 0.0],
                [
                    0.0,
                    0.0,
                    (self.far + self.near) / range,
                    (2.0 * self.far * self.near) / range,
                ],
                [0.0, 0.0, -1.0, 0.0],
            ],
        }
    }

    pub fn view_projection_matrix(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    pub fn to_ndc(&self, clip: Vec4) -> Option<Vec3> {
        if !clip.x.is_finite()
            || !clip.y.is_finite()
            || !clip.z.is_finite()
            || !clip.w.is_finite()
            || clip.w.abs() <= f32::EPSILON
        {
            return None;
        }
        let result = Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
        result.is_finite().then_some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::Camera;
    use crate::math::{Vec3, Vec4};
    use std::f32::consts::FRAC_PI_2;

    fn camera() -> Camera {
        Camera::new(
            Vec3::new(0.0, 0.0, 3.0),
            Vec3::ZERO,
            Vec3::Y,
            FRAC_PI_2,
            1.0,
            0.1,
            100.0,
        )
        .unwrap()
    }
    #[test]
    fn look_at_points_forward_along_negative_z() {
        let view = camera().view_matrix();
        let point = view.transform_point(Vec3::ZERO);
        assert!((point.z + 3.0).abs() < 1e-5);
    }
    #[test]
    fn projection_produces_valid_ndc() {
        let camera = camera();
        let clip = camera.projection_matrix() * Vec4::new(0.0, 0.0, -3.0, 1.0);
        let ndc = camera.to_ndc(clip).unwrap();
        assert!(ndc.x.abs() < 1e-5 && ndc.y.abs() < 1e-5 && ndc.z > -1.0 && ndc.z < 1.0);
    }
    #[test]
    fn invalid_parameters_are_rejected() {
        assert!(Camera::new(Vec3::ZERO, Vec3::ZERO, Vec3::Y, 1.0, 1.0, 0.1, 1.0).is_err());
        assert!(Camera::new(Vec3::Z, Vec3::ZERO, Vec3::Y, 0.0, 1.0, 0.1, 1.0).is_err());
        assert!(Camera::new(Vec3::Z, Vec3::ZERO, Vec3::Y, 1.0, 1.0, 2.0, 1.0).is_err());
    }
}