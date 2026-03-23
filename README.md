# Oxigotchi

> Pwnagotchi on steroids. AngryOxide + patched WiFi on Pi Zero 2W. No dongles needed.

![Oxigotchi e-ink display](docs/oxigotchi-eink.png)

---

## Oxigotchi v3.0 — Rusty Oxigotchi

**Oxigotchi v3.0** is the current release — a full Rust rewrite that replaces the entire Python + bettercap + pwngrid stack with a single ~5MB static binary. ~10MB RAM, boot to scanning in under 5 seconds. No Python interpreter, no venv, no pip, no Go runtime, no garbage collector. Just a lean, mean, handshake-capturing machine.

Everything that made Oxigotchi great (patched firmware, 6 attack types, bull faces, web dashboard, self-healing) has been rebuilt from scratch as native Rust modules compiled into one binary. Your SD card will last a decade.

Full documentation: [docs/RUSTY_V3.md](docs/RUSTY_V3.md). Source code: `rust/` directory.

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

I reverse-engineered the BCM43436B0 firmware — mapped the ROM, found the crash handlers, traced the SDIO bus failures back to their root causes. I built a 7-layer firmware patch:

1. **PSM watchdog threshold** — raised from 5 to 255, preventing premature power-save panics
2. **DPC watchdog threshold** — same treatment, stops the deferred procedure handler from killing the radio
3. **RSSI threshold** — widened to prevent false signal-loss resets
4. **Fatal error wrapper** — intercepts error codes 5, 6, 7 at the firmware level and suppresses them instead of crashing
5. **HardFault recovery** — catches memcpy bus faults that previously killed the SDIO connection
6. **BCOL GTK rekey disable** — prevents a group key rotation that triggers a cascade failure under heavy TX load

The result: **27,982 injected frames in a 5-minute stress test, zero crashes.** The firmware that used to die every 2 minutes now runs indefinitely.

**This firmware patch benefits everyone** — not just Oxigotchi users. If you want to keep using stock bettercap in PWN mode, the patched firmware makes that stable too. No more constant crashes and restarts. I'm contributing these findings back to the nexmon project so the broader community benefits.

Then I integrated [AngryOxide](https://github.com/Ragnt/AngryOxide) — a Rust-based attack engine the community has been asking for. Nobody could get it running on the built-in WiFi because the firmware crashes were even worse under AO's heavier injection load. With the patched firmware, it runs flawlessly.

## How It Works

A single Rust binary (`rusty-oxigotchi`) manages everything: it spawns AngryOxide as a subprocess, drives the e-ink display via SPI, runs the web dashboard on port 8080, manages Bluetooth tethering, executes Lua plugins, and monitors the WiFi firmware for crashes. Only one program touches the WiFi chip at a time — no TX/RX conflicts, no SDIO bus contention.

The daemon operates in two modes: **RAGE** (WiFi monitor mode, AO attacking, BT off) and **SAFE** (WiFi managed mode, BT tethered to phone, no attacks). Toggle between them with the PiSugar3 button or the dashboard. A self-healing stack (PSM counter reset, crash loop detection, modprobe recovery, GPIO power cycle) handles firmware edge cases automatically.

For the full technical deep dive, see **[docs/RUSTY_V3.md](docs/RUSTY_V3.md)** and **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)**.

## The Numbers

| Metric | Stock Pwnagotchi | Oxigotchi v3.0 |
|--------|-----------------|----------------|
| **WiFi crashes** | Every 2-5 minutes | Zero (v6 firmware, 27,982 frames tested) |
| **Attack types** | 2 (deauth, PMKID) | 6 (+ CSA, disassoc, anon reassoc, rogue M2) |
| **Memory usage** | ~80 MB (bettercap + Python) | ~10 MB (single Rust binary) |
| **Capture quality** | Raw pcaps, often incomplete | Validated .pcapng + .22000 hashcat-ready |
| **Boot time** | 2-3 min (parses full log) | <5 sec (no Python, no venv) |
| **Channel strategy** | Fixed hop | Smart autohunt with dwell, live channel on display |
| **Language** | Python + Go | 100% Rust |
| **Web dashboard** | Basic status page | 22 live cards, full control panel (axum) |
| **Faces** | Korean text emoticons | 26 bull face PNGs (SPI direct to e-ink) |
| **SD card lifespan** | ~1-2 years | 10+ years (tmpfs capture pipeline, near-zero writes) |
| **Binary size** | 150MB+ (Python venv + Go) | ~5 MB static binary |
| **Self-healing** | Manual reboot | PSM reset, crash loop detection, modprobe cycle, GPIO recovery |
| **XP/Leveling** | Basic (level^3/2 curve) | Exponential (level^1.05*5), cap 999, passive + active XP |

