# Oxigotchi Architecture

> Technical guide to how Oxigotchi works. Covers both the current Rust v3.0 daemon and the legacy Python v2.x system.

For user-facing documentation, see [RUSTY_V3.md](RUSTY_V3.md).

---

## How Oxigotchi Works

### The Problem with Pwnagotchi + Bettercap

Stock pwnagotchi on a Pi Zero 2W has a fundamental architecture problem: two programs fight over one WiFi chip.

**Bettercap** runs `wifi.recon` on `wlan0mon`, constantly scanning channels and reading frames. **Pwnagotchi's AI** tells bettercap when to inject deauth and PMKID association frames. Both programs hammer the BCM43436B0 WiFi chip simultaneously — one reading, one writing — through a shared SDIO bus that was never designed for this workload.

The result: the WiFi firmware crashes every 1-25 minutes. The SDIO bus dies with error -110 or -22. `wlan0mon` vanishes. Pwnagotchi restarts, bettercap restarts, the driver reloads, and the whole cycle repeats. Most pwnagotchis spend more time recovering from crashes than actually capturing handshakes.

When the SDIO bus fully dies (error -22), even `modprobe -r brcmfmac && modprobe brcmfmac` cannot recover it. The only fix is a hardware power cycle of the WiFi chip itself — toggling GPIO 41 (`WL_REG_ON`) to cut and restore power to the BCM43436B0.

### The Oxigotchi Solution: Full AO Mode

Oxigotchi replaces bettercap's attack engine with [AngryOxide](https://github.com/Ragnt/AngryOxide) (AO), a single Rust binary that handles both scanning and attacking:

- **AO runs continuously on `wlan0mon`** — it scans channels, discovers APs, and injects attack frames, all in one process with one coherent state machine
- **Only one program touches the WiFi chip** — no TX+RX conflicts, no SDIO bus contention
- **Bettercap still runs** but with `wifi.recon OFF` — it serves as a dummy API endpoint so pwnagotchi's core code does not crash when it tries to query bettercap's session API
- **AP count comes from AO's output**, not bettercap's scanner
- **6 attack types** instead of bettercap's 2 — deauth, PMKID, CSA (Channel Switch Announcement), disassociation, anonymous reassociation, and rogue M2

The plugin (`plugin/angryoxide.py`) is the glue layer. It:
1. Starts AO as a subprocess with the right flags
2. Disables bettercap's `deauth` and `associate` personality settings so bettercap does not inject frames
3. Turns off `wifi.recon` so bettercap does not scan
4. Monitors AO's output directory for new `.pcapng` captures
5. Emits pwnagotchi's standard `association`, `deauthentication`, and `handshake` plugin events so downstream plugins (exp, wpa-sec, wigle) work unchanged
6. Resets the blind epoch counter so pwnagotchi's AI does not think the radio is dead and trigger unnecessary restarts

### Why TDM Cycling Was Removed

Early versions tried time-division multiplexing (TDM): 25 seconds of AO attack, then 5 seconds of bettercap passive scanning, in a loop. The idea was to let bettercap discover APs during the scan window so pwnagotchi's AP list stayed populated.

This failed for two reasons:

1. **Every AO stop/start stressed the WiFi chip.** Stopping AO and starting bettercap's `wifi.recon` meant switching who owned the monitor interface. On the BCM43436B0, this transition itself could trigger firmware crashes — the exact problem TDM was supposed to prevent.

2. **Bettercap scanning was redundant.** AO already scans channels internally as part of its attack loop. It discovers APs, tracks clients, and selects targets — all the things bettercap's `wifi.recon` does, but without the overhead of a separate Go process consuming 80MB of RAM.

The solution was simpler: let AO run continuously and never cycle. The plugin injects a dummy AP entry into pwnagotchi's access point list to prevent the blind epoch counter from escalating. AP count for the display comes from parsing AO's capture metadata, not from bettercap.

The TDM code still exists in the plugin (the `_cycle_tick`, `_enable_bettercap_recon`, `_disable_bettercap_recon` methods and related state) but the default configuration runs in full AO mode with bettercap recon permanently off.

### The 5 Firmware Crash Vectors

