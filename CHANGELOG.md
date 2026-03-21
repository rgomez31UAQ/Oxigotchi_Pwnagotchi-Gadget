# Changelog

## [3.0.0-dev] — Rusty Oxigotchi (in development)

The Rust rewrite has a name: **Rusty Oxigotchi** (codename: **Rusty**). This is the full-stack Rust replacement for Python pwnagotchi + bettercap + pwngrid — a single static binary that does everything. The bull is being forged in Rust.

See [docs/RUST_REWRITE_PLAN.md](docs/RUST_REWRITE_PLAN.md) for the full sprint plan and architecture. Initial scaffold lives in `rust/`.

## [2.2.0] - 2026-03-21

### Added
- **PiSugar 3 button controls**: single press = toggle BT tethering, double press = toggle AUTO/MANU mode, long press = toggle AO/PWN mode
- **Standalone bt-tether daemon**: Bluetooth tethering decoupled from pwnagotchi's bt-tether plugin (which threw "Error with mac address" even when disabled). Independent daemon toggled via PiSugar button.
- **wlan_keepalive native daemon**: C binary replaces tcpdump-based keepalive. Sends probe frames on wlan0mon every 100ms to prevent BCM43436B0 SDIO bus idle crashes. Cross-compiled for aarch64 in bake_v2.sh.
- **wifi-recovery.service**: GPIO power-cycles BCM43436B0 via WL_REG_ON (GPIO 41) on boot if wlan0 fails to appear within 4 seconds. Recovers dead SDIO bus without full power cycle.
- **bake_v2.sh image builder**: 20-step reproducible image build script. Mounts base image via loopback, applies all fixes (plugins, config, faces, tools, services, hostname, dual-IP, blacklists, cleanup), runs full verification, unmounts. One command produces a deterministic image.
- **Kernel module blacklist**: `blacklist bcm2835_v4l2` in `/etc/modprobe.d/blacklist-camera.conf` prevents camera/video modules from loading, eliminating VCHI errors and saving RAM.
- **Handshake directory consolidation**: `/root/handshakes` symlinked to `/etc/pwnagotchi/handshakes/` — single canonical directory for all captures.
- **Rootfs sentinel**: `/var/lib/.rootfs-expanded` created to silence resize-rootfs.service false failures.
- **480/480 tests passing** (281 Python + 199 Rust)

### Fixed
- **Blind epochs (H1)**: AO plugin now emits `association`, `deauth`, and `handshake` events for every AO capture, feeding the pwnagotchi AI accurate reward signals. Synthetic AP heartbeat (`AO-active`) injected when AP list empty prevents false blind restart.
- **Peer error (H2)**: `update_peers` AttributeError (`'Array' object has no attribute 'read'`) suppressed in patched agent.py. Peer discovery is non-critical.
- **Capture filenames (H3)**: AO plugin passes `--name` flag with hostname (defaults to `oxigotchi`). Captures now named `oxigotchi-DATETIME.pcapng` instead of `-DATETIME.pcapng`.
- **Boot time**: Reduced from ~65s to ~20s. Fixed usb0-fallback blocking (30.5s), merged fix-ndev + wifi-recovery, fixed bt-agent race, made bootlog async, disabled unused services.
- **BT-Tether errors (M3)**: Plugin disabled in config.toml. Standalone daemon replaces it.
- **resize-rootfs failure (M2)**: Sentinel file prevents every-boot failure.
- **Service file permissions (M8)**: All service files set to `chmod 644` in bake_v2.sh.
- **AO default rate**: Set to 1 (rate 2 crashes BCM43436B0 in ~68 seconds under load).
- **Zombie process**: Fixed in pwnagotchi agent reap logic.
- **rpi-usb-gadget-ics disabled**: Was causing NM-dispatcher spam in logs.
- **Mode indicator position**: Fixed in PWN mode UI.

### Changed
- **AO mode is now the default**: Ships as oxigotchi (not pwnagotchi) with AO mode enabled, rate 1, bull faces.
- **Dual-IP networking**: USB gadget configured with both `10.0.0.2/24` and `192.168.137.2/24` for Windows ICS compatibility.
- **Whitelist**: Set to `["Alpha", "Alpha 5G"]` in both config.toml and angryoxide-v5.toml overlay.
- **Documentation updated**: DISPLAY_SPEC.md, IMAGE_FIXES.md, DEEP_ANALYSIS.md, README.md, CHANGELOG.md all updated to reflect current state.
- **No security hardening (intentional)**: This is a toy for beginners. USB-only access mitigates risk.