**Key point:** Even if you never use AngryOxide, the firmware patch alone makes stock pwnagotchi dramatically more stable. Switch to PWN mode and enjoy a bettercap that actually works.

## Why an Ox?

The name started practical: **Angry**Oxide → **Ox**ide → **Ox**. Then it stuck.

Pwnagotchi has its cute ghost face. Fancygotchi has dolphins and pikachus. But a hacking tool that brute-forces WiFi handshakes with 6 attack types and a patched firmware that refuses to die? That's not cute. That's a bull.

The ox is stubborn — it doesn't stop when the firmware crashes, it recovers. It's strong — 28,000 injected frames without breaking a sweat. And it has horns — when the bull is scanning, the horns point up (peaceful, grazing). When it captures a handshake, the horns come down (charging, triumphant).

28 hand-drawn bull faces show you exactly what your Oxigotchi is doing. No guessing, no random mood swings. Each face means something specific — from the sleeping bull at shutdown to the raging bull when the firmware crashes and recovers.

The pwnagotchi is a pet. The Oxigotchi is a workbull.

## Features

- **No dongles needed** — Most people give up on the built-in WiFi and buy a $15 Alfa dongle. Oxigotchi patches the Pi Zero 2W's BCM43436B0 chip for full monitor mode and TX injection. No external adapters, no USB hubs, no extra bulk. Plug in a battery, put it in your pocket, done.
- **6 attack types** — Deauth, PMKID, CSA, disassociation, anonymous reassociation, and rogue M2. Captures handshakes that bettercap simply cannot get.
- **Stable firmware** — 7-layer patch (v6), stress-tested with 27,982 injected frames and zero crashes. Works for both AO and bettercap modes.
- **Validated captures** — AO validates every capture before saving. No junk pcaps. Every `.pcapng` has a matching `.22000` hashcat-ready file. No need for cleanup tools like `hashie-clean` or `pcap-convert-to-hashcat`.
- **Web dashboard** — Full control from your phone. 22 live cards: attack toggles, nearby networks, per-file capture downloads, cracked passwords, system health, BT visibility control, channel config with autohunt, whitelist management, WPA-SEC upload, Discord notifications, plugin manager, mode switch, system controls, log viewer.
- **26 bull faces** — Custom 1-bit e-ink art for every mood and system state. Each face is a diagnostic indicator, not decoration.
- **Auto-crack integration** — Captures automatically upload to WPA-SEC for cloud cracking. Cracked passwords appear in the dashboard.
- **Discord notifications** — Optional webhook integration sends a Discord message every time a handshake is captured. Disabled by default.
- **XP & leveling** — The bull earns XP passively (+1 per epoch just for scanning, +1 per AP seen) and actively (+100 per handshake, +15 per association, +10 per deauth, +5 per new AP). An exponential curve (`level^1.05 * 5`) makes early levels fast and high levels a grind — max level 999 takes about 7 months of daily use. XP persists across reboots.
- **Live channel display** — The current AO channel updates on the e-ink screen every 5 seconds, parsed from AO's stdout.
- **Channel hopping** — Default channels are 1, 6, 11 (non-overlapping 2.4GHz). Configurable from the dashboard with 13 toggleable channel buttons and a dwell time slider. Autohunt mode lets AO choose channels intelligently.
- **Smart Skip** — Auto-whitelists APs with existing captures, focusing on new targets.
- **Fast boot** — Under 5 seconds from power-on to scanning. No Python, no venv, no log parsing.
- **RAGE/SAFE mode** — PiSugar3 button or dashboard toggles between WiFi attack mode (RAGE) and BT internet tethering mode (SAFE). The BCM43436B0's shared UART prevents both simultaneously.
- **Self-healing stack** — PSM watchdog counter reset every 15 minutes, crash loop detection (3+ SIGABRT triggers modprobe recovery), exponential AO restart backoff, GPIO power cycle, graceful give-up (daemon stays up, never reboots).
- **tmpfs capture pipeline** — AO writes to RAM. Only proven handshakes move to SD card. Near-zero SD card wear during attacks.
- **State persistence** — Attack toggles, whitelist, WPA-SEC key, Discord config, and channel settings survive restarts (saved to `/var/lib/oxigotchi/state.json`).
- **Reproducible image builds** — `tools/bake_v3.sh` builds a complete SD card image from the repo. Single binary flash, no venv, no pip.
- **Legacy auto-disable** — Stops and disables pwnagotchi and bettercap services on first boot, freeing ~66 MB of RAM.
- **GPS auto-detection** — If gpsd is running, captures automatically include GPS coordinates.
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
6. **Power on.** Wait about 5 seconds for boot.
7. **That's it.** The bull appears on the e-ink display and AngryOxide begins scanning automatically in RAGE mode (the default).

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