The BCM43436B0 WiFi chip has multiple firmware-level failure modes triggered by monitor mode and packet injection. The v6 firmware patch addresses all five:

1. **GTK rekey processing** — The firmware's EAPOL handler attempts group key rotation while in monitor mode, triggering a cascade failure under heavy TX load. The patch disables this codepath in monitor mode.

2. **PSM watchdog timeout** — The Power Save Mode watchdog fires when TX injection overloads the firmware's deferred procedure call (DPC) thread. The default threshold is too aggressive. The patch raises it to prevent premature panics.

3. **Small frame DMA underrun** — Frames smaller than ~600 bytes can cause a DMA underrun in the firmware's TX path, crashing the chip. The frame padding plugin (`frame_padding.py`) pads all injected frames to 650+ bytes as a software workaround.

4. **memcpy HardFault** — A bulk memory copy operation in the firmware's ROM hits a bus fault when the SDIO interface cannot keep up. The patch adds a recovery stub that catches the HardFault and resumes execution instead of crashing.

5. **TX rate assertion** — The firmware's TX path has an assertion that fires at higher transmission rates. The patch neutralizes this assertion. Stress testing (2026-03-26) confirmed all three rates (1, 2, 3) are stable on the built-in BCM43436B0 across all channel/dwell combinations — the only failure was rate 3 + 500ms dwell + all 13 channels (AO crashed at 50s, daemon auto-recovered).

### Stress Test Results (2026-03-26)

With all v6 firmware patches active (PSM threshold 0xFF, DPC fix, HardFault recovery, frame padding 650B), systematic stress testing of 27 rate/dwell/channel combinations over ~60 minutes produced:

| Rate | Dwell Range | Channels | Result |
|------|-------------|----------|--------|
| 1 | 500ms–2000ms | 1,6,11 and all 13 | **All PASS** — 0 PSM fires |
| 2 | 500ms–5000ms | 1,6,11 and all 13 | **All PASS** — 1 PSM fire (recovered) |
| 3 | 1000ms–10000ms | 1,6,11 and all 13 | **All PASS** — 2 PSM fires (recovered) |
| 3 | 500ms | All 13 | **FAIL at 50s** — AO crashed, daemon auto-recovered |

**26/27 passed. The single failure was rate 3 + 500ms dwell + all 13 channels.** This is the absolute worst-case combination (maximum attack speed + fastest hopping + maximum channels). All other combinations are stable, including rate 3 at 1000ms+ dwell.

Note: Tests were conducted in a low-traffic environment. Dense urban environments with many responding APs add more TX load and may shift the stability boundary.

### Self-Healing Stack

Oxigotchi has multiple layers of crash detection and recovery, each catching failures the layer above missed:

#### `wlan_keepalive` (tools/wlan_keepalive.c)
A lightweight C daemon (~20KB binary) that prevents SDIO bus idle crashes. Opens a raw packet socket on `wlan0mon` in promiscuous mode, drains incoming frames, and injects broadcast probe request frames every 3 seconds. This keeps the SDIO bus active even when AO temporarily stops transmitting. Without it, the WiFi chip dies every 1-3 minutes during idle periods.

Built with `gcc -O2 -o wlan_keepalive wlan_keepalive.c`. No dependencies.

#### `wifi-watchdog.service` (tools/wifi-watchdog.sh)
Runs independently of pwnagotchi. Checks every 10 seconds whether `wlan0` and `wlan0mon` exist. If either disappears:
- Stops pwnagotchi, bettercap, and wlan-keepalive
- Unloads `brcmfmac` driver
- Unbinds the MMC controller (`3f300000.mmcnr`)
- Pulls GPIO 41 (`WL_REG_ON`) LOW to power off the WiFi chip
- Waits 3 seconds, pushes GPIO 41 HIGH
- Rebinds MMC, reloads driver, restarts all services
- If recovery fails, reboots the Pi

Has a 60-second cooldown between recovery attempts to prevent recovery loops.

