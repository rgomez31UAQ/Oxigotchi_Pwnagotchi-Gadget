/// SPI e-ink driver for Waveshare 2.13" V4 (SSD1680 controller).
///
/// On `aarch64` (Raspberry Pi), uses real rppal SPI/GPIO.
/// On other platforms, uses a mock backend for testing.

use std::cell::{Cell, RefCell};

use super::buffer::FrameBuffer;
use crate::config::DisplayConfig;

// ── Display geometry ────────────────────────────────────────────────
/// Physical pixel width of the Waveshare 2.13" V4 panel.
pub const EPD_WIDTH: u32 = 122;
/// Physical pixel height (long axis).
pub const EPD_HEIGHT: u32 = 250;
/// Bytes per scanline in SSD1680 RAM (width / 8, rounded up).
pub const EPD_WIDTH_BYTES: u32 = (EPD_WIDTH + 7) / 8; // 16

// ── GPIO pin assignments (BCM numbering) ────────────────────────────
pub const RST_PIN: u8 = 17;
pub const DC_PIN: u8 = 25;
pub const CS_PIN: u8 = 8;
pub const BUSY_PIN: u8 = 24;

// ── SSD1680 command constants ───────────────────────────────────────
pub const CMD_SW_RESET: u8 = 0x12;
pub const CMD_DRIVER_OUTPUT: u8 = 0x01;
pub const CMD_DATA_ENTRY_MODE: u8 = 0x11;
pub const CMD_SET_RAM_X_RANGE: u8 = 0x44;
pub const CMD_SET_RAM_Y_RANGE: u8 = 0x45;
pub const CMD_SET_RAM_X_COUNTER: u8 = 0x4E;
pub const CMD_SET_RAM_Y_COUNTER: u8 = 0x4F;
pub const CMD_WRITE_RAM_BW: u8 = 0x24;
pub const CMD_WRITE_RAM_RED: u8 = 0x26;
pub const CMD_DISPLAY_UPDATE_CTRL2: u8 = 0x22;
pub const CMD_MASTER_ACTIVATION: u8 = 0x20;
pub const CMD_BORDER_WAVEFORM: u8 = 0x3C;
pub const CMD_DISPLAY_UPDATE_CTRL1: u8 = 0x21;
pub const CMD_TEMP_SENSOR: u8 = 0x18;
pub const CMD_DEEP_SLEEP: u8 = 0x10;

/// Full refresh update sequence byte (for command 0x22).
pub const UPDATE_FULL: u8 = 0xF7;
/// Partial refresh update sequence byte (for command 0x22).
pub const UPDATE_PARTIAL: u8 = 0xFF;

/// Maximum time (in milliseconds) to wait for BUSY pin to go low.
pub const BUSY_TIMEOUT_MS: u64 = 5_000;

// ── Refresh mode ────────────────────────────────────────────────────

/// Whether to do a full or partial display refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshMode {
    /// Full refresh: clears ghosting, flashes black/white. Takes ~2s.
    Full,
    /// Partial refresh: faster (~0.3s), may accumulate ghosting.
    Partial,
}

// ── SPI transfer log (for testing) ──────────────────────────────────

/// A single SPI bus transfer: either a command byte or a data payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpiTransfer {
    /// DC=low, then SPI write of one command byte.
    Command(u8),
    /// DC=high, then SPI write of data bytes.
    Data(Vec<u8>),
    /// BUSY pin wait.
    WaitBusy,
    /// Hardware reset pulse (full: HIGH 20ms, LOW 2ms, HIGH 20ms).
    Reset,
    /// Short reset pulse for partial refresh (LOW 1ms, HIGH, no trailing delay).
    PartialReset,
}

// ── HAL trait (hardware abstraction) ────────────────────────────────

/// Hardware abstraction for SPI + GPIO needed by the SSD1680 driver.
/// On real hardware this wraps rppal; in tests it records transfers.
pub trait Ssd1680Hal {
    /// Send a command byte (DC=low, SPI write).
    fn send_command(&mut self, cmd: u8) -> Result<(), String>;
    /// Send data bytes (DC=high, SPI write).
    fn send_data(&mut self, data: &[u8]) -> Result<(), String>;
    /// Wait for BUSY pin to go low, with timeout.
    fn wait_busy(&self) -> Result<(), String>;
    /// Pulse the RST pin (high-low-high).
    fn hardware_reset(&mut self) -> Result<(), String>;
    /// Short reset pulse for partial refresh (RST LOW 1ms, HIGH, no trailing delay).
    fn partial_reset(&mut self) -> Result<(), String>;
}

// ── Mock HAL (all platforms) ────────────────────────────────────────

/// Mock HAL that records all transfers for test assertions.
/// Uses Cell/RefCell for interior mutability in wait_busy(&self).
#[derive(Debug)]
pub struct MockHal {
    /// Ordered log of all transfers.
    pub transfers: RefCell<Vec<SpiTransfer>>,
    /// If set, wait_busy will fail after this many successful calls.
    pub busy_fail_after: Option<usize>,
    /// Counter for wait_busy calls.
    busy_count: Cell<usize>,
}

impl Default for MockHal {
    fn default() -> Self {
        Self {
            transfers: RefCell::new(Vec::new()),
            busy_fail_after: None,
            busy_count: Cell::new(0),
        }
    }
}

