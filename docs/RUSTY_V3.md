# Rusty Oxigotchi v3.0

> The full Rust rewrite. One binary to rule them all.

Rusty Oxigotchi replaces the entire Python + bettercap + pwngrid stack with a single ~5MB Rust binary. It handles everything: e-ink display, WiFi attacks (via AngryOxide), Bluetooth tethering, web dashboard, Lua plugins, and personality — all compiled into one static executable.

| Stat | Value |
|------|-------|
| Binary size | ~5 MB |
| RAM usage | ~10 MB |
| Boot to scanning | <5 seconds |
| Language | 100% Rust |
| Display | Waveshare 2.13" V4 e-ink (250x122, 1-bit monochrome) |
| Web UI | axum + htmx |
| Plugins | Lua 5.4 (mlua, vendored) |
| Battery | PiSugar3 support (level, charging, button events) |

No Python interpreter, no venv, no pip, no Go runtime, no garbage collector. Your SD card will outlive the Pi.

---

## Quick Start

1. **Flash the Oxigotchi v3 image** to a microSD card.
2. Insert the card into your Pi Zero 2W.
3. Connect the micro USB **data** port (center port, not the edge one).
4. Power on. Wait ~5 seconds.
5. The bull face appears on the e-ink display. The daemon starts in SAFE mode by default — switch to RAGE via the web dashboard or PiSugar3 button to begin scanning.

**Connect to your Oxigotchi:**

| Method | Address |
|--------|---------|
| SSH | `ssh pi@10.0.0.2` (default password: `raspberry`) |
| Web dashboard | `http://10.0.0.2:8080` |

> If using Bluetooth tethering in SAFE mode, the web dashboard is also reachable at the BT-assigned IP on port 8080.

---

## Web Dashboard

**URL:** `http://10.0.0.2:8080`

The dashboard is a single-page app with a dark terminal theme. Connect your phone or laptop to the Pi via USB tethering and open the URL in any browser. When in SAFE mode with Bluetooth tethering, the dashboard is also reachable at the BT-assigned IP on port 8080.

Everything auto-refreshes — you never need to hit reload.

### Dashboard Cards

The dashboard has 23 live cards:

| # | Card | What it shows | Refresh |
|---|------|--------------|---------|
| 1 | **Face Display** | Current bull face and status message | 5s |
| 2 | **Core Stats** | Channel, APs seen, handshakes captured, epoch, uptime, attack rate | 5s |
| 3 | **Live Display** | Real-time screenshot of the e-ink screen | 5s |
| 4 | **Battery** | Level, charging state, voltage, progress bar | 15s |
| 5 | **Bluetooth** | Connection status, device name, IP address | 15s |
| 6 | **WiFi** | Monitor mode state, current channel, tracked APs, channel list, dwell time | 5s |
| 7 | **RAGE Slider** | 7-level aggression preset (Chill→YOLO), toggle on/off, YOLO disclaimer | instant |
| 8 | **Attack Types** | 6 toggle switches (deauth, PMKID, CSA, disassociation, anon reassoc, rogue M2) plus attack rate selector (1-3) | 10s |
| 9 | **Recent Captures** | Capture file count, handshake count, pending uploads, total size | 30s |
| 10 | **Recovery Status** | WiFi/AO/Recovery/GPS health dots, crash count, AO PID and uptime | 15s |
| 11 | **Personality** | Mood percentage, current face, XP, level, blind epochs | 10s |
| 12 | **System Info** | CPU temp, CPU usage, memory, disk, system uptime, GPS status | 15s |
| 13 | **Cracked Passwords** | Passwords cracked from captured handshakes (SSID, BSSID, password) | 60s |
| 14 | **Download Captures** | "Download All (ZIP)" button plus individual file download links | 30s |
| 15 | **Mode Switch** | RAGE / BT / SAFE toggle buttons | via status |
| 16 | **Actions** | Restart AO, Shutdown Pi, Restart Pwnagotchi, Restart Pi, Restart SSH | on-demand |
| 17 | **Plugins** | Toggle plugins on/off, edit x/y positions, shows version/author/tag | 15s |
| 18 | **Nearby Networks** | AP list sorted by signal strength — SSID, BSSID, RSSI, channel, client count, handshake status | 10s |
| 19 | **Whitelist** | View/add/remove MAC addresses and SSIDs excluded from attacks | 30s |
| 20 | **Channel Config** | Set scan channels (comma-separated) and dwell time slider (500-10000ms) | via WiFi |
| 21 | **WPA-SEC Upload** | Enter API key, view upload status | 30s |
| 22 | **Discord Notifications** | Enable/disable toggle, webhook URL input | 30s |
| 23 | **Logs** | Collapsible live log viewer (daemon journal output) | 10s |

### Using the Dashboard