#### `wifi-recovery.service` (tools/wifi-recovery.sh)
Boot-time recovery. Runs before bettercap and pwnagotchi. If `wlan0` does not appear within 15 seconds:
1. Tries a simple `modprobe` cycle (handles soft crashes)
2. Falls back to GPIO power cycle with standard timing
3. Falls back to GPIO power cycle with extended timing (longer delays)
4. If all three attempts fail, exits with error — the hardware watchdog or manual intervention is needed

#### AO process watchdog (in plugin)
The `_check_health` method in `angryoxide.py` runs every epoch. If the AO process has died:
- Increments crash counter
- Checks kernel logs for firmware crash signatures (`brcmf.*Set Channel failed.*-110`)
- If firmware crash detected, triggers the GPIO power cycle sequence directly from Python
- Applies exponential backoff: 5s, 10s, 20s, 40s, ... up to 5 minutes between restarts
- Gives up after 10 consecutive crashes (configurable via `max_crashes`)

#### Hardware watchdog
The Pi Zero 2W's hardware watchdog timer is enabled. If the kernel hangs and no process resets the watchdog counter, the Pi reboots automatically.

#### Kernel panic auto-reboot
`panic=10` kernel parameter: if the kernel panics, the Pi reboots after 10 seconds instead of hanging forever.

#### Boot diagnostics (`bootlog.service`)
Writes diagnostic info to the boot partition on every startup for post-mortem analysis. Also performs SSH key auto-heal if the SSH host keys are missing or corrupted.

### Component Architecture (v3.0 Rust)

```
┌──────────────────────────────────────────────────┐
│         rusty-oxigotchi (single Rust binary)      │
│                                                    │
│  ┌────────┐ ┌─────────┐ ┌──────┐ ┌────────────┐  │
│  │   ao   │ │ display │ │ web  │ │    lua     │  │
│  │(AO mgr)│ │ (e-ink) │ │(axum)│ │ (plugins)  │  │
│  └────┬───┘ └────┬────┘ └──┬───┘ └──────┬─────┘  │
│       │          │         │             │         │
│  ┌────┴───┐ ┌────┴────┐   │        ┌────┴─────┐  │
│  │recovery│ │pisugar  │   │        │personality│  │
│  │(heal)  │ │(battery)│   │        │(mood, XP) │  │
│  └────┬───┘ └─────────┘   │        └──────────┘  │
│       │                    │                       │
│  ┌────┴──────┐  ┌─────────┴──────┐                │
│  │ bluetooth │  │    capture     │                │
│  │ (tether)  │  │ (tmpfs + SD)   │                │
│  └───────────┘  └────────────────┘                │
├────────────────────────────────────────────────────┤
│   AngryOxide (subprocess)    Waveshare SPI         │
│   stdout -> reader thread    (250x122 e-ink)       │
│        │                                           │
│    wlan0mon (monitor mode)                         │
│        │                                           │
│    BCM43436B0 (WiFi chip via SDIO)                 │
│        │                                           │
│    PSM reset (ioctl 0x500 every 15 min)            │
│    modprobe cycle (crash loop detection)           │
│    GPIO 41 recovery (hard power cycle)             │
│    hw watchdog (kernel hang reboot)                │
└────────────────────────────────────────────────────┘
```

### Component Architecture (v2.x Python, legacy)

```
┌─────────────────────────────────────────────┐
│             Pwnagotchi (Python)              │
│  ┌───────────┐ ┌──────────┐  ┌────────────┐ │
│  │ AO Plugin │ │ Display  │  │  Web UI    │ │
│  │  (glue)   │ │ (e-ink)  │  │ (Flask)    │ │
│  └─────┬─────┘ └────┬─────┘  └──────┬─────┘ │
│        │             │               │        │
├────────┼─────────────┼───────────────┼────────┤
│        ▼             ▼               ▼        │
│   AngryOxide    Waveshare SPI    Flask :8080  │
│   (Rust bin)    (e-ink driver)   (22 endpoints)│
│        │                                      │
│        ▼                                      │
│    wlan0mon ◄─── wlan_keepalive               │
│        │          (probe inject every 3s)     │
│        ▼                                      │
│    BCM43436B0 (WiFi chip via SDIO)            │
│        │                                      │
│    wifi-watchdog (GPIO 41 recovery)           │
│    wifi-recovery (boot-time GPIO recovery)    │
│    hw watchdog   (kernel hang reboot)         │
└───────────────────────────────────────────────┘
```

