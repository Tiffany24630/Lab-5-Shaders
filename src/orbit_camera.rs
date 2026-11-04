use std::f32::consts::{FRAC_PI_2, PI};

use crate::camera::Camera;
use crate::math::Vec3;
use crate::scene::BodyId;

const PITCH_LIMIT: f32 = 0.01;
pub const PITCH_MIN: f32 = -FRAC_PI_2 + PITCH_LIMIT;
pub const PITCH_MAX: f32 = FRAC_PI_2 - PITCH_LIMIT;
pub const DISTANCE_MIN: f32 = 1.5;
pub const DISTANCE_MAX: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCameraController {
    pub target_id: BodyId,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

impl OrbitCameraController {
    pub fn new(target_id: BodyId, fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        Self {
            target_id,
            yaw: 0.0,
            pitch: 0.0,
            distance: DISTANCE_MIN,
            fov_y,
            aspect,
            near,
            far,
        }
    }

    pub fn set_target(&mut self, target_id: BodyId) {
        self.target_id = target_id;
    }

    pub fn rotate_yaw(&mut self, delta: f32) {
        if delta.is_finite() {
            self.yaw = normalize_yaw(self.yaw + delta);
        }
    }

    pub fn rotate_pitch(&mut self, delta: f32) {
        if delta.is_finite() {
            self.pitch = (self.pitch + delta).clamp(PITCH_MIN, PITCH_MAX);
        }
    }

    pub fn zoom(&mut self, delta: f32) {
        if delta.is_finite() {
            self.distance = (self.distance + delta).clamp(DISTANCE_MIN, DISTANCE_MAX);
        }
    }

    pub fn set_orbit(&mut self, yaw: f32, pitch: f32, distance: f32) {
        if yaw.is_finite() {
            self.yaw = normalize_yaw(yaw);
        }
        if pitch.is_finite() {
            self.pitch = pitch.clamp(PITCH_MIN, PITCH_MAX);
        }
        if distance.is_finite() {
            self.distance = distance.clamp(DISTANCE_MIN, DISTANCE_MAX);
        }
    }

    pub fn camera_for_target(&self, target: Vec3) -> Result<Camera, String> {
        if !target.is_finite() {
            return Err("Orbit camera target must be finite".into());
        }
        let distance = self.distance.clamp(DISTANCE_MIN, DISTANCE_MAX);
        let pitch = self.pitch.clamp(PITCH_MIN, PITCH_MAX);
        let horizontal = distance * pitch.cos();
        let offset = Vec3::new(
            horizontal * self.yaw.cos(),
            distance * pitch.sin(),
            horizontal * self.yaw.sin(),
        );
        Camera::new(
            target + offset,
            target,
            Vec3::Y,
            self.fov_y,
            self.aspect,
            self.near,
            self.far,
        )
    }
}

fn normalize_yaw(yaw: f32) -> f32 {
    let wrapped = (yaw + PI).rem_euclid(2.0 * PI) - PI;
    if wrapped.is_finite() { wrapped } else { 0.0 }
}