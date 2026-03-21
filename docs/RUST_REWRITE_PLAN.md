# Oxigotchi Rust Rewrite Plan

## Why Rewrite

| Metric | Python (pwnagotchi) | Rust (target) |
|--------|-------------------|---------------|
| RAM | 92MB RSS | ~5-10MB |
| Startup | ~20s (Python venv + imports) | ~1s |
| CPU idle | 8% (interpreter overhead) | <1% |
| Binary size | 150MB+ (venv + deps) | ~5MB static |
| Dependencies | Python 3.13 + venv + pip + PIL + flask | Single static binary |
| SD card wear | Constant tmpfs I/O from Python | Minimal |

AngryOxide already proves Rust works for WiFi tooling on Pi Zero 2W. The rewrite extends AngryOxide into a full oxigotchi daemon.

## Architecture

```
oxigotchi (single Rust binary)
├── core/           - Main loop, epoch tracking, config
├── display/        - E-ink driver (waveshare v4, SPI)
├── wifi/           - Monitor mode, channel hopping (reuse AO)
├── attacks/        - Deauth, PMKID, CSA (reuse AO engine)
├── capture/        - Pcapng management, hashcat conversion
├── web/            - Dashboard (axum + htmx, no JS framework)
├── bluetooth/      - BT PAN tether (standalone)
├── pisugar/        - Battery monitoring (I2C)
├── personality/    - Faces, status messages, mood
├── plugins/        - WASM or Lua plugin system (optional)
└── recovery/       - WiFi recovery, watchdog, self-healing
```

### Key Decisions

1. **Single binary** — no Python, no venv, no bettercap, no pwngrid
2. **AngryOxide as library** — import AO's WiFi/attack code as a Rust crate, not a subprocess
3. **No bettercap** — saves 52MB RAM; channel scanning done natively
4. **E-ink via SPI** — direct GPIO/SPI using `rppal` crate, no Python PIL
5. **Web UI via axum** — lightweight async HTTP, serves embedded HTML
6. **Config via TOML** — `toml` crate, same format as pwnagotchi for migration
7. **Cross-compile** — build on x86 with `cross` for `aarch64-unknown-linux-gnu`
8. **Plugin system** — optional, Phase 3. WASM (wasmtime) or Lua (mlua) for user scripts

## Sprint Plan

### Phase 1: Core + Display (Sprints 1-3)

**Sprint 1: Project scaffold + e-ink display**
- [ ] Cargo workspace: `oxigotchi` binary + `oxigotchi-display` crate
- [ ] SPI e-ink driver for Waveshare 2.13" V4 using `rppal`
- [ ] Render text, faces (kaomoji), status bar
- [ ] TDD: display buffer rendering, text positioning, face selection
- [ ] Cross-compile and test on Pi

**Sprint 2: Config + personality + faces**
- [ ] TOML config parser (migrate pwnagotchi config.toml format)
- [ ] Personality system: faces, moods, status messages
- [ ] PNG face support (embedded in binary or loaded from disk)
- [ ] TDD: config parsing, face selection, mood transitions

**Sprint 3: Main loop + epoch tracking**
- [ ] Epoch loop: scan → attack → capture → update display
- [ ] Metrics: blind epochs, handshakes, channel hops, uptime
- [ ] Mood engine: happy on captures, sad on blind, bored on idle
- [ ] TDD: epoch state machine, mood transitions, metric calculations

### Phase 2: WiFi + Attacks (Sprints 4-6)

**Sprint 4: Monitor mode + channel scanning**
- [ ] Create wlan0mon via netlink (no `iw`/`airmon-ng`)
- [ ] Channel hopping with configurable dwell time
- [ ] AP discovery from beacon/probe frames
- [ ] TDD: channel hop scheduling, AP tracking, dedup

**Sprint 5: AngryOxide integration**
- [ ] Import AO as a Rust crate (or extract attack engine)
- [ ] PMKID, deauth, CSA, disassoc attacks
- [ ] Rate limiting (rate 1 default for BCM43436B0)
- [ ] Whitelist support
- [ ] TDD: attack scheduling, rate limiting, whitelist filtering

