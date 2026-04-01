# Bluetooth Pentest Mode

← [Back to Wiki Home](Home)

---

The Pi Zero 2W's BCM43436B0 chip uses **two independent buses** — SDIO for WiFi and UART for Bluetooth. BT phone tethering stays connected in RAGE and SAFE modes. Only BT attack mode requires exclusive UART access.

## Three Operating Modes

- **RAGE** — WiFi monitor mode, AngryOxide attacking, BT tether stays connected. The wardriving mode.
- **BT** — Bluetooth offensive: HCI scanning, GATT resolution, BT attacks. WiFi off, phone tether disconnected.
- **SAFE** (default) — WiFi managed mode, BT tethered to phone for internet, no attacks.

Switch via the **PiSugar3 button** (single tap) or the **web dashboard** mode buttons. Transitions happen at the next epoch boundary (~30 seconds) and are managed atomically by `RadioManager` — the lock file prevents partial states.

### RAGE Mode

The bull is hunting. All WiFi attack types active, monitor mode on wlan0mon. BT phone tethering stays connected — you keep SSH and web dashboard access over BT while wardriving.

### BT Mode

The bull goes Bluetooth hunting. WiFi is fully released, the UART is reclaimed for BT, and a custom patchram is loaded to enable attack-capable firmware. The daemon:

1. Stops AngryOxide and releases wlan0mon
2. Loads the BT patchram via `hciattach` (BCM43430B0 HCD with attack extensions)
3. Runs HCI scanning to discover nearby BT devices
4. Resolves GATT services on discoverable targets
5. Identifies vendor/model via BT device class and manufacturer data
6. Launches BT attacks against selected or auto-targeted devices

**BT Aggression Levels** (BT:1 / BT:2 / BT:3):
- **BT:1** — Passive scanning only, no attacks
- **BT:2** — Scanning + targeted attacks on selected devices
- **BT:3** — Full offensive: scan, enumerate, and attack all reachable devices

The aggression level shows in the e-ink mode indicator (e.g., `BT:2`).

### BT Attack Types

| Attack | What It Does |
|--------|-------------|
| **ATT Fuzz** | Sends malformed ATT (Attribute Protocol) requests to crash or confuse GATT servers |
| **BLE ADV** | Crafted BLE advertisement flooding |
| **KNOB** | Key Negotiation of Bluetooth — forces minimum encryption key length (1 byte) during pairing |
| **L2CAP Fuzz** | Sends malformed L2CAP signaling packets to trigger parser bugs |
| **L2CAP Flood** | Connection flood — opens maximum concurrent L2CAP channels |
| **SMP** | Security Manager Protocol attacks — pairing manipulation and key extraction attempts |

All attacks are implemented in `rust/src/bluetooth/attacks/` and use raw HCI sockets.

### SAFE Mode

The bull is resting. WiFi switches to managed mode, BT tethers to your phone for internet access. This enables:
- **WPA-SEC auto-upload** — captured handshakes upload to wpa-sec for cloud cracking
- **Discord notifications** — webhook fires when handshakes are captured
- **SSH over BT** — if USB isn't connected, BT PAN provides network access to the Pi

## Mode Transitions

When switching modes, the daemon handles radio teardown and bringup:

**Any → RAGE:**
1. Release previous mode's radio (BT patchram or managed WiFi)
2. Enter WiFi monitor mode, start AngryOxide
3. Reconnect BT tether (auto via `ensure_connected()`)

**Any → BT Attack:**
1. Stop AngryOxide/release WiFi (if in RAGE)
2. Load BT attack patchram — **disconnects phone tethering**
3. Begin HCI scanning and BT attacks

**Any → SAFE:**
1. Release previous mode's radio
2. Switch WiFi to managed mode
3. Ensure BT tether is connected

The `RadioManager` uses a lock file to prevent concurrent mode transitions and ensure clean handoff.

## Bluetooth Tethering (Always-On)

BT tethering is set up at boot and stays connected in RAGE and SAFE modes:

1. At boot, powers on Bluetooth and connects to phone **before** starting WiFi monitor mode
2. Each epoch, checks BT connection health and auto-reconnects if dropped
3. Only BT attack mode disconnects phone tethering (web dashboard shows a warning)
4. When returning from BT attack mode, tether auto-reconnects

## Configuration

Configure your phone's Bluetooth MAC address in `/etc/oxigotchi/config.toml`:

```toml
[bluetooth]
enabled = true
phone_mac = "AA:BB:CC:DD:EE:FF"
```

Replace `AA:BB:CC:DD:EE:FF` with your phone's Bluetooth MAC address. To find it:
- **Android:** Settings → About Phone → Status → Bluetooth address
- **iPhone:** Settings → General → About → Bluetooth

Your phone must be paired with the Pi beforehand. See [docs/BT_TETHERING.md](https://github.com/CoderFX/oxigotchi/blob/main/docs/BT_TETHERING.md) for full pairing and setup instructions.

### Dashboard Controls

The web dashboard's Bluetooth card shows:
- Current BT state (off/scanning/attacking/tethered)
- Discovered devices with vendor identification
- BT aggression level selector (BT:1/BT:2/BT:3)
- Mode toggle buttons (RAGE/BT/SAFE)
- BT visibility toggle (for initial pairing)
