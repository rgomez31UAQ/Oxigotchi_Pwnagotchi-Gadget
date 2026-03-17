# Oxigotchi E-Ink Display Specification

**Hardware:** Waveshare 2.13" V4 (250×122 pixels, 1-bit, partial refresh)
**Orientation:** Landscape, 250px wide × 122px tall
**Colors:** Black (0xFF rendered) on white (0x00 rendered) — inverted internally

---

## Display Layout — Shared Elements

Both modes share the same hardware layout grid:

```
┌──────────────────────────────────────────────────────────┐
│ CH 00    APS 0 (00)                         UP 00:00:00  │  ← Top bar (Y=0..13)
├──────────────────────────────────────────────────────────┤  ← line1 (Y=14)
│                                                          │
│  [NAME]              [STATUS TEXT]                        │  ← Y=20 zone
│                                                          │
│  [FACE]                                                  │  ← Y=16 or Y=34 zone
│                                                          │
│                                                          │
│                                                          │
│  [FRIEND FACE + NAME]                                    │  ← Y=92..94
├──────────────────────────────────────────────────────────┤  ← line2 (Y=108)
│ PWND 0 (00)                                        AUTO  │  ← Bottom bar (Y=109+)
└──────────────────────────────────────────────────────────┘
```

### Element Positions (Waveshare 2.13" V4)

| Element | Key | Position (x, y) | Font | Content | Mode |
|---------|-----|-----------------|------|---------|------|
| Channel | `channel` | (0, 0) | Bold 10pt label + Medium 10pt value | "CH 00" | Both |
| APs | `aps` | (28, 0) | Bold 10pt label + Medium 10pt value | "APS 0 (00)" | Both |
| Bluetooth | `bluetooth` | (115, 0) | Bold 10pt label + Medium 10pt value | "BT C" / "BT -" | Both (bt-tether plugin) |
| Battery | `bat` | (140, 0) | Bold 10pt label + Medium 10pt value | "BAT 85%" | Both (pisugarx plugin) |
| Uptime | `uptime` | (185, 0) | Bold 10pt label + Medium 10pt value | "UP HH:MM:SS" | Both |
| Line 1 | — | (0, 14) → (250, 14) | — | Horizontal divider, 1px | Both |
| Name | `name` | (5, 20) | Bold 10pt | "Pwnagotchi> █" / empty | PWN only |
| Status | `status` | (125, 20) | Medium custom (status_font) | Wrapping text, max 20 chars/line | Both |
| Face | `face` | (0, 16) AO / (0, 34) PWN | Huge 35pt text / PNG paste | Bull PNG 120×66 / Korean text | Both |
| WalkBy | `walkby_status` | (0, 82) | Small 9pt | "BLITZ 5atk 2cap" / empty | PWN only |
| AO Status | `angryoxide` | (0, 85) | Small 9pt label + Small 9pt value | "AO: 5 \| 01:23" / empty | AO only |
| Friend face | `friend_face` | (0, 92) | Bold 10pt | Peer's face text | Both (hidden if no peer) |
| Friend name | `friend_name` | (40, 94) | BoldSmall 9pt | "▌▌▌│ buddy 3 (15) of 4" | Both (hidden if no peer) |
| Line 2 | — | (0, 108) → (250, 108) | — | Horizontal divider, 1px | Both |
| Handshakes | `shakes` | (0, 109) | Bold 10pt label + Medium 10pt value | "PWND 0 (00)" | Both |
| Mode | `mode` | (225, 109) | Bold 10pt | "AUTO" or "MANU" | Both |

### Font Sizes (Waveshare V4 override)

```
fonts.setup(10, 9, 10, 35, 25, 9)
         Bold BoldSmall Medium Huge BoldBig Small
```

- **Huge** (35pt Bold): Face text in PWN mode
- **Bold** (10pt Bold): Name, labels, mode indicator
- **Medium** (10pt): Values, channel, APS, uptime
- **BoldSmall** (9pt Bold): Friend name
- **Small** (9pt): Plugin status elements (walkby, AO capture count)

---

## AO Mode (AngryOxide)

**Activated by:** `bettercap.disabled = true` in config overlay (`angryoxide-v5.toml`)
**Switched via:** `pwnoxide-mode ao`

