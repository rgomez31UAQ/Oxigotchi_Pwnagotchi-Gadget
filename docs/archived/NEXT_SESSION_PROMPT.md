# Next Session: Fix Rusty E-ink Display Driver

## Context
Rusty Oxigotchi (Rust rewrite of pwnagotchi) is at `/c/msys64/home/user/oxigotchi/rust/`. It compiled, deployed to Pi Zero 2W, spawned AO successfully, web dashboard works on :8080, but the **e-ink display is broken** — garbled/blank output, EPD BUSY timeout errors.

We switched back to Python pwnagotchi for now (`systemctl enable pwnagotchi bettercap`). Rusty is disabled but the binary is at `/usr/local/bin/rusty-oxigotchi`.

## The Problem
The Rust SSD1680 driver at `rust/src/display/driver.rs` was built from the datasheet, not from the working Python driver. The init sequence, timing, LUT tables, and byte ordering don't match what the real Waveshare 2.13" V4 board expects.

Symptoms:
- Display flashes/garbles on refresh
- `EPD BUSY timeout (5000ms)` errors in logs
- Display goes blank with remnants of old content

## What Needs To Be Done

1. **SSH to Pi** (pi@10.0.0.2) and read the WORKING Python waveshare driver:
   ```
   cat /home/pi/.pwn/lib/python3.13/site-packages/pwnagotchi/ui/hw/waveshare2in13_V4.py
   ```
   Also read the underlying EPD library it imports (likely waveshare_epd or similar).

2. **Port the exact Python SPI sequence to Rust** — byte-for-byte, same init commands, same LUT tables, same timing delays. The file to modify is `rust/src/display/driver.rs`.

3. **Key things to match from Python driver:**
   - Full init command sequence with exact register values
   - LUT (Look-Up Table) waveform data for full and partial refresh
   - BUSY pin polling (GPIO 24, active LOW on this board)
   - RST pin toggle sequence (GPIO 17)
   - DC pin (GPIO 25) — command vs data mode
   - CS pin (GPIO 8)
   - SPI speed and mode
   - Framebuffer byte order: the Python driver transposes landscape buffer to portrait for the SSD1680
   - Rotation handling (config says rotation=180)

4. **Test on Pi:** Cross-compile with WSL:
   ```
   wsl -d Ubuntu -e bash -c 'source ~/.cargo/env && cd /mnt/c/msys64/home/user/oxigotchi/rust && export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-gnu-gcc && cargo build --release --target aarch64-unknown-linux-musl'
   ```
   Then deploy:
   ```
   sudo systemctl stop pwnagotchi bettercap
   scp rust binary to pi:/usr/local/bin/rusty-oxigotchi
   sudo systemctl start rusty-oxigotchi
   ```

5. **Display should only refresh when content changes** — there's already a content_hash check in `display/mod.rs`.

## Key Files
- `rust/src/display/driver.rs` — SSD1680 SPI driver (NEEDS FIXING)
- `rust/src/display/mod.rs` — Screen struct, flush logic
- `rust/src/display/buffer.rs` — FrameBuffer with DrawTarget
- `rust/src/main.rs` — Daemon with update_display()
- `rust/Cargo.toml` — rppal for GPIO/SPI on aarch64

## Current Rust Architecture
- 638+ tests, 2.2MB static ARM64 binary
- 16 modules: display, wifi, ao, web, config, personality, epoch, capture, bluetooth, pisugar, recovery, migration, network, attacks
- axum web server on :8080
- AO subprocess management with auto-restart
- WiFi monitor mode via iw commands
- PiSugar I2C battery reading

## Pi Details
- Pi Zero 2W, SSH: pi@10.0.0.2
- Waveshare 2.13" V4 e-ink (SSD1680 controller)
- SPI0, GPIO: RST=17, DC=25, CS=8, BUSY=24
- Display: 250x122 pixels, 1-bit, rotation 180
- Config: ui.invert=true (white background = 0x00)

## Don't Forget
- Deploy to Pi AND repo
- No security hardening (toy for newbies)
- Never share ROM addresses or firmware disassembly
- Check /c/msys64/home/user/oxigotchi/docs/ for full project docs
