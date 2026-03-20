# Oxigotchi Deep Analysis

**Date:** 2026-03-20 (data collected from Pi at 2026-03-15 20:52 EET)
**Uptime at collection:** 8 minutes since last boot
**Host:** oxigotchi (Pi Zero 2W, aarch64)
**OS:** Debian 13 (trixie), kernel 6.12.62+rpt-rpi-v8
**Firmware:** BCM43430/2 nexmon 2.2.2-552-gb8c6-2 (Jan 12 2025)
**AngryOxide:** 0.9.1
**Pwnagotchi:** v2.9.5.4

---

## Current State

### What is Working

| Component | Status | Notes |
|-----------|--------|-------|
| Pwnagotchi core | Running | v2.9.5.4, auto mode, web UI on :8080 |
| AngryOxide | Running | PID 942, rate 1, dwell 2, headless mode |
| wlan0mon | UP | Monitor mode active, promiscuous mode |
| wlan-keepalive | Running | 1493 frames after 8 min, probes every 3s |
| Bettercap | Running | API on :8081, channel scanning operational |
| PiSugar3 | Running | Battery 79.2%, web UI on :8421, TCP on :8423 |
| USB gadget (RNDIS) | Working | usb0 UP, 10.0.0.2/24 + 192.168.137.2/24 |
| Emergency SSH | Running | Port 22, password auth enabled |
| WiFi recovery | Operational | wlan0 appeared after 4s, no recovery needed |
| Bluetooth agent | Running | Discoverable, pairable (recovered after 1 retry) |
| NetworkManager | Running | DNS via 192.168.137.1 |
| Handshake capture | Working | 17 captures in /etc/pwnagotchi/handshakes/ (101MB) |
| quickdic cracking | Working | Auto-cracking new captures in background |
| E-ink display | Working | waveshare v2in13_V4, rotation 180, init in ~5s |
| zram mounts | Working | /etc/pwnagotchi/log (90M), /var/tmp/pwnagotchi (15M) |
| Boot diagnostics | Working | bootlog.service logging to /boot/firmware/bootlog.txt |
| NM watchdog timer | Working | Runs every ~3 min, ensures usb0 IP persists |
| PiSugar watchdog | Working | Runs every ~15s |
| Buffer cleaner | Working | Runs every ~5 min |
| BT keepalive timer | Working | Runs every ~30s |

### What is Not Working

| Component | Status | Impact |
|-----------|--------|--------|
| oxigotchi-splash.service | FAILED | No boot splash on e-ink ("GPIO busy" error) |
| resize-rootfs.service | FAILED | Harmless (partition already at full size) |
| Internet connectivity | None | No default gateway, no outbound access |
| ssh.service (stock) | Dead | Replaced by emergency-ssh (intentional) |
| epd-startup.service | Inactive | Completed successfully on previous boot, failed on one |
| BT-Tether | Error | "Error with mac address" (bt-tether is disabled, but plugin still loads and errors) |
| Peer updates | Error | `'Array' object has no attribute 'read'` in Session Fetcher |
| WPA-SEC uploads | Disabled | API key not set ("API-KEY isn't set. Can't upload.") |
| watchdog.service | Missing | No hardware watchdog configured |

### Resource Utilization

| Resource | Value | Assessment |
|----------|-------|------------|
| **RAM** | 261MB / 464MB used (56%) | Moderate - 202MB available |
| **Swap** | 0B / 100MB used | Good - no swap pressure |
| **CPU** | Load avg 1.32, 1.15, 0.62 | Elevated (4 cores) |
| **Temperature** | 59.1C | Normal (throttle at 80C) |
| **Throttling** | 0x0 | None - no undervoltage or thermal throttle |
| **Disk** | 5.6GB / 58GB used (10%) | Excellent headroom |
| **Boot partition** | 66MB / 510MB (13%) | Fine |

**Top memory consumers:**
1. pwnagotchi (Python) - 92MB RSS (19.3%)
2. bettercap - 52MB RSS (11.0%)
3. pwngrid - 26MB RSS (5.5%)
4. NetworkManager - 19MB RSS (4.0%)
5. angryoxide - 15MB RSS (3.1%)

