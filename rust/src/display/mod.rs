pub mod buffer;
pub mod driver;
pub mod faces;
pub mod fonts;

use crate::config::DisplayConfig;
use crate::personality::Face;
use buffer::FrameBuffer;
use embedded_graphics::{
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
    last_hash: u64,
    pub flush_count: u32,
}

impl Screen {
    /// Create a new screen with the given display configuration.
    pub fn new(config: DisplayConfig) -> Self {
        Self {
            fb: FrameBuffer::new(DISPLAY_WIDTH, DISPLAY_HEIGHT),
            config,
            last_hash: 0,
            flush_count: 0,
        }
    }

    /// Clear the entire framebuffer to white.
    pub fn clear(&mut self) {
        self.fb.clear();
    }

    /// Draw a bull face sprite at (0, 16) — matches Python AO mode.
    /// Uses embedded 120x66 1-bit bitmap sprites from faces/eink/.
    pub fn draw_face(&mut self, face: &Face) {
        let bitmap = faces::bitmap_for_face(face);
        self.draw_bitmap(bitmap, 0, 16, faces::FACE_WIDTH, faces::FACE_HEIGHT);
    }

    /// Blit a 1-bit packed bitmap onto the framebuffer at (x, y).
    /// Bitmap format: MSB first, row-major, 1=black, 0=white.
    pub fn draw_bitmap(&mut self, data: &[u8], x: u32, y: u32, w: u32, h: u32) {
        let stride = ((w + 7) / 8) as usize;
        for row in 0..h {
            for col in 0..w {
                let byte_idx = (row as usize) * stride + (col as usize) / 8;
                let bit_idx = 7 - (col % 8);
                if byte_idx < data.len() && (data[byte_idx] >> bit_idx) & 1 == 1 {
                    let px = x + col;
                    let py = y + row;
                    if px < DISPLAY_WIDTH && py < DISPLAY_HEIGHT {
                        self.fb.set_pixel(px, py, BinaryColor::On);
                    }
                }
            }
        }
    }

    /// Draw the device name (12pt bold font). Python spec: name at (5, 20).
    pub fn draw_name(&mut self, name: &str) {
        let label = format!("{}>", name);
        let style = fonts::bold();
        // 12pt: ~10px ascent. Visual top y=20, baseline ~y=30.
        let _ = Text::new(&label, Point::new(5, 30), style).draw(&mut self.fb);
    }

    /// Draw bold text at arbitrary position (for boot screen, centered text).
    /// y is visual top of the text.
    pub fn draw_name_at(&mut self, text: &str, x: i32, y: i32) {
        let style = fonts::bold();
        let _ = Text::new(text, Point::new(x, y + 10), style).draw(&mut self.fb);
    }

    /// Draw a status message (10pt font) with word wrap.
    /// Python spec: status at (125, 20), max 20 chars per line.
    pub fn draw_status(&mut self, text: &str) {
        let style = fonts::medium();
        let max_chars = 17; // 125px available / 7px per ProFont 10pt char
        let line_height = 12; // 10pt font + 2px spacing
        let x = 125i32;
        let mut y = 28i32; // baseline for first line

        // Simple word wrap at max_chars
        let mut remaining = text;
        while !remaining.is_empty() && y < 90 {
            if remaining.len() <= max_chars {
                let _ = Text::new(remaining, Point::new(x, y), style).draw(&mut self.fb);
                break;
            }
            // Find last space within max_chars
            let break_at = remaining[..max_chars]
                .rfind(' ')
                .unwrap_or(max_chars);
            let (line, rest) = remaining.split_at(break_at);
            let _ = Text::new(line, Point::new(x, y), style).draw(&mut self.fb);
            remaining = rest.trim_start();
            y += line_height;
        }
    }

    /// Draw raw text at (x, y) using small font (9pt).
    /// y is the visual top of the text.
    pub fn draw_text(&mut self, text: &str, x: i32, y: i32) {
        let style = fonts::small();
        // 9pt: ~7px ascent.
        let _ = Text::new(text, Point::new(x, y + 7), style).draw(&mut self.fb);
    }

