# Next Session: Full Bluetooth Tethering

## Context
The Rust bluetooth module (`rust/src/bluetooth/mod.rs`) can connect via an existing `nmcli` profile and monitor `bnep0`, but it can't auto-pair, create profiles, or manage visibility. The Python `bt-tether` plugin did all of this automatically.

## What Needs To Be Done

### 1. Auto-Discovery + Pairing
- Scan for nearby BT devices via `bluetoothctl scan on`
- Match against configured phone name/MAC from config.toml (or accept any PAN-capable device)
- Auto-pair: `bluetoothctl pair <MAC>`, `bluetoothctl trust <MAC>`
- No hardcoded device names — everything from config

### 2. Auto-Connect on Boot
- After pairing, create nmcli connection profile: `nmcli connection add type bluetooth con-name "bt-tether" bt-type panu autoconnect yes`
- Bring up connection: `nmcli connection up bt-tether`
- Detect IP on bnep0 (already implemented)

### 3. Hide Bluetooth After Connect
- `bluetoothctl discoverable off` after successful connection
- Prevents other devices from seeing the Pi

### 4. Auto-Reconnect
- Already partially implemented (check_status + should_connect + retry backoff)
- Need to handle phone going out of range and coming back
- Exponential backoff already in place

### 5. PiSugar Button Toggle
- Single press = toggle BT on/off (already have `toggle()` method)
- Wire to PiSugar button handler in `pisugar/mod.rs`

### 6. Display Indicator
- Already showing `BT:C` / `BT:-` at (115,0) in top bar
- Should also show in bottom bar between WWW and CHG (Python had this)
- When connected, IP rotates in the IP display area (already wired)

### 7. Web Dashboard
- Show BT status, IP, retry count (already in /api/bluetooth endpoint)
- Add connect/disconnect buttons to dashboard

## Python bt-tether Reference
The Python plugin (`bt-tether.py`) does:
1. `bluetoothctl power on`
2. `bluetoothctl agent on` + `bluetoothctl default-agent`
3. `bluetoothctl scan on` (waits for phone)
4. `bluetoothctl pair <MAC>` + `bluetoothctl trust <MAC>`
5. `nmcli connection add type bluetooth ...`
6. `nmcli connection up <name>`
7. `bluetoothctl discoverable off`
8. Periodic health check on bnep0
9. Auto-reconnect with backoff

## Config (config.toml)
```toml
[bluetooth]
enabled = true
phone_name = ""        # auto-detect if empty
phone_mac = ""         # auto-detect if empty
auto_connect = true
auto_pair = true
hide_after_connect = true
retry_interval = 30
max_retries = 0        # 0 = unlimited
```

## Key Files
- `rust/src/bluetooth/mod.rs` — main module (extend)
- `rust/src/config.rs` — add bluetooth config section
- `rust/src/main.rs` — boot sequence + epoch loop wiring
- `rust/src/pisugar/mod.rs` — button handler wiring

## Don't Forget
- No hardcoded device names or MACs
- Deploy to Pi AND repo
- Test with actual phone BT hotspot
