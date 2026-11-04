use std::time::Duration;

use minifb::{Key, Window, WindowOptions};

use crate::framebuffer::Framebuffer;

pub struct CpuWindow {
    window: Window,
    pixels: Vec<u32>,
    width: usize,
    height: usize,
}

impl CpuWindow {
    pub fn new(title: &str, framebuffer: &Framebuffer) -> Result<Self, String> {
        let (width, height) = framebuffer.dimensions();
        let pixels = framebuffer_to_window_pixels(framebuffer);
        let window = Window::new(title, width, height, WindowOptions::default())
            .map_err(|error| error.to_string())?;
        Ok(Self { window, pixels, width, height })
    }

    pub fn is_open(&self) -> bool {
        self.window.is_open() && !self.window.is_key_down(Key::Escape)
    }

    pub fn present(&mut self) -> Result<(), String> {
        self.window
            .update_with_buffer(&self.pixels, self.width, self.height)
            .map_err(|error| error.to_string())
    }

    pub fn run_until_closed(&mut self) -> Result<(), String> {
        while self.is_open() {
            self.present()?;
            std::thread::sleep(Duration::from_millis(16));
        }
        Ok(())
    }
}

fn framebuffer_to_window_pixels(framebuffer: &Framebuffer) -> Vec<u32> {
    let (width, height) = framebuffer.dimensions();
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let [red, green, blue] = framebuffer
                .get_pixel(x, y)
                .unwrap_or(crate::color::Color::BLACK)
                .to_rgb8();
            pixels.push(u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue));
        }
    }
    pixels
}