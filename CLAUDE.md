# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Oxigotchi

Oxigotchi ("Rusty Oxigotchi") is a Rust rewrite of the Python-based Pwnagotchi WiFi capture tool, targeting the Pi Zero 2W with a Waveshare e-ink display and PiSugar battery. It captures WPA handshakes via AngryOxide (AO), performs Bluetooth attacks, and renders a personality-driven face on e-ink. It is a single `rusty-oxigotchi` binary (v3.3.0, Rust edition 2024) that runs as a systemd service on the Pi.

## Build & Test Commands

### Cross-compile for Pi (aarch64)
```bash
wsl -d Ubuntu -- bash -c "source ~/.cargo/env && cd /mnt/c/msys64/home/gelum/oxigotchi/rust && cargo build --release --target aarch64-unknown-linux-gnu"
```

### Run tests (host, from MSYS2)
```bash
cd rust && cargo test
```

### Run a single test
```bash
cd rust && cargo test test_daemon_construction
```

### Check / clippy
```bash
cd rust && cargo check
cd rust && cargo clippy
```

### Deploy to Pi
```bash
scp rust/target/aarch64-unknown-linux-gnu/release/oxigotchi pi@<RNDIS_IP>:/home/pi/oxigotchi
ssh pi@<RNDIS_IP> "sudo cp /home/pi/oxigotchi /usr/local/bin/rusty-oxigotchi && sudo systemctl restart rusty-oxigotchi"
```

## Architecture

### Daemon Main Loop (`rust/src/main.rs`)
The `Daemon` struct owns all subsystem state (~47 fields). On startup: `main()` runs migration, loads config, spawns the axum web server on a tokio task, then runs `Daemon::boot()` followed by an infinite `run_epoch()` loop in a blocking thread.

Each epoch (~0-30s configurable) runs phases in sequence: **Scan -> Attack -> Capture -> Display -> Sleep**. BT mode has its own `run_bt_epoch()` path.

### Three Operating Modes (`OperatingMode`)
- **RAGE** — WiFi attacks via AngryOxide, channel hopping, handshake capture
- **BT** — Bluetooth offensive: HCI scanning, GATT resolution, BT attacks (ATT fuzz, KNOB, L2CAP, SMP)
- **SAFE** — Passive BT tethering (PAN internet), no attacks

Mode transitions are atomic via `RadioManager` (`radio.rs`), which manages a lock file and coordinates WiFi/BT hardware teardown/bringup, including firmware loading for BT attack mode.

### Key Modules

| Module | Purpose |
|--------|---------|
| `ao` | AngryOxide process lifecycle (start/stop/crash recovery) |
| `attacks` | WiFi attack scheduler (deauth, association) |
| `bluetooth/` | BT tethering, HCI scanning, GATT discovery, BT attacks, firmware loading |
| `bluetooth/dbus.rs` | D-Bus BlueZ PAN: Network1 connect/disconnect, ObjectManager device enumeration, Agent1 pairing |
| `bluetooth/attacks/` | BT attack implementations: ATT fuzz, BLE adv, KNOB, L2CAP fuzz/flood, SMP (8 files) |
| `bluetooth/adapter/` | BlueZ adapter management, btmon integration, WiFi-BT coexistence |
| `bluetooth/ui/` | BT-specific dashboard and e-ink display integration |
| `capture` | Capture pipeline: tmpfs staging -> validation -> SD card, wpa-sec upload |
| `config` | TOML config loading from `/etc/oxigotchi/config.toml` (nested sections: main, ui, bluetooth, bt_feature, bt_attacks, gpu, qpu) |
| `display` | E-ink driver (SPI via rppal), framebuffer, fonts, face rendering |
| `epoch` | Epoch result tracking (APs, handshakes, attacks) |
| `firmware` | WiFi firmware health monitoring |
| `gpu/` | VideoCore IV GPU: runtime telemetry, snapshot optimization, QPU offload, trace infrastructure |
| `gpu/ui/` | GPU telemetry dashboard |
| `lua` | Lua 5.4 plugin runtime (mlua): loads plugins, exposes `register_indicator`/`set_indicator` |
| `migration` | First-boot pwnagotchi -> oxigotchi config/capture migration |
| `network` | Network state: interface detection, IP tracking, internet checks |
| `personality` | Face/mood state machine, XP system, message variety engine |
| `pisugar` | PiSugar battery HAT: level, charging, voltage, shutdown |
| `qpu/` | QPU compute engine: mailbox interface, shader programs, ring buffers, RF classifier (7 files) |
| `radio` | Radio lock manager: atomic WiFi<->BT mode transitions |
| `rage` | Rage level presets (7 levels of aggression tuning: rate, dwell, channels) |
| `recovery` | Crash loop detection, modprobe cycling, WiFi recovery escalation |
| `web` | Axum web server: ~31 REST routes, WebSocket live updates, dashboard HTML, Discord webhooks |
| `wifi` | WiFi/monitor mode management, channel scoring |

