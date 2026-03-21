/// SPI e-ink driver for Waveshare 2.13" V4 (aarch64-only).
///
/// This module is only compiled on `target_arch = "aarch64"` (Raspberry Pi).
/// On other platforms the Screen::flush() method is a no-op.
use crate::config::DisplayConfig;
use super::buffer::FrameBuffer;

#[cfg(target_arch = "aarch64")]
use rppal::gpio::Gpio;
#[cfg(target_arch = "aarch64")]
use rppal::spi::{Bus, Mode, SlaveSelect, Spi};

// Waveshare 2.13" V4 GPIO pins (BCM numbering)
#[cfg(target_arch = "aarch64")]
const RST_PIN: u8 = 17;
#[cfg(target_arch = "aarch64")]
const DC_PIN: u8 = 25;
#[cfg(target_arch = "aarch64")]
const CS_PIN: u8 = 8;
#[cfg(target_arch = "aarch64")]
const BUSY_PIN: u8 = 24;

/// Send the framebuffer to the Waveshare 2.13" V4 e-ink display via SPI.
#[cfg(target_arch = "aarch64")]
pub fn flush_to_hardware(fb: &FrameBuffer, _config: &DisplayConfig) {
    use std::thread;
    use std::time::Duration;

    let gpio = Gpio::new().expect("Failed to initialize GPIO");
    let mut rst = gpio.get(RST_PIN).expect("RST pin").into_output();
    let mut dc = gpio.get(DC_PIN).expect("DC pin").into_output();
    let busy = gpio.get(BUSY_PIN).expect("BUSY pin").into_input();

    let mut spi =
        Spi::new(Bus::Spi0, SlaveSelect::Ss0, 4_000_000, Mode::Mode0).expect("SPI init failed");

    // Helper: send command
    let send_command = |spi: &mut Spi, dc: &mut rppal::gpio::OutputPin, cmd: u8| {
        dc.set_low();
        spi.write(&[cmd]).expect("SPI write cmd");
    };

    // Helper: send data
    let send_data = |spi: &mut Spi, dc: &mut rppal::gpio::OutputPin, data: &[u8]| {
        dc.set_high();
        spi.write(data).expect("SPI write data");
    };

    // Helper: wait until BUSY pin goes low
    let wait_busy = |busy: &rppal::gpio::InputPin| {
        while busy.is_high() {
            thread::sleep(Duration::from_millis(10));
        }
    };

    // Hardware reset
    rst.set_high();
    thread::sleep(Duration::from_millis(20));
    rst.set_low();
    thread::sleep(Duration::from_millis(2));
    rst.set_high();
    thread::sleep(Duration::from_millis(20));

    // Init sequence for SSD1680 (Waveshare 2.13" V4)
    wait_busy(&busy);
    send_command(&mut spi, &mut dc, 0x12); // SW reset
    wait_busy(&busy);

    send_command(&mut spi, &mut dc, 0x01); // Driver output control
    send_data(&mut spi, &mut dc, &[0x79, 0x00, 0x00]); // 122-1 = 0x79

    send_command(&mut spi, &mut dc, 0x11); // Data entry mode
    send_data(&mut spi, &mut dc, &[0x03]); // X inc, Y inc

    send_command(&mut spi, &mut dc, 0x44); // Set RAM X address
    send_data(&mut spi, &mut dc, &[0x00, 0x1F]); // 0 to 31 (250/8=31.25)

    send_command(&mut spi, &mut dc, 0x45); // Set RAM Y address
    send_data(&mut spi, &mut dc, &[0x00, 0x00, 0x79, 0x00]); // 0 to 121

    send_command(&mut spi, &mut dc, 0x4E); // Set RAM X counter
    send_data(&mut spi, &mut dc, &[0x00]);

    send_command(&mut spi, &mut dc, 0x4F); // Set RAM Y counter
    send_data(&mut spi, &mut dc, &[0x00, 0x00]);

    wait_busy(&busy);

    // Write image data
    send_command(&mut spi, &mut dc, 0x24); // Write RAM (BW)
    // Invert: e-ink 1=white, 0=black, but our fb uses 1=black
    let inverted: Vec<u8> = fb.as_bytes().iter().map(|b| !b).collect();
    send_data(&mut spi, &mut dc, &inverted);

    // Display update
    send_command(&mut spi, &mut dc, 0x22);
    send_data(&mut spi, &mut dc, &[0xF7]);
    send_command(&mut spi, &mut dc, 0x20);

    wait_busy(&busy);
}

// Non-Pi stub — this file is only compiled on aarch64, but the function
// is also gated in Screen::flush(). This is just a safety net.
#[cfg(not(target_arch = "aarch64"))]
pub fn flush_to_hardware(_fb: &FrameBuffer, _config: &DisplayConfig) {
    // No-op on non-Pi platforms.
}
