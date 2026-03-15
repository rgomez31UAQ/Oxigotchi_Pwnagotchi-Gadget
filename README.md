# Oxigotchi

> Pwnagotchi on steroids. AngryOxide + patched WiFi on Pi Zero 2W. No dongles needed.

![Oxigotchi e-ink display](docs/oxigotchi-eink.png)

---

## The Problem with Stock Pwnagotchi

Stock pwnagotchi on a Pi Zero 2W is barely functional. Here's what's actually happening under the hood:

The BCM43436B0 WiFi chip was never designed for packet injection. The nexmon patch that enables monitor mode is essentially duct tape — it forces the firmware into a state Broadcom never intended, and the chip fights back constantly. The PSM (Power Save Mode) watchdog fires every few seconds under injection load, the DPC (Deferred Procedure Call) handler panics when frame queues overflow, and memcpy operations trigger hard faults when the SDIO bus can't keep up. The result: **your WiFi module crashes every 2-5 minutes**. Bettercap tries to send a deauth frame, the firmware panics, the SDIO bus dies, wlan0mon disappears, pwnagotchi restarts, and the cycle repeats.

Most people's pwnagotchis spend more time recovering from crashes than actually capturing handshakes. It looks like a cute hacking toy on the outside, but when you dig into the logs, it's barely working — limping along with constant firmware resets, missing most handshakes because the radio is dead half the time.

And it's not just the WiFi. The crash cascade causes a chain of secondary problems:

- **SSH drops constantly** — You're SSH'd in trying to debug something, the firmware crashes, pwnagotchi restarts, your SSH session dies. Reconnect, wait for boot, crash again. Repeat.
- **`monstop` reloads the entire driver** — Every time pwnagotchi restarts, it calls `modprobe -r brcmfmac && modprobe brcmfmac`, which re-enumerates the SDIO bus. Do this enough times in quick succession and the SDIO bus dies permanently — only a full power cycle recovers it.
- **Restart storms kill the SD card** — Pwnagotchi has `Restart=always` in systemd with no rate limit. Crash → restart → crash → restart, over and over, writing logs and thrashing the SD card each time.
- **Boot takes forever** — On every restart, pwnagotchi re-parses its entire log file backwards using `FileReadBackwards`. With a 10MB log, this takes 30-60 seconds of pure I/O on the slow SD card. Every crash costs you a minute of downtime.
- **Bettercap eats memory** — Written in Go, bettercap uses ~80MB of RAM on a Pi Zero 2W that only has 512MB total. Combined with pwnagotchi's Python, you're constantly near memory pressure.
- **Captures are often junk** — Bettercap saves raw pcap files that may contain incomplete handshakes. You think you captured something, upload it to wpa-sec, and get nothing back. Community tools like `hashie-clean` and `pcap-convert-to-hashcat` exist specifically because this is such a common problem.
- **No real-time control** — Want to whitelist your home WiFi? Edit a TOML file over SSH. Want to see what networks are nearby? Check the tiny e-ink text. Want to download a capture? SCP it manually. The stock web UI shows a PNG of the e-ink display and a config editor. That's it.
- **The "AI" doesn't work** — The original pwnagotchi used reinforcement learning to optimize attacks. The jayofelony fork disabled it because it consumed too many resources and didn't actually improve capture rates. The mood faces that were supposed to reflect AI state just cycle randomly now.

On top of all that, bettercap only supports 2 attack types (deauth and PMKID), while modern tools like AngryOxide support 6 — including CSA, rogue M2, and anonymous reassociation that capture handshakes bettercap simply cannot get.

## What I Did About It

I reverse-engineered the BCM43436B0 firmware — mapped the ROM, found the crash handlers, traced the SDIO bus failures back to their root causes. I built a 6-layer firmware patch:

1. **PSM watchdog threshold** — raised from 5 to 255, preventing premature power-save panics
2. **DPC watchdog threshold** — same treatment, stops the deferred procedure handler from killing the radio
3. **RSSI threshold** — widened to prevent false signal-loss resets
4. **Fatal error wrapper** — intercepts error codes 5, 6, 7 at the firmware level and suppresses them instead of crashing
5. **HardFault recovery** — catches memcpy bus faults that previously killed the SDIO connection
6. **BCOL GTK rekey disable** — prevents a group key rotation that triggers a cascade failure under heavy TX load

The result: **27,982 injected frames in a 5-minute stress test, zero crashes.** The firmware that used to die every 2 minutes now runs indefinitely.

