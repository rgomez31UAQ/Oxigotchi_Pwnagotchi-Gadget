# Oxigotchi

> The pwnagotchi was a pet. This is a workbull.

![Oxigotchi e-ink display](docs/oxigotchi-eink.png)

**This is not a proof of concept.** Oxigotchi is a finished, tested, daily-driven tool — weeks of field testing, overnight soak tests, stress tests, and real-world refinement. Flash the image, plug in the Pi, and it works. No setup wizards, no dependency hell, no "coming soon" features. Everything described here is shipping.

---

## The Story

Stock pwnagotchi on a Pi Zero 2W crashes every 2-5 minutes. The BCM43436B0 WiFi chip panics, SSH drops, the e-ink freezes, and you're standing in a parking lot wondering if your Pi is alive. You SSH back in, restart everything, walk 50 meters, and it crashes again. This is the experience for thousands of pwnagotchi owners.

So we reverse-engineered the entire WiFi firmware. Mapped 6,965 functions. Reconstructed 313 struct fields. Traced 24,328 cross-references. Found that 5 of the crash paths originate in read-only ROM — no software patch can touch them. Built a DWT hardware watchpoint that intercepts them anyway using the ARM debug unit. Then rewrote everything else in Rust, replaced bettercap with [AngryOxide](https://github.com/Ragnt/AngryOxide), and gave it a bull with opinions.

The result: 27,982 injected frames in a 5-minute stress test. Zero crashes. The chip that died every 2 minutes now runs indefinitely.

---

## What It's Like

You flash the SD card, plug in the Pi, and about 30 seconds later a bull face appears on the e-ink display. AngryOxide is already scanning.

Walk through a busy area and the bull comes alive. He sweeps channels, spots APs, starts sending PMKID associations and deauths. His face changes — **intense** when attacking, **cool** when deauthing, **happy** when he captures a handshake, **excited** when he's on a streak. Leave him in a dead zone and he gets **bored**, then **sad**, then **lonely**. He'll mutter things like *"Even the APs left..."* or *"Tumbleweed just rolled by my antenna."*

Capture a handshake on the first try and he gets **grateful**: *"A WPA2 handshake on the first try!"* Hit a long dry spell and he gets philosophical: *"If boredom were a sport, I'd medal."* He tells bull jokes between epochs. He levels up. He has a mood score that responds to the RF spectrum around him — busy airwaves make him excited, silence makes him lonely, deauth storms make him angry.

He's a pet that also happens to run 12 concurrent attack types across WiFi and Bluetooth, classify 256 frames per millisecond, and auto-upload captures to the cloud for password recovery. The **Mooooood** is real.

---

## The Numbers

| Metric | Stock Pwnagotchi | Oxigotchi v3.0 |
|--------|-----------------|----------------|
| WiFi crashes | Every 2-5 min | Zero (8-layer firmware patch) |
| WiFi attack types | 2 | 6 (+ CSA, disassoc, anon reassoc, rogue M2) |
| BT attack types | 0 | 6 (ATT fuzz, BLE ADV, KNOB, L2CAP fuzz/flood, SMP) |
| Memory | ~80 MB | ~10 MB |
| Boot time | 2-3 min | ~30 sec |
| RF awareness | None | 10 frame types, 256 frames/ms, live RF stats |
| Binary size | 150MB+ (Python + Go + bettercap) | ~5 MB (single Rust binary) |
| SD card lifespan | ~1-2 years | 10+ years (tmpfs capture pipeline) |
| Language | Python + Go | 100% Rust |
| Capture quality | Junk pcaps, needs hashie-clean | Every capture validated, hashcat-ready |

---

## AngryOxide + Rust: Why It Works

Stock pwnagotchi uses bettercap — a Go-based network attack tool that wasn't designed for embedded systems. It's slow, memory-hungry, and generates junk captures that need post-processing.

Oxigotchi replaces the entire stack with [AngryOxide](https://github.com/Ragnt/AngryOxide) — a purpose-built Rust 802.11 attack engine. AO validates every capture before saving. No junk pcaps. Every `.pcapng` has a matching `.22000` hashcat-ready file. Six WiFi attack types run simultaneously, rate-limited to what the patched firmware can handle without breaking a sweat.

The Rust daemon wraps AO in a full lifecycle manager: crash recovery with exponential backoff, stdout parsing for real-time AP counts, tmpfs-based capture pipeline that protects the SD card. When AO finds handshakes, the bull gets happy. When AO crashes (rare, but it happens), the bull shows his **AO Crashed** face, waits a few seconds, and restarts it automatically. You never have to intervene.

The combination of patched firmware (no crashes) + AO (validated captures) + Rust daemon (10MB RAM, ~30s boot) is what makes Oxigotchi actually work as a carry-everywhere tool instead of a weekend project that needs constant babysitting.

---

## Three Modes

**RAGE** (default) — WiFi monitor mode. AO attacking. The bull is hunting. This is wardriving mode.

**BT** *(experimental)* — Bluetooth offensive. WiFi off, custom patchram loaded. HCI scanning, GATT resolution, vendor identification, then 6 attack types: ATT fuzz, KNOB, L2CAP flood, SMP pairing attacks, BLE advertisement flooding. Modern BT is surprisingly well-hardened — this mode was built to explore what's possible, and it turns out the answer is "not much against anything patched in the last few years." Still fun to scan with. Aggression levels BT:1 (scan only), BT:2 (targeted), BT:3 (full offensive).

**SAFE** — WiFi managed + BT tethered to your phone. No attacks. Internet access for WPA-SEC auto-upload, Discord notifications, and SSH. The bull is resting but your captures are uploading.

One button cycles between them. The Pi Zero 2W's BCM43436B0 shares a single UART between WiFi and Bluetooth — they can't run at the same time. The `RadioManager` handles the teardown and bringup atomically. No partial states, no stuck radios.

---

## The Bull

A few of his 28 faces:

| | | |
|:---:|:---:|:---:|
| ![awake](faces/eink/awake.png) | ![intense](faces/eink/intense.png) | ![happy](faces/eink/happy.png) |
| **Awake** — just booted | **Intense** — sending PMKIDs | **Happy** — captured a handshake |
| ![excited](faces/eink/excited.png) | ![angry](faces/eink/angry.png) | ![sleep](faces/eink/sleep.png) |
| **Excited** — on a streak | **Angry** — long dry spell | **Sleep** — between epochs |
| ![lonely](faces/eink/lonely.png) | ![cool](faces/eink/cool.png) | ![grateful](faces/eink/grateful.png) |
| **Lonely** — nobody around | **Cool** — deauthing | **Grateful** — first-try capture |

See all 28 faces with trigger conditions: **[Bull Faces Reference →](../../wiki/Bull-Faces)**

The bull earns XP: +100 per handshake, +15 per association, +10 per deauth, +1 per epoch. Leveling is exponential — early levels fly by, but level 999 takes about a year of daily use. His mood responds to the RF environment: busy airwaves make him excited, diverse APs make him curious, dead silence makes him sad.

---

## Self-Healing

Stock pwnagotchi: firmware crashes, SSH drops, you're locked out for hours.

Oxigotchi: the daemon **never reboots the Pi**. Six recovery layers handle everything automatically:

1. **PSM watchdog reset** — prevents long-running firmware degradation
2. **Crash loop detection** — 3+ AO crashes triggers full driver recovery, not endless restarts
3. **AO watchdog** — exponential backoff restart (5s, 10s, 20s... up to 5 min)
4. **GPIO power cycle** — power-cycles the WiFi chip via hardware GPIO if SDIO bus errors
5. **Graceful give-up** — if all recovery fails, daemon gives up on WiFi gracefully
6. **USB lifeline** — SSH at `10.0.0.2` is always available, even when WiFi is completely dead

The web dashboard stays accessible no matter what. You will never be locked out.

---

## Hardware You Need

> **Pi Zero 2W only.** The firmware patches target the BCM43436B0 chip. Other Pi models have different chips and will not work.

| Component | Required? | Notes |
|---|---|---|
| **Raspberry Pi Zero 2W** | **YES** | Must be the Zero **2** W (not the original Zero W). |
| **microSD card (16GB+)** | **YES** | Class 10 or faster. 32GB recommended. |
| **Micro USB cable** | **YES** | For power and data (USB tethering). |
| **Waveshare 2.13" V4 e-ink display** | Recommended | Shows the bull faces. The "V4" matters — other versions have different drivers. |
| **PiSugar 3 battery** | Optional | Makes it portable. 3-4 hours of active scanning. The bull warns at 20% and 15%. |
| **3D-printed case** | Optional | Protects the stack. Many designs on Thingiverse. |

---

## Installation

1. **Download the image** from the [Releases](../../releases) section.
2. **Flash it** using [Raspberry Pi Imager](https://www.raspberrypi.com/software/) or [balenaEtcher](https://etcher.balena.io/).
3. **Insert the SD card** into your Pi Zero 2W.
4. **Windows users:** install the [USB gadget driver](https://github.com/jayofelony/pwnagotchi/releases) first. Mac/Linux don't need this.
5. **Plug in** the micro USB **data** port (center port, not the edge one).
6. **Wait about 30 seconds.** The bull appears. AngryOxide is scanning. You're live.

> **Default credentials** (change after first boot):
> - SSH: `ssh pi@10.0.0.2` — password `raspberry`
> - Web dashboard: `http://10.0.0.2:8080` — `changeme` / `changeme`
>
> Edit `/etc/oxigotchi/config.toml` to set your WiFi whitelist and WPA-SEC API key.

---

## Deep Dives

- **[WiFi Firmware RE](../../wiki/WiFi-Firmware)** — The 8-layer patch: DWT watchpoints, ROM interception, 6,965 functions mapped
- **[RF Classification](../../wiki/RF-Classification-Pipeline)** — Real-time 802.11 frame classification, 26x faster than bettercap
- **[Bluetooth Attacks](../../wiki/Bluetooth)** — RAGE/BT/SAFE modes, 6 BT attack types, patchram, HCI scanning
- **[Capture Pipeline](../../wiki/Capture-Pipeline)** — tmpfs staging, hashcat conversion, SD card protection
- **[Web Dashboard](../../wiki/Web-Dashboard)** — Live cards, REST API, mobile-friendly control panel
- **[Architecture](../../wiki/Architecture)** — Daemon design, epoch loop, crash recovery, module overview
- **[Building](../../wiki/Building)** — Cross-compile for aarch64, Pi sysroot, deployment
- **[Troubleshooting](../../wiki/Troubleshooting-and-FAQ)** — Common issues, safe apt upgrades, XP system, plugin authoring

---

## Maintenance

This project is provided **as-is**. It's stable, tested (480+ unit tests, overnight soak test, 28,000-frame stress test), and production-ready.

**I will not be maintaining this project actively.** No issue tracking, no PR reviews. The code is GPL-3.0 — fork it, modify it, make it yours.

The pwnagotchi community is active and helpful: [Discord](https://discord.gg/pwnagotchi) · [Reddit](https://www.reddit.com/r/pwnagotchi/) · [Forums](https://community.pwnagotchi.ai/)

The bull will take care of himself.

## Support

If Oxigotchi has been useful to you:

**BTC:** `bc1qnssffujsx5j2h7ep4wzyfa47azjlpwmaq8xtxk`

**ADA:** `addr1qymlyk49yaezevvm525ah6vey3sgah4clt83jmvcp60g5j25v6ukmh4628xn0hanrxwrae2j4huz3j36zt76ph40d44q703236`

No pressure — this is free and always will be. But firmware reverse-engineering takes a lot of coffee.

## Credits

- [**Pwnagotchi**](https://pwnagotchi.ai) — The original WiFi audit pet by evilsocket and the pwnagotchi community
- [**AngryOxide**](https://github.com/Ragnt/AngryOxide) — Rust-based 802.11 attack engine by Ragnt
- [**Nexmon**](https://nexmon.org) — Firmware patching framework by the Secure Mobile Networking Lab
- [**wpa-sec**](https://wpa-sec.stanev.org) — Free distributed WPA handshake cracking service
- **Pwnagotchi plugin authors** — The Lua plugins in Oxigotchi were rebuilt from ideas pioneered by the pwnagotchi community: exp (XP/leveling), memtemp (system stats), bt-tether (Bluetooth), age (uptime), and many others. The original plugin ecosystem made pwnagotchi what it is.

## Legal

This tool is intended for **authorized security testing, educational use, and research only**. Only use Oxigotchi on networks and devices you own or have explicit written permission to test. Unauthorized interception of network traffic is illegal in most jurisdictions.

The WiFi firmware reverse engineering was performed for interoperability purposes under applicable law. The firmware binary on the SD image is a patched version of Broadcom's BCM43436B0 firmware that ships with every Pi Zero 2W. No Broadcom source code is included in this repository.

## License

[GNU General Public License v3.0](LICENSE)