#### Data Flow

1. AO scans channels, discovers APs, and injects attack frames on `wlan0mon`
2. When AO captures a handshake, it writes `.pcapng` + `.22000` files to `/etc/pwnagotchi/handshakes/`
3. The plugin's `_scan_captures()` method (called every epoch) detects new files by comparing mtimes
4. For each new capture, the plugin:
   - Copies the file to pwnagotchi's handshake directory (if different from AO output)
   - Emits `association`, `deauthentication`, `handshake` events for downstream plugins
   - Registers the capture in pwnagotchi's handshake tracking for display/mood
   - Resets `blind_for` counter so the AI stays happy
   - Sends a Discord notification (if configured)
5. When a `.22000` companion file appears for a capture, a bonus `handshake` event fires for extra XP

#### Plugin State Persistence

The plugin saves runtime configuration (targets, whitelist, rate, attack toggles, channels, etc.) to `/etc/pwnagotchi/custom-plugins/angryoxide_state.json`. State is saved:
- Every 10 epochs (survives crashes)
- On shutdown (clean state)
- On explicit user changes via the web dashboard
- Debounced to at most once per 30 seconds to minimize disk writes

### Rusty Oxigotchi v3.0 (Current)

The full Rust rewrite replaces the entire Python + bettercap + pwngrid stack with a single ~5MB static binary. Source code is in `rust/src/`.

#### Module Architecture

```
rust/src/
  main.rs           # Daemon entry point, boot sequence, epoch loop
  ao.rs             # AngryOxide subprocess management, stdout parsing
  attacks/mod.rs    # Attack scheduling, whitelist, per-type toggles
  bluetooth/mod.rs  # BT PAN tethering (D-Bus), HCI scanning, GATT discovery, phone pairing
  bluetooth/dbus.rs # D-Bus BlueZ wrapper: Network1 PAN, Agent1, ObjectManager
  capture/mod.rs    # Capture file management, hcxpcapngtool, WPA-SEC upload
  config/mod.rs     # TOML config parsing
  display/          # E-ink display driver (SPI), framebuffer, face sprites, fonts
    mod.rs          # Screen abstraction, draw methods
    buffer.rs       # 250x122 1-bit framebuffer
    driver.rs       # Waveshare 2.13" V4 SPI protocol
    faces.rs        # 120x66 1-bit face bitmaps (compiled in)
    fonts.rs        # ProFont bitmap font rendering
  epoch.rs          # Epoch loop state machine (Scan -> Attack -> Capture -> Display -> Sleep)
  lua/              # Lua 5.4 plugin runtime (mlua, vendored)
    mod.rs          # Plugin loading, sandboxed execution, indicator registration
    config.rs       # plugins.toml parsing and merging
    state.rs        # Epoch state table construction for Lua hooks
  migration/mod.rs  # Pwnagotchi -> Oxigotchi config migration
  network.rs        # USB RNDIS networking, IP display rotation, internet checks
  personality/      # Mood, XP, faces, status messages, jokes
    mod.rs          # Mood engine, XP tracker, personality state
    jokes.rs        # Bull-themed joke database
    messages.rs     # Context-aware status message generator
    variety.rs      # Face variety engine (milestones, idle rotation, rare faces)
  pisugar/mod.rs    # PiSugar3 battery monitoring, button events, watchdog
  recovery/mod.rs   # Self-healing: health checks, modprobe cycle, GPIO recovery, PSM reset
  web/              # axum HTTP server (port 8080)
    mod.rs          # REST API routes, shared daemon state
    html.rs         # Embedded HTML/CSS/JS dashboard (single-page app)
  wifi/mod.rs       # Monitor mode, channel hopping, AP tracking, beacon parsing
```

#### Self-Healing Stack (v3.0)

