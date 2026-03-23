# Bluetooth Tethering on Pi Zero 2W

> **Note:** In Rusty Oxigotchi v3.0, BT and WiFi monitor mode never run simultaneously. The daemon uses RAGE/SAFE mode cycling — BT is only active in SAFE mode, WiFi monitor is only active in RAGE mode. The PiSugar3 button toggles between them. See [RUSTY_V3.md](RUSTY_V3.md) for details.

## Hardware Limitation: BCM43436B0 Shared UART

The Pi Zero 2W uses a BCM43436B0 combo WiFi/BT chip. WiFi and Bluetooth share a single UART bus. This creates a critical constraint:

**Once WiFi enters monitor mode, the BT UART cannot be initialized.**

Symptoms:
- `hciconfig hci0 up` returns "Connection timed out (110)"
- `bluetoothctl power on` fails with "org.bluez.Error.Failed"
- The adapter shows as `DOWN` and cannot be recovered without a reboot or chip reset

## Boot Sequence (Correct Order)

The daemon MUST set up BT **before** starting WiFi monitor mode:

```
1. Power on BT adapter (hciconfig hci0 up / bluetoothctl power on)
2. Pair and connect to phone (bluetoothctl pair/connect + nmcli)
3. Verify bnep0 interface is up
4. THEN start WiFi monitor mode (iw phy0 interface add wlan0mon type monitor)
5. THEN start AngryOxide
```

The current Rust daemon does this in `boot()` — see `rust/src/main.rs`.

> **Note (v3.0):** This boot sequence only applies to the initial SAFE mode transition, not boot. RAGE is the default boot mode — no BT is started at boot. BT is only powered on when the user switches to SAFE mode via the PiSugar3 button.

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

## RAGE to SAFE Transition (hci_uart Reset)

When the user switches from RAGE to SAFE mode, the daemon performs these steps in order:

1. Stop AngryOxide
2. Exit WiFi monitor mode (`ip link set wlan0mon down`, `iw dev wlan0mon del`)
3. Reload hci_uart kernel module:
   - `rmmod hci_uart` (removes the BT UART driver)
   - Wait 1 second
   - `modprobe hci_uart` (reloads with clean state)
   - Wait 3 seconds (hci0 re-registers with the kernel)
4. Power on BT adapter via bluetoothctl
5. Pair/connect to configured phone via nmcli

This hci_uart reset is critical because WiFi monitor mode leaves the shared BCM43436B0 UART in a state where BT HCI commands time out. Without the reload, `bluetoothctl power on` fails with error 110.

## SAFE to RAGE Transition

1. Disconnect BT from phone (nmcli + bluetoothctl)
2. Power off BT adapter (`bluetoothctl power off`)
3. Wait 2 seconds (UART settle delay)
4. Enter WiFi monitor mode
5. Start AngryOxide

## Known Issues

- **WiFi + BT coexistence**: In v3, WiFi and BT never run simultaneously. RAGE mode powers off BT entirely, SAFE mode stops WiFi monitor. This is by design — the shared UART cannot reliably handle both.
- **nmcli error**: "No suitable device found (device wlan0 not available)" — this means the BT UART is down, not a WiFi issue. The error message is misleading. Switch to SAFE mode to fix.
- **10-second scan timeout**: If the phone isn't discoverable during the scan window, pairing fails. Using `phone_mac` bypasses this entirely.
- **BT health monitoring**: In SAFE mode, the daemon checks BT connection status each epoch and auto-reconnects if the connection drops.

## Python Pwnagotchi Comparison

The Python `bt-tether` plugin handled BT differently:
1. Running `hciconfig hci0 up` in a retry loop
2. Using `dbus` to manage BlueZ directly (not bluetoothctl)
3. Having its own keepalive mechanism

The Rust daemon uses `bluetoothctl` and `nmcli` CLI commands instead. The explicit RAGE/SAFE mode separation avoids the UART contention problems that plagued the Python version.
