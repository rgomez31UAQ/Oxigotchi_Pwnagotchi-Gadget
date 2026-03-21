pub mod buffer;
#[cfg(target_arch = "aarch64")]
pub mod driver;

use crate::config::DisplayConfig;
use crate::personality::Face;
use buffer::FrameBuffer;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

/// Width of the Waveshare 2.13" V4 display in pixels.
pub const DISPLAY_WIDTH: u32 = 250;
/// Height of the Waveshare 2.13" V4 display in pixels.
pub const DISPLAY_HEIGHT: u32 = 122;

/// High-level screen abstraction over the e-ink framebuffer.
pub struct Screen {
    pub fb: FrameBuffer,
    pub config: DisplayConfig,
}

impl Screen {
    /// Create a new screen with the given display configuration.
    pub fn new(config: DisplayConfig) -> Self {
        Self {
            fb: FrameBuffer::new(DISPLAY_WIDTH, DISPLAY_HEIGHT),
            config,
        }
    }

    /// Clear the entire framebuffer to white.
    pub fn clear(&mut self) {
        self.fb.clear();
    }

    /// Draw a kaomoji face at the center of the display.
    pub fn draw_face(&mut self, face: &Face) {
        let text = face.as_str();
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        // Center the face text roughly in the middle of the display
        let text_width = text.len() as i32 * 6;
        let x = ((DISPLAY_WIDTH as i32) - text_width) / 2;
        let y = (DISPLAY_HEIGHT as i32) / 2;
        let _ = Text::new(text, Point::new(x, y), style).draw(&mut self.fb);
    }

    /// Draw the device name at the top-left corner (e.g. "oxigotchi>").
    pub fn draw_name(&mut self, name: &str) {
        let label = format!("{}>", name);
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        let _ = Text::new(&label, Point::new(0, 10), style).draw(&mut self.fb);
    }

    /// Draw a status message near the bottom of the display.
    pub fn draw_status(&mut self, text: &str) {
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        let _ = Text::new(text, Point::new(0, DISPLAY_HEIGHT as i32 - 2), style).draw(&mut self.fb);
    }

    /// Draw a "LABEL: value" pair at a given (x, y) pixel position.
    pub fn draw_labeled_value(&mut self, label: &str, value: &str, x: i32, y: i32) {
        let combined = format!("{}: {}", label, value);
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        let _ = Text::new(&combined, Point::new(x, y), style).draw(&mut self.fb);
    }

    /// Set a single pixel in the framebuffer.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: BinaryColor) {
        if x < DISPLAY_WIDTH && y < DISPLAY_HEIGHT {
            self.fb.set_pixel(x, y, color);
        }
    }

    /// Flush the framebuffer to the physical display.
    /// On non-aarch64 platforms this is a no-op.
    pub fn flush(&self) {
        #[cfg(target_arch = "aarch64")]
        {
            driver::flush_to_hardware(&self.fb, &self.config);
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            log::debug!("flush: no-op on non-Pi platform");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DisplayConfig;

    fn test_config() -> DisplayConfig {
        DisplayConfig {
            enabled: true,
            display_type: "waveshare_4".into(),
            rotation: 0,
        }
    }

    #[test]
    fn test_screen_new() {
        let screen = Screen::new(test_config());
        assert_eq!(screen.fb.width, DISPLAY_WIDTH);
        assert_eq!(screen.fb.height, DISPLAY_HEIGHT);
    }

    #[test]
    fn test_screen_clear() {
        let mut screen = Screen::new(test_config());
        // Set a pixel then clear
        screen.set_pixel(10, 10, BinaryColor::On);
        assert_eq!(screen.fb.get_pixel(10, 10), BinaryColor::On);
        screen.clear();
        assert_eq!(screen.fb.get_pixel(10, 10), BinaryColor::Off);
    }

    #[test]
    fn test_draw_face_does_not_panic() {
        let mut screen = Screen::new(test_config());
        for face in Face::all() {
            screen.clear();
            screen.draw_face(&face);
        }
    }

    #[test]
    fn test_draw_name_writes_pixels() {
        let mut screen = Screen::new(test_config());
        screen.draw_name("oxi");
        // At least some pixels should be set in the top area
        let has_pixels = (0..DISPLAY_WIDTH)
            .any(|x| (0..12).any(|y| screen.fb.get_pixel(x, y) == BinaryColor::On));
        assert!(has_pixels, "draw_name should set pixels in the top area");
    }

    #[test]
    fn test_draw_status_writes_pixels() {
        let mut screen = Screen::new(test_config());
        screen.draw_status("testing");
        let has_pixels = (0..DISPLAY_WIDTH).any(|x| {
            ((DISPLAY_HEIGHT - 12)..DISPLAY_HEIGHT)
                .any(|y| screen.fb.get_pixel(x, y) == BinaryColor::On)
        });
        assert!(has_pixels, "draw_status should set pixels near the bottom");
    }

    #[test]
    fn test_draw_labeled_value() {
        let mut screen = Screen::new(test_config());
        screen.draw_labeled_value("CH", "6", 0, 30);
        let has_pixels = (0..60)
            .any(|x| (20..40).any(|y| screen.fb.get_pixel(x, y) == BinaryColor::On));
        assert!(has_pixels, "draw_labeled_value should set pixels");
    }

    #[test]
    fn test_set_pixel_bounds() {
        let mut screen = Screen::new(test_config());
        // In-bounds
        screen.set_pixel(0, 0, BinaryColor::On);
        assert_eq!(screen.fb.get_pixel(0, 0), BinaryColor::On);
        // Out-of-bounds should not panic
        screen.set_pixel(DISPLAY_WIDTH, 0, BinaryColor::On);
        screen.set_pixel(0, DISPLAY_HEIGHT, BinaryColor::On);
        screen.set_pixel(999, 999, BinaryColor::On);
    }

    #[test]
    fn test_flush_no_panic() {
        let screen = Screen::new(test_config());
        screen.flush(); // Should be no-op on non-Pi
    }
}