```
Layer 1: PSM Watchdog Counter Reset
  Every 15 min, write zeros to firmware PSM/DPC/RSSI counter addresses
  via SDIO RAMRW (nexmon ioctl 0x500). Prevents 2.5-hour degradation.

Layer 2: AO Crash Loop Detection
  If AO crashes 3+ times (SIGABRT from degraded firmware), report
  interface as "unresponsive" and trigger soft recovery (modprobe cycle).

Layer 3: Soft Recovery (modprobe cycle)
  ip link set wlan0mon down
  ip link set wlan0 down
  modprobe -r brcmfmac    (unload WiFi driver)
  modprobe brcmfmac       (reload with patched firmware)
  Poll for wlan0 to reappear, restart monitor mode, restart AO.
  Max 3 attempts with 60-second cooldown.

Layer 4: Hard Recovery (GPIO power cycle)
  modprobe -r brcmfmac
  Unbind MMC controller (3f300000.mmcnr)
  GPIO 41 (WL_REG_ON) -> LOW (power off chip)
  Wait 3 seconds
  GPIO 41 -> HIGH (power on)
  Rebind MMC, modprobe brcmfmac, restart AO.
  Max 2 attempts.

Layer 5: GiveUp (graceful failure)
  After all attempts exhausted, stop trying to recover WiFi.
  Daemon stays running — SSH, web dashboard, USB networking all remain accessible.
  The daemon NEVER reboots the Pi from crash recovery.

Layer 6: Hardware Watchdog
  PiSugar3 watchdog and/or Pi hardware watchdog timer.
  If the kernel hangs entirely, the Pi reboots.
```

#### tmpfs Capture Pipeline

```
AO writes to /tmp/ao_captures/ (tmpfs = RAM)
    |
    v
Each epoch: hcxpcapngtool validates .pcapng in RAM
    |
    v
Files with valid .22000 -> moved to /home/pi/captures/ (SD card)
Files without handshakes -> deleted from tmpfs
    |
    v
.22000 files uploaded to WPA-SEC (if API key configured)
    |
    v
Cracked passwords fetched from WPA-SEC every ~25 minutes
```

Zero SD card writes during active attacks. Only proven handshakes touch the SD card.

#### SDIO RAMRW for PSM Reset

The BCM43436B0 firmware has internal watchdog counters that accumulate during monitor mode operation. After ~2.5 hours, these counters can reach thresholds that cause firmware degradation.

The daemon writes zeros to these counter addresses every 15 minutes using nexmon's ioctl 0x500 (SDIO RAMRW). This is implemented via a raw netlink socket that sends a WLC_SET_VAR message to the brcmfmac driver. The addresses are the firmware's PSM, DPC, and RSSI watchdog counter locations.

If the nexmon ioctl is not available (stock firmware without nexmon DKMS module), the write silently fails and is skipped.

#### v7 Firmware Patch Roadmap

The current firmware patch (v6) addresses 5 crash vectors via patched firmware binary. The v7 roadmap explores using ARM DWT (Data Watchpoint and Trace) hardware to automatically reset watchdog counters at the hardware level, eliminating the need for periodic SDIO RAMRW resets from userspace. A DWT watchpoint on the PSM counter address would trap the write and reset it in-place, making the firmware fully autonomous.

---

## How to Hack Further

### Adding New Display Indicators

The e-ink display is 250x122 pixels. The plugin manages custom UI elements through pwnagotchi's `LabeledValue` component.

**1. Register the element in `on_ui_setup`:**

```python
def on_ui_setup(self, ui):
    with ui._lock:
        ui.add_element('my_indicator', LabeledValue(
            color=BLACK,
            label='LBL',          # short prefix shown before value
            value='',             # initial value (empty = hidden)
            position=(x, y),      # pixel coordinates on 250x122 display
            label_font=fonts.Small,
            text_font=fonts.Small
        ))
```

**2. Update it every epoch in `on_ui_update`:**

```python
def on_ui_update(self, ui):
    with ui._lock:
        ui.set('my_indicator', 'some value')
```

Current indicators and their positions:
- `angryoxide` — (0, 0): main AO status line (captures, uptime, rate)
- `ao_ip` — (0, 95): rotating USB/BT IP addresses
- `ao_crash` — (0, 109): firmware crash counter
- `ao_aps` — (140, 0): nearby AP count

To hide an element, either set its value to `''` or move it off-screen: `el.xy = (300, 300)`.

### Modifying Attack Behavior

