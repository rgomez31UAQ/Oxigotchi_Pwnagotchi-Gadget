# Rusty Oxigotchi

Rust-native reimplementation of the pwnagotchi WiFi capture daemon for
Raspberry Pi Zero 2W with Waveshare 2.13" V4 e-ink display, PiSugar 3
battery, and Bluetooth PAN tethering.

## Quick start

```bash
# Build (debug)
cargo build

# Build (release, optimised for size)
cargo build --release

# Run all tests
cargo test

# Run clippy lints
cargo clippy -- -W clippy::all
```

## Cross-compile for Pi Zero 2W (aarch64)

```bash
# Install the target (one-time)
rustup target add aarch64-unknown-linux-gnu

# Install the cross-linker (Debian/Ubuntu)
sudo apt install gcc-aarch64-linux-gnu

# Build
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build --release --target aarch64-unknown-linux-gnu

# Deploy
scp target/aarch64-unknown-linux-gnu/release/oxigotchi pi@<IP>:/home/pi/
ssh pi@<IP> 'sudo cp /home/pi/oxigotchi /usr/local/bin/ && sudo systemctl restart oxigotchi'
```

## Module overview

```
src/
  main.rs           Daemon struct, boot sequence, epoch loop, entry point
  config/mod.rs     TOML config parser (pwnagotchi-compatible format)
  display/
    mod.rs          High-level Screen API (draw_face, draw_name, etc.)
    buffer.rs       1-bit packed framebuffer with embedded-graphics DrawTarget
    driver.rs       SPI e-ink driver for Waveshare 2.13" V4 (aarch64-only)
  epoch.rs          Epoch state machine: Scan -> Attack -> Capture -> Display -> Sleep
  personality/
    mod.rs          Mood, Face (24 kaomoji), XP/leveling, SystemInfo
  attacks/mod.rs    Attack scheduler, rate limiter (BCM43436B0 safe at rate 1)
  capture/mod.rs    Capture file management, WPA-SEC upload queue, auto-backup
  wifi/mod.rs       WiFi monitor mode, channel hopping, AP tracker, whitelist
  pisugar/mod.rs    PiSugar 3 battery I2C, button debouncer, action mapping
  bluetooth/mod.rs  Bluetooth PAN tethering manager
  recovery/mod.rs   WiFi SDIO recovery, GPIO power cycle, watchdog
  web/mod.rs        REST API types, embedded HTML dashboard (15 cards)
  migration/mod.rs  Import legacy pwnagotchi config and captures
```

## Architecture

```
                  +-------------------+
                  |     Daemon        |
                  |  (main.rs)        |
                  +--------+----------+
                           |
          +-------+--------+--------+--------+
          |       |        |        |        |
     EpochLoop  Screen  WifiMgr  Attacks  Captures
     (epoch.rs) (display/) (wifi/) (attacks/) (capture/)
          |
     Personality
     (personality/)
          |
     Mood + Face (24 variants)

  Hardware layer (aarch64 only):
    SPI e-ink driver (display/driver.rs)
    PiSugar I2C     (pisugar/)
    GPIO WL_REG_ON  (recovery/)
```

The `Daemon` struct owns all subsystem state. Each epoch cycles through
five phases: **Scan** (channel hop, discover APs), **Attack** (rate-limited
deauths), **Capture** (check for new pcapng files), **Display** (update
e-ink), and **Sleep** (watchdog ping, wait). Mood adjusts based on
handshakes captured vs blind epochs, producing one of 24 kaomoji faces.

## Test structure

Tests live as `#[cfg(test)] mod tests` at the bottom of each module.
The integration test in `main.rs` (`test_integration_three_epochs`) creates
a full `Daemon`, runs 3 epoch cycles, and verifies the display was updated
and the epoch counter advanced.

## Release profile

The `Cargo.toml` release profile is tuned for Pi Zero 2W:

- `opt-level = "z"` -- optimise for binary size
- `lto = true` -- link-time optimisation
- `codegen-units = 1` -- single codegen unit for better optimisation
- `strip = true` -- strip debug symbols
- `panic = "abort"` -- abort on panic (saves unwinding code)

The resulting x86_64 binary is ~1.1 MB; the aarch64 binary is similar.