**RAGE Slider:** The fastest way to tune aggression. Flip the toggle ON and slide between 7 presets — each level changes exactly one variable from the previous, so you can isolate what works in your environment:

| Level | Name | Rate | Dwell | Channels | Notes |
|-------|------|------|-------|----------|-------|
| 1 | Chill | 1 | 5000ms | 1,6,11 | Battery saver |
| 2 | Lurk | 1 | 2000ms | 1,6,11 | Faster hopping |
| 3 | Prowl | 1 | 2000ms | All 13 | Full channel coverage |
| 4 | Hunt | 2 | 2000ms | All 13 | Double attack rate |
| 5 | RAGE | 2 | 1000ms | All 13 | Faster hopping at rate 2 |
| 6 | FURY | 3 | 1000ms | All 13 | Max attack rate |
| 7 | YOLO | 3 | 500ms | All 13 | Only known crash combo |

All levels except YOLO are stress-test-validated stable (BCM43436B0, v6 firmware, 2 min each). YOLO crashed AO at 50 seconds — the daemon auto-recovered. An orange disclaimer appears at level 7.

Touching any individual control (rate buttons, channel toggles, dwell slider, autohunt) breaks out to Custom mode — the RAGE toggle flips off and those controls take over. Flip RAGE back on anytime to snap to the slider's current level. The active level persists across reboots.

**Attack types:** All 6 attack types are on by default. They complement each other — leaving them all enabled gives the best capture rate. You can toggle individual types off if you want to reduce your footprint.

**Attack rate:** Rate 1 is the safe default. Rates 2 and 3 are stable with the v6 firmware patch. Use the RAGE Slider for validated rate/dwell/channel combos, or set rates manually.

**Mode switch:** Tap RAGE, BT, or SAFE. The switch happens at the next epoch boundary (up to ~30 seconds). The face and indicators will update when the switch completes. Switching to BT attack mode shows a warning that phone tethering will disconnect.

**Whitelist:** Add a MAC address or SSID to exclude it from attacks. Changes take effect at the next epoch. Useful for protecting your own networks.

**Channel config:** The default channels are 1, 6, 11 — the three non-overlapping 2.4GHz channels. You can add others, but keep the dwell time at 5000ms or higher to avoid firmware crashes on the BCM43436B0.

**Downloading captures:** Click "Download All (ZIP)" to grab every capture file at once, or click individual filenames to download them one at a time.