The deployer is an 18-step automated installer. It backs up your existing firmware before making changes.

## First Boot

1. **0:00** — Power LED lights up.
2. **~3s** — Kernel loaded, Rust daemon starts. Boot splash shows the bull on e-ink.
3. **~5s** — AngryOxide launches. Scanning begins in RAGE mode.
4. **~5s+** — Attacks begin automatically. APs appear in the dashboard.

> First boot after flashing takes a few seconds extra (migration from pwnagotchi config runs once).

## Web Dashboard

```
http://10.0.0.2:8080
```

22 live dashboard cards organized by user journey: face display, core stats, live e-ink preview, battery, bluetooth, WiFi, attack type toggles, recent captures, recovery status, personality/XP, system info, cracked passwords, per-file capture downloads, mode switch, system controls (restart AO / shutdown Pi / restart SSH), plugins, nearby networks, whitelist, channel config with autohunt, WPA-SEC upload, Discord notifications, and live logs.

Auto-refreshes every 5-30 seconds. Dark theme, mobile-friendly.

## RAGE / SAFE Mode

The Pi Zero 2W's BCM43436B0 chip shares a UART between WiFi and Bluetooth — they cannot run simultaneously. Oxigotchi cleanly separates them into two modes:

- **RAGE** (default) — WiFi monitor mode, AngryOxide attacking, BT off
- **SAFE** — WiFi managed mode, BT tethered to phone for internet, no attacks

Switch via the **PiSugar3 button** (single tap) or the **web dashboard** (RAGE/SAFE buttons). The switch happens at the next epoch boundary (~30 seconds).

## Bull Faces — What They Mean

Every mood has its own bull. Here are 26 faces:

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
- **PSM watchdog reset** — Every 15 minutes, the daemon resets the firmware's PSM/DPC/RSSI watchdog counters via SDIO RAMRW, preventing long-running degradation.
- **Crash loop detection** — If AO crashes 3+ times (SIGABRT from degraded firmware), the daemon triggers a full `modprobe` recovery cycle instead of endlessly restarting.
- **GiveUp safety** — After all recovery attempts are exhausted, the daemon gives up gracefully. It never reboots the Pi. SSH and the web dashboard stay accessible.
- **GPIO self-heal** — When the SDIO bus dies (error -22), the daemon power-cycles the BCM43436B0 chip via GPIO 41 (WL_REG_ON), rebinds the MMC controller, reloads the driver, and restarts AO.
- **AO watchdog** — Restarts crashed AO with exponential backoff (5s, 10s, 20s... up to 5 minutes).
- **USB lifeline** — SSH always available at `10.0.0.2`, even when WiFi is dead.
- **Safe apt upgrades** — Kernel and firmware packages held, apt hooks auto-protect the patched firmware binary.

## Bluetooth Tethering

Bluetooth tethering is built into the Rust daemon and activates in SAFE mode. Switch to SAFE mode via the PiSugar3 button (single tap) or the web dashboard.

When switching from RAGE to SAFE, the daemon automatically reloads the `hci_uart` kernel module to reset the shared UART, then powers on BT and connects to your configured phone.

