# Next Session: Port Python Plugin Changes to Rusty

## Context
The Python angryoxide plugin (`plugin/angryoxide.py` v2.3.0) has many features that the Rust binary (`rust/`) doesn't have yet. During this session we made massive changes to the Python plugin that need porting to Rust modules.

## Features in Python NOT in Rust

### 1. Full AO Mode (no TDM cycling)
- **Python**: AO runs continuously, no stop/start cycling. `wifi.recon off` permanently.
- **Rust** (`rust/src/ao.rs`): Still has TDM cycling code (25s attack / 5s scan). Remove it. AO should run continuously.

### 2. AO Stdout Parsing for AP Count
- **Python**: Daemon thread reads AO's stdout, parses for AP/target discovery and capture events. Stores in `_ao_ap_count`.
- **Rust** (`rust/src/ao.rs`): Has `parse_output_line()` but subprocess uses `Stdio::null()`. Change to `Stdio::piped()`, spawn reader thread, parse output.

### 3. Face Variety System (8 features)
- **Python**: Achievement milestones (1st/10th/25th/50th/100th capture), capture face cycling, time-of-day faces, idle rotation (Bored→Lonely→Demotivated→Angry→Sad), friend detection, upload face, debug face on boot, random rare face (5% per epoch).
- **Rust** (`rust/src/personality/mod.rs`): Only has basic mood→face mapping. Needs all 8 features.

### 4. 50 Two-Part Bull Jokes
- **Python**: `BULL_JOKES` dict keyed by face name, 50 jokes as (question, punchline) tuples. Question shows for 2 epochs, punchline for 3 epochs. 30% chance per epoch to start a joke.
- **Rust**: Has no jokes. Add `BULL_JOKES` to personality module.

### 5. 119 Bull Status Messages
- **Python**: `BULL_MESSAGES` dict with 5-7 messages per face state. Slow cycling (3 epochs per message).
- **Rust**: Has basic `status_message()` method. Needs the full message dict.

### 6. RAGE/SAFE Mode (replaces AUTO/MANU)
- **Python**: Not yet implemented in Python either, but planned. RAGE = full attack, SAFE = passive (--notx).
- **Rust**: Doesn't have mode switching. Add `Mode` enum { Rage, Safe } with toggle via PiSugar button.

### 7. IP Display Rotation
- **Python**: `ao_ip` element at (0,95) rotates between `USB:10.0.0.2 :8080` and `BT:<bnep0_ip>` every 5 updates.
- **Rust** (`rust/src/network.rs`): Has `display_ip_str()` and `rotate_display()`. Wire into the display update.

### 8. Bottom Bar Layout
- **Python**: `ao_crash` at (0,112), mode at (222,112), `ao_ip` at (0,95). Assoc/deauth at (166,112) and (189,112).
- **Rust**: Has different positions. Match the Python layout exactly.

### 9. Welcome Message
- **Python**: AO mode shows "Hi! I'm Oxigotchi! Starting v2.2.0..."
- **Rust**: Shows "Rusty Oxigotchi v0.1.0 starting". Change to match personality.

### 10. Periodic State Save
- **Python**: Saves state every 10 epochs + on shutdown. XP saves every 5 epochs.
- **Rust** (`rust/src/personality/mod.rs`): Has save/load but not wired to periodic saving in main loop.

### 11. Bettercap wifi.recon OFF
- **Python**: Calls `agent.run('wifi.recon off')` in on_ready.
- **Rust**: Doesn't run bettercap at all (good), but still tries to hop channels via `iw` which conflicts with AO. Remove channel hopping from Rust — AO handles it.

### 12. Web Dashboard WPA-SEC Input
- **Python**: Dashboard has WPA-SEC API key input field, saves to config.toml.
- **Rust** (`rust/src/web/mod.rs`): Has API stubs but no WPA-SEC input in HTML.

### 13. Firmware Crash Recovery (GPIO)
- **Python**: `_try_fw_recovery()` detects brcmfmac crashes via journalctl pattern, does GPIO power cycle (WL_REG_ON pin 41), modprobe cycle, monstart.
- **Rust** (`rust/src/recovery/mod.rs`): Has GPIO code but not wired to crash pattern detection from dmesg/journalctl.

### 14. Capture Scanning + Verified Count
- **Python**: `_scan_captures()` scans output dir for new .pcapng files, counts verified (.22000 companion exists), detects new captures per epoch, fires XP events.
- **Rust** (`rust/src/capture/mod.rs`): Has the code but not wired into main epoch loop properly.