### Lua Plugin System
12 plugins in `rust/plugins/` (ao_status, aps, battery, bt_status, bt_summary, crash, ip_display, mode, status_msg, sys_stats, uptime, www). Each defines `on_load(config)` and `on_epoch(state)` callbacks, registering e-ink indicators with x/y coordinates via `register_indicator()`.

### Web Dashboard
The axum web server (`web/mod.rs`, `web/html.rs`) serves a dashboard with ~31 routes and live WebSocket updates. POST handlers update shared state (`SharedState = Arc<Mutex<DaemonState>>`) optimistically — changes apply immediately, not on next epoch.

### Shared State Pattern
`Daemon` owns all state. The web server communicates via `SharedState` (Arc<Mutex<DaemonState>>). Each epoch, `sync_to_web()` pushes daemon state to the shared struct. Web commands are queued and processed by `process_web_commands()`.

### Systemd Services
24 service/timer units in `services/` manage the Pi runtime: `rusty-oxigotchi.service` (main daemon), bt-agent, buffer-cleaner, emergency-ssh, fix-ndev, nm-watchdog, pisugar-watchdog, safe-shutdown, usb0-fallback, wifi-recovery, wifi-watchdog, wlan-keepalive, and more. Helper scripts live in `scripts/`.

## Conditional Compilation

Hardware-dependent code uses `cfg(unix)`, `cfg(target_os = "linux")`, and `cfg(target_arch = "aarch64")` guards. The `dbus` crate is linux-only, `rppal` is aarch64-only. Tests run on the host (Windows/MSYS2) using stub implementations — real hardware interaction only happens on the Pi.

### Release Profile
Binary is optimized for size: `opt-level="z"`, `lto="thin"`, `strip=true`, `panic="abort"`.

### Cross-Compile Toolchain
`.cargo/config.toml` configures the aarch64 target with gcc linker. WSL Ubuntu needs multiarch setup with `libdbus-1-dev:arm64` from `ports.ubuntu.com` for the dbus crate.

## Key Files Outside Rust

| Path | Purpose |
|------|---------|
| `pi_config.toml` | Reference config (pwnagotchi format, used by migration) |
| `config/angryoxide-v5.toml` | AO config overlay deployed to Pi |
| `services/` | 24 systemd service/timer units for Pi runtime |
| `scripts/` | Pi systemd helpers: bt-keepalive, pisugar-watchdog, safe-shutdown, etc. |
| `tests/test_*.py` | Python integration tests (test AO parsing, v3 integration) |
| `plugin/` | Python helpers: frame analysis, stub client, walkby simulator |
| `firmware_analysis/` | BCM43436B0 WiFi firmware RE documentation |
| `bt_firmware_analysis/` | BT firmware RE: ROM/RAM dump analysis |
| `gpu_firmware_analysis/` | VideoCore IV GPU firmware analysis |

## Pi Deployment Rules

### Always deploy to Pi AND repo
Every code change must go to the Pi (via SCP + apply) AND be committed to the repo. The Pi doesn't auto-pull from GitHub.

### Pi connection
- Pi IP: check `ipconfig` for the RNDIS adapter — don't assume an IP from memory
- SSH: `ssh -o LogLevel=ERROR pi@<IP>` (suppress banners)
- SCP files to `/home/pi/` first (never /tmp), then `sudo cp` to destination
- All files SCP'd from MSYS2 will have CRLF line endings — always run `sed -i 's/\r$//'` on shell scripts after SCP

### Patch deployment checklist
1. Check config overlay FIRST — if angryoxide-v5.toml is missing, everything downstream is wrong
2. Run full verification audit (grep for markers on Pi) — identify ALL gaps before changing anything
3. Collect ALL fixes needed, then apply in ONE pass, then ONE restart
4. Idempotency checks must test for ALL features in the patch, not just the earliest one
5. After deploying patches, restart pwnagotchi ONCE and verify every patch marker
6. Ask the user to confirm visual output on the e-ink screen — you can't see it

## Upstream Contribution Rules
- NEVER mention Claude/AI in upstream PRs, issues, or commits
- NEVER share ROM addresses, RE artifacts, or function maps
- NEVER mention internal patch layers, frame padding, or attack tooling
- Only share GPL kernel code fixes with observable crash symptoms
- Draft for user review before posting anything public