AO's behavior is controlled by the plugin at runtime. The `_build_cmd` method in `angryoxide.py` constructs the AO command line from current state.

**Key parameters:**

| Parameter | Config key | CLI flag | Notes |
|-----------|-----------|----------|-------|
| Rate | `self._rate` | `--rate 1\|2\|3` | All rates stable with v6 firmware patch. Rate 3 + 500ms + all 13ch is the only known crash combo. |
| Channels | `self._channels` | `--channel 1,6,11` | Empty = default (1,6,11). All 13 is stable with firmware patches. |
| Dwell | `self._dwell` | `--dwell 2` | Seconds per channel before hopping |
| Autohunt | `self._autohunt` | `--autohunt` | Smart channel selection based on AP density |
| Attacks | `self._attacks` dict | `--disable-{type}` | Each attack type can be toggled independently |
| Targets | `self._targets` | `--target-entry MAC` | Focus on specific APs (MAC or SSID) |
| Whitelist | `self._whitelist_entries` | `--whitelist-entry MAC` | Never attack these APs |
| Smart Skip | `self._skip_captured` | (computed whitelist) | Auto-whitelist APs with existing captures |

All of these can be changed at runtime via the web dashboard API without restarting pwnagotchi. The plugin stops AO, rebuilds the command, and restarts it.

**To add a new attack parameter:**
1. Add the state variable in `__init__`
2. Add it to `_save_state` / `_load_state` for persistence
3. Add the CLI flag mapping in `_build_cmd`
4. Add the API endpoint in `on_webhook` for dashboard control
5. Add the UI control in `_dashboard_html`

### Adding New Web Dashboard Cards

