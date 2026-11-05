use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use crate::color::Color;

pub struct Framebuffer {
    width: usize,
    height: usize,
    colors: Vec<Color>,
    depths: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DepthPass {
    index: usize,
    depth: f32,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("Framebuffer dimensions must be greater than zero".into());
        }
        let length = width
            .checked_mul(height)
            .ok_or_else(|| "Framebuffer dimensions overflow pixel count".to_string())?;

        Ok(Self {
            width,
            height,
            colors: vec![Color::BLACK; length],
            depths: vec![f32::INFINITY; length],
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn clear(&mut self, color: Color) {
        self.colors.fill(color);
        self.clear_depth();
    }

    pub fn clear_color(&mut self, color: Color) {
        self.colors.fill(color);
    }

    pub fn clear_depth(&mut self) {
        self.depths.fill(f32::INFINITY);
    }

    pub fn get_pixel(&self, x: usize, y: usize) -> Option<Color> {
        self.index(x, y).map(|index| self.colors[index])
    }

    pub fn color_buffer(&self) -> &[Color] {
        &self.colors
    }

    pub fn depth_buffer(&self) -> &[f32] {
        &self.depths
    }

    pub fn restore_colors(&mut self, colors: &[Color]) -> bool {
        if colors.len() != self.colors.len() {
            return false;
        }
        self.colors.copy_from_slice(colors);
        true
    }

    pub fn set_color_pixel(&mut self, x: usize, y: usize, color: Color) -> bool {
        let Some(index) = self.index(x, y) else {
            return false;
        };
        self.colors[index] = color;
        true
    }

    pub fn get_depth(&self, x: usize, y: usize) -> Option<f32> {
        self.index(x, y).map(|index| self.depths[index])
    }

    pub fn depth_test(&self, x: usize, y: usize, depth: f32) -> bool {
        self.index(x, y)
            .map(|index| depth.is_finite() && depth < self.depths[index])
            .unwrap_or(false)
    }

    pub(crate) fn depth_test_and_reserve(&self, x: usize, y: usize, depth: f32) -> Option<DepthPass> {
        let index = self.index(x, y)?;
        (depth.is_finite() && depth < self.depths[index]).then_some(DepthPass { index, depth })
    }

    pub(crate) fn write_reserved_pixel(&mut self, pass: DepthPass, color: Color) -> bool {
        self.colors[pass.index] = color;
        self.depths[pass.index] = pass.depth;
        true
    }

    pub fn depth_test_and_write_pixel(&mut self, x: usize, y: usize, color: Color, depth: f32) -> bool {
        let Some(index) = self.index(x, y) else {
            return false;
        };
        if !depth.is_finite() || depth >= self.depths[index] {
            return false;
        }
        self.colors[index] = color;
        self.depths[index] = depth;
        true
    }

    pub fn write_pixel(&mut self, x: usize, y: usize, color: Color, depth: f32) -> bool {
        let Some(index) = self.index(x, y) else {
            return false;
        };
        if !depth.is_finite() || depth >= self.depths[index] {
            return false;
        }
        self.colors[index] = color;
        self.depths[index] = depth;
        true
    }

    pub fn serialize_ppm(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + self.colors.len() * 3);
        bytes.extend_from_slice(format!("P6\n{} {}\n255\n", self.width, self.height).as_bytes());
        for color in &self.colors {
            bytes.extend_from_slice(&color.to_rgb8());
        }
        bytes
    }

    pub fn save_ppm<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(&self.serialize_ppm())
    }

    fn index(&self, x: usize, y: usize) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        y.checked_mul(self.width)?.checked_add(x)
    }
}

#[cfg(test)]
mod tests {
    use super::Framebuffer;
    use crate::color::Color;

    #[test]
    fn initializes_and_clears() {
        let mut framebuffer = Framebuffer::new(2, 2).unwrap();
        assert_eq!(framebuffer.dimensions(), (2, 2));
        assert_eq!(framebuffer.get_depth(0, 0), Some(f32::INFINITY));
        framebuffer.clear(Color::new(0.2, 0.3, 0.4));
        assert_eq!(framebuffer.get_pixel(1, 1), Some(Color::new(0.2, 0.3, 0.4)));
        assert_eq!(framebuffer.get_depth(1, 1), Some(f32::INFINITY));
    }

    #[test]
    fn validates_access_and_depth() {
        let mut framebuffer = Framebuffer::new(2, 2).unwrap();
        assert_eq!(framebuffer.get_pixel(2, 0), None);
        assert!(framebuffer.write_pixel(0, 0, Color::WHITE, 0.5));
        assert!(!framebuffer.write_pixel(0, 0, Color::BLACK, 0.7));
        assert!(framebuffer.write_pixel(0, 0, Color::BLACK, 0.2));
        assert!(!framebuffer.write_pixel(0, 0, Color::WHITE, f32::NAN));
        assert!(!framebuffer.write_pixel(0, 0, Color::WHITE, f32::INFINITY));
    }

    #[test]
    fn rejects_invalid_dimensions() {
        assert!(Framebuffer::new(0, 1).is_err());
        assert!(Framebuffer::new(usize::MAX, 2).is_err());
    }

    #[test]
    fn serializes_ppm_in_row_major_rgb_order() {
        let mut framebuffer = Framebuffer::new(2, 1).unwrap();
        framebuffer.write_pixel(0, 0, Color::new(1.0, 0.0, 0.0), 0.0);
        framebuffer.write_pixel(1, 0, Color::new(0.0, 1.0, 0.0), 0.0);
        let ppm = framebuffer.serialize_ppm();
        assert!(ppm.starts_with(b"P6\n2 1\n255\n"));
        assert_eq!(&ppm[11..], &[255, 0, 0, 0, 255, 0]);
    }
}