Configure your phone's BT MAC in `/etc/oxigotchi/config.toml` under `[bluetooth]`. See [docs/BT_TETHERING.md](docs/BT_TETHERING.md) for setup details.

## FAQ

**Does this work on Pi 4 / Pi 3 / Pi Zero W / Pi 5?**
No. The firmware patches are for the BCM43436B0 chip in the Pi Zero 2W only. Other Pi models have different chips. No workaround exists.

**Can I write plugins?**
Yes. Oxigotchi v3 uses Lua 5.4 plugins. Place `.lua` files in `/etc/oxigotchi/plugins/`. Plugins can register indicators on the e-ink display and react to epoch, handshake, crash, and BT events. See [docs/RUSTY_V3.md](docs/RUSTY_V3.md) for the full plugin API.

**Can I switch back to stock pwnagotchi?**
The legacy pwnagotchi and bettercap services are disabled on first boot. You can re-enable them with `systemctl enable pwnagotchi bettercap`, but the Rust daemon is designed to fully replace them. To fully remove the firmware patch: `sudo pwnoxide-mode rollback-fw`.

**Is this legal?**
These are WiFi security auditing tools for testing your own networks or networks you have explicit permission to test. Use responsibly.

**Are my captures actually crackable?**
Yes — AO validates every capture before saving. No junk pcaps. Every `.pcapng` has a matching `.22000` hashcat-ready file. No need for `hashie-clean` or `pcap-convert-to-hashcat`.

**How do I set up WPA-SEC auto-cracking?**
Get a free API key from [wpa-sec.stanev.org](https://wpa-sec.stanev.org), paste it in the WPA-SEC card on the dashboard and hit Save. Captured handshakes upload automatically when internet is available (SAFE mode with BT tethering). Cracked passwords appear in the Cracked Passwords card.

**The e-ink display is blank or garbled.**
Make sure you have the **Waveshare 2.13" V4** (not V1/V2/V3 — they use different drivers). Check daemon logs: `journalctl -u rusty-oxigotchi | grep -i spi`

**How does XP and leveling work?**
Your bull earns XP passively (+1 per epoch, +1 per AP seen) and actively (+100 per handshake, +15 per association, +10 per deauth, +5 per new AP). The level formula is exponential: `XP needed = level^1.05 * 5`. Early levels fly by (Lv 1 needs 5 XP, Lv 10 needs 56 XP), but high levels are a grind (Lv 500 needs 3,900 XP, Lv 999 needs ~18,000 XP per level). Max level is **999** — reaching it takes roughly **7 months** of daily use (8 hours/day, ~16 APs). XP persists across reboots.

**Can I change the attack rate?**
The dashboard lets you set rate 1 (Quiet), 2 (Normal), or 3 (Aggressive). **Rate 1 is the default and recommended.** Rate 2 works well at home or in low-density areas, but in busy environments (walking through a city, near many APs) the heavy TX load can overwhelm the BCM43436B0 firmware — WiFi freezes and needs a reboot. This isn't a hard hardware limit — it's a firmware timing issue under high AP density + rapid channel hopping + movement. Rate 1 still uses all 6 attack types, just sends fewer frames per second. Rate 3 is experimental and will likely crash in most environments. If you plug in an external WiFi dongle (Alfa, RT5370, etc.) and configure AO to use it instead of the built-in chip, rate 2 and 3 work perfectly — the limitation is specific to the BCM43436B0.

**Does scanning more channels help?**
Not on the built-in WiFi. The BCM43436B0 firmware is more likely to crash when hopping across many channels — scanning all 13 with a short dwell time stresses the TX path and triggers the same firmware trap (EPC 0x204CA) as high rates. **Stick to channels 1, 6, 11** (the non-overlapping 2.4 GHz channels where 95% of APs live). You won't miss much, and your WiFi won't die mid-walk. If you use an external dongle (Alfa, etc.), scan all channels freely — external chips don't have this limitation.

**How long does the battery last?**
With PiSugar 3 (1200mAh): 3-4 hours active. The bull face warns at 20% and 15%.

## Maintenance & Support

This project is provided **as-is**. It's stable, tested (480+ unit tests, overnight soak test, 28,000-frame injection stress test), and production-ready.

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
