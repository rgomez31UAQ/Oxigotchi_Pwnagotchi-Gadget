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
5. The bull face appears on the e-ink display and scanning starts automatically in RAGE mode.

**Connect to your Oxigotchi:**

| Method | Address |
|--------|---------|
| SSH | `ssh pi@10.0.0.2` (default password: `raspberry`) |
| Web dashboard | `http://10.0.0.2:8080` |

> If using Bluetooth tethering in SAFE mode, the web dashboard is also reachable at the BT-assigned IP.

---

## RAGE / SAFE Mode

Oxigotchi v3 has two operating modes that cycle based on what the BCM43436B0 hardware can do. The chip shares a single UART between WiFi and Bluetooth — they cannot run simultaneously.

### Mode Summary

| | RAGE (default) | SAFE |
|---|---|---|
| WiFi | Monitor mode, AngryOxide attacking | Managed mode, no attacks |
| Bluetooth | Off (powered down) | On, tethered to phone |
| Face pool | angry, intense, excited, upload, motivated | debug, grateful |
| Use case | Capturing handshakes | Phone internet, SSH over BT, dashboard access |

### Why Two Modes?

The BCM43436B0 WiFi/BT combo chip on the Pi Zero 2W uses a shared UART bus. When WiFi is in monitor mode injecting frames, the BT side of the UART is unusable. Attempting to run both causes bus contention, firmware hangs, and SDIO timeouts. Rather than fight the hardware, Oxigotchi cleanly separates the two into dedicated modes.

### Switching Modes

**Toggle:** Single tap the PiSugar3 button.

The mode switch happens at the next epoch boundary, which can take up to ~30 seconds. Be patient — the daemon checks the button state once per epoch.

### Transition Details

**RAGE to SAFE:**
1. Stop AngryOxide
2. Exit WiFi monitor mode
3. Power on BT adapter
4. Pair/connect to configured phone

**SAFE to RAGE:**
1. Disconnect BT from phone
2. Power off BT adapter
3. Wait 2 seconds (UART settle delay — required for BCM43436B0)
4. Enter WiFi monitor mode
5. Start AngryOxide

### Face Pools

Each mode draws from its own pool of random faces:

- **RAGE:** angry, intense, excited, upload, motivated
- **SAFE:** debug, grateful

The face changes each epoch to reflect the current mode's personality.

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

1. Switch to SAFE mode (single tap PiSugar3 button, or reboot into SAFE).
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

### Recovery

If the BT adapter gets stuck in a DOWN state after WiFi was in monitor mode, the only fix is a full reboot. This is a hardware limitation of the BCM43436B0 UART — once the bus times out, software cannot recover it.

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
| `format_duration` | `(secs)` returns string | Format seconds as `"HH:MM:SS"` |
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
| `mood` | number | Current mood (0.0–1.0) |
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
x = 185
y = 0

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
| `ao_status` | AngryOxide state: channel, handshakes, epoch |
| `aps` | Number of visible access points |
| `uptime` | Daemon uptime (HH:MM format) |
| `status_msg` | Personality status message |
| `sys_stats` | CPU, memory, frequency, temperature |
| `ip_display` | Current IP address (USB or BT) |
| `crash` | AO crash counter |
| `www` | Internet connectivity indicator |
| `bt_status` | Bluetooth connection status |
| `battery` | Battery level and charging state |
| `mode` | Current mode (RAGE / SAFE) |

---

## Web Dashboard

**URL:** `http://10.0.0.2:8080`

When connected via Bluetooth tethering in SAFE mode, the dashboard is also reachable at the BT-assigned IP on port 8080.

### Features

- **Plugins panel:** Toggle plugins on/off, edit x/y positions, see version, author, and tag for each plugin.
- Real-time system state updates via htmx.

### API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/plugins` | List all plugins with current config |
| `POST` | `/api/plugins` | Batch update plugin positions and enabled state |

---

## Display Layout

The e-ink display is 250x122 pixels, 1-bit monochrome (black and white only). All text is rendered with ProFont bitmap fonts.

**Fonts:**
- `"small"` — ProFont 9pt, 6px character width
- `"medium"` — ProFont 10pt, 7px character width

### Layout Map

```
┌──────────────────────────────────────────────────────────────┐
│ AO: 3/8 | 01:15         APs:15           UP: 02:15         │  y=0  TOP BAR
├──────────────────────────────────────────────────────────────┤  y=14 HLINE
│                     │  Sniffing the                         │
│   [120x66 FACE]     │  airwaves...                          │  y=20 STATUS
│                     │                                       │
│                     │  Lv 3  [████░░░░░░]                   │  y=73 XP BAR
│                     │  mem  cpu freq temp                   │  y=85 SYS
│ USB:192.168.137.2   │  42%  12% 1.0G 45C                   │  y=95 SYS VALUES
├──────────────────────────────────────────────────────────────┤  y=108 HLINE
│ CRASH:0  WWW  BT:C  BAT:85%  APs:15              RAGE     │  y=112 BOTTOM BAR
└──────────────────────────────────────────────────────────────┘
```

**Regions:**
- **Top bar (y=0):** AO status (channel/handshakes), AP count, uptime
- **Face area (y=14 to y=108):** 120x66 pixel bull face on the left
- **Status area:** Status message and personality text to the right of the face
- **XP bar (y=73):** Level and experience progress bar
- **System stats (y=85-95):** Memory, CPU, frequency, temperature
- **IP display:** USB or BT IP address
- **Bottom bar (y=112):** Crash count, internet indicator, BT status, battery, AP count, current mode

---

## Configuration

### File Locations

| File | Purpose |
|------|---------|
| `/etc/oxigotchi/config.toml` | Main daemon configuration (WiFi, BT, display, personality) |
| `/etc/oxigotchi/plugins.toml` | Plugin positions and enabled state |
| `/etc/oxigotchi/plugins/*.lua` | Lua plugin scripts |

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

## Building from Source

### Cross-Compile (from Windows with WSL)

```bash
wsl -d Ubuntu -- bash -c "source ~/.cargo/env && cd /mnt/c/msys64/home/user/oxigotchi/rust && cargo build --release --target aarch64-unknown-linux-gnu"
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