### Boot Sequence

| Time | Display State | What Happens |
|------|--------------|--------------|
| T=0 | **Black screen** | Pi powers on, kernel loading |
| T=5-8s | **Bull AWAKE face (centered, full refresh)** | `oxigotchi-splash.service` runs before pwnagotchi. Renders `awake.png` centered on display via full EPD refresh. Writes to both RAM banks so image persists through partial refreshes. |
| T=8-11s | Bull face persists | `pwnagotchi-splash-delay.conf` adds 3s `ExecStartPre=/bin/sleep 3` before pwnagotchi starts. Splash stays visible. |
| T=11-15s | **Bull SLEEP face + "Initializing..."** | Pwnagotchi starts, `view.on_starting()` sets SLEEP face + version text. EPD partial refresh begins. |
| T=15-45s | **Bull SLEEP face + "Reading logs..."** | `LastSession.parse()` runs (or loads from cache in ~1s). Face = SLEEP or SMART. |
| T=45-60s | **Bull AWAKE face + "Ready"** | Monitor mode up, AO started, first epoch begins. |

### Steady State Display

```
┌──────────────────────────────────────────────────────────┐
│ CH *     APS 3 (12)                         UP 01:23:45  │
├──────────────────────────────────────────────────────────┤
│                      Sniffing around...                   │
│  ┌────────────┐                                          │
│  │            │                                          │
│  │  BULL PNG  │                                          │
│  │  (120×66)  │                                          │
│  │            │                                          │
│  └────────────┘                                          │
│                                                          │
├──────────────────────────────────────────────────────────┤
│ PWND 5 (23)                                        AUTO  │
└──────────────────────────────────────────────────────────┘
```

**Key differences from PWN mode:**
- **No name label** — the `name` element is empty (`''`), nothing renders at (5, 20)
- **No cursor blink** — cursor animation disabled
- **Face at Y=16** — 2px below line1, almost touching the top bar. Bull PNG gets more vertical space.
- **Face is PNG** — 120×66 pixel 1-bit bull head, rendered at (0, 16) via `canvas.paste()`
- **AO capture count** — plugin adds a LabeledValue showing capture count (bottom area)

### Face → Mood Mapping (AO Mode)

All faces are bull head PNGs at `/etc/pwnagotchi/custom-plugins/faces/`:

| Event | view.py method | Face PNG | Trigger |
|-------|---------------|----------|---------|
| Starting | `on_starting()` | `awake.png` | Boot, initialization |
| Keys generation | `on_keys_generation()` | `awake.png` | Generating mesh identity keys |
| Normal/Idle | `on_normal()` | `awake.png` | Default idle state after sleep cycle |
| Sleeping | `wait(sleeping=True)` | `sleep.png` | Between recon cycles (napping) |
| Looking (good mood) | `wait(sleeping=False)` | `look_r_happy.png` / `look_l_happy.png` | Waiting, alternating L/R every step |
| Looking (neutral) | `wait(sleeping=False)` | `look_r.png` / `look_l.png` | Waiting, alternating L/R every step |
| Association | `on_assoc(ap)` | `intense.png` | Sending PMKID assoc frame |
| Deauth | `on_deauth(sta)` | `cool.png` | Sending deauth frame |
| Missed target | `on_miss(who)` | `sad.png` | Target AP/STA no longer in range |
| Handshake captured | `on_handshakes(n)` | `happy.png` | New handshake file detected |
| New peer (first meet) | `on_new_peer(peer)` | `awake.png` or `cool.png` | First encounter with mesh peer |
| New peer (good friend) | `on_new_peer(peer)` | `motivated.png` / `friend.png` / `happy.png` | Known peer with high bond factor |
| New peer (normal) | `on_new_peer(peer)` | `excited.png` / `happy.png` / `smart.png` | Repeat peer, normal bond |
| Lost peer | `on_lost_peer(peer)` | `lonely.png` | Mesh peer out of range |
| Free channel | `on_free_channel(ch)` | `smart.png` | Empty channel found during recon |
| Reading logs | `on_reading_logs(n)` | `smart.png` | Parsing last session log file |
| Bored | `on_bored()` | `bored.png` | No activity for bored_num_epochs (default 15) |
| Sad | `on_sad()` | `sad.png` | No activity for sad_num_epochs (default 25) |
| Angry | `on_angry()` | `angry.png` | Extended inactivity + no friends nearby |
| Motivated | `on_motivated(r)` | `motivated.png` | Positive reward trend |
| Demotivated | `on_demotivated(r)` | `demotivated.png` | Negative reward trend |
| Excited | `on_excited()` | `excited.png` | Sustained activity for excited_num_epochs (default 10) |
| Grateful | `on_grateful()` | `grateful.png` | Would be sad/bored but has good friends nearby |
| Smart | (via bored/free_ch) | `smart.png` | Reading logs, free channel found |
| Lonely | `on_lonely()` | `lonely.png` | No peers + no support network |
| Unread messages | `on_unread_messages()` | `excited.png` | Unread mesh messages (5s display) |
| Uploading | `on_uploading(to)` | `upload.png` | Uploading captures to wpa-sec |
| Rebooting | `on_rebooting()` | `broken.png` | System reboot triggered |
| Custom/Debug | `on_custom(text)` | `debug.png` | Plugin-triggered custom message |
| Shutdown | `on_shutdown()` | `sleep.png` | Graceful shutdown (display frozen after) |
| Manual mode (good) | `on_manual_mode()` | `happy.png` | MANU mode, last session had handshakes |
| Manual mode (bad) | `on_manual_mode()` | `sad.png` | MANU mode, >3 epochs + 0 handshakes |
| FW crash | (AO plugin) | `fw_crash.png` | Firmware crash detected in journalctl |
| AO crashed | (AO plugin) | `ao_crashed.png` | AO process exited unexpectedly |
| Battery low | (AO plugin) | `battery_low.png` | PiSugar < 20% |
| Battery critical | (AO plugin) | `battery_critical.png` | PiSugar < 5% |
| WiFi down | (AO plugin) | `wifi_down.png` | Monitor interface missing from sysfs |