impl MockHal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a mock that will timeout on the Nth wait_busy call (0-indexed).
    pub fn with_busy_timeout_at(n: usize) -> Self {
        Self {
            busy_fail_after: Some(n),
            ..Default::default()
        }
    }

    /// Return all command bytes in order (filtering out Data/WaitBusy/Reset).
    pub fn commands(&self) -> Vec<u8> {
        self.transfers
            .borrow()
            .iter()
            .filter_map(|t| match t {
                SpiTransfer::Command(c) => Some(*c),
                _ => None,
            })
            .collect()
    }

    /// Return the data payload that immediately follows the given command.
    /// Returns None if the command is not found or not followed by Data.
    pub fn data_after_command(&self, cmd: u8) -> Option<Vec<u8>> {
        let transfers = self.transfers.borrow();
        for (i, t) in transfers.iter().enumerate() {
            if matches!(t, SpiTransfer::Command(c) if *c == cmd) {
                if let Some(SpiTransfer::Data(d)) = transfers.get(i + 1) {
                    return Some(d.clone());
                }
            }
        }
        None
    }

    /// Return all data payloads that follow the given command (for commands
    /// that appear multiple times).
    pub fn all_data_after_command(&self, cmd: u8) -> Vec<Vec<u8>> {
        let transfers = self.transfers.borrow();
        let mut result = Vec::new();
        for (i, t) in transfers.iter().enumerate() {
            if matches!(t, SpiTransfer::Command(c) if *c == cmd) {
                if let Some(SpiTransfer::Data(d)) = transfers.get(i + 1) {
                    result.push(d.clone());
                }
            }
        }
        result
    }

    /// Get a snapshot of the transfer log.
    pub fn snapshot(&self) -> Vec<SpiTransfer> {
        self.transfers.borrow().clone()
    }
}

impl Ssd1680Hal for MockHal {
    fn send_command(&mut self, cmd: u8) -> Result<(), String> {
        self.transfers.borrow_mut().push(SpiTransfer::Command(cmd));
        Ok(())
    }

    fn send_data(&mut self, data: &[u8]) -> Result<(), String> {
        self.transfers
            .borrow_mut()
            .push(SpiTransfer::Data(data.to_vec()));
        Ok(())
    }

    fn wait_busy(&self) -> Result<(), String> {
        let count = self.busy_count.get() + 1;
        self.busy_count.set(count);
        self.transfers.borrow_mut().push(SpiTransfer::WaitBusy);
        if let Some(limit) = self.busy_fail_after {
            if count > limit {
                return Err("EPD BUSY timeout (mock)".into());
            }
        }
        Ok(())
    }

    fn hardware_reset(&mut self) -> Result<(), String> {
        self.transfers.borrow_mut().push(SpiTransfer::Reset);
        Ok(())
    }

    fn partial_reset(&mut self) -> Result<(), String> {
        self.transfers.borrow_mut().push(SpiTransfer::PartialReset);
        Ok(())
    }
}

// ── Real HAL (aarch64 only) ─────────────────────────────────────────

#[cfg(target_arch = "aarch64")]
pub struct RppalHal {
    spi: rppal::spi::Spi,
    dc: rppal::gpio::OutputPin,
    rst: rppal::gpio::OutputPin,
    busy: rppal::gpio::InputPin,
}

#[cfg(target_arch = "aarch64")]
impl RppalHal {
    pub fn new() -> Result<Self, String> {
        use rppal::gpio::Gpio;
        use rppal::spi::{Bus, Mode, SlaveSelect, Spi};

        let gpio = Gpio::new().map_err(|e| format!("GPIO init failed: {e}"))?;
        let dc = gpio
            .get(DC_PIN)
            .map_err(|e| format!("DC pin {DC_PIN}: {e}"))?
            .into_output();
        let rst = gpio
            .get(RST_PIN)
            .map_err(|e| format!("RST pin {RST_PIN}: {e}"))?
            .into_output();
        let busy = gpio
            .get(BUSY_PIN)
            .map_err(|e| format!("BUSY pin {BUSY_PIN}: {e}"))?
            .into_input();
        let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 4_000_000, Mode::Mode0)
            .map_err(|e| format!("SPI init failed: {e}"))?;

        Ok(Self { spi, dc, rst, busy })
    }
}

#[cfg(target_arch = "aarch64")]
impl Ssd1680Hal for RppalHal {
    fn send_command(&mut self, cmd: u8) -> Result<(), String> {
        self.dc.set_low();
        self.spi
            .write(&[cmd])
            .map_err(|e| format!("SPI write cmd 0x{cmd:02X}: {e}"))?;
        Ok(())
    }

    fn send_data(&mut self, data: &[u8]) -> Result<(), String> {
        self.dc.set_high();
        // SPI transfers are limited to ~4096 bytes on some kernels, chunk it.
        for chunk in data.chunks(4096) {
            self.spi
                .write(chunk)
                .map_err(|e| format!("SPI write data ({} bytes): {e}", chunk.len()))?;
        }
        Ok(())
    }

