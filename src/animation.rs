use crate::math::{Mat4, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct SimulationClock {
    time: f32,
    delta_time: f32,
    speed: f32,
    paused: bool,
}

impl SimulationClock {
    pub const fn new(speed: f32) -> Self {
        Self {
            time: 0.0,
            delta_time: 0.0,
            speed,
            paused: false,
        }
    }

    pub fn advance(&mut self, delta_time: f32) {
        let delta_time = if delta_time.is_finite() {
            delta_time.max(0.0)
        } else {
            0.0
        };
        self.delta_time = if self.paused { 0.0 } else { delta_time };
        self.time += self.delta_time * finite_or(self.speed, 1.0);
        if !self.time.is_finite() {
            self.time = 0.0;
        }
    }

    pub fn advance_manual(&mut self, delta_time: f32) {
        let delta_time = if delta_time.is_finite() {
            delta_time.max(0.0)
        } else {
            0.0
        };
        self.delta_time = delta_time;
        self.time += delta_time * finite_or(self.speed, 1.0);
        if !self.time.is_finite() {
            self.time = 0.0;
        }
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn set_speed(&mut self, speed: f32) {
        if speed.is_finite() {
            self.speed = speed;
        }
    }

    pub const fn speed(&self) -> f32 {
        self.speed
    }

    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    pub const fn time(&self) -> f32 {
        self.time
    }

    pub const fn delta_time(&self) -> f32 {
        self.delta_time
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Orbit {
    pub center: Vec3,
    pub radius: f32,
    pub angular_velocity: f32,
    pub phase: f32,
    pub inclination: f32,
}

impl Orbit {
    pub fn position_at(&self, time: f32) -> Vec3 {
        let radius = finite_or(self.radius, 0.0).max(0.0);
        let angle = finite_or(self.phase, 0.0)
            + finite_or(self.angular_velocity, 0.0) * finite_or(time, 0.0);
        let (sin_angle, cos_angle) = angle.sin_cos();
        let local = Vec3::new(radius * cos_angle, 0.0, radius * sin_angle);
        let inclined = Mat4::rotation_x(finite_or(self.inclination, 0.0))
            .transform_point(local);
        self.center + inclined
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RotationAxis {
    X,
    Y,
    Z,
}

#[derive(Clone, Copy, Debug)]
pub struct BodyMotion {
    pub orbit: Orbit,
    pub base_rotation: Mat4,
    pub scale: f32,
    pub rotation_axis: RotationAxis,
    pub rotation_speed: f32,
    pub rotation_phase: f32,
}

impl BodyMotion {
    pub fn model_matrix_at(&self, time: f32) -> Mat4 {
        let angle = finite_or(self.rotation_phase, 0.0)
            + finite_or(self.rotation_speed, 0.0) * finite_or(time, 0.0);
        let axial = match self.rotation_axis {
            RotationAxis::X => Mat4::rotation_x(angle),
            RotationAxis::Y => Mat4::rotation_y(angle),
            RotationAxis::Z => Mat4::rotation_z(angle),
        };
        Mat4::translation(self.orbit.position_at(time))
            * self.base_rotation
            * axial
            * Mat4::scale(Vec3::new(self.scale, self.scale, self.scale))
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}