### Bull Face PNG Specs

- **Size:** 120×66 pixels
- **Mode:** 1-bit grayscale (black and white only)
- **Format:** PNG, non-interlaced
- **Background:** White (transparent areas converted to white)
- **Rendering:** `Image.open()` → RGBA → alpha→white → colorize if inverted → convert to '1' → `canvas.paste()` at (0, 16)
- **28 faces total** covering all mood states + diagnostic states

### Shutdown Sequence (AO Mode)

| Time | Display State |
|------|--------------|
| T=0 | **Bull SLEEP face + "Zzz..."** | `view.on_shutdown()` called, display frozen |
| T=0-5s | Display frozen (no more updates) | Pwnagotchi stopping |
| T=5s | **Bull SHUTDOWN face (centered, full refresh)** | `oxigotchi-splash.service` ExecStop renders `shutdown.png` |
| T=5-10s | Shutdown face persists | System powering off, display retains last image |

---

## PWN Mode (Pwnagotchi / Bettercap)

**Activated by:** removing config overlay (no `bettercap.disabled` key)
**Switched via:** `pwnoxide-mode pwn`

### Boot Sequence

| Time | Display State | What Happens |
|------|--------------|--------------|
| T=0 | **Black screen** | Pi powers on, kernel loading |
| T=5-8s | **Nothing** | `oxigotchi-splash.service` detects no AO overlay → exits immediately. No splash shown. |
| T=8-15s | **Korean SLEEP face + "Pwnagotchi>" + "Initializing..."** | Pwnagotchi starts, `view.on_starting()`. EPD Clear() → fresh white canvas → partial refresh begins. |
| T=15-45s | **Korean SLEEP/SMART face + "Reading logs..."** | `LastSession.parse()` runs. |
| T=45-60s | **Korean AWAKE face + "Pwnagotchi>" + "Ready"** | Bettercap API ready, monitor mode up, first epoch. |

### Steady State Display

```
┌──────────────────────────────────────────────────────────┐
│ CH 06    APS 5 (18)                         UP 00:45:12  │
├──────────────────────────────────────────────────────────┤
│  Pwnagotchi> █        Sniffing around...                 │
│                                                          │
│  (◕‿‿◕)                                                │
│                                                          │
│                                                          │
│                                                          │
│  ▌▌▌│ buddy 3 (15)                                      │
├──────────────────────────────────────────────────────────┤
│ PWND 3 (18)                                        AUTO  │
└──────────────────────────────────────────────────────────┘
```

