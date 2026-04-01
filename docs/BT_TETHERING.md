# Bluetooth Tethering on Pi Zero 2W

> **Note (v3.1):** BT tethering now stays connected in all modes — RAGE, BT attack, and SAFE. The BCM43436B0 uses independent buses for WiFi (SDIO) and BT (UART), so both can operate simultaneously. BT tethering is set up at boot before AngryOxide starts, and the daemon auto-reconnects if the connection drops during any mode.

## Hardware: BCM43436B0 Dual-Bus Architecture

The Pi Zero 2W's BCM43436B0 combo chip uses **two independent buses**:
- **WiFi** — SDIO bus (parallel, high-bandwidth)
- **Bluetooth** — UART bus (serial, HCI protocol)

Because the buses are independent, BT tethering can stay alive while WiFi is in monitor mode (RAGE). The only exception is **BT attack mode**, which requires exclusive UART access to load a custom patchram — this disconnects phone tethering.

## Boot Sequence

The daemon sets up BT tethering **before** starting WiFi monitor mode at boot:

```
1. Power on BT adapter (hciconfig hci0 up / bluetoothctl power on)
2. Pair and connect to phone (bluetoothctl pair/connect + nmcli)
3. Verify bnep0 interface is up
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
phone_mac = "XX:XX:XX:XX:XX:XX"   # REQUIRED — get from bluetoothctl devices
phone_name = "Phone Name"          # Used for scan matching if MAC is missing
auto_pair = true
auto_connect = true
hide_after_connect = true
```

### Getting Your Phone's MAC Address

1. Pair your phone to the Pi manually first (while BT is still up)
2. Run `bluetoothctl devices` to see the MAC
3. Add it to the config as `phone_mac`

Having the MAC address is important — without it, the daemon scans for 10 seconds which may fail if your phone isn't discoverable.

## Recovery: BT Adapter Stuck DOWN

If the BT adapter is stuck in DOWN state (common after WiFi monitor mode was started before BT):

1. **Reboot the Pi** — this is the only reliable way to reset the BCM43436B0 UART
2. The daemon will handle the correct boot order on restart

There is no software-only way to recover the UART once it's timed out. `systemctl restart bluetooth`, `hciconfig hci0 reset`, and `hciattach` all fail.

However, the v3 daemon handles this automatically: when switching from RAGE to SAFE mode, it reloads the `hci_uart` kernel module (`rmmod hci_uart` + `modprobe hci_uart`) before bringing up BT. This gives the UART a clean reset without requiring a full reboot. The reload takes about 4 seconds (1s for rmmod + 3s for hci0 to re-register with the kernel).

## For SD Card Image Flashers

When someone flashes a new SD card with the oxigotchi image:

1. The daemon starts with `bluetooth.enabled = true` but no `phone_mac`
2. It will scan for 10 seconds looking for a device matching `phone_name`
3. If no phone is found, BT is skipped and WiFi monitor mode starts normally
4. The user should:
   - SSH into the Pi
   - Run `sudo bluetoothctl` → `power on` → `scan on` → find their phone
   - Note the MAC address
   - Add `phone_mac = "XX:XX:XX:XX:XX:XX"` to `/etc/oxigotchi/config.toml`
   - Reboot

Alternatively, the web dashboard has a **BT Scan** feature (on the Bluetooth card) that performs a 10-second scan and shows discovered devices. You can pair directly from the dashboard without SSH.

## Mode Transitions and BT Tethering

### RAGE Mode (BT tether stays connected)
BT tethering remains active during RAGE mode. The daemon auto-reconnects each epoch if the connection drops. WiFi monitor mode and BT PAN coexist on independent buses.

### BT Attack Mode (BT tether disconnects)
Switching to BT attack mode disconnects phone tethering — the UART is reclaimed for the attack patchram. A warning appears in the web dashboard: "BT mode disconnects phone tethering. You will lose remote access until switching back to RAGE or SAFE."

When returning from BT attack mode to RAGE or SAFE, the daemon automatically reconnects BT tethering via `ensure_connected()`.

### SAFE Mode (BT tether active)
BT tethering is fully active. WiFi is in managed mode (no monitor, no attacks).

## Known Issues

- **BT attack mode**: Switching to BT attack mode drops your phone tether — you lose SSH/web access over BT until switching back.
- **10-second scan timeout**: If the phone isn't discoverable during the scan window, pairing fails. Using `phone_mac` bypasses this entirely.
- **BT health monitoring**: In RAGE and SAFE modes, the daemon checks BT connection status each epoch and auto-reconnects if the connection drops.

## Python Pwnagotchi Comparison

The Python `bt-tether` plugin handled BT differently:
1. Running `hciconfig hci0 up` in a retry loop
2. Using `dbus` to manage BlueZ directly (not bluetoothctl)
3. Having its own keepalive mechanism

The Rust daemon uses `bluetoothctl` and `nmcli` CLI commands instead. The explicit RAGE/SAFE mode separation avoids the UART contention problems that plagued the Python version.