**This firmware patch benefits everyone** — not just Oxigotchi users. If you want to keep using stock bettercap in PWN mode, the patched firmware makes that stable too. No more constant crashes and restarts. I'm contributing these findings back to the nexmon project so the broader community benefits.

Then I integrated [AngryOxide](https://github.com/Ragnt/AngryOxide) — a Rust-based attack engine the community has been asking for. Nobody could get it running on the built-in WiFi because the firmware crashes were even worse under AO's heavier injection load. With the patched firmware, it runs flawlessly.

## The Numbers

| Metric | Stock Pwnagotchi | Oxigotchi (AO mode) | Oxigotchi (PWN mode) |
|--------|-----------------|--------------------|--------------------|
| **WiFi crashes** | Every 2-5 minutes | Zero (27,982 frames tested) | Zero (same firmware patch) |
| **Attack types** | 2 (deauth, PMKID) | 6 (+ CSA, disassoc, anon reassoc, rogue M2) | 2 (stock bettercap, but stable) |
| **Memory usage** | ~80 MB (bettercap) | ~15 MB (AO) | ~80 MB (bettercap) |
| **Capture quality** | Raw pcaps, often incomplete | Validated .pcapng + .22000 hashcat-ready | Raw pcaps (stock behavior) |
| **Boot time** | 2-3 min (parses full log) | ~90 sec (session cache) | ~90 sec (session cache) |
| **Channel strategy** | Fixed hop | Smart autohunt with dwell | Fixed hop |
| **Language** | Go | Rust | Go |
| **Web dashboard** | Basic status page | Full control panel (15 cards, 22 API endpoints) | Basic status page |
| **Faces** | Korean text emoticons | 28 bull face PNGs | Korean text emoticons |

**Key point:** Even if you never use AngryOxide, the firmware patch alone makes stock pwnagotchi dramatically more stable. Switch to PWN mode and enjoy a bettercap that actually works.

## Why an Ox?

The name started practical: **Angry**Oxide → **Ox**ide → **Ox**. Then it stuck.

Pwnagotchi has its cute ghost face. Fancygotchi has dolphins and pikachus. But a hacking tool that brute-forces WiFi handshakes with 6 attack types and a patched firmware that refuses to die? That's not cute. That's a bull.

The ox is stubborn — it doesn't stop when the firmware crashes, it recovers. It's strong — 28,000 injected frames without breaking a sweat. And it has horns — when the bull is scanning, the horns point up (peaceful, grazing). When it captures a handshake, the horns come down (charging, triumphant).

28 hand-drawn bull faces show you exactly what your Oxigotchi is doing. No guessing, no random mood swings. Each face means something specific — from the sleeping bull at shutdown to the raging bull when the firmware crashes and recovers.

The pwnagotchi is a pet. The Oxigotchi is a workbull.

## Features

- **No dongles needed** — The Pi Zero 2W's built-in WiFi chip is patched for full monitor mode and TX injection. Plug in a battery, put it in your pocket, done.
- **6 attack types** — Deauth, PMKID, CSA, disassociation, anonymous reassociation, and rogue M2. Captures handshakes that bettercap simply cannot get.
- **Stable firmware** — 6-layer patch, stress-tested with 27,982 injected frames and zero crashes. Works for both AO and bettercap modes.
- **Validated captures** — AO validates every capture before saving. No junk pcaps. Every `.pcapng` has a matching `.22000` hashcat-ready file. No need for cleanup tools like `hashie-clean` or `pcap-convert-to-hashcat`.
- **Web dashboard** — Full control from your phone. 15 cards: attack toggles, AP list with target/whitelist buttons, capture downloads, cracked passwords, system health, BT visibility control, config editor, log viewer.
- **28 bull faces** — Custom 1-bit e-ink art for every mood and system state. Each face is a diagnostic indicator, not decoration.
- **Auto-crack integration** — Captures automatically upload to wpa-sec. Cracked passwords appear in the dashboard.
- **Smart Skip** — Auto-whitelists APs with existing captures, focusing on new targets.
- **Fast boot** — Session data cached, skips the 30-60 second log parsing phase that slows stock pwnagotchi.
- **Backwards compatible** — All existing plugins work. Switch to PWN mode anytime for stock bettercap (now stable with our firmware patch). Your handshakes, config, and plugins are untouched.
- **Firmware rollback** — One command to restore original firmware.
- **Safe updates** — `apt upgrade` works without breaking anything. Kernel and firmware packages are held, apt hooks protect the patched firmware.

## Hardware You Need

> **This project is for the Raspberry Pi Zero 2W ONLY.**
>
> The firmware patches target the BCM43436B0 WiFi chip, which is specific to the Pi Zero 2W. **Other Pi models (Pi 3, Pi 4, Pi Zero W, Pi 5) have different WiFi chips and WILL NOT WORK.**

| Component | Required? | Notes |
|---|---|---|
| **Raspberry Pi Zero 2W** | **YES** | Must be the Zero **2** W (not the original Zero W). |
| **microSD card (16GB+)** | **YES** | Class 10 or faster. 32GB recommended. |
| **Micro USB cable** | **YES** | For power and data (USB tethering). |
| **Waveshare 2.13" V4 e-ink display** | Recommended | Shows the bull faces. The "V4" matters — other versions have different drivers. |
| **PiSugar 3 battery** | Optional | Makes it portable. Battery level shows on dashboard and triggers low-battery faces. |
| **3D-printed case** | Optional | Protects the stack. Many designs on Thingiverse. |

## Installation

### Option 1: Flash the Image (Recommended)

1. **Download the Oxigotchi image** from the [Releases](../../releases) section.
2. **Flash it to your microSD card** using [Raspberry Pi Imager](https://www.raspberrypi.com/software/) or [balenaEtcher](https://etcher.balena.io/).
3. **Insert the SD card** into your Pi Zero 2W.
4. **Windows users: install the USB gadget driver** — Download and run [rpi-usb-gadget-driver-setup.exe](https://github.com/jayofelony/pwnagotchi/releases) before connecting. macOS and Linux don't need this.
5. **Connect the Pi** via the micro USB **data** port (the one closest to the center, not the edge).
6. **Power on.** Wait about 2 minutes for the first boot.
7. **That's it.** The bull appears on the e-ink display and AngryOxide begins scanning automatically.

> **Default credentials** (change these after first boot):
> - SSH: `pi` / `raspberry`
> - Web UI: `changeme` / `changeme`
>
> To SSH in: `ssh pi@10.0.0.2`

### Option 2: Install on Existing Pwnagotchi (Advanced)

```bash
git clone https://github.com/CoderFX/oxigotchi.git /home/pi/Oxigotchi
cd /home/pi/Oxigotchi/tools
sudo python3 deploy_pwnoxide.py
```

The deployer is a 13-step automated installer. It backs up your existing firmware before making changes.

## First Boot

1. **0:00** — Power LED lights up. Boot splash shows the bull on e-ink.
2. **0:30** — Linux finishes booting.
3. **0:40** — Session data loads from cache (stock pwnagotchi spends 30-60s here parsing logs).
4. **1:00** — Pwnagotchi initializes. Bull face changes to "awake."
5. **1:30** — AngryOxide launches.
6. **2:00** — Scanning begins. Bull looks left and right. APs appear in dashboard.
7. **2:00+** — Attacks begin automatically.

> First boot after flashing takes ~30s extra (no session cache yet). Every boot after is faster.

## Web Dashboard

```
http://10.0.0.2:8080/plugins/angryoxide/
```

15 dashboard cards: system health, live e-ink preview, nearby networks with target/whitelist buttons, attack toggles, smart skip, rate control, channel config, targets, whitelist table, controls (mode switch + BT visibility + actions + Discord), captures with type badges and download links, cracked passwords, log viewer, settings editor, installed plugins list.

Auto-refreshes every 5-30 seconds. Dark theme, mobile-friendly.

## Mode Switching

```bash
sudo pwnoxide-mode ao       # AngryOxide + bull faces (default)
sudo pwnoxide-mode pwn      # Stock bettercap + Korean faces (still stable with patched firmware!)
sudo pwnoxide-mode status   # Show current mode
sudo pwnoxide-mode rollback-fw  # Restore original firmware
```

Your mode persists across reboots. Switching takes ~90 seconds.

**Both modes benefit from the firmware patch.** PWN mode gives you a stock pwnagotchi experience that's actually stable — no more constant WiFi crashes.

## Bull Faces — What They Mean

Every mood has its own bull. Here are all 28:

| Face | Name | What's Happening |
|---|---|---|
| ![awake](faces/eink/awake.png) | **Awake** | System booting or starting a new epoch |
| ![look_r](faces/eink/look_r.png) | **Scanning** | Sweeping channels, looking for targets |
| ![look_r_happy](faces/eink/look_r_happy.png) | **Scanning (happy)** | Sweeping channels, good capture rate |
| ![intense](faces/eink/intense.png) | **Intense** | Sending PMKID association frames |
| ![cool](faces/eink/cool.png) | **Cool** | Sending deauthentication frames |
| ![happy](faces/eink/happy.png) | **Happy** | Just captured a handshake |
| ![excited](faces/eink/excited.png) | **Excited** | On a capture streak |
| ![smart](faces/eink/smart.png) | **Smart** | Found optimal channel or processing logs |
| ![motivated](faces/eink/motivated.png) | **Motivated** | High capture rate |
| ![sad](faces/eink/sad.png) | **Sad** | Long dry spell, no captures |
| ![bored](faces/eink/bored.png) | **Bored** | Nothing happening for a while |
| ![demotivated](faces/eink/demotivated.png) | **Demotivated** | Low success rate |
| ![angry](faces/eink/angry.png) | **Angry** | Very long inactivity or many failed attacks |
| ![lonely](faces/eink/lonely.png) | **Lonely** | No other pwnagotchis nearby |
| ![grateful](faces/eink/grateful.png) | **Grateful** | Active captures + good peer network |
| ![friend](faces/eink/friend.png) | **Friend** | Met another pwnagotchi |
| ![sleep](faces/eink/sleep.png) | **Sleep** | Idle between epochs |
| ![broken](faces/eink/broken.png) | **Broken** | Crash recovery, forced restart |
| ![upload](faces/eink/upload.png) | **Upload** | Sending captures to wpa-sec/wigle |
| ![wifi_down](faces/eink/wifi_down.png) | **WiFi Down** | Monitor interface lost |
| ![fw_crash](faces/eink/fw_crash.png) | **FW Crash** | WiFi firmware crashed, recovering |
| ![ao_crashed](faces/eink/ao_crashed.png) | **AO Crashed** | AngryOxide process died, restarting |
| ![battery_low](faces/eink/battery_low.png) | **Battery Low** | Battery below 20% |
| ![battery_critical](faces/eink/battery_critical.png) | **Battery Critical** | Battery below 15%, shutdown soon |
| ![debug](faces/eink/debug.png) | **Debug** | Debug mode active |
| ![shutdown](faces/eink/shutdown.png) | **Shutdown** | Clean power off |

## Safety Features

- **Firmware rollback** — `pwnoxide-mode rollback-fw` restores original firmware at any time.
- **Firmware crash recovery** — Detects brcmfmac crashes via kernel log monitoring and auto-reloads the driver.
- **AO watchdog** — Restarts crashed AO process with exponential backoff (5s → 5min).
- **Restart rate limiting** — Pwnagotchi capped at 3 restarts per 5 minutes.
- **USB lifeline** — SSH always available at `10.0.0.2`, even when WiFi is dead.
- **Mode escape hatch** — `pwnoxide-mode pwn` returns to stock pwnagotchi instantly.
- **Safe apt upgrades** — See below.

## Linux Updates — Fixed

Stock pwnagotchi **blocks 100% of system updates**. Every package is either held or the system is configured to never run `apt upgrade`. This means no security patches, no bug fixes, no library updates — ever. The reasoning was that any update could break nexmon, bettercap, or the Python venv.

**Oxigotchi makes Linux updatable again.** Here's how:

**What's held (will NOT upgrade):**
| Package | Why |
|---------|-----|
| `linux-image-*` | Kernel change would break the nexmon driver (compiled against specific kernel) |
| `firmware-brcm80211` | Would overwrite our patched WiFi firmware with stock |
| `firmware-nexmon` | Would overwrite the nexmon driver |
| `brcmfmac-nexmon-dkms` | Related nexmon packages |
| `firmware-misc-nonfree` | Contains Broadcom firmware files |
| `libpcap-dev`, `libpcap0.8-dev` | Bettercap depends on specific version |
| `firmware-atheros`, `firmware-libertas`, `firmware-realtek` | Held by stock pwnagotchi (kept for safety) |

**What's safe to upgrade (and does upgrade):**
Everything else — security patches, libraries (libssl, libgnutls, libpng), system tools (bash, sudo, openssh), Python packages, raspi-utils, and more. On this image, 72 packages were upgraded successfully with zero breakage.

**How it's protected:**
- `apt-mark hold` on kernel and firmware packages
- An apt hook (`/etc/apt/apt.conf.d/99-protect-firmware`) that automatically backs up the patched firmware before any package install and restores it after — even if a firmware package somehow gets through
- A verification script (`verify-oxigotchi`) you can run anytime to confirm nothing broke

```bash
sudo apt update && sudo apt upgrade -y   # safe on Oxigotchi
sudo verify-oxigotchi                      # confirm everything's intact
```

## Bluetooth Tether — Easy but Read This

This image ships with an **older bt-tether plugin that auto-pairs without PIN confirmation**:

- **For you**: Enable Bluetooth tethering on your phone and the Pi connects automatically. No pairing codes.
- **The catch**: Anyone nearby could potentially pair if left unattended.

**To stay safe:**

1. **Toggle BT visibility from the dashboard** — Controls section has a BT Visible toggle. Turn OFF in public.
2. Replace bt-tether with the latest version from the pwnagotchi repo (uses secure PIN pairing).
3. Or disable bt-tether entirely in config.toml.
4. Or use USB tethering only (`ssh pi@10.0.0.2`).

## FAQ

**Does this work on Pi 4 / Pi 3 / Pi Zero W / Pi 5?**
No. The firmware patches are for the BCM43436B0 chip in the Pi Zero 2W only. Other Pi models have different chips. No workaround exists.

**Can I use my existing pwnagotchi plugins?**
Yes. All standard plugins work. AO captures trigger the standard `on_handshake` event for downstream plugins (wpa-sec, wigle, exp, etc.).

**Can I switch back to stock pwnagotchi?**
Yes. `sudo pwnoxide-mode pwn` returns to bettercap with Korean faces. The firmware patch stays active, so bettercap is stable too. To fully remove the firmware patch: `sudo pwnoxide-mode rollback-fw`.

**Is this legal?**
These are WiFi security auditing tools for testing your own networks or networks you have explicit permission to test. Use responsibly.

**Are my captures actually crackable?**
Yes — AO validates every capture before saving. No junk pcaps. Every `.pcapng` has a matching `.22000` hashcat-ready file. No need for `hashie-clean` or `pcap-convert-to-hashcat`.

**How do I set up wpa-sec auto-cracking?**
Get a free API key from [wpa-sec.stanev.org](https://wpa-sec.stanev.org), add it in the dashboard's Discord/settings section or edit config.toml directly.

**The e-ink display is blank or garbled.**
Make sure you have the **Waveshare 2.13" V4** (not V1/V2/V3). Check `ui.display.type = "waveshare_4"` in config.

**How long does the battery last?**
With PiSugar 3 (1200mAh): 3-4 hours active. The bull face warns at 20% and 15%.

## Maintenance & Support

This project is provided **as-is**. It's stable, tested (197 unit tests, overnight soak test, 28,000-frame injection stress test), and production-ready.

**I will not be maintaining this project actively.** No issue tracking, no PR reviews. The code is GPL-3.0 — fork it, modify it, make it yours.

The pwnagotchi community is active and helpful: [Discord](https://discord.gg/pwnagotchi) · [Reddit](https://www.reddit.com/r/pwnagotchi/) · [Forums](https://community.pwnagotchi.ai/)

The bull will take care of itself.

## Support

If Oxigotchi has been useful to you and you'd like to support the work:

**BTC:** `bc1qnssffujsx5j2h7ep4wzyfa47azjlpwmaq8xtxk`

**ADA:** `addr1qymlyk49yaezevvm525ah6vey3sgah4clt83jmvcp60g5j25v6ukmh4628xn0hanrxwrae2j4huz3j36zt76ph40d44q703236`

No pressure — this project is free and always will be. But firmware reverse-engineering takes a lot of coffee.

## Credits

- [**Pwnagotchi**](https://pwnagotchi.ai) — The original WiFi audit pet by evilsocket and the pwnagotchi community
- [**AngryOxide**](https://github.com/Ragnt/AngryOxide) — Rust-based 802.11 attack engine by Ragnt
- [**Nexmon**](https://nexmon.org) — Firmware patching framework by the Secure Mobile Networking Lab
- [**wpa-sec**](https://wpa-sec.stanev.org) — Free distributed WPA handshake cracking service

## License

[GNU General Public License v3.0](LICENSE)

The WiFi firmware binary on the SD image is a patched version of Broadcom's BCM43436B0 firmware that ships with every Pi Zero 2W. No Broadcom source code is included in this repository.