**Key differences from AO mode:**
- **Name label visible** — "Pwnagotchi>" at (5, 20), Bold 10pt, with blinking cursor (█)
- **Cursor blinks** — `_refresh_handler` toggles "█" suffix on name at ui.fps rate
- **Face at Y=34** — below the name, leaving 2px gap (name ends ~Y=32)
- **Face is Korean text** — Unicode emoticons rendered with Huge font (35pt Bold DejaVuSansMono)
- **PNG mode OFF** — `ui.faces.png = false`, all face values are strings like `(◕‿‿◕)`
- **No AO plugin UI elements** — no capture count, no AO status

### Face → Mood Mapping (PWN Mode)

All faces are Korean Unicode text rendered with Huge 35pt font:

| Event | view.py method | Face Text | Variants |
|-------|---------------|-----------|----------|
| Starting | `on_starting()` | `(◕‿‿◕)` | AWAKE |
| Keys generation | `on_keys_generation()` | `(◕‿‿◕)` | AWAKE |
| Normal/Idle | `on_normal()` | `(◕‿‿◕)` | AWAKE |
| Sleeping | `wait(sleeping=True)` | `(⇀‿‿↼)` | `(≖‿‿≖)`, `(－_－)` |
| Looking R | `wait(sleeping=False)` | `( ⚆_⚆)` | Neutral mood, even steps |
| Looking L | `wait(sleeping=False)` | `(☉_☉ )` | Neutral mood, odd steps |
| Looking R happy | `wait(sleeping=False)` | `( ◕‿◕)` | `( ≧◡≦)` — good mood, even steps |
| Looking L happy | `wait(sleeping=False)` | `(◕‿◕ )` | `(≧◡≦ )` — good mood, odd steps |
| Association | `on_assoc(ap)` | `(°▃▃°)` | `(°ロ°)` — INTENSE |
| Deauth | `on_deauth(sta)` | `(⌐■_■)` | COOL |
| Missed target | `on_miss(who)` | `(╥☁╥ )` | `(╥﹏╥)`, `(ಥ﹏ಥ)` — SAD |
| Handshake | `on_handshakes(n)` | `(•‿‿•)` | `(^‿‿^)`, `(^◡◡^)` — HAPPY |
| New peer (first) | `on_new_peer(peer)` | AWAKE or COOL | Random choice |
| New peer (friend) | `on_new_peer(peer)` | MOTIVATED/FRIEND/HAPPY | Random choice |
| New peer (normal) | `on_new_peer(peer)` | EXCITED/HAPPY/SMART | Random choice |
| Lost peer | `on_lost_peer(peer)` | `(ب__ب)` | `(｡•́︿•̀｡)`, `(︶︹︺)` — LONELY |
| Free channel | `on_free_channel(ch)` | `(✜‿‿✜)` | SMART |
| Reading logs | `on_reading_logs(n)` | `(✜‿‿✜)` | SMART |
| Bored | `on_bored()` | `(-__-)` | `(—__—)` |
| Sad | `on_sad()` | `(╥☁╥ )` | `(╥﹏╥)`, `(ಥ﹏ಥ)` |
| Angry | `on_angry()` | `(-_-')` | `(⇀__⇀)`, `` (`___´) `` |
| Motivated | `on_motivated(r)` | `(☼‿‿☼)` | `(★‿★)`, `(•̀ᴗ•́)` |
| Demotivated | `on_demotivated(r)` | `(≖__≖)` | `(￣ヘ￣)`, `(¬､¬)` |
| Excited | `on_excited()` | `(ᵔ◡◡ᵔ)` | `(✜‿‿✜)` |
| Grateful | `on_grateful()` | `(^‿‿^)` | |
| Smart | (via events above) | `(✜‿‿✜)` | |
| Lonely | `on_lonely()` | `(ب__ب)` | `(｡•́︿•̀｡)`, `(︶︹︺)` |
| Friend | (via on_new_peer) | `(♥‿‿♥)` | `(♡‿‿♡)`, `(♥‿♥ )`, `(♥ω♥ )` |
| Unread messages | `on_unread_messages()` | `(ᵔ◡◡ᵔ)` | EXCITED (displayed 5s) |
| Uploading | `on_uploading(to)` | `(1__0)` | `(1__1)`, `(0__1)` |
| Rebooting | `on_rebooting()` | `(☓‿‿☓)` | BROKEN |
| Custom/Debug | `on_custom(text)` | `(#__#)` | DEBUG |
| Shutdown | `on_shutdown()` | `(⇀‿‿↼)` | SLEEP (display frozen after) |
| Manual mode (good) | `on_manual_mode()` | `(•‿‿•)` | HAPPY — had handshakes |
| Manual mode (bad) | `on_manual_mode()` | `(╥☁╥ )` | SAD — >3 epochs, 0 handshakes |

### Shutdown Sequence (PWN Mode)

| Time | Display State |
|------|--------------|
| T=0 | **Korean SLEEP face + "Zzz..."** | `view.on_shutdown()`, display frozen |
| T=0-5s | Display frozen | Pwnagotchi stopping |
| T=5s | **Nothing new** | Splash service exits (no AO overlay). Display retains last Korean face. |
| T=5-10s | Korean face persists | System powers off, e-ink retains last image indefinitely |

---

## Mode Switching Behavior

### AO → PWN (`pwnoxide-mode pwn`)

1. Overlay moved: `angryoxide-v5.toml` → `angryoxide-v5.toml.disabled`
2. Bettercap service enabled and started
3. Pwnagotchi restarted
4. On restart:
   - Config loads without overlay → `bettercap.disabled` absent → `_ao_mode = False`
   - `ui.faces.png = false` (defaults.toml) → Korean text faces
   - `name` = "Pwnagotchi>" with cursor blink
   - `face` position = (0, 34) — below name
   - Splash service detects no overlay → does nothing on next boot

### PWN → AO (`pwnoxide-mode ao`)

1. Overlay moved: `angryoxide-v5.toml.disabled` → `angryoxide-v5.toml`
2. Bettercap service disabled and stopped
3. Pwnagotchi restarted
4. On restart:
   - Config loads overlay → `bettercap.disabled = true` → `_ao_mode = True`
   - `ui.faces.png = true` (overlay) → bull PNG faces
   - `name` = empty string, no cursor blink
   - `face` position = (0, 16) — near top, no name above
   - Splash service detects overlay → shows bull on next boot

---

## Rules & Constraints

### No Overlap Rule
- **AO mode:** No name rendered. Face at Y=16. Status at (125, 20). No conflict.
- **PWN mode:** Name at Y=20 (ends ~Y=32). Face at Y=34. 2px gap. No overlap.
- **Friend area:** Y=92-94, well below face zone. No conflict in either mode.
- **Plugin elements** (walkby status, AO capture count): Must be placed at Y ≥ 82 and ≤ 107 to avoid face and bottom bar.

### Bull Faces — Never in PWN Mode
- Splash service checks for overlay file → skips if PWN mode
- Config overlay disabled → `png = false` → faces.py loads Korean text defaults
- components.py Text widget: if PNG load fails, falls back to text rendering
- No bull PNG path should appear in defaults.toml — only in the overlay

### Korean Faces — Never in AO Mode
- Config overlay sets `png = true` + all 28 face paths to PNG files
- faces.py `load_from_config()` overwrites all globals with PNG paths
- Text widget sees `png = True` → loads PNG file instead of rendering text

### Display Refresh
- **Partial refresh** for all normal updates (fast, no full-screen flicker)
- **Full refresh** only for splash service (boot/shutdown) — writes to both EPD RAM banks
- Splash full refresh ensures image survives pwnagotchi's `epd.Clear()` + `displayPartBaseImage()`

### Cursor Behavior
- **AO mode:** Cursor disabled (`_ao_mode` check in `_refresh_handler`)
- **PWN mode:** Cursor blinks at `ui.fps` rate — toggles " █" suffix on name

### Status Text Position
- Always at (125, 20) in both modes
- Max 20 characters per line, wrapping enabled
- In AO mode, status text has the full width since no name is at (5, 20)
- In PWN mode, name "Pwnagotchi> █" occupies ~(5-120, 20), status starts at (125, 20)

---

## Plugin Indicators

### Indicator Positions (Pixel Map)

```
┌──────────────────────────────────────────────────────────┐
│ CH 00  APS 0 (00)  [BT -]  [BAT 0%]        UP 00:00:00  │  Y=0 (top bar)
│  (0,0)  (28,0)    (115,0) (140,0)           (185,0)      │
├──────────────────────────────────────────────────────────┤  Y=14 (line1)
│  [NAME]  (5,20)    [STATUS] (125,20)                     │  Y=20
│  [FACE]  (0,16 AO / 0,34 PWN)                           │  Y=16-80
│                                                          │
│  [WALKBY]  (0,82)                                        │  Y=82
│  [AO STATUS]  (0,85)                                     │  Y=85
│  [FRIEND FACE]  (0,92)   [FRIEND NAME]  (40,94)         │  Y=92-94
├──────────────────────────────────────────────────────────┤  Y=108 (line2)
│  PWND 0 (00)  (0,109)                     AUTO (225,109) │  Y=109
└──────────────────────────────────────────────────────────┘
```

### All Indicators by Zone

**Top Bar (Y=0..13) — Mode-independent, always visible:**

| Element | Key | Position | Font | Source | Shows in |
|---------|-----|----------|------|--------|----------|
| Channel | `channel` | (0, 0) | Bold+Medium | Core | Both |
| APs | `aps` | (28, 0) | Bold+Medium | Core | Both |
| Bluetooth | `bluetooth` | (115, 0) | Bold+Medium | bt-tether plugin | Both |
| Battery | `bat` | (140, 0) | Bold+Medium | pisugarx plugin | Both |
| Uptime | `uptime` | (185, 0) | Bold+Medium | Core | Both |

**Middle Zone (Y=14..107) — Mode-dependent:**

| Element | Key | Position | Font | Source | Shows in |
|---------|-----|----------|------|--------|----------|
| Name | `name` | (5, 20) | Bold 10pt | Core | **PWN only** (empty in AO) |
| Status | `status` | (125, 20) | Medium custom | Core | Both |
| Face | `face` | (0, 16) or (0, 34) | Huge 35pt / PNG | Core | Both (PNG in AO, text in PWN) |
| WalkBy status | `walkby_status` | (0, 82) | Small 9pt | walkby plugin | **PWN only** (disabled in AO config) |
| AO status | `angryoxide` | (0, 85) | Small 9pt | angryoxide plugin | **AO only** (hidden in PWN) |
| Friend face | `friend_face` | (0, 92) | Bold 10pt | Core | Both (hidden when no peer) |
| Friend name | `friend_name` | (40, 94) | BoldSmall 9pt | Core | Both (hidden when no peer) |

**Bottom Bar (Y=108+) — Mode-independent, always visible:**

| Element | Key | Position | Font | Source | Shows in |
|---------|-----|----------|------|--------|----------|
| line2 | — | Y=108 | — | Core | Both |
| Handshakes | `shakes` | (0, 109) | Bold+Medium | Core | Both |
| Mode | `mode` | (225, 109) | Bold 10pt | Core | Both |

### Cross-Mode Indicator Hiding

The angryoxide plugin actively manages indicator visibility in `on_ui_update()`:

**When AO mode is active:**
- Hides: `name`, `walkby`, `blitz`, `walkby_status` (set to `''`)
- Shows: `angryoxide` indicator with "AO: {captures} | {uptime}"
- Overrides BT-tether status text that bleeds into status area

**When PWN mode is active:**
- Hides: `angryoxide` indicator (set to `''`)
- Shows: `name` with "Pwnagotchi>" + cursor blink
- WalkBy plugin manages its own `walkby_status` visibility

### Indicators That Are Always Visible (Both Modes)

These are hardware/system indicators that are mode-independent:
- **BT** (bluetooth tether status) — connectivity, not attack-related
- **BAT** (battery percentage) — hardware monitoring
- **CH** (current channel) — shows `*` during recon in both modes
- **APS** (access point count) — from session data (StubClient in AO, bettercap in PWN)
- **UP** (uptime) — system uptime
- **PWND** (handshake count) — total captures, relevant in both modes
- **AUTO/MANU** (mode) — pwnagotchi operating mode

---

## Error & Crash States

### AO Mode Error Faces

The angryoxide plugin handles diagnostic face states beyond normal moods:

| Condition | Face | Detection | Recovery |
|-----------|------|-----------|----------|
| **WiFi down** | `wifi_down.png` | Monitor interface missing from `/sys/class/net/` | Plugin polls, shows wifi_down until interface returns |
| **Firmware crash** | `fw_crash.png` | journalctl pattern: "-110 Set Channel failed" or "firmware has halted" | Plugin runs modprobe -r/modprobe cycle, shows fw_crash for up to 120s |
| **AO process died** | `ao_crashed.png` | `process.poll() != None` (AO exited) | Exponential backoff restart: 5s, 10s, 20s, 40s... up to 300s. Face shows until restart. |
| **AO stopped permanently** | `ao_crashed.png` | Crash count exceeds `max_crashes` (default 10) | Shows "AO: ERR" in indicator. No more restarts. Manual reset via webhook. |
| **Battery low** | `battery_low.png` | PiSugar reports < 20% via `/tmp/pisugar-battery` | Face overrides mood face on each epoch |
| **Battery critical** | `battery_critical.png` | PiSugar reports < 5% | Face overrides mood face, takes priority over battery_low |
| **SDIO bus death** | `broken.png` | wlan0/wlan0mon disappears AND modprobe reload fails | Unrecoverable without power cycle. Display stuck on last face. |

**Face priority** (highest wins): battery_critical > fw_crash > ao_crashed > wifi_down > battery_low > normal mood

### PWN Mode Error States

PWN mode uses standard pwnagotchi error handling:

| Condition | Face | Detection |
|-----------|------|-----------|
| **Bettercap unreachable** | `(☓‿‿☓)` BROKEN | API timeout during `_wait_bettercap()` |
| **Monitor mode failed** | `(☓‿‿☓)` BROKEN | Interface not found after mon_start_cmd |
| **Blind (no APs)** | `(╥☁╥ )` SAD → restart | `blind_for >= mon_max_blind_epochs` (default 5) triggers service restart |
| **Rebooting** | `(☓‿‿☓)` BROKEN | `on_rebooting()` called |

---

## Manual Mode (MANU)

Triggered by starting pwnagotchi with `--manual` flag. Applies to both AO and PWN.

**Display differences from AUTO:**
- Mode indicator shows **"MANU"** instead of "AUTO" at (225, 109)
- Face: SAD if last session had >3 epochs and 0 handshakes, else HAPPY
- Channel shows "-" (no scanning)
- APS shows last session's associated count
- Status shows last session summary text
- Uptime shows last session duration
- PWND shows last session handshakes + total unique

**No automatic scanning or attacking in MANU mode.** Display is static until manually switched to AUTO.

---

## Display Configuration

### Rotation

```toml
[ui.display]
rotation = 180    # degrees: 0, 90, 180, 270
```

- **Default for Oxigotchi: 180°** — Pi Zero 2W mounted upside-down with PiSugar battery underneath
- Rotation is applied in `display.py` via `canvas.rotate()` before sending to EPD
- The splash service also rotates 180° via `canvas.transpose(Image.ROTATE_180)`
- If rotation is 90° or 270°, width/height swap (portrait mode — not recommended for 2.13")

### Invert Mode

```toml
[ui]
invert = false    # false = black on white (default), true = white on black
```

- **false (default):** White background, black text/art — standard e-ink appearance
- **true:** Black background, white text/art — higher contrast in bright light
- When inverted: `BLACK = 0x00`, `WHITE = 0xFF` (swapped)
- PNG faces are colorized via `ImageOps.colorize()` when `self.color == 255`
- All plugin elements inherit the global BLACK/WHITE values

### FPS (Refresh Rate)

```toml
[ui]
fps = 0.0    # 0 = manual updates only, >0 = continuous refresh
```

- **0.0 (default):** Display only updates on major state changes (face, status, handshakes). Uptime and name are in the `_ignore_changes` list — they don't trigger refreshes.
- **>0 (e.g., 1.0):** `_refresh_handler` thread runs at this rate. Enables cursor blink on name. Uptime updates live. More e-ink wear.
- Recommended: `0.0` for AO mode (no cursor needed), `1.0` for PWN mode (cursor blink)

### tweak_view.json (Position Overrides)

Deployed to `/etc/pwnagotchi/custom-plugins/tweak_view.json`. Overrides default element positions for the Waveshare V4 layout. Used by the VSS (Volts/Sats/Status) plugin framework.

Current overrides on the Pi:

```json
{
    "VSS.shakes.xy": "0,0",
    "VSS.uptime.xy": "187,0",
    "VSS.channel.xy": "0,109",
    "VSS.channel.label_font": "Small",
    "VSS.aps.xy": "40,109",
    "VSS.aps.label": "AP",
    "VSS.aps.label_font": "Small",
    "VSS.connection_status.xy": "85,109",
    "VSS.bluetooth.xy": "120,109",
    "VSS.bluetooth.label": "BT",
    "VSS.bat.xy": "155,109",
    "VSS.bat.label": "",
    "VSS.mode.xy": "220,109"
}
```

**Effect:** Moves PWND to top-left (0,0), pushes CH/AP/BT/BAT/MODE to the bottom bar (Y=109) with Small fonts. This frees up more vertical space in the middle zone for the face and status text.

**Note:** tweak_view.json positions take priority over hardcoded layout positions. If a plugin reads from `self._layout`, it gets the hardware default. The VSS framework applies JSON overrides on top.

---

## Web UI Display Preview

### `/ui` Endpoint

```
GET http://10.12.194.1:8080/ui
```

Returns the current e-ink display as a **PNG image** (250×122, 1-bit).

- Updated on every `view.update()` call via `web.update_frame(canvas)`
- Served by `handler.py` with `send_file(web.frame_path, mimetype="image/png")`
- Frame is saved to a temp file with lock protection (`web.frame_lock`)
- The main web page (`/`) includes this as `<img src="/ui">` with auto-refresh

### `/` Main Page

Shows the e-ink preview image at the top, with navigation to plugins page. This is stock pwnagotchi — works in both modes.

### AO Dashboard (`/plugins/angryoxide/`)

Full-featured web dashboard (only meaningful in AO mode). Shows live status, nearby networks, attack controls, capture history. Auto-refreshes every 5 seconds.

---

## Friend Face & Peer Display

### Format

```
▌▌▌│ buddy 3 (15) of 4
```

- **Signal bars:** 1-4 filled bars based on peer RSSI
  - ≥ -67 dBm: 4 bars (▌▌▌▌)
  - ≥ -70 dBm: 3 bars (▌▌▌│)
  - ≥ -80 dBm: 2 bars (▌▌││)
  - < -80 dBm: 1 bar (▌│││)
- **Name:** Peer's advertised name
- **Numbers:** `pwnd_run (pwnd_total)` — handshakes this session (lifetime)
- **"of N":** Total peers visible (shown if >1, "of over 9000" if >9000)

### Position

- `friend_name` at (0, 92) — BoldSmall 9pt
- Only visible when a peer is in range
- Set to `None` (hidden) when no peers detected
- Works identically in both AO and PWN modes

---

## E-Ink Display Properties

### Image Persistence

E-ink displays retain their last image **indefinitely** without power. When the Pi shuts down:
- **AO mode:** Last image is the shutdown bull face (from splash ExecStop)
- **PWN mode:** Last image is the Korean sleep face (from `view.on_shutdown()`)
- The display will show this face for hours/days until next power-on

### Partial vs Full Refresh

| Refresh Type | Speed | Flicker | Used By |
|-------------|-------|---------|---------|
| **Full** (`epd.display()`) | ~2-3s | Full screen flash | Splash service only (boot/shutdown) |
| **Partial** (`epd.displayPartial()`) | ~0.3-0.5s | None (in-place update) | All pwnagotchi UI updates |

- Full refresh writes to both EPD RAM banks — image survives a subsequent `Clear()` + `displayPartBaseImage()`
- Partial refresh only updates changed pixels — faster but can accumulate ghosting over time
- Pwnagotchi calls `displayPartBaseImage()` once during init, then `displayPartial()` for all updates

### Ghosting

After extended use (hours), partial refresh can leave ghost artifacts. The splash service's full refresh on boot/shutdown helps clear ghosting. No automatic ghost-clearing cycle is implemented.