    /// Draw a "LABEL: value" pair using small font (9pt).
    /// y is the visual top of the text.
    pub fn draw_labeled_value(&mut self, label: &str, value: &str, x: i32, y: i32) {
        let combined = format!("{}: {}", label, value);
        let style = fonts::small();
        let _ = Text::new(&combined, Point::new(x, y + 7), style).draw(&mut self.fb);
    }

    /// Draw a horizontal line (1px tall) for layout dividers.
    pub fn draw_hline(&mut self, x: i32, y: i32, width: u32) {
        for px in 0..width {
            let xi = x + px as i32;
            if xi >= 0 && (xi as u32) < DISPLAY_WIDTH && y >= 0 && (y as u32) < DISPLAY_HEIGHT {
                self.fb.set_pixel(xi as u32, y as u32, BinaryColor::On);
            }
        }
    }

    /// Set a single pixel in the framebuffer.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: BinaryColor) {
        if x < DISPLAY_WIDTH && y < DISPLAY_HEIGHT {
            self.fb.set_pixel(x, y, color);
        }
    }

    /// Flush the framebuffer to the physical display.
    /// On non-aarch64 platforms this is a no-op.
    /// Logs errors instead of panicking — the display can fail transiently.
    /// Only flushes if content has changed since last flush.
    pub fn flush(&mut self) {
        let new_hash = self.fb.content_hash();
        if new_hash == self.last_hash {
            return; // No change, skip refresh
        }
        self.last_hash = new_hash;
        self.flush_count += 1;
        if let Err(e) = driver::flush_to_hardware(&self.fb, &self.config) {
            log::error!("display flush failed: {e}");
        }
    }

    /// Force a full display refresh regardless of content change.
    pub fn force_flush(&mut self) {
        self.last_hash = 0;
        self.flush();
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
        // Name at (5, 30 baseline) — pixels in y range ~21..31
        let has_pixels = (0..DISPLAY_WIDTH)
            .any(|x| (20..35).any(|y| screen.fb.get_pixel(x, y) == BinaryColor::On));
        assert!(has_pixels, "draw_name should set pixels in the name zone (y 20-35)");
    }

    #[test]
    fn test_draw_status_writes_pixels() {
        let mut screen = Screen::new(test_config());
        screen.draw_status("testing");
        // Status at (125, 30 baseline) — pixels in y range ~21..31
        let has_pixels = (125..DISPLAY_WIDTH).any(|x| {
            (20..35).any(|y| screen.fb.get_pixel(x, y) == BinaryColor::On)
        });
        assert!(has_pixels, "draw_status should set pixels in the status zone (y 20-35)");
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
    fn test_draw_hline() {
        let mut screen = Screen::new(test_config());
        screen.draw_hline(0, 14, 250);
        // All pixels at y=14 should be set
        let count = (0..250u32)
            .filter(|&x| screen.fb.get_pixel(x, 14) == BinaryColor::On)
            .count();
        assert_eq!(count, 250, "hline should set all 250 pixels at y=14");
        // Pixel above/below should be clear
        assert_eq!(screen.fb.get_pixel(0, 13), BinaryColor::Off);
        assert_eq!(screen.fb.get_pixel(0, 15), BinaryColor::Off);
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
        let mut screen = Screen::new(test_config());
        screen.flush(); // Should be no-op on non-Pi
    }

    #[test]
    fn test_empty_framebuffer_flush() {
        // Flushing without any draws should succeed and produce an all-white buffer.
        let mut screen = Screen::new(test_config());
        assert_eq!(screen.fb.count_set_pixels(), 0);
        screen.flush(); // should not panic
    }

    #[test]
    fn test_draw_empty_strings() {
        let mut screen = Screen::new(test_config());
        screen.draw_name("");
        screen.draw_status("");
        screen.draw_labeled_value("", "", 0, 0);
        // No crash, may or may not set pixels (font draws colon separator)
    }
}