## [2.1.0] - 2026-03-17

### Added
- **AO mode display overhaul** — complete e-ink layout redesign for AngryOxide mode:
  - Top bar: `AO: {session}/{total} | {uptime} | CH:{channels}` replaces PWND counter
  - Bottom bar: `CRASH:0` (firmware crash counter) replaces CH indicator
  - CH and AP indicators hidden (useless in AO mode — AO manages its own)
  - No name label, no cursor blink — bull face gets full middle zone
  - Bull face positioned at Y=16, almost touching top bar line
- **StubClient** (`stub_client.py`): bettercap API replacement for AO-only mode
- **Frame padding** (`frame_padding.py`): pads injection frames to 650+ bytes to prevent BCM43436B0 PSM watchdog crashes
- **WalkBy plugin** (`walkby.py`): concurrent blitz attack for walk-by handshake capture (PWN mode only)
- **Synthetic blind epoch fix**: injects heartbeat AP when monitor interface is up, preventing false "blind" restarts in AO mode
- **Display spec document** (`docs/DISPLAY_SPEC.md`): 600+ line comprehensive spec covering every pixel position, every event-to-face mapping, boot/shutdown sequences, error states, and mode switching
- **SD card image builder** (`tools/build_image.py`): SSH to Pi, strip personal data, zero free space, stream dd, gzip compress

### Fixed
- **Bull faces in PWN mode boot**: splash service now checks for AO overlay before rendering bull face — PWN mode gets clean Korean faces from start
- **[unknown] in PWND counter**: removed last-captured AP hostname from display in AO mode (AO indicator shows capture count instead)
- **Misleading attack messages**: `associate()` and `deauth()` now early-return in AO mode — no more "Associating to AP_NAME" when AO handles attacks
- **Rate 2 recommendation**: dashboard description fixed to warn that rate 1 is maximum safe for BCM43436B0 (rate 2 causes firmware crash at 0x204CA)
- **Blind epoch hack**: `mon_max_blind_epochs` can stay at default 5 instead of 9999 — synthetic AP heartbeat keeps pwnagotchi alive in AO mode
- **Agent.py idempotency**: patch script now checks for all sub-patches (`ao_active` + `AO handles attacks`) before skipping

### Changed
- **AO indicator position**: moved from (0, 85) to (0, 0) — top-left of display, replacing PWND
- **AO indicator format**: now shows `AO: {captures}/{total} | {uptime} | CH:{channels}` — captures, total unique, uptime, and active channel list in one line
- **Bottom bar in AO mode**: `CRASH:0` (firmware health) + BT + CHG + AUTO — all AO-relevant
- **apply_patches.sh**: expanded from 5 to 10 patches — adds __init__.py, agent.py (AO mode + blind epoch + attack skip + PWND skip), view.py (hide name, face near top), cli.py (empty name), components.py (PNG fallback)
- **Nexmon upstream**: submitted ndev_global dangling pointer fix to seemoo-lab/nexmon#677

## [2.0.0] - 2026-03-15