**Top CPU consumers:**
1. angryoxide - 34.2% sustained (expected for continuous WiFi injection)
2. pwnagotchi - 8.4%
3. bettercap - 2.9%
4. pwngrid - 2.4%

### Stability Assessment

**Overall: GOOD with caveats**

WiFi stability appears solid with the wlan-keepalive service active. The BCM43430 SDIO bus has not crashed in this session. AngryOxide is capturing handshakes successfully. The brcmfmac driver loaded cleanly on second attempt (first load/unload cycle is intentional for monitor mode setup). No OOM events, no kernel panics, no thermal throttling.

The main concerns are: (1) the recurring `update_peers` error suggesting a pwngrid compatibility issue, (2) the persistent blind epochs indicating bettercap's wifi.recon may not be feeding AP data to the AI properly, and (3) the capture filename prefix is just a dash (missing hostname/identifier).

---

## Issues Found

### CRITICAL

*None*

### HIGH

#### H1: Persistent Blind Epochs (AI learning degraded)

**Evidence:** Epochs 1-10 show `blind=1` through `blind=10` with monotonically increasing blind count. Only epoch 2 saw any active APs (active=1), and only 1 handshake was captured by the pwnagotchi epoch tracker despite angryoxide capturing 17 pcapng files.

**Analysis:** The blind epoch counter never resets. Bettercap's wifi.recon is running and scanning channels, but the pwnagotchi AI is not seeing the APs that angryoxide is finding. This is because in AO mode, bettercap's deauth/assoc are disabled, and angryoxide operates independently. The AI's reward function is mostly negative (-0.2 to -0.42), meaning it's "sad" about poor performance even though angryoxide is working fine.

**Impact:** The reinforcement learning model is being trained on misleading data. It thinks it's performing poorly when captures are actually happening. Over time this degrades the AI's decision-making.

**Root cause:** Architectural disconnect between angryoxide (which does the actual work) and bettercap (which feeds the AI). When AO handles attacks, bettercap doesn't see the deauths/assocs/handshakes.

#### H2: Session Fetcher Error (peer discovery broken)

**Evidence:** `[agent:_fetch_stats] self.update_peers: AttributeError("'Array' object has no attribute 'read'")`

**Analysis:** This error fires on every epoch. It's a Python-level bug in the pwnagotchi agent where `update_peers` receives an Array object but tries to call `.read()` on it. Likely a bettercap API response format change or pwngrid incompatibility.

**Impact:** Peer discovery and grid interaction are broken. The device cannot bond with other pwnagotchis.

#### H3: AngryOxide Capture Filenames Missing Network Name

**Evidence:** All capture files are named `-2026-03-15_*.pcapng` (note leading dash, no SSID/BSSID prefix). Example: `-2026-03-15_17-12-14.pcapng`

**Analysis:** AngryOxide 0.9.1 names captures with the target network prefix, but here all files start with just a dash, meaning the network identifier is empty. This makes it impossible to tell which capture belongs to which network without parsing the pcapng.

**Impact:** Manual review of captures is difficult. Automated tools that parse filenames for SSID/BSSID won't work.

#### H4: Web UI Uses Default Credentials with No Auth

**Evidence:** `auth = false`, `username = "changeme"`, `password = "changeme"` in config.toml.

**Analysis:** The web UI on port 8080 is accessible without authentication. Anyone on the USB network (or if Bluetooth tethering is enabled later) can access the full pwnagotchi web interface.

**Impact:** Low risk currently (only accessible via USB), but becomes a security issue if network connectivity is added.

### MEDIUM

#### M1: oxigotchi-splash.service Fails with "GPIO busy"

**Evidence:** `python3[462]: Error: 'GPIO busy'` then service fails.

**Analysis:** The splash service tries to use GPIO pins for the e-ink display, but another service (likely epd-startup.service, which starts at the same target) has already claimed them. The services race for GPIO access during early boot.

**Impact:** No custom boot splash displayed. The e-ink shows whatever was last on it until pwnagotchi fully starts (~47s).

**Fix:** Add `After=epd-startup.service` to oxigotchi-splash.service, or merge the two services into one.

#### M2: resize-rootfs.service Fails Every Boot

**Evidence:** `NOCHANGE: partition 2 is size 123670495. it cannot be grown`