**WPA-SEC:** Paste your API key from [wpa-sec.stanev.org](https://wpa-sec.stanev.org) and hit Save. Captured handshakes will be automatically uploaded for cloud cracking. The key is saved to disk and persists across restarts.

**Discord:** Paste a Discord webhook URL and enable the toggle. You will receive a notification in your Discord channel every time a handshake is captured.

---

## Operating Modes

Oxigotchi has three operating modes. The BCM43436B0 chip uses independent buses for WiFi (SDIO) and BT (UART), so BT phone tethering stays connected in RAGE and SAFE modes. Only BT attack mode requires exclusive UART access.

### Mode Summary

| | RAGE | BT | SAFE (default) |
|---|---|---|---|
| WiFi | Monitor mode, AngryOxide attacking | Off | Managed mode, no attacks |
| Bluetooth | Tethered to phone (always-on) | Attack mode (patchram loaded) | Tethered to phone |
| Face pool | angry, intense, excited, upload, motivated, raging | intense, raging | debug, grateful, grazing |
| Use case | Capturing handshakes | BT pentesting | Phone internet, maintenance |

> **v3.1:** BT tethering now stays connected in RAGE mode. WiFi (SDIO) and BT (UART) use independent buses on the BCM43436B0, so both work simultaneously. Only BT attack mode requires exclusive UART access and disconnects phone tethering.

### Why Three Modes?

The BCM43436B0 WiFi/BT combo chip on the Pi Zero 2W uses **two independent buses** — SDIO for WiFi and UART for BT. WiFi monitor mode and BT PAN tethering can coexist (RAGE mode). However, BT attack mode requires loading a custom patchram via `hciattach` which takes exclusive UART control, so it cannot run alongside phone tethering.

### Default Boot Mode

The daemon boots into **SAFE** mode by default. This is the safest option for new users — no WiFi attacks run until you explicitly switch to RAGE. To change the default, set `default_mode` in `/etc/oxigotchi/config.toml`:

```toml
[main]
default_mode = "SAFE"   # Options: "SAFE", "RAGE", "BT"
```

### Switching Modes

There are two ways to switch:

1. **PiSugar3 button:** Single tap the button on your PiSugar3 battery.
2. **Web dashboard:** Tap the RAGE or SAFE button on the Mode card.

The mode switch happens at the next epoch boundary, which can take up to ~30 seconds. Be patient — the daemon checks the button state once per epoch.

### Transition Details

**Any → RAGE:**
1. Stop previous mode's radio (BT attack patchram or managed WiFi)
2. Enter WiFi monitor mode, start AngryOxide
3. Reconnect BT tether (if returning from BT attack mode)

**Any → BT Attack:**
1. Stop AngryOxide and WiFi monitor (if in RAGE)
2. Load BT attack patchram via `hciattach` (disconnects phone tether)
3. Begin HCI scanning and BT attacks

**Any → SAFE:**
1. Stop previous mode's radio
2. Switch WiFi to managed mode
3. Ensure BT tether connected

### Face Pools

Each mode draws from its own pool of random faces:

- **RAGE:** angry, intense, excited, upload, motivated, raging
- **SAFE:** debug, grateful, grazing

The face changes each epoch to reflect the current mode's personality.

---

## E-ink Display

The e-ink display is a 250x122 pixel Waveshare 2.13" V4 (1-bit monochrome, black and white only). The bull face and all indicators are rendered with ProFont bitmap fonts.

### What You See

- **Bull face** on the left side — 120x66 pixel sprite, changes with mood and mode
- **AO status** (top left) — handshake count, minutes running, live channel (e.g., `AO: 3/8 | 1m | CH:6`). The channel number updates every 5 seconds from AO's stdout as it hops between configured channels
- **Uptime** (top right) — how long the daemon has been running (`DD:HH:MM`)
- **Status message** — personality text like "Sniffing the airwaves..."
- **XP bar** — level and experience progress
- **System stats** — memory, CPU, frequency, temperature
- **IP address** — your USB tether IP. In SAFE mode, rotates between USB and BT IPs every 5 seconds
- **Bottom bar** — crash count, internet indicator, BT status, battery level, current mode (shows `RAGE:N` when slider active, `RAGE` in custom, `SAFE` in safe mode)

### Mood Faces

Oxigotchi has 26 unique bull face expressions. The current face depends on mood (which rises when capturing handshakes and falls during idle "blind epochs") and special events:

| Face | When |
|------|------|
| Excited | Mood above 90% — lots of captures |
| Happy | Mood 70-90% |
| Awake | Mood 50-70% (default at boot) |
| Bored | Mood 30-50% — not finding much |
| Sad | Mood 10-30% |
| Demotivated | Mood below 10% — nothing to capture |
| Battery Critical | Battery below 15% |
| Battery Low | Battery below 20% |
| WiFi Down | WiFi interface disappeared |
| FW Crash | Firmware crash detected |
| AO Crashed | AngryOxide process died |

### Display Layout

```
+---------------------------------------------------------+
| AO: 3/8 | 1m | CH:AH              UP: 01:02:15         |  y=0  TOP BAR
|---------------------------------------------------------|  y=14 HLINE
|                    |  Sniffing the                       |
|   [120x66 FACE]    |  airwaves...                       |  y=20 STATUS
|                    |                                     |
|                    |  Lv 3  [####......]                 |  y=73 XP BAR
|                    |  mem  cpu freq temp                 |  y=85 SYS
| USB:192.168.137.2  |  42%  12% 1.0G 45C                 |  y=95 SYS VALUES
|---------------------------------------------------------|  y=108 HLINE
| CRASH:0  WWW  BT:C  BAT:85%    APs:15          RAGE:4  |  y=112 BOTTOM BAR
+---------------------------------------------------------+
```

---

## Capture Pipeline

Oxigotchi uses a two-stage capture pipeline designed to minimize SD card wear.

### How It Works

1. **AngryOxide writes to RAM.** All capture output goes to `/tmp/ao_captures/`, which is a tmpfs mount (RAM disk). No SD card writes happen during active attacks.

2. **hcxpcapngtool validates in RAM.** Each epoch, the daemon runs hcxpcapngtool on new `.pcapng` files in tmpfs. This converts valid captures to `.22000` (hashcat format) and identifies files with proven handshakes.

3. **Only proven handshakes go to SD.** Files that produced a valid `.22000` are moved (along with the `.22000` companion) to the permanent capture directory on the SD card (`/home/pi/captures/`). Files that produced nothing are deleted from tmpfs after 60 seconds.

4. **Automatic WPA-SEC upload.** If you have configured a WPA-SEC API key (via the dashboard or config), validated captures are automatically uploaded to [wpa-sec.stanev.org](https://wpa-sec.stanev.org) for cloud cracking.

5. **Download anytime.** Grab individual files or a bulk ZIP from the Download Captures card on the dashboard.

### Why This Matters

- **Zero SD card wear during attacks.** The Pi Zero's SD card is the weakest link. By buffering in RAM, you avoid the constant write/delete cycle that kills cards.
- **No junk files.** Only captures with proven handshakes are kept. You do not accumulate gigabytes of empty pcapng files.
- **Automatic cracking.** WPA-SEC handles the heavy lifting — no need to run hashcat yourself.

---

## Self-Healing

The BCM43436B0 WiFi chip on the Pi Zero 2W is prone to firmware crashes, especially during aggressive frame injection. Oxigotchi has a built-in recovery system that handles this automatically.

### What Gets Monitored

- **WiFi interface** (`wlan0mon`) — checked every 10 seconds for existence and responsiveness
- **AngryOxide process** — monitored for unexpected exits
- **PiSugar3 watchdog** — hardware watchdog integration for full system recovery

### Recovery Levels

| Level | Trigger | Action | Max attempts |
|-------|---------|--------|-------------|
| **Healthy** | Everything is fine | Nothing | — |
| **Soft recovery** | `wlan0mon` missing OR AO crashed 3+ times | `rmmod brcmfmac` + `modprobe brcmfmac` + restart AO | 3 |
| **Hard recovery** | Soft recovery failed | GPIO power cycle of WiFi chip via WL_REG_ON | 2 |
| **Give up** | All recovery attempts exhausted | Stop trying — daemon stays up, web dashboard accessible | — |

**Safety guarantees:**
- 60-second cooldown between recovery attempts (no rapid-fire loops)
- Maximum 5 total attempts (3 soft + 2 hard) then stops
- The daemon **never reboots** from crash recovery — only gives up gracefully
- USB networking and web dashboard remain accessible even when WiFi is dead
- AO crash counter resets after successful recovery to prevent immediate re-trigger

### AO Crash Loop Detection

Over extended operation (~2.5 hours), the BCM43436B0 firmware can enter a degraded state where `wlan0mon` still exists but the radio is sick (PSM watchdog wedged). AO detects the broken radio and exits with SIGABRT. Without crash loop detection, AO would restart and crash forever.

The daemon detects this: when AO crashes 3+ times consecutively, it reports the firmware as "unresponsive" and triggers the soft recovery path (full modprobe cycle). This reloads the firmware from disk with the patched v5 binary, giving the radio a fresh start.

### AO Auto-Restart

If AngryOxide crashes (which happens after firmware recovery), the daemon automatically restarts it with exponential backoff (5s, 10s, 20s... up to 5 minutes). After 10 stable epochs (~5 minutes), the crash counter resets. The crash counter and recovery status are visible on the Recovery Status card in the dashboard.

### PSM Watchdog Counter Reset

The BCM43436B0 firmware has internal watchdog counters (PSM, DPC, RSSI) that accumulate over time. After ~2.5 hours of continuous operation, these counters can reach thresholds that cause firmware degradation — the radio becomes sluggish and AO starts crashing.

The daemon preventively resets these counters every 15 minutes (every 30 epochs) by writing zeros to the firmware's counter addresses via SDIO RAMRW (nexmon ioctl 0x500). This keeps the firmware in a healthy state indefinitely.

Requirements: the brcmfmac-nexmon DKMS module must be loaded and `wlan0` must be up. The reset is silent — if the ioctl fails (e.g., on stock firmware without nexmon), it is skipped with a debug log.

### GiveUp Safety

When all recovery attempts are exhausted (3 soft + 2 hard), the daemon **gives up gracefully** — it stops trying to recover WiFi but stays running. The USB network, SSH, and web dashboard remain accessible. The daemon never reboots the Pi as part of crash recovery, preventing infinite reboot loops.

### Legacy Service Auto-Disable

On first boot, the daemon checks if the legacy `pwnagotchi` and `bettercap` systemd services are active. If they are, it stops and disables them. This frees approximately 66 MB of RAM (bettercap ~36 MB + pwnagotchi ~30 MB) that the Rust daemon does not need.

### GPS Auto-Detection

At startup, the daemon probes `localhost:2947` for a gpsd connection. If found, it automatically passes `--gpsd` to AngryOxide so your captures include GPS coordinates.

---

## XP and Leveling

The bull earns XP passively and actively. XP persists across reboots (saved to `/home/pi/exp_stats.json` every 5 epochs).

### XP Sources

| Event | XP |
|-------|-----|
| Each epoch (passive, just for being active) | +1 |
| Each AP visible this epoch | +1 per AP |
| Association sent | +15 |
| Deauth sent | +10 |
| Handshake captured | +100 |
| New AP discovered | +5 |

### Leveling Curve

The XP needed for each level follows an exponential formula: `level^1.05 * 5`.

| Level | XP to complete |
|-------|----------------|
| 1 | 5 |
| 2 | 10 |
| 10 | 56 |
| 22 | 128 |
| 100 | 629 |
| 500 | 3,900 |
| 999 | 18,000 |

The maximum level is **999**. Reaching it requires approximately 3.4 million total XP — roughly 7 months of daily use (8 hours/day with ~16 APs visible). Handshake captures accelerate leveling significantly.

### Personality and Jokes

The bull has a mood system (0.0 to 1.0) that affects its face expression. Mood rises on captures and falls during idle "blind epochs." Status messages rotate with each epoch, and the bull tells jokes with specific timing: the question appears for one epoch (~10 seconds), then the punchline appears for the next epoch (~5 seconds).

---

## Migration from Pwnagotchi

If you are coming from a pwnagotchi setup, Oxigotchi handles migration automatically on first boot.

### What Gets Migrated

- **Device name** — your pwnagotchi's name is imported into the Oxigotchi config
- **Whitelist** — MAC addresses and SSIDs you had whitelisted carry over
- **Channels** — your configured scan channels are imported
- **Attack settings** — personality and bettercap-related settings are mapped to the Rust equivalents
- **Existing captures** — handshake `.pcapng` files from `/home/pi/handshakes/` and `/etc/pwnagotchi/handshakes/` are deduplicated and imported to `/home/pi/captures/`

### What Gets Disabled

The daemon automatically stops and disables legacy pwnagotchi and bettercap systemd services on first boot. This frees up ~66MB of RAM (bettercap ~36MB + pwnagotchi ~30MB).

### How It Works

1. On first boot, the daemon checks for a sentinel file at `/var/lib/.rusty-first-boot`.
2. If the sentinel does not exist, it reads the pwnagotchi config from `/etc/pwnagotchi/config.toml`.
3. It extracts relevant settings and writes them into `/etc/oxigotchi/config.toml`.
4. It scans handshake directories, deduplicates by BSSID, and copies unique captures.
5. It creates the sentinel file so migration only runs once.

You do not need to do anything — just flash the Oxigotchi image and boot.

---

## Bluetooth Tethering

### Hardware Constraint

The BCM43436B0 shares a UART between WiFi and BT. Bluetooth can only run when WiFi is in managed mode (SAFE mode). In RAGE mode, BT is powered off entirely.

### Configuration

Edit `/etc/oxigotchi/config.toml`:

```toml
[bluetooth]
enabled = true
phone_mac = "XX:XX:XX:XX:XX:XX"
phone_name = "Phone Name"
auto_pair = true
auto_connect = true
```

Replace `XX:XX:XX:XX:XX:XX` with your phone's Bluetooth MAC address, and `"Phone Name"` with your phone's Bluetooth name.

### First-Time Pairing

1. Switch to SAFE mode (single tap PiSugar3 button, or use the web dashboard).
2. SSH in: `ssh pi@10.0.0.2`
3. Run:
   ```bash
   sudo bluetoothctl
   power on
   scan on
   ```
4. Find your phone's MAC address in the scan results.
5. Exit bluetoothctl (`exit`).
6. Add the MAC to `/etc/oxigotchi/config.toml` under `phone_mac`.
7. Reboot: `sudo reboot`

After pairing, SAFE mode will automatically connect to your phone on every mode switch.

### Without phone_mac

If `phone_mac` is not set, the daemon performs a 10-second BT scan looking for `phone_name`. This may fail if your phone is not actively discoverable. Setting the MAC directly is strongly recommended.

### hci_uart Reset

When switching from RAGE to SAFE, the daemon automatically reloads the `hci_uart` kernel module before bringing up Bluetooth. This is necessary because WiFi monitor mode leaves the shared UART in a state where BT HCI commands time out. The reload gives BT a clean UART connection.

---

## Runtime State Persistence

Settings you change through the web dashboard are saved to disk and survive restarts.

### What Gets Saved

| Setting | Persisted |
|---------|-----------|
| Attack type toggles (deauth, PMKID, CSA, disassoc, anon reassoc, rogue M2) | Yes |
| Attack rate (1, 2, or 3) | Yes |
| Whitelist entries | Yes |
| WPA-SEC API key | Yes |
| Discord webhook URL | Yes |
| Discord enabled/disabled | Yes |

### Where

State is saved to `/var/lib/oxigotchi/state.json`. The file is written automatically after changes and loaded at daemon startup.

Plugin positions and enabled state are saved separately to `/etc/oxigotchi/plugins.toml`.

---

## Channel Configuration

### Defaults

The default scan channels are **1, 6, 11** — the three non-overlapping 2.4GHz channels. This covers the vast majority of consumer networks.

### Changing Channels

Use the **Channel Config** card on the web dashboard:

1. Enter channels as a comma-separated list (e.g., `1,6,11` or `1,2,3,4,5,6,7,8,9,10,11`).
2. Adjust the dwell time slider. Default is 2000ms.
3. Click **Apply**.

Changes take effect at the next epoch.

### Dwell Time

Dwell time is how long AngryOxide stays on each channel before hopping. AngryOxide's autohunt mode (`CH:AH` on the display) automatically dwells longer on channels with active networks.

With the v6 firmware patch, all dwell/channel combinations are stable. The only known crash is rate 3 + 500ms dwell + all 13 channels simultaneously. Lower dwell times mean faster channel hopping and quicker AP discovery, but less time to capture traffic on each channel.

### 5GHz Channels

The BCM43436B0 on the Pi Zero 2W is a 2.4GHz-only chip. Channels above 14 will not work.

---

## Lua Plugin System

### Architecture

Plugins are written in Lua 5.4, executed via mlua (vendored, no system Lua required). Each plugin runs in a sandboxed `_ENV` within a shared Lua VM. Plugins can register text indicators on the e-ink display and react to system events.

### File Locations

| What | Path |
|------|------|
| Plugin scripts | `/etc/oxigotchi/plugins/*.lua` |
| Plugin config | `/etc/oxigotchi/plugins.toml` |

### Writing a Plugin

Create a `.lua` file in `/etc/oxigotchi/plugins/`:

```lua
plugin.name = "my_plugin"
plugin.version = "1.0.0"
plugin.author = "you"
plugin.tag = "community"

function on_load(config)
    register_indicator("my_plugin", {
        x = config.x or 0,
        y = config.y or 0,
        font = "small",
        label = "MY"
    })
end

function on_epoch(state)
    set_indicator("my_plugin", tostring(state.aps_seen))
end

function on_unload()
    -- cleanup if needed
end
```

### API Reference

| Function | Signature | Description |
|----------|-----------|-------------|
| `register_indicator` | `(name, opts)` | Register a text indicator on the display. `opts`: `x`, `y`, `font` (`"small"` or `"medium"`), `label` (optional prefix), `wrap_width` (optional) |
| `set_indicator` | `(name, value)` | Set the text value of a registered indicator |
| `format_duration` | `(secs)` returns string | Format seconds as `"DD:HH:MM"` |
| `log` | `(message)` | Write a message to the daemon log |

### Event Hooks

All hooks are optional. Implement only what you need.

| Hook | Argument | When it fires |
|------|----------|---------------|
| `on_load(config)` | Plugin config from `plugins.toml` | Plugin loaded at startup |
| `on_epoch(state)` | Full state table (see below) | Every epoch (~30 seconds) |
| `on_handshake(state)` | Full state table | Handshake captured |
| `on_ao_crash(state)` | Full state table | AngryOxide process crashed |
| `on_bt_change(state)` | Full state table | Bluetooth connection state changed |
| `on_unload()` | None | Plugin being unloaded |

### State Table Reference

The `state` table passed to `on_epoch` and other hooks contains:

**Timing:**

| Field | Type | Description |
|-------|------|-------------|
| `uptime_secs` | number | Daemon uptime in seconds |
| `epoch` | number | Current epoch counter |

**WiFi / AngryOxide:**

| Field | Type | Description |
|-------|------|-------------|
| `channel` | number | Current WiFi channel |
| `aps_seen` | number | Access points visible |
| `handshakes` | number | Handshakes captured this session |
| `captures_total` | number | Total captures (all time) |
| `blind_epochs` | number | Consecutive epochs with no new APs |
| `ao_state` | string | AngryOxide state (e.g., "running", "stopped") |
| `ao_pid` | number | AngryOxide process ID |
| `ao_crash_count` | number | AO crashes this session |
| `ao_uptime_str` | string | AO uptime formatted |
| `ao_uptime_secs` | number | AO uptime in seconds |
| `ao_channels` | string | AO channel mode (e.g. "AH" for autohunt) |

**Battery (PiSugar3):**

| Field | Type | Description |
|-------|------|-------------|
| `battery_level` | number | Battery percentage (0-100) |
| `battery_charging` | boolean | Currently charging |
| `battery_voltage_mv` | number | Battery voltage in millivolts |
| `battery_low` | boolean | Below 20% |
| `battery_critical` | boolean | Below 15% |
| `battery_available` | boolean | PiSugar3 detected |

**Bluetooth:**

| Field | Type | Description |
|-------|------|-------------|
| `bt_connected` | boolean | Phone connected via BT |
| `bt_short` | string | Short BT status string |
| `bt_ip` | string | BT tether IP address |
| `bt_internet` | boolean | Internet reachable via BT |

**Network:**

| Field | Type | Description |
|-------|------|-------------|
| `internet_online` | boolean | Internet reachable (any interface) |
| `display_ip` | string | IP address shown on display |

**Personality:**

| Field | Type | Description |
|-------|------|-------------|
| `mood` | number | Current mood (0.0-1.0) |
| `face` | string | Current face name |
| `level` | number | Current level |
| `xp` | number | Current XP |
| `status_message` | string | Status text shown on display |

**System:**

| Field | Type | Description |
|-------|------|-------------|
| `cpu_temp` | number | CPU temperature (Celsius) |
| `mem_used_mb` | number | Memory used (MB) |
| `mem_total_mb` | number | Total memory (MB) |
| `cpu_percent` | number | CPU usage percentage |
| `cpu_freq_ghz` | string | CPU frequency (e.g. "1.0G") |

**Mode:**

| Field | Type | Description |
|-------|------|-------------|
| `mode` | string | `"RAGE"` or `"SAFE"` |

### Plugin Configuration (plugins.toml)

Each plugin's position and enabled state is stored in `/etc/oxigotchi/plugins.toml`:

```toml
[plugins.battery]
enabled = true
x = 140
y = 112

[plugins.uptime]
enabled = true
x = 178
y = 0

[plugins.aps]
enabled = true
x = 178
y = 112

[plugins.mode]
enabled = true
x = 222
y = 112

[plugins.ao_status]
enabled = true
x = 0
y = 0
```

Position and enabled changes made through the web dashboard take effect at the next epoch (~30 seconds) without restarting the daemon. However, plugin *code* changes (editing `.lua` files) require a daemon restart — only position/enabled changes from the web dashboard are hot-reloaded.

### Default Plugins

Oxigotchi ships with 11 built-in plugins:

| Plugin | What it shows |
|--------|---------------|
| `ao_status` | AngryOxide state: `"AO: N/N | Nm | CH:AH"` (running), `"AO: off"` (stopped), `"AO: ERR"` (failed) |
| `aps` | Number of visible access points |
| `uptime` | Daemon uptime (DD:HH:MM format) |
| `status_msg` | Personality status message |
| `sys_stats` | CPU, memory, frequency, temperature |
| `ip_display` | Current IP address (USB or BT). In RAGE mode, only shows USB tether IP. In SAFE mode, rotates between USB and BT IPs every 5 seconds. |
| `crash` | AO crash counter |
| `www` | Internet connectivity indicator |
| `bt_status` | Bluetooth connection status |
| `battery` | Battery level and charging state |
| `mode` | Current mode (RAGE / SAFE) |

---

## Configuration

### File Locations

| File | Purpose |
|------|---------|
| `/etc/oxigotchi/config.toml` | Main daemon configuration (WiFi, BT, display, personality) |
| `/etc/oxigotchi/plugins.toml` | Plugin positions and enabled state |
| `/etc/oxigotchi/plugins/*.lua` | Lua plugin scripts |
| `/var/lib/oxigotchi/state.json` | Runtime state (attack toggles, whitelist, WPA-SEC key, Discord config) |

### Main Config Example

```toml
[bluetooth]
enabled = true
phone_mac = "XX:XX:XX:XX:XX:XX"
phone_name = "Phone Name"
auto_pair = true
auto_connect = true
```

---

## AP Counting

The daemon counts access points by tracking unique BSSIDs from AO's stdout. AO prints attack lines containing MAC addresses — each unique BSSID is counted as one AP. The count displayed on the e-ink screen and in the dashboard reflects all unique APs seen this session, not just the ones visible right now.

---

## Image Building

The `tools/bake_v3.sh` script builds a complete Oxigotchi v3 SD card image. It takes a v2 base image and layers the Rust daemon on top:

1. Mounts the base image via loopback
2. Copies the cross-compiled `rusty-oxigotchi` binary to `/usr/local/bin/`
3. Creates config directories (`/etc/oxigotchi/`, `/var/lib/oxigotchi/`)
4. Installs the `rusty-oxigotchi.service` systemd unit
5. Deploys default Lua plugins and config
6. Cleans first-boot state (forces migration to run on first boot)
7. Unmounts cleanly

Run inside WSL:
```bash
sudo bash /path/to/oxigotchi/tools/bake_v3.sh
```

---

## Firmware Roadmap (v7)

The current firmware patch (v6) addresses 5 crash vectors. The v7 patch roadmap includes:

- **DWT watchpoint-based PSM reset** — Use the ARM DWT (Data Watchpoint and Trace) unit to trap writes to the PSM watchdog counter address. When the counter exceeds a threshold, the watchpoint handler resets it automatically at the hardware level, eliminating the need for periodic SDIO RAMRW resets from userspace.
- **RSSI threshold fix** — Widen the RSSI rejection window to prevent false signal-loss resets during active channel hopping.

These improvements would make the firmware fully autonomous — no userspace intervention needed to prevent watchdog-triggered crashes.

---

## Building from Source

### Cross-Compile (from Windows with WSL)

```bash
wsl -d Ubuntu -- bash -c "source ~/.cargo/env && cd /path/to/oxigotchi/rust && cargo build --release --target aarch64-unknown-linux-gnu"
```

This produces the binary at:
```
rust/target/aarch64-unknown-linux-gnu/release/rusty-oxigotchi
```

### Deploy to Pi

```bash
scp rust/target/aarch64-unknown-linux-gnu/release/rusty-oxigotchi pi@10.0.0.2:/tmp/
ssh pi@10.0.0.2 "sudo systemctl stop rusty-oxigotchi && sudo cp /tmp/rusty-oxigotchi /usr/local/bin/ && sudo systemctl start rusty-oxigotchi"
```

The systemd service is `rusty-oxigotchi.service`. Check logs with:

```bash
ssh pi@10.0.0.2 "journalctl -u rusty-oxigotchi -f"
```

---

## Troubleshooting

### BT adapter stuck DOWN

**Symptom:** `hciconfig` shows the adapter as DOWN, `hciconfig hci0 up` fails.

**Cause:** The BCM43436B0 UART timed out after WiFi was in monitor mode. Once the bus is in this state, software cannot recover it.

**Fix:** Reboot the Pi. This is the only fix.

### WiFi monitor mode won't start

**Symptom:** AngryOxide fails to start, WiFi interface missing.

**Fix:** Check rfkill:
```bash
rfkill list
```
If WiFi is soft-blocked:
```bash
sudo rfkill unblock wifi
```

### WiFi firmware crash (wlan0mon disappeared)

**Symptom:** The AO status shows "AO: ERR" and the recovery card shows a non-zero crash count.

**What happens automatically:** The daemon detects the missing `wlan0mon` interface, runs `rmmod brcmfmac` followed by `modprobe brcmfmac` to reload the WiFi driver, then restarts AngryOxide. If soft recovery fails, it tries a GPIO power cycle of the WiFi chip.

**If auto-recovery keeps failing:** Reboot the Pi. Some firmware crashes leave the SDIO bus in an unrecoverable state.

**How to reduce crashes:** Keep the attack rate at 1 (the default). Keep dwell time at 5000ms or above. Stick to channels 1, 6, 11.

### Plugin not loading

**Symptom:** Plugin doesn't appear on display or in dashboard.

**Fix:**
1. Check the daemon log for Lua errors:
   ```bash
   journalctl -u rusty-oxigotchi | grep -i lua
   ```
2. Verify the `.lua` file has valid syntax (test with `luac -p your_plugin.lua` if you have Lua installed).
3. Confirm the plugin is listed and enabled in `/etc/oxigotchi/plugins.toml`.

### Mode not switching after button press

**Symptom:** Pressed the PiSugar3 button but mode didn't change.

**Cause:** The daemon checks the button state once per epoch (~30 seconds). The switch happens at the next epoch boundary.

**Fix:** Wait up to 30 seconds. The face will change when the mode switches.

### Web dashboard unreachable

**Symptom:** `http://10.0.0.2:8080` doesn't load.

**Fix:**
1. Verify the USB gadget interface is up:
   ```bash
   ssh pi@10.0.0.2 "ip addr show usb0"
   ```
2. If `usb0` doesn't exist, the USB data cable may not be connected to the correct port (use the center port, not the edge power-only port).
3. Check that the daemon is running:
   ```bash
   ssh pi@10.0.0.2 "systemctl status rusty-oxigotchi"
   ```

### Display is blank

**Symptom:** E-ink display shows nothing after boot.

**Fix:**
- Confirm you have a **Waveshare 2.13" V4** display (not V1/V2/V3 — they use different drivers).
- Check daemon logs for SPI errors:
  ```bash
  journalctl -u rusty-oxigotchi | grep -i "display\|spi\|eink"
  ```

### Captures not appearing

**Symptom:** AO is running and finding networks but no captures show in the dashboard.

**Fix:**
1. Check that hcxpcapngtool is installed:
   ```bash
   which hcxpcapngtool
   ```
2. Look for files in the tmpfs staging directory:
   ```bash
   ls -la /tmp/ao_captures/
   ```
3. Check the permanent capture directory:
   ```bash
   ls -la /home/pi/captures/
   ```
4. If tmpfs has `.pcapng` files but no `.22000` companions, the captures do not contain valid handshakes — this is normal. AO needs time and the right conditions to capture handshakes.

### WPA-SEC uploads not working

**Symptom:** API key is set but "Pending Upload" count stays the same.

**Fix:**
1. Make sure you are in SAFE mode with BT tethering active (internet is required for uploads).
2. Check that the API key is correct — log in to [wpa-sec.stanev.org](https://wpa-sec.stanev.org) and verify.
3. Check daemon logs for upload errors:
   ```bash
   journalctl -u rusty-oxigotchi | grep -i wpasec
   ```