### Added
- **AngryOxide plugin v2.0** (`angryoxide_v2.py`): complete rewrite of the pwnagotchi plugin with 22 API endpoints (9 GET, 13 POST) and a full web dashboard
- **Web dashboard**: mobile-friendly dark-theme control panel with live auto-refresh (5s status, 10s AP list, 30s logs), served at the plugin's webhook root
- **28 bull face PNGs** for e-ink display in `faces/eink/`: awake, angry, bored, broken, cool, debug, demotivated, excited, friend, fw_crash, ao_crashed, battery_low, battery_critical, grateful, happy, intense, lonely, look_l, look_l_happy, look_r, look_r_happy, motivated, sad, shutdown, sleep, smart, upload, wifi_down
- **Mode switcher** (`pwnoxide-mode.sh`): switch between AO mode (AngryOxide + bull faces) and PWN mode (stock bettercap) with watchdog and firmware rollback support
- **Boot/shutdown splash service** (`Oxigotchi-splash.py` + `Oxigotchi-splash.service`): systemd unit that displays bull face on e-ink at boot and shutdown
- **Smart Skip toggle**: auto-whitelist APs that already have captured handshakes, skipping them to focus on new targets
- **Capture file downloads**: individual capture download via `/api/download/capture/:filename` and bulk ZIP download via `/api/download/all`, covering both AO and bettercap handshake directories
- **Discord notifications**: POST `/api/discord-webhook` to configure a Discord webhook URL for capture alerts
- **GPS integration**: automatic `--gpsd` flag when gpsd is detected running on 127.0.0.1:2947
- **Session stats**: live capture count, capture rate per epoch, stable epoch counter, uptime tracking (formatted as Xm/Xh)
- **Log viewer**: `/api/logs` endpoint filters journalctl for angryoxide-related entries, displayed in a monospace log panel on the dashboard
- **Capture type detection**: heuristic classification of captures as PMKID (< 2KB) vs 4-way handshake based on file size
- **AP targeting from dashboard**: nearby networks table sorted by RSSI with one-click "target" button per AP; `/api/targets/add` and `/api/targets/remove` endpoints
- **Whitelist table**: combined view of AO plugin whitelist entries and config.toml whitelist, with MAC/SSID display, source labels, and per-entry remove buttons
- **BT keepalive timer**: UI update cycle suppresses bt-tether status text and overlapping plugin UI elements to keep AO display clean
- **Attack type toggles**: 6 individual attack types (deauth, PMKID, CSA, disassociation, anon reassoc, rogue M2) controllable via dashboard switches with per-toggle descriptions
- **Attack rate control**: 3-level rate selector (quiet/normal/aggressive) with immediate AO restart on change
- **Channel configuration**: custom channel list, autohunt mode, and dwell time (1-30s) slider with apply button
- **State persistence**: runtime config (targets, whitelist, rate, attacks, channels, autohunt, dwell, skip_captured) saved to JSON and restored on plugin load
- **Exponential crash backoff**: restart delay follows 5s * 2^(n-1) up to 300s cap, with automatic reset after 5 minutes of stability
- **Firmware crash recovery**: detects brcmfmac -110 channel set errors and firmware halt via regex on kernel logs, triggers modprobe cycle with interface polling
- **Battery monitoring**: reads PiSugar battery level with critical (< 15%) and low (< 20%) face/status overrides on epoch
- **13-step deployer** (`deploy_pwnoxide.py`): one-command SSH installer covering preflight, firmware backup, v5 firmware upload, angryoxide binary, plugin, config, mode switcher, set-iovars disable, WiFi stability fixes, face PNGs, splash service, verification with MD5 checksums, and reboot with post-boot validation
- **180 unit tests across 30 test classes** (`test_angryoxide.py`): command building, backoff math, capture parsing, whitelist normalization, uptime formatting, all webhook endpoints, health checks, firmware crash patterns, skip-captured logic, state persistence, file downloads, boot/shutdown faces, AP list with captured flags, mode API, face helper, battery level, name removal, UI updates, epoch edge cases, MAC extraction, and more
- **XSS prevention**: `esc()` and `escAttr()` helper functions in dashboard JavaScript to sanitize all user-supplied strings rendered in HTML

### Fixed
- **cache.py TypeError**: deployer patches pwnagotchi's cache.py to guard `isinstance(access_point, dict)` check, preventing TypeError when AO handshake objects are passed instead of dicts
- **WiFi crash on restart**: deployer patches `pwnlib` to comment out `reload_brcm` in `stop_monitor_interface`, preventing SDIO bus crash during bettercap restarts
- **bettercap-launcher crash loop**: deployer patches `bettercap-launcher` to make `reload_brcm` conditional (only runs if wlan0/wlan0mon are both missing), preventing unnecessary driver reloads that trigger firmware faults
- **Path traversal in capture download**: `/api/download/capture/` endpoint uses `os.path.basename()` to strip directory traversal attempts
- **Pwnagotchi restart storm**: deployer adds systemd rate-limit override (3 starts per 5 minutes) to prevent crash loops from exhausting the SD card

### Changed
- **Plugin architecture**: moved from v1 single-process model to v2 with thread-safe locking, process group management (SIGTERM then SIGKILL with timeout), and agent reference caching for webhook-triggered restarts
- **Handshake integration**: captures are now copied to bettercap's handshake directory and trigger `plugins.on('handshake')` events for downstream plugins (wigle, wpa-sec, pwncrack)
- **UI layout**: hides overlapping pwnagotchi UI elements (name, walkby, blitz, bluetooth, display-password, ip_display) when AO is active, overrides bt-tether status text with AO capture count
- **Face system**: PNG-first with text fallback -- checks `/etc/pwnagotchi/custom-plugins/faces/` for PNG files, falls back to stock text faces (ANGRY, BROKEN, etc.) if not found
- **Deployer renamed**: `deploy_pwnoxide.py` replaces earlier single-purpose deployers (deploy_and_patch, deploy_minimal, deploy_fatal_wrapper, etc.) as the canonical installer
- **set-iovars service**: disabled by deployer as obsolete for v5 firmware