**Sprint 6: Capture management**
- [ ] Pcapng file management, naming (hostname-timestamp)
- [ ] Hashcat .22000 conversion (hcxtools integration or native)
- [ ] Capture dedup, cleanup rotation
- [ ] WPA-SEC upload integration
- [ ] TDD: filename generation, capture counting, upload queue

### Phase 3: Networking + Web (Sprints 7-9)

**Sprint 7: USB gadget + RNDIS**
- [ ] USB gadget configuration (g_ether)
- [ ] Static IP setup (10.0.0.2/24 + 192.168.137.2/24)
- [ ] SSH server (or just ensure systemd SSH works)
- [ ] TDD: IP configuration, connectivity checks

**Sprint 8: Web dashboard**
- [ ] axum HTTP server on port 8080
- [ ] Dashboard HTML (embed in binary, htmx for reactivity)
- [ ] API endpoints: status, captures, attacks, rate, mode, config
- [ ] Live display preview (render e-ink frame as PNG)
- [ ] TDD: API endpoints, JSON responses, config updates

**Sprint 9: Bluetooth tether**
- [ ] BlueZ D-Bus integration for BT PAN
- [ ] Phone pairing helper
- [ ] Connection toggle via PiSugar button
- [ ] TDD: BT state machine, connection lifecycle

### Phase 4: Hardware + Recovery (Sprints 10-11)

**Sprint 10: PiSugar integration**
- [ ] I2C communication with PiSugar 3
- [ ] Battery level, charging status, low-power shutdown
- [ ] Button handler: single tap, double tap, long press
- [ ] TDD: I2C protocol, button debounce, shutdown sequence

**Sprint 11: Self-healing + recovery**
- [ ] WiFi SDIO keepalive (replace wlan_keepalive.c — built into main binary)
- [ ] GPIO power cycle recovery (WL_REG_ON)
- [ ] Watchdog integration
- [ ] Boot diagnostics logging
- [ ] TDD: recovery state machine, crash detection, GPIO sequences

### Phase 5: Migration + Polish (Sprints 12-13)

**Sprint 12: Migration tooling**
- [ ] Import existing config.toml
- [ ] Import existing captures/handshakes
- [ ] Systemd service file for oxigotchi (replace pwnagotchi.service)
- [ ] Coexistence mode: run alongside pwnagotchi during migration
- [ ] TDD: config migration, capture import

**Sprint 13: Polish + documentation**
- [ ] Performance benchmarking vs Python pwnagotchi
- [ ] Memory profiling
- [ ] Boot time measurement
- [ ] User documentation
- [ ] Image builder (bake_v3.sh)
- [ ] Release build optimization (LTO, strip, UPX)

## Crate Dependencies

```toml
[dependencies]
rppal = "0.19"          # GPIO, SPI, I2C for Pi
tokio = { version = "1", features = ["full"] }
axum = "0.7"            # Web server
serde = { version = "1", features = ["derive"] }
toml = "0.8"            # Config
image = "0.25"          # PNG face rendering
embedded-graphics = "0.8"  # Display primitives
pcap = "2"              # Packet capture
neli = "0.6"            # Netlink (nl80211)
log = "0.4"
env_logger = "0.11"
chrono = "0.4"
```

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| AO crate extraction is hard | Start with subprocess, migrate to crate later |
| SPI e-ink driver complexity | Port existing Python driver, test extensively |
| nl80211 monitor mode | Use existing `iw` commands initially, native later |
| Cross-compilation issues | Use `cross` tool, test in CI |
| Feature parity takes too long | Ship incrementally — display-only first, then attacks |
| Plugin ecosystem loss | WASM plugins in Phase 3, or accept native-only |

## Success Criteria

- [ ] Single 5MB static binary replaces pwnagotchi + bettercap + pwngrid
- [ ] RAM usage < 10MB (vs 170MB+ today)
- [ ] Boot to scanning < 5s (vs 20s+ today)
- [ ] All current features: AO/PWN modes, display, web UI, BT tether, PiSugar
- [ ] SD card image < 2GB (vs 13GB today)