The web dashboard is a single HTML page served by `_dashboard_html()` in `angryoxide.py`. It uses [htmx](https://htmx.org/) for auto-refresh without JavaScript frameworks.

**1. Add an API endpoint in `on_webhook`:**

```python
if request.method == 'GET' and path == '/api/my-data':
    return jsonify({
        'value': self._my_value,
        'updated': time.time(),
    })
```

**2. Add a card in `_dashboard_html`:**

```html
<div class="card">
    <h3>My Card</h3>
    <div hx-get="/plugins/angryoxide/api/my-data"
         hx-trigger="every 5s"
         hx-swap="innerHTML">
        Loading...
    </div>
</div>
```

The dashboard currently has 15 cards and 22+ API endpoints. Cards auto-refresh on intervals ranging from 5s (status) to 30s (health).

**Existing API endpoints (GET):**
- `/api/status` — AO running state, captures, crashes, attack config, TDM state
- `/api/health` — WiFi, monitor, firmware, USB, battery status
- `/api/mode` — Current mode (AO or PWN)
- `/api/aps` — Nearby access points with RSSI, channel, capture status
- `/api/captures` — Capture files with type badges, download links
- `/api/cracked` — Cracked passwords from wpa-sec potfile
- `/api/log` — Last N lines of pwnagotchi log
- `/api/plugins` — List of installed plugins with status
- `/api/config` — Current pwnagotchi config
- `/api/exp` — XP and level stats from exp plugin

**Existing API endpoints (POST):**
- `/api/attacks` — Toggle individual attack types
- `/api/rate` — Set attack rate (1-3)
- `/api/channels` — Set channel list
- `/api/targets` — Add/remove target APs
- `/api/whitelist` — Add/remove whitelisted APs
- `/api/skip-captured` — Toggle smart skip
- `/api/mode` — Switch AO/PWN mode
- `/api/restart` — Restart AO process
- `/api/shutdown` — Shutdown Pi
- `/api/reboot` — Reboot Pi
- `/api/bt-visible` — Toggle Bluetooth visibility
- `/api/discord` — Set Discord webhook URL
- `/api/pwn-attacks` — Toggle bettercap deauth/associate in PWN mode

### PiSugar Button Customization

The PiSugar 3 battery board has a programmable button. Configuration is in `/etc/pisugar-server/config.json`:

```json
{
    "single_tap_shell": "/usr/local/bin/toggle-bt.sh",
    "double_tap_shell": "/usr/local/bin/toggle-mode.sh",
    "long_tap_shell": "/usr/local/bin/toggle-ao-pwn.sh"
}
```

Scripts live in `/usr/local/bin/toggle-*.sh`. To add new button actions, either modify these scripts or add new scripts and update the PiSugar config.

The `pisugarx` plugin reads battery level and writes it to `/tmp/pisugar-battery` (or exposes it via `/sys/class/power_supply/battery/capacity`). The AO plugin reads this for battery-related face changes and dashboard display.

### Creating Custom Faces

Bull face PNGs live in `/etc/pwnagotchi/custom-plugins/faces/` on the Pi. Each is a 1-bit (black and white) PNG sized for the 250x122 e-ink display.

The `_face()` method in the plugin maps face names to file paths:
- If PNG mode is enabled in config (`ui.faces.png = true`), it looks for `{name}.png` in the face directory
- If the PNG is missing or PNG mode is off, it falls back to stock pwnagotchi text emoticons

Current face names (28 total): `awake`, `look_r`, `look_r_happy`, `intense`, `cool`, `happy`, `excited`, `smart`, `motivated`, `sad`, `bored`, `demotivated`, `angry`, `lonely`, `grateful`, `friend`, `sleep`, `broken`, `upload`, `wifi_down`, `fw_crash`, `ao_crashed`, `battery_low`, `battery_critical`, `debug`, `shutdown`.

To add a new face:
1. Create a 1-bit PNG (250x122 or smaller) named `{face_name}.png`
2. Place it in the faces directory
3. Reference it in the plugin with `self._face('face_name')`
4. Add a fallback mapping in the `_face()` method's fallback dict

### Building and Deploying Changes

**Plugin changes (angryoxide.py):**
```bash
# From MSYS2 on Windows:
scp plugin/angryoxide.py pi@10.0.0.2:/tmp/
ssh pi@10.0.0.2 "sudo cp /tmp/angryoxide.py /etc/pwnagotchi/custom-plugins/ && sudo systemctl restart pwnagotchi"
```

**Keepalive daemon:**
```bash
# Cross-compile or compile on Pi:
ssh pi@10.0.0.2 "gcc -O2 -o /tmp/wlan_keepalive /tmp/wlan_keepalive.c && sudo cp /tmp/wlan_keepalive /usr/local/bin/ && sudo systemctl restart wlan-keepalive"
```

**Watchdog/recovery scripts:**
```bash
scp tools/wifi-watchdog.sh pi@10.0.0.2:/tmp/
ssh pi@10.0.0.2 "sed -i 's/\r$//' /tmp/wifi-watchdog.sh && sudo cp /tmp/wifi-watchdog.sh /usr/local/bin/ && sudo systemctl restart wifi-watchdog"
```

Always fix CRLF line endings after SCP from Windows: `sed -i 's/\r$//'`.

**Building the Rust version:**
```bash
cd rust && cargo test    # runs on any platform
# Cross-compile for Pi:
cross build --release --target aarch64-unknown-linux-gnu
scp target/aarch64-unknown-linux-gnu/release/oxigotchi pi@10.0.0.2:/tmp/
ssh pi@10.0.0.2 "sudo cp /tmp/oxigotchi /usr/local/bin/rusty-oxigotchi"
```

### Key Files Reference

| File | Purpose |
|------|---------|
| `plugin/angryoxide.py` | Main plugin — AO lifecycle, dashboard, API, display, capture scanning |
| `tools/wlan_keepalive.c` | SDIO keepalive daemon (C, ~200 lines) |
| `tools/wifi-watchdog.sh` | Runtime WiFi crash recovery (GPIO power cycle) |
| `tools/wifi-recovery.sh` | Boot-time WiFi recovery |
| `services/*.service` | Systemd unit files for all daemons |
| `faces/eink/*.png` | 28 bull face PNGs for e-ink display |
| `config/angryoxide-v5.toml` | Config overlay for AO mode |
| `tools/bake_v2.sh` | SD card image builder |
| `tools/deploy_pwnoxide.py` | Installer for existing pwnagotchi |
| `rust/` | Rusty Oxigotchi v3.0 scaffold |
| `docs/RUST_REWRITE_PLAN.md` | Rust rewrite sprint plan |
