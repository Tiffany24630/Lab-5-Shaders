#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub const BLACK: Self = Self::new_const(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::new_const(1.0, 1.0, 1.0);

    pub const fn new_const(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self {
            r: sanitize_component(r),
            g: sanitize_component(g),
            b: sanitize_component(b),
        }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = if t.is_finite() { t } else { 0.0 };
        Self::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
        )
    }

    pub fn to_rgb8(self) -> [u8; 3] {
        [to_u8(self.r), to_u8(self.g), to_u8(self.b)]
    }

    pub fn linear_to_srgb(self) -> Self {
        Self::new(
            linear_to_srgb_component(self.r),
            linear_to_srgb_component(self.g),
            linear_to_srgb_component(self.b),
        )
    }

    pub fn srgb_to_linear(self) -> Self {
        Self::new(
            srgb_to_linear_component(self.r),
            srgb_to_linear_component(self.g),
            srgb_to_linear_component(self.b),
        )
    }
}

fn sanitize_component(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn to_u8(value: f32) -> u8 {
    let normalized = sanitize_component(value);
    (normalized * 255.0).round() as u8
}

fn linear_to_srgb_component(value: f32) -> f32 {
    let value = sanitize_component(value);
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_to_linear_component(value: f32) -> f32 {
    let value = sanitize_component(value);
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::Color;

    #[test]
    fn lerp_interpolates_components() {
        let result = Color::BLACK.lerp(Color::WHITE, 0.5);
        assert_eq!(result, Color::new(0.5, 0.5, 0.5));
    }

    #[test]
    fn rgb8_conversion_is_safe_and_clamped() {
        assert_eq!(Color::new(-1.0, 0.5, 2.0).to_rgb8(), [0, 128, 255]);
        assert_eq!(
            Color::new(f32::NAN, f32::INFINITY, f32::NEG_INFINITY),
            Color::BLACK
        );
    }

    #[test]
    fn color_space_helpers_are_inverse_approximately() {
        let linear = Color::new(0.5, 0.2, 0.8);
        let recovered = linear.linear_to_srgb().srgb_to_linear();
        assert!((recovered.r - linear.r).abs() < 1e-5);
        assert!((recovered.g - linear.g).abs() < 1e-5);
        assert!((recovered.b - linear.b).abs() < 1e-5);
        assert_eq!(Color::new(0.5, 0.5, 0.5).to_rgb8(), [128, 128, 128]);
    }
}