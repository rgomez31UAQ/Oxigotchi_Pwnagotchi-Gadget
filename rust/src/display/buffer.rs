use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};

/// Raw 1-bit framebuffer for the e-ink display.
/// Pixels are packed 8 per byte, MSB first, row-major order.
/// A set bit (1) = black, cleared bit (0) = white.
pub struct FrameBuffer {
    pub width: u32,
    pub height: u32,
    /// Packed pixel data. Each byte holds 8 horizontal pixels.
    pub data: Vec<u8>,
}

impl FrameBuffer {
    /// Create a new framebuffer initialized to white (all zeros).
    pub fn new(width: u32, height: u32) -> Self {
        let stride = Self::stride(width);
        let data = vec![0u8; (stride * height) as usize];
        Self {
            width,
            height,
            data,
        }
    }

    /// Bytes per row (each row is padded to full bytes).
    pub fn stride(width: u32) -> u32 {
        (width + 7) / 8
    }

    /// Clear all pixels to white (0).
    pub fn clear(&mut self) {
        self.data.fill(0);
    }

    /// Set a single pixel. `color = On` means black (bit=1).
    pub fn set_pixel(&mut self, x: u32, y: u32, color: BinaryColor) {
        if x >= self.width || y >= self.height {
            return;
        }
        let stride = Self::stride(self.width) as usize;
        let byte_idx = (y as usize) * stride + (x as usize) / 8;
        let bit_idx = 7 - (x % 8);
        match color {
            BinaryColor::On => self.data[byte_idx] |= 1 << bit_idx,
            BinaryColor::Off => self.data[byte_idx] &= !(1 << bit_idx),
        }
    }

    /// Get the color of a single pixel.
    pub fn get_pixel(&self, x: u32, y: u32) -> BinaryColor {
        if x >= self.width || y >= self.height {
            return BinaryColor::Off;
        }
        let stride = Self::stride(self.width) as usize;
        let byte_idx = (y as usize) * stride + (x as usize) / 8;
        let bit_idx = 7 - (x % 8);
        if self.data[byte_idx] & (1 << bit_idx) != 0 {
            BinaryColor::On
        } else {
            BinaryColor::Off
        }
    }

    /// Return total number of set (black) pixels. Useful for tests.
    pub fn count_set_pixels(&self) -> u32 {
        self.data.iter().map(|b| b.count_ones()).sum()
    }

    /// Return raw data slice for hardware transmission.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// Implement `DrawTarget` so embedded-graphics can draw directly into our buffer.
impl DrawTarget for FrameBuffer {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels {
            if coord.x >= 0 && coord.y >= 0 {
                self.set_pixel(coord.x as u32, coord.y as u32, color);
            }
        }
        Ok(())
    }
}

impl OriginDimensions for FrameBuffer {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer_is_white() {
        let fb = FrameBuffer::new(250, 122);
        assert_eq!(fb.count_set_pixels(), 0);
        assert_eq!(fb.get_pixel(0, 0), BinaryColor::Off);
        assert_eq!(fb.get_pixel(249, 121), BinaryColor::Off);
    }

    #[test]
    fn test_stride_calculation() {
        assert_eq!(FrameBuffer::stride(250), 32); // 250/8 = 31.25 -> 32
        assert_eq!(FrameBuffer::stride(8), 1);
        assert_eq!(FrameBuffer::stride(1), 1);
        assert_eq!(FrameBuffer::stride(9), 2);
    }

    #[test]
    fn test_set_get_pixel() {
        let mut fb = FrameBuffer::new(16, 8);
        fb.set_pixel(0, 0, BinaryColor::On);
        assert_eq!(fb.get_pixel(0, 0), BinaryColor::On);
        assert_eq!(fb.get_pixel(1, 0), BinaryColor::Off);

        fb.set_pixel(7, 0, BinaryColor::On);
        assert_eq!(fb.get_pixel(7, 0), BinaryColor::On);

        fb.set_pixel(8, 0, BinaryColor::On);
        assert_eq!(fb.get_pixel(8, 0), BinaryColor::On);
        assert_eq!(fb.get_pixel(9, 0), BinaryColor::Off);
    }

    #[test]
    fn test_pixel_across_rows() {
        let mut fb = FrameBuffer::new(16, 4);
        fb.set_pixel(5, 0, BinaryColor::On);
        fb.set_pixel(5, 1, BinaryColor::On);
        fb.set_pixel(5, 3, BinaryColor::On);
        assert_eq!(fb.count_set_pixels(), 3);
        assert_eq!(fb.get_pixel(5, 2), BinaryColor::Off);
    }

    #[test]
    fn test_clear() {
        let mut fb = FrameBuffer::new(16, 8);
        fb.set_pixel(3, 3, BinaryColor::On);
        fb.set_pixel(10, 5, BinaryColor::On);
        assert_eq!(fb.count_set_pixels(), 2);
        fb.clear();
        assert_eq!(fb.count_set_pixels(), 0);
    }

    #[test]
    fn test_out_of_bounds() {
        let mut fb = FrameBuffer::new(16, 8);
        fb.set_pixel(16, 0, BinaryColor::On); // out of bounds x
        fb.set_pixel(0, 8, BinaryColor::On); // out of bounds y
        assert_eq!(fb.count_set_pixels(), 0);
        assert_eq!(fb.get_pixel(16, 0), BinaryColor::Off);
    }

    #[test]
    fn test_draw_target() {
        use embedded_graphics::prelude::*;
        use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
        let mut fb = FrameBuffer::new(16, 8);
        let style = PrimitiveStyle::with_fill(BinaryColor::On);
        let _ = Rectangle::new(Point::new(0, 0), Size::new(4, 4))
            .into_styled(style)
            .draw(&mut fb);
        // 4x4 rectangle = 16 pixels
        assert_eq!(fb.count_set_pixels(), 16);
    }

    #[test]
    fn test_as_bytes_length() {
        let fb = FrameBuffer::new(250, 122);
        assert_eq!(fb.as_bytes().len(), 32 * 122);
    }

    #[test]
    fn test_unset_pixel() {
        let mut fb = FrameBuffer::new(16, 8);
        fb.set_pixel(5, 3, BinaryColor::On);
        assert_eq!(fb.get_pixel(5, 3), BinaryColor::On);
        fb.set_pixel(5, 3, BinaryColor::Off);
        assert_eq!(fb.get_pixel(5, 3), BinaryColor::Off);
    }
}