    fn wait_busy(&self) -> Result<(), String> {
        use std::time::{Duration, Instant};
        let start = Instant::now();
        let timeout = Duration::from_millis(BUSY_TIMEOUT_MS);
        while self.busy.is_high() {
            if start.elapsed() > timeout {
                return Err(format!("EPD BUSY timeout ({}ms)", BUSY_TIMEOUT_MS));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    fn hardware_reset(&mut self) -> Result<(), String> {
        use std::thread;
        use std::time::Duration;
        self.rst.set_high();
        thread::sleep(Duration::from_millis(20));
        self.rst.set_low();
        thread::sleep(Duration::from_millis(2));
        self.rst.set_high();
        thread::sleep(Duration::from_millis(20));
        Ok(())
    }

    fn partial_reset(&mut self) -> Result<(), String> {
        use std::thread;
        use std::time::Duration;
        self.rst.set_low();
        thread::sleep(Duration::from_millis(1));
        self.rst.set_high();
        Ok(())
    }
}

// ── SSD1680 Driver ──────────────────────────────────────────────────

/// High-level driver for the SSD1680 e-ink controller.
///
/// Generic over the HAL backend so tests can use `MockHal` while the Pi
/// uses `RppalHal`.
pub struct Ssd1680Driver<H: Ssd1680Hal> {
    pub hal: H,
    /// Display width in the logical coordinate system (after rotation).
    pub width: u32,
    /// Display height in the logical coordinate system (after rotation).
    pub height: u32,
    /// Rotation in degrees (0, 180). Only 0 and 180 are supported for
    /// the 2.13" V4 since the SSD1680 scanline direction is fixed.
    pub rotation: u16,
}

impl<H: Ssd1680Hal> Ssd1680Driver<H> {
    /// Create a new driver wrapping the given HAL.
    /// `rotation` must be 0 or 180; other values default to 0.
    pub fn new(hal: H, rotation: u16) -> Self {
        // The display is physically 122 x 250. In our logical space
        // (matching pwnagotchi conventions), we expose it as 250 x 122
        // (landscape). Rotation=180 flips the image but keeps the same
        // logical dimensions.
        Self {
            hal,
            width: EPD_HEIGHT,  // 250 (landscape)
            height: EPD_WIDTH,  // 122 (landscape)
            rotation: if rotation == 180 { 180 } else { 0 },
        }
    }

    /// Run the full SSD1680 initialization sequence.
    /// Matches the Python epd2in13_V4.py init() byte-for-byte.
    pub fn init(&mut self) -> Result<(), String> {
        // Hardware reset
        self.hal.hardware_reset()?;
        self.hal.wait_busy()?;

        // Software reset
        self.hal.send_command(CMD_SW_RESET)?;
        self.hal.wait_busy()?;

        // Driver output control: MUX = 249
        self.hal.send_command(CMD_DRIVER_OUTPUT)?;
        self.hal.send_data(&[0xF9, 0x00, 0x00])?;

        // Data entry mode: X+, Y+
        self.hal.send_command(CMD_DATA_ENTRY_MODE)?;
        self.hal.send_data(&[0x03])?;

        // Set RAM window
        self.hal.send_command(CMD_SET_RAM_X_RANGE)?;
        self.hal
            .send_data(&[0x00, (EPD_WIDTH_BYTES - 1) as u8])?;

        self.hal.send_command(CMD_SET_RAM_Y_RANGE)?;
        self.hal.send_data(&[0x00, 0x00, 0xF9, 0x00])?;

        // Set RAM cursor (Python does this BEFORE border/ctrl1/temp)
        self.set_ram_cursor(0, 0)?;

        // Border waveform control: follow LUT
        self.hal.send_command(CMD_BORDER_WAVEFORM)?;
        self.hal.send_data(&[0x05])?;

        // Display update control 1: normal RAM, inverted source output
        self.hal.send_command(CMD_DISPLAY_UPDATE_CTRL1)?;
        self.hal.send_data(&[0x00, 0x80])?;

        // Temperature sensor: internal
        self.hal.send_command(CMD_TEMP_SENSOR)?;
        self.hal.send_data(&[0x80])?;

        self.hal.wait_busy()?;

        Ok(())
    }

    /// Set the RAM address cursor to (x_byte, y_line).
    fn set_ram_cursor(&mut self, x_byte: u8, y_line: u16) -> Result<(), String> {
        self.hal.send_command(CMD_SET_RAM_X_COUNTER)?;
        self.hal.send_data(&[x_byte])?;
        self.hal.send_command(CMD_SET_RAM_Y_COUNTER)?;
        self.hal
            .send_data(&[(y_line & 0xFF) as u8, ((y_line >> 8) & 0x01) as u8])?;
        Ok(())
    }

    /// Send framebuffer data to the SSD1680 and trigger a display update.
    ///
    /// The `FrameBuffer` is in our logical coordinate system (250 wide x 122 tall,
    /// 1=black, 0=white). This method:
    /// 1. Applies rotation if needed
    /// 2. Transposes from logical (landscape) to physical (portrait) layout
    /// 3. Inverts polarity (SSD1680: 1=white, 0=black)
    /// 4. Writes to SSD1680 RAM
    /// 5. Triggers the update sequence
    pub fn flush(&mut self, fb: &FrameBuffer, mode: RefreshMode) -> Result<(), String> {
        let spi_data = self.prepare_spi_data(fb);

        // Reset RAM cursor to start
        self.set_ram_cursor(0, 0)?;

        // Write BW RAM
        self.hal.send_command(CMD_WRITE_RAM_BW)?;
        self.hal.send_data(&spi_data)?;

        // For partial refresh, also write to RED RAM (SSD1680 uses it as "old" data)
        if mode == RefreshMode::Partial {
            self.set_ram_cursor(0, 0)?;
            self.hal.send_command(CMD_WRITE_RAM_RED)?;
            self.hal.send_data(&spi_data)?;
        }

        // Trigger display update
        self.hal.send_command(CMD_DISPLAY_UPDATE_CTRL2)?;
        match mode {
            RefreshMode::Full => self.hal.send_data(&[UPDATE_FULL])?,
            RefreshMode::Partial => self.hal.send_data(&[UPDATE_PARTIAL])?,
        }
        self.hal.send_command(CMD_MASTER_ACTIVATION)?;

        self.hal.wait_busy()?;

        Ok(())
    }

    /// Enter deep sleep mode to save power. After this, init() must be
    /// called again before any further display updates.
    pub fn deep_sleep(&mut self) -> Result<(), String> {
        self.hal.send_command(CMD_DEEP_SLEEP)?;
        self.hal.send_data(&[0x01])?; // Mode 1: retain RAM
        Ok(())
    }

    /// Write framebuffer to both BW and RED RAM, then do a full refresh.
    /// Matches Python `displayPartBaseImage()` — called once after init
    /// to establish the base image for subsequent partial updates.
    pub fn flush_base(&mut self, fb: &FrameBuffer) -> Result<(), String> {
        let spi_data = self.prepare_spi_data(fb);

        self.set_ram_cursor(0, 0)?;
        self.hal.send_command(CMD_WRITE_RAM_BW)?;
        self.hal.send_data(&spi_data)?;

        self.set_ram_cursor(0, 0)?;
        self.hal.send_command(CMD_WRITE_RAM_RED)?;
        self.hal.send_data(&spi_data)?;

        // Full refresh
        self.hal.send_command(CMD_DISPLAY_UPDATE_CTRL2)?;
        self.hal.send_data(&[UPDATE_FULL])?;
        self.hal.send_command(CMD_MASTER_ACTIVATION)?;

        self.hal.wait_busy()?;

        Ok(())
    }

    /// Partial refresh: short reset, re-init registers, write BW RAM only.
    /// Matches Python `displayPartial()` — fast, no heavy flash.
    pub fn flush_partial(&mut self, fb: &FrameBuffer) -> Result<(), String> {
        let spi_data = self.prepare_spi_data(fb);

        // Short hardware reset (Python: RST LOW 1ms, HIGH)
        self.hal.partial_reset()?;

        // Border waveform for partial mode (0x80, different from init's 0x05)
        self.hal.send_command(CMD_BORDER_WAVEFORM)?;
        self.hal.send_data(&[0x80])?;

        // Re-send basic config (needed after reset)
        self.hal.send_command(CMD_DRIVER_OUTPUT)?;
        self.hal.send_data(&[0xF9, 0x00, 0x00])?;

        self.hal.send_command(CMD_DATA_ENTRY_MODE)?;
        self.hal.send_data(&[0x03])?;

        self.hal.send_command(CMD_SET_RAM_X_RANGE)?;
        self.hal
            .send_data(&[0x00, (EPD_WIDTH_BYTES - 1) as u8])?;

        self.hal.send_command(CMD_SET_RAM_Y_RANGE)?;
        self.hal.send_data(&[0x00, 0x00, 0xF9, 0x00])?;

        self.set_ram_cursor(0, 0)?;

        // Write BW RAM only (partial doesn't touch RED RAM)
        self.hal.send_command(CMD_WRITE_RAM_BW)?;
        self.hal.send_data(&spi_data)?;

        // Trigger partial update
        self.hal.send_command(CMD_DISPLAY_UPDATE_CTRL2)?;
        self.hal.send_data(&[UPDATE_PARTIAL])?;
        self.hal.send_command(CMD_MASTER_ACTIVATION)?;

        self.hal.wait_busy()?;

        Ok(())
    }

    /// Prepare the SPI byte array from the logical framebuffer.
    ///
    /// Our FrameBuffer is 250 wide x 122 tall (landscape, row-major, MSB-first,
    /// 1=black). The SSD1680 RAM is 122 wide x 250 tall (portrait), with X
    /// being the fast axis (16 bytes per scanline), and 1=white.
    ///
    /// This function:
    /// 1. Optionally rotates 180 degrees
    /// 2. Transposes from landscape to portrait
    /// 3. Inverts polarity
    ///
    /// Returns a Vec<u8> of exactly EPD_WIDTH_BYTES * EPD_HEIGHT = 16 * 250 = 4000 bytes.
    pub fn prepare_spi_data(&self, fb: &FrameBuffer) -> Vec<u8> {
        let phys_w = EPD_WIDTH; // 122
        let phys_h = EPD_HEIGHT; // 250
        let stride = EPD_WIDTH_BYTES as usize; // 16 bytes per scanline
        let total = stride * phys_h as usize; // 4000 bytes

        let mut out = vec![0xFFu8; total]; // 0xFF = all white in SSD1680

        // The logical framebuffer is fb.width=250 x fb.height=122.
        // Python's getbuffer() does PIL rotate(90, expand=True) which maps:
        //   portrait(px, py) = landscape(249-py, px)
        // With pwnagotchi rotation=180 applied first:
        //   portrait(px, py) = original(py, 121-px)
        //
        // We match this by reading logical pixels for each physical position:
        //   rotation 0:   (lx, ly) = (249-py, px)
        //   rotation 180: (lx, ly) = (py, 121-px)

        for py in 0..phys_h {
            for px in 0..phys_w {
                // Map physical -> logical (matches Python getbuffer rotate(90))
                let (lx, ly) = if self.rotation == 180 {
                    // Matches Python: pwnagotchi rotation=180 + getbuffer rotate(90)
                    (py, fb.height - 1 - px)
                } else {
                    // Matches Python: getbuffer rotate(90) only
                    (fb.width - 1 - py, px)
                };

                if lx >= fb.width || ly >= fb.height {
                    continue;
                }

                // Read logical pixel
                let fb_pixel = fb.get_pixel(lx, ly);

                if fb_pixel == embedded_graphics::pixelcolor::BinaryColor::On {
                    // Black pixel in our framebuffer -> 0 in SSD1680
                    let byte_idx = (py as usize) * stride + (px as usize) / 8;
                    let bit_idx = 7 - (px % 8);
                    out[byte_idx] &= !(1 << bit_idx); // Clear bit = black
                }
                // White pixel: already 0xFF (bit set = white), no action needed
            }
        }

        out
    }
}

// ── Public convenience function (backward-compatible) ───────────────

/// Send the framebuffer to the Waveshare 2.13" V4 e-ink display via SPI.
///
/// This is the high-level entry point called by `Screen::flush()`.
/// On non-aarch64 platforms this is a no-op.
#[cfg(target_arch = "aarch64")]
pub fn flush_to_hardware(fb: &FrameBuffer, config: &DisplayConfig) -> Result<(), String> {
    use std::sync::Mutex;

    static DRIVER: Mutex<Option<Ssd1680Driver<RppalHal>>> = Mutex::new(None);

    let mut guard = DRIVER.lock().map_err(|e| format!("driver lock: {e}"))?;

    match guard.as_mut() {
        None => {
            // First call: full init + base image (both RAMs + full refresh)
            let hal = RppalHal::new()?;
            let mut driver = Ssd1680Driver::new(hal, config.rotation);
            driver.init()?;
            driver.flush_base(fb)?;
            *guard = Some(driver);
        }
        Some(driver) => {
            // Subsequent calls: partial refresh (fast, no heavy flash)
            driver.flush_partial(fb)?;
        }
    }

    Ok(())
}

#[cfg(not(target_arch = "aarch64"))]
pub fn flush_to_hardware(_fb: &FrameBuffer, _config: &DisplayConfig) -> Result<(), String> {
    // No-op on non-Pi platforms.
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::pixelcolor::BinaryColor;

    // ── Helpers ─────────────────────────────────────────────────────
    fn make_fb() -> FrameBuffer {
        FrameBuffer::new(250, 122)
    }

    fn make_config(rotation: u16) -> DisplayConfig {
        DisplayConfig {
            enabled: true,
            display_type: "waveshare_4".into(),
            rotation,
        }
    }

    // ================================================================
    // Test group 1: Init command sequence
    // ================================================================

    #[test]
    fn test_init_sends_sw_reset() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let cmds = drv.hal.commands();
        assert!(
            cmds.contains(&CMD_SW_RESET),
            "init must send SW_RESET (0x12), got: {:02X?}",
            cmds
        );
    }

    #[test]
    fn test_init_command_order() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let cmds = drv.hal.commands();

        let expected = vec![
            CMD_SW_RESET,
            CMD_DRIVER_OUTPUT,
            CMD_DATA_ENTRY_MODE,
            CMD_SET_RAM_X_RANGE,
            CMD_SET_RAM_Y_RANGE,
            CMD_SET_RAM_X_COUNTER, // cursor before border (matches Python)
            CMD_SET_RAM_Y_COUNTER,
            CMD_BORDER_WAVEFORM,
            CMD_DISPLAY_UPDATE_CTRL1, // 0x21: normal RAM, inverted source
            CMD_TEMP_SENSOR,
        ];
        assert_eq!(
            cmds, expected,
            "init commands must match Python waveshare driver order"
        );
    }

    #[test]
    fn test_init_display_update_ctrl1() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let data = drv
            .hal
            .data_after_command(CMD_DISPLAY_UPDATE_CTRL1)
            .unwrap();
        assert_eq!(
            data,
            vec![0x00, 0x80],
            "display update ctrl1 should be [0x00, 0x80] (normal RAM, inverted source)"
        );
    }