### 15. Whitelist Filtering + Target Management
- **Python**: Runtime whitelist/target add/remove via web API (`/api/whitelist/add`, `/api/targets/add`). Passes `--whitelist-entry` and `--target` flags to AO command. Skip-captured mode auto-whitelists captured APs.
- **Rust**: AO command builder has whitelist support but no runtime add/remove via web API.

### 16. Delayed Plugin Loading (Fast Boot)
- **Python**: `_DELAY_PLUGINS` list defers non-essential plugins for 30s after AO starts. `_save_delayed_plugins()` / `_restore_delayed_plugins()`.
- **Rust**: Not applicable (no plugin system), but the concept of deferring heavy init should be noted.

### 17. Web Dashboard Full API (22 endpoints)
- **Python**: 22 API endpoints including /api/aps (AP list with signal/channel), /api/cracked (cracked passwords), /api/captures (file list with download), /api/pwn-attacks (PWN mode attack toggles), /api/channels, /api/targets/add-remove, /api/whitelist/add-remove, /api/skip-captured, /api/stop, /api/reset, /api/wpasec, /api/discord-webhook, /api/bt-visibility.
- **Rust** (`rust/src/web/mod.rs`): Has 14 endpoints. Missing: /api/aps detail, /api/pwn-attacks, /api/channels, /api/targets, /api/whitelist runtime, /api/skip-captured, /api/stop, /api/reset, /api/wpasec save, /api/discord-webhook, /api/bt-visibility.

### 18. Discord Webhook Notifications
- **Python**: `_notify_capture()` sends Discord webhook on new captures with capture details.
- **Rust**: Not implemented.

### 19. Monkey-patched update_peers Fix
- **Python**: Patches `agent._update_peers` to suppress AttributeError from pwngrid.
- **Rust**: Not applicable (no pwngrid), but note that peer discovery is planned for Sprint 14.

### 20. Off-screen Element Hiding
- **Python**: Moves pwnagotchi core elements (shakes, channel, aps, display-password) to (300,300) to hide them since bettercap rewrites them after plugin clears.
- **Rust**: Not applicable (no pwnagotchi core), but display layout must match positions exactly.

### 21. PiSugar Button Actions
- **Python**: Button config in /etc/pisugar-server/config.json — single=toggle-bt.sh, double=toggle-mode.sh, long=toggle-ao-pwn.sh.
- **Rust** (`rust/src/pisugar/mod.rs`): Has button detection and mapping but not wired to actual shell scripts or mode switching.

## Priority Order
1. Fix display driver first (see NEXT_SESSION_PROMPT.md)
2. Remove TDM cycling from ao.rs (simple deletion)
3. Remove channel hopping from main loop (AO does it)
4. Add face variety + jokes to personality
5. Fix bottom bar layout positions
6. Add IP display rotation to display update
7. Port welcome message
8. Add WPA-SEC dashboard input
9. Add RAGE/SAFE mode enum

## Key Files to Modify
- `rust/src/ao.rs` — remove TDM, keep stdout parsing
- `rust/src/personality/mod.rs` — add jokes, messages, face variety
- `rust/src/main.rs` — remove channel hopping, fix display positions, periodic save
- `rust/src/web/mod.rs` — add WPA-SEC input to dashboard HTML
- `rust/src/display/mod.rs` — fix layout positions
- `rust/src/network.rs` — wire IP rotation into display

## Build & Deploy
```bash
# Test
cd rust && cargo test

# Cross-compile
wsl -d Ubuntu -e bash -c 'source ~/.cargo/env && cd /mnt/c/msys64/home/user/oxigotchi/rust && export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-gnu-gcc && cargo build --release --target aarch64-unknown-linux-musl'

# Deploy (stop first to avoid "text file busy")
ssh pi@10.0.0.2 "sudo systemctl stop pwnagotchi bettercap"
scp rust/target/.../oxigotchi pi@10.0.0.2:/tmp/rusty-oxigotchi
ssh pi@10.0.0.2 "sudo cp /tmp/rusty-oxigotchi /usr/local/bin/rusty-oxigotchi && sudo systemctl start rusty-oxigotchi"
```

## Don't Forget
- Deploy to Pi AND repo
- No security hardening
- Never share ROM addresses
- Python is the fallback: `sudo systemctl stop rusty-oxigotchi && sudo systemctl start pwnagotchi bettercap`
