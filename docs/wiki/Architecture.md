# Architecture & Self-Healing

← [Back to Wiki Home](Home)

---

## How It Works

A single Rust binary (`rusty-oxigotchi`) manages everything: it spawns AngryOxide as a subprocess, drives the e-ink display via SPI, runs the web dashboard on port 8080, manages Bluetooth tethering, executes Lua plugins, and monitors the WiFi firmware for crashes. Only one program touches the WiFi chip at a time — no TX/RX conflicts, no SDIO bus contention.

The daemon operates in three modes:

- **RAGE** — WiFi monitor mode, AngryOxide attacking, BT tether stays connected. The wardriving mode.
- **BT** — Bluetooth offensive: HCI scanning, GATT resolution, BT attacks (ATT fuzz, KNOB, L2CAP fuzz/flood, SMP). WiFi off, phone tether disconnected.
- **SAFE** (default) — WiFi managed mode, BT tethered to phone for internet, no attacks. Used for uploads and maintenance.

Toggle between them with the **PiSugar3 button** (single tap) or the **web dashboard**. Mode transitions are atomic via `RadioManager`, which coordinates WiFi/BT hardware teardown and bringup including patchram loading for BT attack mode.

## Module Overview

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
    mod.rs          Mood, Face (26 variants), XP/leveling, SystemInfo
  attacks/mod.rs    Attack scheduler, rate limiter (BCM43436B0 safe at rate 1)
  capture/mod.rs    Capture file management, WPA-SEC upload queue, auto-backup
  wifi/mod.rs       WiFi monitor mode, channel hopping, AP tracker, whitelist
  pisugar/mod.rs    PiSugar 3 battery I2C, button debouncer, action mapping
  bluetooth/mod.rs  Bluetooth PAN tethering, HCI scanning, GATT discovery
  bluetooth/dbus.rs D-Bus BlueZ wrapper: PAN connect/disconnect, Agent1, device enumeration
  bluetooth/attacks/ BT attack implementations: ATT fuzz, KNOB, L2CAP, SMP
  recovery/mod.rs   WiFi SDIO recovery, GPIO power cycle, watchdog
  qpu/
    mod.rs          QPU feature config (TOML serde)
    capture.rs      Pcap capture thread (libpcap FFI, radiotap parsing)
    classifier.rs   Frame classifier (CPU path + preserved QPU kernel)
    engine.rs       QPU engine orchestrator (mailbox, V3D, ring buffer)
    mailbox.rs      VideoCore IV mailbox interface (/dev/vcio, GPU memory)
    rf.rs           Per-epoch RF environment statistics
    ringbuf.rs      SPSC ring buffer in GPU memory, FrameEntry extraction
  rage/mod.rs       Rage level presets (1-3: Chill/Hunt/RAGE)
  radio/mod.rs      Radio lock manager: atomic WiFi<->BT mode transitions
  web/mod.rs        REST API types, embedded HTML dashboard
  migration/mod.rs  Import legacy pwnagotchi config and captures
```

## Architecture Diagram

```
                  +-------------------+
                  |     Daemon        |
                  |  (main.rs)        |
                  +--------+----------+
                           |
     +-------+--------+--------+--------+--------+
     |       |        |        |        |        |
EpochLoop  Screen  WifiMgr  Attacks  Captures  QpuEngine
(epoch.rs) (display/) (wifi/) (attacks/) (capture/) (qpu/)
     |                                            |
Personality  <── RF mood deltas ──  RfEnvironment
(personality/)                      (qpu/rf.rs)
     |
Mood + Face (26 variants)

  Hardware layer (aarch64 only):
    SPI e-ink driver    (display/driver.rs)
    PiSugar I2C        (pisugar/)
    GPIO WL_REG_ON     (recovery/)
    VideoCore IV GPU   (qpu/mailbox.rs, qpu/engine.rs)
    libpcap/wlan0mon   (qpu/capture.rs)
```

## Epoch Loop

The `Daemon` struct owns all subsystem state. Each epoch cycles through five phases:

1. **Scan** — Channel hop, discover APs. AO scans across configured channels (default: 1, 6, 11) with configurable dwell time. New APs are added to the tracker.

2. **Attack** — Rate-limited deauths and other attacks against discovered APs. The attack scheduler respects per-type toggles and the Smart Skip setting. Rate is configurable (1-3) via dashboard or RAGE Slider.

3. **Capture** — Check `/tmp/ao_captures/` for new pcapng files. Validate via hcxpcapngtool, convert to .22000, move proven handshakes to SD card. Delete junk from tmpfs.

4. **Display** — Update e-ink with current face, stats, channel. The personality engine selects a face based on mood score (influenced by captures, blind epochs, RF environment).

5. **Sleep** — Watchdog ping, PSM counter reset (every 15 min), wait for next epoch (~30 seconds).

## Self-Healing Stack

The daemon includes a multi-layer recovery system that handles firmware edge cases automatically:

| Layer | Trigger | Action |
|-------|---------|--------|
| **PSM watchdog reset** | Every 15 minutes | Resets PSM/DPC/RSSI firmware counters via SDIO RAMRW, preventing long-running degradation |
| **Crash loop detection** | 3+ SIGABRT from AO | Triggers full `modprobe -r brcmfmac && modprobe brcmfmac` recovery cycle instead of endlessly restarting AO |
| **AO watchdog** | AO process dies | Restarts AO with exponential backoff (5s, 10s, 20s... up to 5 minutes) |
| **GPIO power cycle** | SDIO bus error -22 | Power-cycles the BCM43436B0 chip via GPIO 41 (WL_REG_ON), rebinds MMC controller, reloads driver |
| **Graceful give-up** | All recovery exhausted | Daemon gives up on WiFi gracefully — **never reboots the Pi**. SSH and web dashboard stay accessible |
| **USB lifeline** | Always | SSH always available at `10.0.0.2`, even when WiFi is dead |

**Key principle:** The daemon never reboots the Pi. No matter how badly the WiFi firmware misbehaves, SSH and the web dashboard remain accessible. This is the opposite of stock pwnagotchi, which can leave you unable to connect for hours.

## First Boot Sequence

1. **0:00** — Power LED lights up.
2. **~3s** — Kernel loaded, Rust daemon starts. Boot splash shows the bull on e-ink.
3. **~5s** — AngryOxide launches. Scanning begins in RAGE mode.
4. **~5s+** — Attacks begin automatically. APs appear in the dashboard.

First boot after flashing takes a few seconds extra (migration from pwnagotchi config runs once).

## State Persistence

The daemon saves state to `/var/lib/oxigotchi/state.json` on every epoch:

- Attack type toggles (which attacks are enabled/disabled)
- Whitelist entries
- WPA-SEC API key
- Discord webhook configuration
- Channel configuration and autohunt state
- RAGE Slider level
- Smart Skip toggle
- XP and level

All settings survive reboots. The state file is small (~2KB) and written atomically (write to temp file, rename).