    #[test]
    fn test_init_driver_output_data() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let data = drv.hal.data_after_command(CMD_DRIVER_OUTPUT).unwrap();
        // MUX = 249 = 0xF9, high byte 0, scan direction 0
        assert_eq!(data, vec![0xF9, 0x00, 0x00]);
    }

    #[test]
    fn test_init_data_entry_mode() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let data = drv.hal.data_after_command(CMD_DATA_ENTRY_MODE).unwrap();
        assert_eq!(data, vec![0x03], "data entry mode should be X+/Y+ (0x03)");
    }

    #[test]
    fn test_init_ram_x_range() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let data = drv.hal.data_after_command(CMD_SET_RAM_X_RANGE).unwrap();
        // 0 to 15 (122/8 = 15.25, rounded up = 16, minus 1 = 15)
        assert_eq!(data, vec![0x00, 0x0F]);
    }

    #[test]
    fn test_init_ram_y_range() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let data = drv.hal.data_after_command(CMD_SET_RAM_Y_RANGE).unwrap();
        // 0,0 to 249,0 (249 = 0xF9)
        assert_eq!(data, vec![0x00, 0x00, 0xF9, 0x00]);
    }

    #[test]
    fn test_init_starts_with_hw_reset() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let snap = drv.hal.snapshot();
        assert_eq!(
            snap[0],
            SpiTransfer::Reset,
            "first transfer must be hardware reset"
        );
    }

    #[test]
    fn test_init_waits_busy_after_reset() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let snap = drv.hal.snapshot();
        assert_eq!(snap[1], SpiTransfer::WaitBusy);
    }

    #[test]
    fn test_init_waits_busy_after_sw_reset() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let snap = drv.hal.snapshot();
        let idx = snap
            .iter()
            .position(|t| *t == SpiTransfer::Command(CMD_SW_RESET))
            .unwrap();
        assert_eq!(
            snap[idx + 1],
            SpiTransfer::WaitBusy,
            "must wait for busy after SW_RESET"
        );
    }

    // ================================================================
    // Test group 2: Framebuffer-to-SPI data conversion
    // ================================================================

    #[test]
    fn test_empty_fb_produces_all_white_spi_data() {
        let drv = Ssd1680Driver::new(MockHal::new(), 0);
        let fb = make_fb();
        let data = drv.prepare_spi_data(&fb);
        assert_eq!(data.len(), (EPD_WIDTH_BYTES * EPD_HEIGHT) as usize);
        assert!(
            data.iter().all(|&b| b == 0xFF),
            "empty framebuffer should map to all-white (0xFF) SPI data"
        );
    }

    #[test]
    fn test_full_black_fb_produces_correct_spi_data() {
        let drv = Ssd1680Driver::new(MockHal::new(), 0);
        let mut fb = make_fb();
        for y in 0..122 {
            for x in 0..250 {
                fb.set_pixel(x, y, BinaryColor::On);
            }
        }
        let data = drv.prepare_spi_data(&fb);
        for scanline in 0..250usize {
            let offset = scanline * 16;
            for byte_idx in 0..15 {
                assert_eq!(
                    data[offset + byte_idx],
                    0x00,
                    "scanline {}, byte {} should be 0x00 (black)",
                    scanline,
                    byte_idx
                );
            }
            // Byte 15: pixels 120-121 valid (bits 7-6), padding bits 5..0 = white
            assert_eq!(
                data[offset + 15],
                0x3F,
                "scanline {}, byte 15 should have pixels 120-121 black, padding white",
                scanline
            );
        }
    }

    #[test]
    fn test_single_pixel_at_origin() {
        let drv = Ssd1680Driver::new(MockHal::new(), 0);
        let mut fb = make_fb();
        fb.set_pixel(0, 0, BinaryColor::On);
        let data = drv.prepare_spi_data(&fb);

        // Logical (0, 0) rotation=0 -> lx=249-py, ly=px -> py=249, px=0
        // byte 249*16=3984, bit 7
        assert_eq!(
            data[3984],
            0x7F, // 0xFF with bit 7 cleared
            "pixel at origin should map to physical (0, 249)"
        );

        // First scanline byte 0 should be untouched
        assert_eq!(data[0], 0xFF, "physical scanline 0 byte 0 should be untouched");
    }

    #[test]
    fn test_spi_data_length() {
        let drv = Ssd1680Driver::new(MockHal::new(), 0);
        let fb = make_fb();
        let data = drv.prepare_spi_data(&fb);
        assert_eq!(
            data.len(),
            4000,
            "SPI data must be exactly 4000 bytes (16 bytes/scanline * 250 scanlines)"
        );
    }

    #[test]
    fn test_spi_polarity_inversion() {
        let drv = Ssd1680Driver::new(MockHal::new(), 0);
        let mut fb = make_fb();
        for x in 0..8 {
            fb.set_pixel(x, 0, BinaryColor::On);
        }
        let data = drv.prepare_spi_data(&fb);
        // logical (x, 0) rotation=0 -> py=249-x, px=0
        // Pixels at scanlines 249, 248, ..., 242, each at byte 0 bit 7
        for x in 0..8u32 {
            let py = 249 - x as usize;
            let byte_idx = py * 16;
            assert_eq!(
                data[byte_idx] & 0x80,
                0,
                "scanline {}: bit 7 should be cleared (black pixel from logical ({}, 0))",
                py,
                x
            );
        }
    }

    // ================================================================
    // Test group 3: Busy timeout handling
    // ================================================================

    #[test]
    fn test_busy_timeout_during_init() {
        let mut drv = Ssd1680Driver::new(MockHal::with_busy_timeout_at(0), 0);
        let result = drv.init();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("BUSY timeout"));
    }

    #[test]
    fn test_busy_timeout_during_flush() {
        // Init uses 3 wait_busy calls; fail on the 4th (during flush)
        let mut drv = Ssd1680Driver::new(MockHal::with_busy_timeout_at(3), 0);
        drv.init().unwrap();
        let result = drv.flush(&make_fb(), RefreshMode::Full);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("BUSY timeout"));
    }

    #[test]
    fn test_no_busy_timeout_normal_operation() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        drv.flush(&make_fb(), RefreshMode::Full).unwrap();
    }

    // ================================================================
    // Test group 4: Rotation transforms
    // ================================================================

    #[test]
    fn test_rotation_0_pixel_mapping() {
        let drv = Ssd1680Driver::new(MockHal::new(), 0);
        let mut fb = make_fb();
        fb.set_pixel(0, 0, BinaryColor::On);
        let data = drv.prepare_spi_data(&fb);
        // rotation 0: (0,0) -> phys (px=0, py=249), byte 249*16=3984, bit 7
        assert_eq!(data[3984] & 0x80, 0x00, "rotation 0: (0,0) -> phys (0,249)");
    }

    #[test]
    fn test_rotation_180_pixel_mapping() {
        let drv = Ssd1680Driver::new(MockHal::new(), 180);
        let mut fb = make_fb();
        fb.set_pixel(0, 0, BinaryColor::On);
        let data_180 = drv.prepare_spi_data(&fb);

        // rotation 180: (0,0) -> lx=py, ly=121-px -> py=0, px=121
        // phys (121, 0): byte 0*16 + 121/8 = 15, bit 7-(121%8) = 6
        assert_eq!(
            data_180[15] & (1 << 6),
            0,
            "rotation 180: logical (0,0) should map to physical (121, 0)"
        );
    }

    #[test]
    fn test_rotation_180_vs_0_are_different() {
        let mut fb = make_fb();
        fb.set_pixel(0, 0, BinaryColor::On);

        let drv0 = Ssd1680Driver::new(MockHal::new(), 0);
        let drv180 = Ssd1680Driver::new(MockHal::new(), 180);

        let data0 = drv0.prepare_spi_data(&fb);
        let data180 = drv180.prepare_spi_data(&fb);

        assert_ne!(
            data0, data180,
            "rotation 0 and 180 should produce different SPI data for asymmetric images"
        );
    }

    #[test]
    fn test_rotation_180_symmetric_pattern() {
        let mut fb = make_fb();
        for y in 0..122 {
            for x in 0..250 {
                fb.set_pixel(x, y, BinaryColor::On);
            }
        }

        let drv0 = Ssd1680Driver::new(MockHal::new(), 0);
        let drv180 = Ssd1680Driver::new(MockHal::new(), 180);

        let data0 = drv0.prepare_spi_data(&fb);
        let data180 = drv180.prepare_spi_data(&fb);

        assert_eq!(
            data0, data180,
            "fully filled framebuffer should look identical at 0 and 180 rotation"
        );
    }

    #[test]
    fn test_rotation_dimensions() {
        let drv0 = Ssd1680Driver::new(MockHal::new(), 0);
        assert_eq!(drv0.width, 250);
        assert_eq!(drv0.height, 122);
        assert_eq!(drv0.rotation, 0);

        let drv180 = Ssd1680Driver::new(MockHal::new(), 180);
        assert_eq!(drv180.width, 250);
        assert_eq!(drv180.height, 122);
        assert_eq!(drv180.rotation, 180);
    }

    #[test]
    fn test_invalid_rotation_defaults_to_0() {
        let drv = Ssd1680Driver::new(MockHal::new(), 90);
        assert_eq!(drv.rotation, 0, "unsupported rotation should default to 0");

        let drv = Ssd1680Driver::new(MockHal::new(), 270);
        assert_eq!(drv.rotation, 0);
    }

    // ================================================================
    // Test group 5: Flush command sequence
    // ================================================================

    #[test]
    fn test_flush_full_sends_correct_commands() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let pre_len = drv.hal.snapshot().len();

        drv.flush(&make_fb(), RefreshMode::Full).unwrap();

        let snap = drv.hal.snapshot();
        let flush_cmds: Vec<u8> = snap[pre_len..]
            .iter()
            .filter_map(|t| match t {
                SpiTransfer::Command(c) => Some(*c),
                _ => None,
            })
            .collect();

        assert_eq!(
            flush_cmds,
            vec![
                CMD_SET_RAM_X_COUNTER,
                CMD_SET_RAM_Y_COUNTER,
                CMD_WRITE_RAM_BW,
                CMD_DISPLAY_UPDATE_CTRL2,
                CMD_MASTER_ACTIVATION,
            ]
        );
    }

    #[test]
    fn test_flush_full_update_byte() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        drv.flush(&make_fb(), RefreshMode::Full).unwrap();
        let all = drv.hal.all_data_after_command(CMD_DISPLAY_UPDATE_CTRL2);
        let last = all.last().unwrap();
        assert_eq!(last, &vec![UPDATE_FULL], "full refresh should send 0xF7");
    }

    #[test]
    fn test_flush_partial_update_byte() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        drv.flush(&make_fb(), RefreshMode::Partial).unwrap();
        let all = drv.hal.all_data_after_command(CMD_DISPLAY_UPDATE_CTRL2);
        let last = all.last().unwrap();
        assert_eq!(
            last,
            &vec![UPDATE_PARTIAL],
            "partial refresh should send 0xFF"
        );
    }

    #[test]
    fn test_flush_partial_writes_red_ram() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        drv.flush(&make_fb(), RefreshMode::Partial).unwrap();
        let cmds = drv.hal.commands();
        assert!(
            cmds.contains(&CMD_WRITE_RAM_RED),
            "partial refresh should also write to RED RAM (0x26)"
        );
    }

    #[test]
    fn test_flush_full_does_not_write_red_ram() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let pre_len = drv.hal.snapshot().len();
        drv.flush(&make_fb(), RefreshMode::Full).unwrap();
        let snap = drv.hal.snapshot();
        let flush_cmds: Vec<u8> = snap[pre_len..]
            .iter()
            .filter_map(|t| match t {
                SpiTransfer::Command(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert!(
            !flush_cmds.contains(&CMD_WRITE_RAM_RED),
            "full refresh should NOT write to RED RAM"
        );
    }

    #[test]
    fn test_flush_sends_4000_bytes_of_image_data() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        drv.flush(&make_fb(), RefreshMode::Full).unwrap();
        let all = drv.hal.all_data_after_command(CMD_WRITE_RAM_BW);
        let last = all.last().unwrap();
        assert_eq!(last.len(), 4000, "image data must be exactly 4000 bytes");
    }

    #[test]
    fn test_flush_ends_with_wait_busy() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        drv.flush(&make_fb(), RefreshMode::Full).unwrap();
        let snap = drv.hal.snapshot();
        let last = snap.last().unwrap();
        assert_eq!(
            *last,
            SpiTransfer::WaitBusy,
            "flush must end with wait_busy"
        );
    }

    // ================================================================
    // Test group 6: Deep sleep
    // ================================================================

    #[test]
    fn test_deep_sleep_command() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        drv.deep_sleep().unwrap();
        let cmds = drv.hal.commands();
        assert!(cmds.contains(&CMD_DEEP_SLEEP));
        let data = drv.hal.all_data_after_command(CMD_DEEP_SLEEP);
        assert_eq!(
            data.last().unwrap(),
            &vec![0x01],
            "deep sleep mode 1 (retain RAM)"
        );
    }

    // ================================================================
    // Test group 7: Backward-compatible flush_to_hardware
    // ================================================================

    #[test]
    fn test_flush_to_hardware_noop_on_non_pi() {
        let fb = make_fb();
        let config = make_config(0);
        let result = flush_to_hardware(&fb, &config);
        #[cfg(not(target_arch = "aarch64"))]
        assert!(result.is_ok());
        #[cfg(target_arch = "aarch64")]
        let _ = result;
    }

    // ================================================================
    // Test group 8: Border and temperature sensor init
    // ================================================================

    #[test]
    fn test_init_border_waveform() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let data = drv.hal.data_after_command(CMD_BORDER_WAVEFORM).unwrap();
        assert_eq!(
            data,
            vec![0x05],
            "border waveform should be 0x05 (follow LUT)"
        );
    }

    #[test]
    fn test_init_temp_sensor() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let data = drv.hal.data_after_command(CMD_TEMP_SENSOR).unwrap();
        assert_eq!(data, vec![0x80], "temp sensor should be internal (0x80)");
    }

    // ================================================================
    // Test group 9: Pixel-level transpose correctness
    // ================================================================

    #[test]
    fn test_pixel_at_bottom_right() {
        let drv = Ssd1680Driver::new(MockHal::new(), 0);
        let mut fb = make_fb();
        fb.set_pixel(249, 121, BinaryColor::On);
        let data = drv.prepare_spi_data(&fb);

        // rotation 0: lx=249-py=249, ly=px=121 -> py=0, px=121
        // byte = 0*16 + 121/8 = 15, bit = 7-(121%8) = 6
        assert_eq!(
            data[15] & (1 << 6),
            0,
            "pixel at logical (249,121) should be black at physical (121, 0)"
        );
    }

    #[test]
    fn test_horizontal_line_transposes_to_vertical() {
        let drv = Ssd1680Driver::new(MockHal::new(), 0);
        let mut fb = make_fb();
        for x in 0..250 {
            fb.set_pixel(x, 0, BinaryColor::On);
        }
        let data = drv.prepare_spi_data(&fb);

        // rotation 0: logical (x, 0) -> py=249-x, px=0
        // Each scanline py has pixel at px=0 (byte 0, bit 7)
        for x in 0..250usize {
            let py = 249 - x;
            let byte_idx = py * 16;
            assert_eq!(
                data[byte_idx] & 0x80,
                0,
                "scanline {}: pixel column 0 should be black (from logical ({}, 0))",
                py,
                x
            );
            for bi in 1..15 {
                assert_eq!(data[py * 16 + bi], 0xFF);
            }
        }
    }

    // ================================================================
    // Test group 10: Multiple flushes reuse driver
    // ================================================================

    #[test]
    fn test_multiple_flushes() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();

        let mut fb1 = make_fb();
        fb1.set_pixel(0, 0, BinaryColor::On);
        drv.flush(&fb1, RefreshMode::Full).unwrap();

        let mut fb2 = make_fb();
        fb2.set_pixel(249, 121, BinaryColor::On);
        drv.flush(&fb2, RefreshMode::Partial).unwrap();

        let activation_count = drv
            .hal
            .commands()
            .iter()
            .filter(|c| **c == CMD_MASTER_ACTIVATION)
            .count();
        assert_eq!(activation_count, 2, "should have two display update cycles");
    }

    // ================================================================
    // Test group 11: flush_base and flush_partial
    // ================================================================

    #[test]
    fn test_flush_base_writes_both_rams() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let pre_len = drv.hal.snapshot().len();
        drv.flush_base(&make_fb()).unwrap();
        let snap = drv.hal.snapshot();
        let flush_cmds: Vec<u8> = snap[pre_len..]
            .iter()
            .filter_map(|t| match t {
                SpiTransfer::Command(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert!(
            flush_cmds.contains(&CMD_WRITE_RAM_BW),
            "flush_base must write BW RAM"
        );
        assert!(
            flush_cmds.contains(&CMD_WRITE_RAM_RED),
            "flush_base must write RED RAM"
        );
    }

    #[test]
    fn test_flush_base_triggers_full_update() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        drv.flush_base(&make_fb()).unwrap();
        let all = drv.hal.all_data_after_command(CMD_DISPLAY_UPDATE_CTRL2);
        let last = all.last().unwrap();
        assert_eq!(
            last,
            &vec![UPDATE_FULL],
            "flush_base should trigger full update (0xF7)"
        );
    }

    #[test]
    fn test_flush_partial_starts_with_partial_reset() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let pre_len = drv.hal.snapshot().len();
        drv.flush_partial(&make_fb()).unwrap();
        let snap = drv.hal.snapshot();
        assert_eq!(
            snap[pre_len],
            SpiTransfer::PartialReset,
            "flush_partial must start with partial reset"
        );
    }

    #[test]
    fn test_flush_partial_uses_border_0x80() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let pre_len = drv.hal.snapshot().len();
        drv.flush_partial(&make_fb()).unwrap();
        let snap = drv.hal.snapshot();
        let flush_transfers = &snap[pre_len..];
        let border_idx = flush_transfers
            .iter()
            .position(|t| *t == SpiTransfer::Command(CMD_BORDER_WAVEFORM))
            .expect("flush_partial must send border waveform command");
        assert_eq!(
            flush_transfers[border_idx + 1],
            SpiTransfer::Data(vec![0x80]),
            "partial refresh border should be 0x80"
        );
    }

    #[test]
    fn test_flush_partial_writes_bw_ram_only() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        let pre_len = drv.hal.snapshot().len();
        drv.flush_partial(&make_fb()).unwrap();
        let snap = drv.hal.snapshot();
        let flush_cmds: Vec<u8> = snap[pre_len..]
            .iter()
            .filter_map(|t| match t {
                SpiTransfer::Command(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert!(
            flush_cmds.contains(&CMD_WRITE_RAM_BW),
            "flush_partial must write BW RAM"
        );
        assert!(
            !flush_cmds.contains(&CMD_WRITE_RAM_RED),
            "flush_partial must NOT write RED RAM"
        );
    }

    #[test]
    fn test_flush_partial_triggers_partial_update() {
        let mut drv = Ssd1680Driver::new(MockHal::new(), 0);
        drv.init().unwrap();
        drv.flush_partial(&make_fb()).unwrap();
        let all = drv.hal.all_data_after_command(CMD_DISPLAY_UPDATE_CTRL2);
        let last = all.last().unwrap();
        assert_eq!(
            last,
            &vec![UPDATE_PARTIAL],
            "flush_partial should trigger partial update (0xFF)"
        );
    }
}
