# Bluetooth Tethering on Pi Zero 2W

> **Note (v3.1):** BT tethering uses D-Bus BlueZ directly — no nmcli, no manual MAC address needed. Pair your phone from the web dashboard, and the daemon handles everything: auto-reconnect with exponential backoff, iOS/Android MAC randomization via BlueZ bonding (IRK), and PAN networking via BlueZ `Network1.Connect("nap")`.

## Hardware: BCM43436B0 Dual-Bus Architecture

The Pi Zero 2W's BCM43436B0 combo chip uses **two independent buses**:
- **WiFi** — SDIO bus (parallel, high-bandwidth)
- **Bluetooth** — UART bus (serial, HCI protocol)

Because the buses are independent, BT tethering can stay alive while WiFi is in monitor mode (RAGE). The only exception is **BT attack mode**, which requires exclusive UART access to load a custom patchram — this disconnects phone tethering.

## Boot Sequence

The daemon sets up BT tethering **before** starting WiFi monitor mode at boot:

```
1. Power on BT adapter (bluetoothctl power on)
2. Connect to paired phone via D-Bus Network1.Connect("nap")
3. Run DHCP on the PAN interface (bnep0)
4. THEN start WiFi monitor mode (iw phy0 interface add wlan0mon type monitor)
5. THEN start AngryOxide
```

This order is important — starting WiFi monitor mode first can sometimes interfere with BT initialization. The daemon handles this automatically in `boot()` (see `rust/src/main.rs`).

> **Default boot mode (v3.1):** The daemon boots into SAFE mode by default. Users can override this by setting `default_mode = "RAGE"` in `/etc/oxigotchi/config.toml` under `[main]`.

## Config

In `/etc/oxigotchi/config.toml`:

```toml
[bluetooth]
enabled = true
phone_name = "Phone Name"          # Display name for dashboard (optional)
auto_connect = true                # Auto-connect to paired phone at boot
hide_after_connect = true          # Hide BT adapter after connecting
```

No MAC address is needed in the config. The daemon discovers paired devices via D-Bus `ObjectManager` and connects to the best candidate automatically. iOS and Android MAC randomization is handled transparently through BlueZ bonding (the IRK exchanged during pairing resolves randomized addresses).

## Pairing Your Phone

### Via Web Dashboard (recommended)

1. Open the web dashboard at `http://<pi-ip>:8080`
2. In the **Phone Tethering** section, click **Scan for Devices**
3. Make your phone discoverable (Bluetooth settings → visible/discoverable)
4. Your phone appears in the device list — click **Pair**
5. Confirm the passkey on both the Pi dashboard and your phone
6. The daemon connects automatically after pairing

### Via SSH (fallback)

1. SSH into the Pi
2. Run `sudo bluetoothctl` → `power on` → `scan on`
3. When your phone appears, run `pair XX:XX:XX:XX:XX:XX`
4. Confirm the passkey on your phone
5. Run `trust XX:XX:XX:XX:XX:XX`
6. The daemon picks up the paired device automatically on next epoch

## Auto-Reconnect

The daemon auto-reconnects with exponential backoff if the BT connection drops:

- **Schedule:** 30s → 60s → 120s → 300s (caps at 5 minutes)
- **No max retry limit** — keeps trying indefinitely
- Reconnect runs every epoch in all modes (RAGE, BT, SAFE)
- If the user explicitly disconnects via the dashboard, auto-reconnect is paused until manually re-enabled

## Recovery: BT Adapter Stuck DOWN

If the BT adapter is stuck in DOWN state (common after WiFi monitor mode was started before BT):

1. **Reboot the Pi** — this is the only reliable way to reset the BCM43436B0 UART
2. The daemon will handle the correct boot order on restart

There is no software-only way to recover the UART once it's timed out. `systemctl restart bluetooth`, `hciconfig hci0 reset`, and `hciattach` all fail.

However, the v3 daemon handles this automatically: when switching from RAGE to SAFE mode, it reloads the `hci_uart` kernel module (`rmmod hci_uart` + `modprobe hci_uart`) before bringing up BT. This gives the UART a clean reset without requiring a full reboot. The reload takes about 4 seconds (1s for rmmod + 3s for hci0 to re-register with the kernel).

## For SD Card Image Flashers

When someone flashes a new SD card with the oxigotchi image:

1. The daemon starts with `bluetooth.enabled = true` and no paired devices
2. BT tethering is skipped — WiFi monitor mode starts normally
3. The user pairs their phone via the web dashboard:
   - Open `http://<pi-ip>:8080` (connect via USB at `10.0.0.2:8080`)
   - Click **Scan for Devices** in the Phone Tethering section
   - Select their phone and click **Pair**
   - Confirm the passkey on both devices
4. From then on, the daemon auto-connects to the paired phone at every boot

No config file editing required. No MAC address needed.

## Mode Transitions and BT Tethering

### RAGE Mode (BT tether stays connected)
BT tethering remains active during RAGE mode. The daemon auto-reconnects each epoch if the connection drops. WiFi monitor mode and BT PAN coexist on independent buses.

### BT Attack Mode (BT tether disconnects)
Switching to BT attack mode disconnects phone tethering — the UART is reclaimed for the attack patchram. A warning appears in the web dashboard: "BT mode disconnects phone tethering. You will lose remote access until switching back to RAGE or SAFE."

When returning from BT attack mode to RAGE or SAFE, the daemon automatically reconnects BT tethering.

### SAFE Mode (BT tether active)
BT tethering is fully active. WiFi is in managed mode (no monitor, no attacks).

## Dashboard Controls

The web dashboard's **Phone Tethering** section provides:
- **Scan for Devices** — discovers nearby BT devices
- **Pair** — pairs and connects to a selected device
- **Disconnect** — manually disconnects the phone tether (pauses auto-reconnect)
- **Forget** — removes a paired device from BlueZ
- **Passkey confirmation** — shows passkey during pairing for user confirmation

## Known Issues

- **BT attack mode**: Switching to BT attack mode drops your phone tether — you lose SSH/web access over BT until switching back.
- **Agent1 auto-accepts**: The D-Bus Agent1 handler auto-accepts pairing requests (headless mode). The web UI displays the passkey for visual confirmation but the pairing proceeds automatically.
- **BT health monitoring**: In all modes, the daemon checks BT connection status each epoch and auto-reconnects if the connection drops.

## Python Pwnagotchi Comparison

The Python `bt-tether` plugin handled BT differently:
1. Running `hciconfig hci0 up` in a retry loop
2. Using `dbus` to manage BlueZ directly (not bluetoothctl)
3. Having its own keepalive mechanism
4. Requiring a hardcoded MAC address in config

The Rust daemon uses D-Bus BlueZ directly (like the Python version) but improves on it: no MAC address needed (auto-discovers paired devices), exponential backoff reconnect, web UI pairing, and transparent iOS/Android MAC randomization handling via BlueZ bonding IRK exchange.