**Analysis:** The partition is already at full size (58GB), but the service runs every boot because `/var/lib/.rootfs-expanded` was never created (growpart returns non-zero when there's nothing to grow, so the `&&` chain fails before `touch`).

**Impact:** Wastes ~1.7s on every boot and shows as a failed service in systemd.

**Fix:** Either create `/var/lib/.rootfs-expanded` manually, or change the script to handle the "already expanded" case.

#### M3: BT-Tether Plugin Loads Despite Being Disabled

**Evidence:** `[BT-Tether] Error with mac address` appears in logs even though `bt-tether.enabled = false`.

**Analysis:** The plugin loads and initializes even when disabled in config, throwing an error because no MAC address is configured. This is a pwnagotchi plugin loader issue - it loads all plugins then checks enabled state.

**Impact:** Log noise. Wasted CPU cycles on initialization.

#### M4: Bettercap Running Redundantly in AO Mode

**Evidence:** Bettercap is active, consuming 52MB RSS (11% of RAM), but AO mode disables its deauth/assoc. It only does wifi.recon (channel scanning).

**Analysis:** In the angryoxide-v5.toml overlay, `[bettercap] disabled = true` is set, but the overlay is in `/etc/pwnagotchi/` not in `/etc/pwnagotchi/conf.d/`. The main config.toml does not have `disabled = true` for bettercap. Bettercap still runs because the overlay may not be applied.

**Impact:** 52MB of RAM wasted on a service whose attack functions are disabled. On a 464MB system, that's 11% of total RAM.

#### M5: No Default Gateway / No Internet Access

**Evidence:** `ip route` shows only two local subnets (10.0.0.0/24 and 192.168.137.0/24), no default route. `ping 8.8.8.8` fails.

**Analysis:** The Pi relies on the host PC for internet sharing. Currently, NAT/ICS may not be configured on the host, or the Pi doesn't have a default route set.

**Impact:** Cannot upload handshakes to wpa-sec, cannot update plugins, cannot sync with grid. The `grid` plugin is enabled but can't reach the server.

#### M6: Duplicate Handshake Directories

**Evidence:** `/etc/pwnagotchi/handshakes/` has 17 files (101MB), `/home/pi/handshakes/` has 16 files (78MB). Config says `handshakes will be collected inside /home/pi/handshakes` (bettercap), but angryoxide outputs to `/etc/pwnagotchi/handshakes/`.

**Analysis:** There are two handshake directories:
- `bettercap.handshakes = "/home/pi/handshakes"` (bettercap's output)
- angryoxide `--output /etc/pwnagotchi/handshakes/` (AO's output)

Files are being copied/symlinked between them, but the count differs (17 vs 16), indicating a sync issue.

**Impact:** Disk space wasted by duplicates (~179MB total). Confusion about which directory is authoritative.

#### M7: VCHI Service Initialization Failures

**Evidence:** `vc_sm_cma_vchi_init: failed to open VCHI service (-22)` and `bcm2835_mmal_vchiq: Failed to open VCHI service connection (status=-22)` (3 times).

**Analysis:** These are VideoCore hardware interface errors. The camera/video subsystem can't initialize because the Pi Zero 2W doesn't have these features enabled or the required kernel modules conflict. These are from staging drivers that load automatically.

**Impact:** Cosmetic - no camera functionality is needed. Adds noise to kernel log and journalctl errors.

#### M8: wlan-keepalive.service File Permissions Warning

**Evidence:** `Configuration file /etc/systemd/system/wlan-keepalive.service is marked executable. Please remove executable permission bits.`

**Analysis:** The service file was SCP'd and retains execute permissions.

**Impact:** Cosmetic warning in journal. systemd still processes it correctly.

### LOW

#### L1: fix-ndev.service Has Escaped Characters Warning

**Evidence:** `Ignoring unknown escape sequences` in systemd journal for the ExecStart line.

**Analysis:** The inline bash script in the service file contains `$` characters that systemd interprets as escape sequences. The script still works because systemd ignores the unrecognized escapes.

**Impact:** Cosmetic journal noise. No functional impact.

#### L2: `iwlist` Wireless Extensions Deprecation

**Evidence:** `warning: 'iwlist' uses wireless extensions which will stop working for Wi-Fi 7 hardware; use nl80211`

**Analysis:** Something calls `iwlist` during boot. This is deprecated in favor of `iw` (nl80211).

**Impact:** No functional impact on BCM43430 (WiFi 4), but should migrate to `iw` for future-proofing.

#### L3: ui.fps Set to 0

**Evidence:** `ui.fps is 0, the display will only update for major changes`

**Analysis:** This is intentional for e-ink to avoid unnecessary refreshes, but it means status updates are delayed.

**Impact:** Minor UX issue - display updates are sluggish.

#### L4: Emergency SSH Allows Root Login with Password

**Evidence:** `sshd -D -p 22 -o PasswordAuthentication=yes -o PermitRootLogin=yes`

**Analysis:** Emergency SSH service overrides sshd_config to allow root login with password. This is by design for recovery, but is a security concern on shared networks.

**Impact:** Low risk on USB-only access, but should be hardened if BT-tether or other network access is enabled.

#### L5: PiSugar Web UI Exposed on All Interfaces

**Evidence:** PiSugar server listens on `0.0.0.0:8421` (HTTP), `:8422` (WS), `:8423` (TCP).

**Analysis:** The PiSugar web UI and API are accessible from any network interface without authentication.

**Impact:** Anyone on the network can read battery status, change power settings, or trigger shutdown.

#### L6: One Zombie Process Reported by `top` but Not Found

**Evidence:** `top` shows "1 zombie" in the task summary, but `ps aux | awk '$8 ~ /Z/'` finds 0 zombies.

**Analysis:** Race condition - the zombie was reaped between the two checks. Likely a short-lived child process from quickdic cracking or bettercap.

**Impact:** Transient. No action needed unless it recurs persistently.

---

## Improvements

### Performance Optimizations

1. **Eliminate bettercap in pure AO mode** - Save 52MB RAM by not starting bettercap when angryoxide handles all attacks. This requires the AO plugin to handle channel scanning and AP discovery itself, or a lightweight replacement. Alternatively, apply the `angryoxide-v5.toml` overlay properly to `/etc/pwnagotchi/conf.d/` so `bettercap.disabled = true` takes effect.

2. **Reduce angryoxide CPU usage** - angryoxide at 34% CPU is high for a Pi Zero 2W. Consider:
   - Increase `--dwell` from 2 to 5 (fewer channel hops)
   - The `--rate 1` is already the minimum; do not lower it
   - Evaluate if continuous scanning is needed or if duty cycling would suffice

3. **Disable unused kernel modules** - Blacklist `snd_bcm2835`, `vc_sm_cma`, `bcm2835_mmal_vchiq`, `bcm2835_isp`, `bcm2835_v4l2`, `bcm2835_codec` to save RAM and eliminate VCHI errors. Add to `/etc/modprobe.d/blacklist-camera.conf`.

4. **Consolidate handshake directories** - Use a single directory for captures. Either symlink `/home/pi/handshakes` to `/etc/pwnagotchi/handshakes/` or change the angryoxide output path.

### Stability Improvements

5. **Fix the GPIO race for boot splash** - Add ordering dependency between epd-startup and oxigotchi-splash services, or combine them.

6. **Fix resize-rootfs to stop failing** - Create `/var/lib/.rootfs-expanded` sentinel file, or change the script to: `growpart /dev/mmcblk0 2 2>/dev/null; resize2fs /dev/mmcblk0p2 2>/dev/null; touch /var/lib/.rootfs-expanded; exit 0`

7. **Add hardware watchdog** - Enable the BCM2835 watchdog (`bcm2835_wdt` module) with a systemd watchdog or `watchdog` daemon. This auto-reboots on kernel hangs.

8. **Fix the `update_peers` AttributeError** - Investigate the bettercap API response format. The pwngrid peer list may return a different type than expected. This is likely a version mismatch between pwnagotchi and bettercap/pwngrid.

9. **Fix wlan-keepalive file permissions** - `chmod -x /etc/systemd/system/wlan-keepalive.service`

### Security Hardening

10. **Set web UI authentication** - Change `auth = true` and set non-default username/password in config.toml.

11. **Restrict emergency-ssh** - Remove `-o PermitRootLogin=yes` or at least change to `prohibit-password`. Bind to USB interface only.

12. **Bind PiSugar to localhost** - Change pisugar-server to listen on `127.0.0.1` instead of `0.0.0.0`, or add firewall rules.

13. **Add nftables/iptables rules** - Restrict inbound access to SSH (22) and pwnagotchi web (8080) on usb0 only. Block all inbound on other interfaces.

### UI/UX Improvements

14. **Fix blind epoch feedback loop** - When AO mode is active, the pwnagotchi AI should receive handshake data from angryoxide captures, not from bettercap. The angryoxide plugin should feed capture events into the epoch tracker to give the AI accurate reward signals.

15. **Fix capture filenames** - Investigate why angryoxide produces filenames with empty prefix (just a dash). May need to pass `--name oxigotchi` or similar flag, or it may be an angryoxide 0.9.1 bug.

16. **Suppress BT-Tether errors when disabled** - Either fix the plugin to not load when disabled, or configure a dummy MAC to silence the error.

### Missing Features

17. **No internet/gateway configuration** - Add default route via USB host for plugin updates, wpa-sec uploads, and grid sync. Could be done in usb0-fallback.sh or nm-watchdog.

18. **No WPA-SEC integration** - API key is blank. When internet is available, this should be configured for automated handshake cracking.

19. **No hardware watchdog** - System has no recovery mechanism for kernel-level hangs.

20. **No automatic capture cleanup** - Captures accumulate indefinitely (currently 101MB after one session). A rotation/archival policy is needed.

### Boot Time Optimization

**Current total boot: 1 min 5.4s** (18.2s kernel + 47.2s userspace)

Top offenders:
| Service | Time | Action |
|---------|------|--------|
| usb0-fallback.service | 30.5s | Likely waiting for DHCP/link. Add timeout or parallelize |
| pwngrid-peer.service | 30.1s | Likely waiting for network. Can these two overlap? |
| fix-ndev.service | 10.6s | Loops waiting for wlan0 (up to 15 iterations x 1s) |
| epd-startup.service | 10.3s | E-ink refresh is slow by nature, hard to optimize |
| bt-agent.service | 7.5s | Includes 1 failure + restart. Fix the race to save ~5s |
| NetworkManager | 5.6s | Standard, hard to reduce |
| wifi-recovery.service | 5.2s | Waits for wlan0 (4s). Could share with fix-ndev |
| bootlog.service | 4.7s | Diagnostic collection. Move to background/async |
| oxigotchi-splash.service | 4.2s | Currently failing (GPIO busy). Fix would add 4s of useful boot splash |

**Potential savings:** ~30-40s by:
- Merging fix-ndev and wifi-recovery into a single service (saves ~5s overlap)
- Making usb0-fallback non-blocking with a shorter timeout
- Fixing bt-agent race condition (saves ~5s)
- Running bootlog.service asynchronously (saves ~4.7s)
- Making pwngrid-peer start independently of network readiness

---

## Next Steps

### Priority 1 (Quick Wins)

- [ ] Create `/var/lib/.rootfs-expanded` to silence resize-rootfs failure
- [ ] `chmod -x /etc/systemd/system/wlan-keepalive.service` to fix permissions warning
- [ ] Set `auth = true` with custom credentials in config.toml `[ui.web]` section
- [ ] Disable BT-Tether plugin from loading (or add dummy MAC to silence error)

### Priority 2 (Functional Fixes)

- [ ] Fix oxigotchi-splash GPIO race (add After=epd-startup.service)
- [ ] Investigate and fix the `update_peers` AttributeError
- [ ] Fix capture filename prefix (empty SSID/BSSID in angryoxide output)
- [ ] Consolidate handshake directories (single source of truth)
- [ ] Add default route via USB host for internet access

### Priority 3 (Optimization)

- [ ] Evaluate disabling bettercap in AO mode to reclaim 52MB RAM
- [ ] Blacklist unused camera/video kernel modules to save RAM
- [ ] Optimize boot time (merge wifi services, fix bt-agent race, async bootlog)
- [ ] Fix blind epoch counter to reflect angryoxide capture data
- [ ] Add capture rotation/cleanup policy

### Priority 4 (Hardening)

- [ ] Enable BCM2835 hardware watchdog
- [ ] Restrict emergency-ssh PermitRootLogin
- [ ] Bind PiSugar server to localhost
- [ ] Add nftables firewall rules
- [ ] Configure WPA-SEC API key when internet is available
