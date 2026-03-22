# Oxigotchi E-Ink Display Specification

**Hardware:** Waveshare 2.13" V4 (250x122 pixels, 1-bit, partial refresh)
**Orientation:** Landscape, 250px wide x 122px tall
**Colors:** Black (0xFF rendered) on white (0x00 rendered) — inverted internally

> The Rust binary drives the display directly via SPI (`rppal` crate). No Python PIL,
> no plugins. All elements are rendered in `main.rs::update_display()`.
> The Python sections below are the **reference spec** — the Rust layout must match them.

---

## Rust Implementation — Current State

**Source:** `rust/src/main.rs` → `update_display()`
**Renderer:** `rust/src/display/mod.rs` → `Screen` struct
**Fonts:** `rust/src/display/fonts.rs` → ProFont bitmap fonts via `profont` crate
**Faces:** `rust/src/display/faces.rs` → 120x66 1-bit bitmaps compiled in from `faces/*.raw`

### Rust Font Mapping

| Python font | Python size | Rust function | ProFont size | Used by |
|-------------|------------|---------------|-------------|---------|
| Small | 9pt | `fonts::small()` | PROFONT_9_POINT | `draw_text()`, `draw_labeled_value()` — indicators, IP, crash |
| Medium | 10pt | `fonts::medium()` | PROFONT_10_POINT | `draw_status()` — status text with word wrap |
| Bold | 10pt bold | `fonts::bold()` | PROFONT_12_POINT | `draw_name()` — device name (12pt compensates for no bold) |
| Huge | 35pt | `fonts::face()` | PROFONT_24_POINT | Kaomoji fallback (not used when bitmap faces active) |
| — | — | `fonts::tiny()` | PROFONT_7_POINT | Dense indicators (available, not currently used) |

### Rust Draw Methods

| Method | Font | y parameter | Baseline calc | Notes |
|--------|------|-------------|---------------|-------|
| `draw_text(text, x, y)` | Small 9pt | visual top | `y + 7` | General text, indicators |
| `draw_labeled_value(label, value, x, y)` | Small 9pt | visual top | `y + 7` | "LABEL: value" format |
| `draw_name(name)` | Bold 12pt | hardcoded y=30 baseline | `Point(5, 30)` | Always at (5, 20) visual |
| `draw_status(text)` | Medium 10pt | hardcoded y=28 baseline | `Point(125, 28)` | Word wrap, max 20 chars/line |
| `draw_face(face)` | — | hardcoded (0, 16) | bitmap blit | 120x66 bull PNG sprite |
| `draw_hline(x, y, width)` | — | pixel y | direct pixel set | 1px divider line |
| `draw_bitmap(data, x, y, w, h)` | — | pixel y | direct pixel set | Raw 1-bit bitmap blit |

### Rust Layout — Current vs Python Reference

```
PYTHON AO MODE (reference — what Rust must match):
┌──────────────────────────────────────────────────────────┐
│ AO: 5/302 | 12m | CH:1,6,11                  APs:47     │  Y=0  top bar (all Small 9pt)
│ (0,0)                                       (145,0)      │
│                     [BT C]  [BAT85%]     [UP 01:23:45]   │  core elements at their
│                    (115,0)  (140,0)        (185,0)        │  default positions
├──────────────────────────────────────────────────────────┤  Y=14 line1
│                      Udderly bored                       │  (125,20) Medium 10pt
│  ┌────────────┐      (bull status msg,                   │  word-wrapped max 20c/line
│  │ BULL PNG   │       or 2-part joke)                    │
│  │ 120×66     │                                          │  (0,16) bitmap blit
│  │ bored.png  │                                          │
│  └────────────┘                                          │
│                                                          │
│  USB:10.0.0.2 :8080                                      │  (0,95) Small 9pt, rotates
├──────────────────────────────────────────────────────────┤  Y=108 line2
│ CRASH:0                                            AUTO  │  Y=112 (Small 9pt / Bold 10pt)
│ (0,112)                                        (222,112) │
└──────────────────────────────────────────────────────────┘

TOP BAR details:
  angryoxide (0,0)  = "AO: V/T | Xm | CH:channels"   V=verified(.22000), T=total pcapng
  ao_aps (145,0)    = "APs:N"                          N from AO stdout parsing
  bluetooth (115,0) = "BT C" / "BT -"                  core pwnagotchi element (not hidden)
  bat (140,0)       = "BAT85%"                         core pisugarx element (not hidden)
  uptime (185,0)    = "UP HH:MM:SS"                    core pwnagotchi element (not hidden)
  channel (300,300) = HIDDEN (moved off-screen by AO plugin)
  aps (300,300)     = HIDDEN (moved off-screen, replaced by ao_aps)

HIDDEN elements (moved to 300,300 off-screen):
  shakes, channel, aps, display-password

BLANKED elements (set to empty string):
  name, walkby, blitz, walkby_status

BOTTOM BAR:
  ao_crash (0,112)  = "CRASH:N"    firmware crash count
  mode (222,112)    = "AUTO"       repositioned by AO plugin every ui_update
```

```
RUST (current — gaps marked with ✗):
┌──────────────────────────────────────────────────────────┐
│ AO: 0/0 | 00:00:00           BAT100%  UP: 00:00:00       │  Y=0  all Small 9pt
│ (0,0)                       (155,0)    (185,0)            │
│                                                    ✗ BT  │  ✗ MISSING bluetooth
│                                                  ✗ APs   │  ✗ MISSING ao_aps
├──────────────────────────────────────────────────────────┤  Y=14 line1
│                      Bull status msg...                  │  (125,20) Medium ✓
│  ┌────────────┐                                          │  (0,16) bitmap ✓
│  │ BULL PNG   │                                          │
│  │ 120×66     │                                          │
│  └────────────┘                                          │
│                                                          │
│  USB:10.0.0.2 :8080                                      │  (0,95) ✓
├──────────────────────────────────────────────────────────┤  Y=108 line2
│ CRASH:0  PWND:0  APs:0                            AUTO  │  Y=109 all Small 9pt
│ (0,109)  (70,109)(140,109)                     (222,109) │
└──────────────────────────────────────────────────────────┘
```

### Gap Analysis — Rust vs Python

| # | Element | Python | Rust Current | Fix Needed |
|---|---------|--------|-------------|------------|
| 1 | AO status | (0,0) Small 9pt `"AO: V/T \| HH:MM \| CH:1,6,11"` | (0,0) Small 9pt `"AO: H/C \| HH:MM:SS"` | **Match format** — add channels |
| 2 | BT status | (115,0) Bold+Medium `"BT C"` / `"BT -"` | **MISSING** | **Add** at (115,0) |
| 3 | Battery | (140,0) Bold+Medium `"BAT 85%"` | (155,0) Small `"BAT100%"` | **Move to (140,0)**, match font |
| 4 | Uptime | (185,0) Bold+Medium `"UP HH:MM:SS"` | (185,0) Small `"UP: HH:MM:SS"` | **Match font** (Bold+Medium) |
| 5 | AO APs | (145,0) Small 9pt `"APs:N"` | (140,109) Small — **wrong zone** | **Move to (145,0)** top bar |
| 6 | Face | (0,16) PNG 120x66 | (0,16) bitmap 120x66 | ✓ Correct |
| 7 | Status | (125,20) Medium word-wrap | (125,20) Medium word-wrap | ✓ Correct |
| 8 | IP display | (0,95) Small `"USB:10.0.0.2 :8080"` | (0,95) Small | ✓ Correct |
| 9 | Crash | (0,112) Small `"CRASH:N"` | (0,109) Small | **Move to y=112** |
| 10 | PWND | **hidden** in AO mode | (70,109) shown | **Remove** — not shown in AO mode |
| 11 | Mode | (222,112) Bold 10pt `"AUTO"` | (222,109) Small 9pt | **Move to y=112**, use bold font |

### Face Variety — Python Reference (from on_epoch, lines 1510-1667)

Priority order (first match wins, `face_set` flag prevents lower-priority overrides):

| # | Feature | Condition | Face | Status | Priority |
|---|---------|-----------|------|--------|----------|
| 7 | Debug on boot | `debug_epochs_left > 0` | debug→awake | bull_status('debug', 2) | Highest |
| 1a | Active milestone | `milestone_epochs_left > 0` | milestone face | milestone status | High |
| 1b | New milestone | captures hit 1/10/25/50/100 | 1=excited, 10=cool, 25=intense, 50=smart, 100=grateful | bull_status(face, 2) | High |
| 1c | Level-up | every 10 captures (not at 10/25/50/100) | motivated | bull_status('motivated', 2) | High |
| 2 | Capture variety | `captures_this_epoch > 0` | random(['happy','cool','grateful','excited']) | bull_status(face, 2) | High |
| 5 | Friend (active) | `friend_epochs_left > 0` | friend | bull_status('friend', 1) | Medium |
| 5 | Friend (new) | peers detected | friend | bull_status('friend', 1) | Medium |
| 6 | Upload (active) | `upload_epochs_left > 0` | upload | bull_status('upload', 1) | Medium |
| 6 | Upload (new) | wpa-sec active + new captures | upload | bull_status('upload', 1) | Medium |
| 3 | Time: 2-5am | `2 <= hour <= 5` and no captures | sleep | bull_status('sleep', 0) | Low |
| 3 | Time: 6-8am | `6 <= hour <= 8` (once per boot) | motivated | bull_status('motivated', 1) | Low |
| 3 | Time: 10pm-1am | `hour >= 22 or hour <= 1` | cool | bull_status('cool', 0) | Low |
| 4 | Idle 0-10 | `idle_epochs % 50` in 0-10 | bored | bull_status('bored', 0) | Low |
| 4 | Idle 11-20 | `idle_epochs % 50` in 11-20 | lonely | bull_status('lonely', 0) | Low |
| 4 | Idle 21-30 | `idle_epochs % 50` in 21-30 | demotivated | bull_status('demotivated', 0) | Low |
| 4 | Idle 31-40 | `idle_epochs % 50` in 31-40 | angry | bull_status('angry', 0) | Low |
| 4 | Idle 41-50 | `idle_epochs % 50` in 41-50 | sad | bull_status('sad', 0) | Low |
| 8 | Rare face | 5% random chance | random(['cool','intense','smart','grateful','motivated']) | bull_status(face, 0) | Low |
| — | Default | nothing else triggered | awake | bull_status('awake', 0) | Lowest |

### Required Fixes for Rust update_display()

```rust
fn update_display(&mut self) {
    self.screen.clear();
    let m = &self.epoch_loop.metrics;

    // ---- TOP BAR (y=0) ----

    // AO status line at (0,0) — Small 9pt
    // Format: "AO: V/T | HH:MM | CH:1,6,11"
    let ao_status = format!("AO: {}/{} | {}",
        m.handshakes, self.captures.count(), self.ao.uptime_str());
    self.screen.draw_text(&ao_status, 0, 0);

    // BT status at (115,0) — Small 9pt
    self.screen.draw_text(self.bluetooth.status_short(), 115, 0);

    // Battery at (140,0) — Small 9pt
    self.screen.draw_text(&self.battery.display_str(), 140, 0);

    // AO APs count at (145,0) — Small 9pt (overlaps battery — pick one)
    // Python puts APs at 145 which overlaps battery at 140.
    // In practice the AO plugin draws APs OVER the battery area.
    // For Rust, put APs right of battery or skip if battery is shown.

    // Uptime at (185,0) — Small 9pt "UP HH:MM:SS"
    self.screen.draw_labeled_value("UP", &self.epoch_loop.uptime_str(), 185, 0);

    // ---- LINE 1 (y=14) ----
    self.screen.draw_hline(0, 14, DISPLAY_WIDTH);

    // ---- FACE at (0,16) — 120x66 bull bitmap ----
    let face = self.epoch_loop.current_face();
    self.screen.draw_face(&face);

    // ---- STATUS at (125,20) — Medium 10pt, word-wrapped ----
    let status = self.epoch_loop.personality.status_msg();
    self.screen.draw_status(&status);

    // ---- IP DISPLAY at (0,95) — Small 9pt ----
    let ip_str = self.network.display_ip_str(
        self.bluetooth.ip_address.as_deref());
    self.screen.draw_text(&ip_str, 0, 95);

    // ---- LINE 2 (y=108) ----
    self.screen.draw_hline(0, 108, DISPLAY_WIDTH);

    // ---- BOTTOM BAR (y=112) ----
    // Crash counter at (0,112) — Small 9pt — only if crashes
    if self.ao.crash_count > 0 {
        self.screen.draw_text(
            &format!("CRASH:{}", self.ao.crash_count), 0, 112);
    }
    // Mode at (222,112) — Small 9pt "AUTO"
    self.screen.draw_text("AUTO", 222, 112);

    self.screen.flush();
}
```

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
│ PWND 0 (00)                                        AUTO  │  ← Bottom bar (Y=109+, mode at Y=112)
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
| AO Status | `angryoxide` | (0, 0) | Small 9pt label + Small 9pt value | "AO: 5/302 \| 1h23m \| CH:1,6,11" / empty | AO only (top bar, left) |
| Friend face | `friend_face` | (0, 92) | Bold 10pt | Peer's face text | Both (hidden if no peer) |
| Friend name | `friend_name` | (40, 94) | BoldSmall 9pt | "▌▌▌│ buddy 3 (15) of 4" | Both (hidden if no peer) |
| Line 2 | — | (0, 108) → (250, 108) | — | Horizontal divider, 1px | Both |
| Handshakes | `shakes` | (0, 109) | Bold 10pt label + Medium 10pt value | PWN: "PWND 1 (294) [AP_NAME]" / AO: **hidden** (AO indicator replaces it) | PWN only |
| Mode | `mode` | (222, 112) | Bold 10pt | "AUTO" or "MANU" | Both |

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
| T=3-5s | **Bull AWAKE face (centered, full refresh)** | `oxigotchi-splash.service` runs before pwnagotchi. Renders `awake.png` centered on display via full EPD refresh. Writes to both RAM banks so image persists through partial refreshes. |
| T=5-8s | Bull face persists | `pwnagotchi-splash-delay.conf` adds 3s `ExecStartPre=/bin/sleep 3` before pwnagotchi starts. Splash stays visible. |
| T=8-12s | **Bull SLEEP face + "Initializing..."** | Pwnagotchi starts, `view.on_starting()` sets SLEEP face + version text. EPD partial refresh begins. |
| T=12-15s | **Bull SLEEP face + "Reading logs..."** | `LastSession.parse()` runs (loads from cache in ~1s). Face = SLEEP or SMART. |
| T=15-20s | **Bull AWAKE face + "Ready"** | Monitor mode up, AO started, first epoch begins. |

> **Boot time: ~20 seconds** (optimized from ~65s by disabling unused services, fixing usb0-fallback, and eliminating boot ordering delays).

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

**Key differences from PWN mode (all driven by plugins, not hardcoded):**
- **No name label** — `ao_layout` plugin sets `name` to `''`, nothing renders at (5, 20)
- **No cursor blink** — `ao_cursor` plugin sets `ui._cursor_enabled = False`
- **Face at Y=16** — `ao_layout` plugin repositions face to line1+2, almost touching the top bar
- **Face is PNG** — 120×66 pixel 1-bit bull head, rendered at (0, 16) via `canvas.paste()`
- **PWND hidden** — `ao_pwnd` plugin suppresses the PWND counter (AO indicator shows captures)
- **Blind restart prevention** — `ao_compat` plugin injects synthetic AP when AP list is empty
- **AO capture count** — `angryoxide` plugin adds a LabeledValue showing capture count

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
| T=3-5s | **Nothing** | `oxigotchi-splash.service` detects no AO overlay → exits immediately. No splash shown. |
| T=5-10s | **Korean SLEEP face + "Pwnagotchi>" + "Initializing..."** | Pwnagotchi starts, `view.on_starting()`. EPD Clear() → fresh white canvas → partial refresh begins. |
| T=10-15s | **Korean SLEEP/SMART face + "Reading logs..."** | `LastSession.parse()` runs. |
| T=15-20s | **Korean AWAKE face + "Pwnagotchi>" + "Ready"** | Bettercap API ready, monitor mode up, first epoch. |

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

### Boot Display Order — No Raw Paths or Garbage on Screen

During boot, the display must NEVER show raw file paths (e.g., `/etc/pwnagot...`),
config text, error tracebacks, or any non-face content in the face area. The user
should only ever see clean faces and status text.

**Required boot order:**

1. **Splash service renders first** — `oxigotchi-splash.service` runs `Before=pwnagotchi.service`
   and uses a full EPD refresh to write the bull face to both RAM banks.
2. **Pwnagotchi delay** — `pwnagotchi-splash-delay.conf` adds `ExecStartPre=/bin/sleep 3` so
   the splash face is visible for at least 3 seconds before pwnagotchi starts.
3. **Pwnagotchi init** — when pwnagotchi starts, it calls `epd.Clear()` + `displayPartBaseImage()`
   which clears the splash. The very first partial refresh must show a valid face (SLEEP),
   not a file path string.

**What can go wrong:**
- If `ui.faces.png = true` but the PNG file doesn't exist or fails to load, the Text widget
  falls back to rendering the face *value* as text — which is a file path like
  `/etc/pwnagotchi/custom-plugins/faces/awake.png`. This MUST NOT appear on screen.
- The fallback in `components.py` checks `os.path.sep in self.value` — if the value contains
  a path separator, it does NOT render it as text (prevents path strings on display).
- If the face value is a valid Korean text string (no path separator), it renders as text
  (correct fallback for PWN mode).

**Rules:**
- Splash service must complete and write sentinel file before pwnagotchi starts
- Pwnagotchi's first face set must be a valid face (SLEEP on starting), never a path
- PNG face paths must only exist in the overlay config — never in defaults.toml
- The Text widget must silently suppress any value containing `/` rather than rendering it
- If a PNG face file is missing, the display should show nothing (blank) rather than the path

### Status Text — Mode-Appropriate Content

In AO mode, pwnagotchi's voice messages about individual AP names are **irrelevant** because
AO handles all attacks internally. Pwnagotchi doesn't send deauths or assocs — it only runs
the epoch loop and observes. Showing "Deauthenticating aa:bb:cc..." or "Hey AP_NAME let's
be friends!" is misleading because those actions aren't happening.

**AO mode status text should show:**
- Boot/init messages: "Initializing...", version info (normal, from `on_starting()`)
- AO-specific status: "AO: {captures} captures | {uptime}" (set by angryoxide plugin overriding BT-tether bleeds)
- Mood messages: "Sniffing around...", "Zzz...", "Looking around..." (from voice.py, still relevant)
- Handshake messages: "Cool, we got N new handshakes!" (relevant — AO captures trigger this)
- Peer messages: friend/lost peer (relevant — mesh peers are mode-independent)

**AO mode status text should NOT show:**
- `on_assoc(ap)`: "Associating to {AP_NAME}" — AO does its own assocs, pwnagotchi doesn't
- `on_deauth(sta)`: "Deauthenticating {MAC}" — AO does its own deauths, pwnagotchi doesn't
- `on_miss(who)`: "Missed {who}" — pwnagotchi isn't attacking, so it can't miss

**PWN mode status text:** All messages are appropriate — pwnagotchi is doing the attacks via bettercap.

**Implementation note:** The `associate()` and `deauth()` methods in agent.py still run in AO mode
(the main epoch loop calls them), but they go through StubClient which no-ops the commands. The
voice messages still fire though. To suppress them in AO mode, the angryoxide plugin should override
the status text on `on_ui_update()` when it detects bettercap-style attack messages. Currently the
plugin only overrides BT-tether status bleeds — it should also suppress assoc/deauth messages.

**IMPLEMENTED:** The angryoxide config overlay sets `personality.associate = false` and
`personality.deauth = false`, which prevents attack commands and misleading status text.
The previous `_ao_mode` early-return guards in `associate()`/`deauth()` have been removed
as redundant — the personality config is the canonical control.

### PWND Counter Format — Mode-Dependent

The handshake counter (`shakes` element) format differs by mode:

- **AO mode:** `"N (total)"` — just session count and total unique handshakes.
  No `[hostname]` suffix. The last captured AP name is irrelevant in AO mode because
  AO handles captures internally and the AO indicator already shows capture details.
- **PWN mode:** `"N (total) [hostname]"` — includes the hostname of the last AP
  that yielded a handshake. Shows `[unknown]` if the AP had no broadcast name.

**WalkBy/Blitz plugin** is PWN mode only — it uses bettercap `wifi.assoc`/`wifi.deauth`
commands. Disabled in AO config overlay (`[main.plugins.walkby] enabled = false`).
AO handles its own concurrent attacks natively.

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
- **AO mode:** Cursor disabled by `ao_cursor` plugin (sets `ui._cursor_enabled = False`)
- **PWN mode:** Cursor blinks at `ui.fps` rate — toggles " █" suffix on name
- Core uses `self._cursor_enabled` flag (default `True`), checked in `_refresh_handler`

### Status Text Position
- Always at (125, 20) in both modes
- Max 20 characters per line, wrapping enabled
- In AO mode, status text has the full width since no name is at (5, 20)
- In PWN mode, name "Pwnagotchi> █" occupies ~(5-120, 20), status starts at (125, 20)

---

## AO Display Plugins

AO mode display changes are implemented as 4 toggleable plugins (not hardcoded in core).
Each can be independently enabled/disabled from the plugins page or config.

| Plugin | File | Responsibility | Hook |
|--------|------|---------------|------|
| `ao_layout` | `plugins/default/ao_layout.py` | Moves face to Y=16, hides name label | `on_ui_setup`, `on_ui_update`, `on_unload` |
| `ao_cursor` | `plugins/default/ao_cursor.py` | Disables cursor blink (`_cursor_enabled = False`) | `on_ui_setup`, `on_unload` |
| `ao_pwnd` | `plugins/default/ao_pwnd.py` | Suppresses PWND counter (AO indicator replaces it) | `on_ui_setup`, `on_ui_update`, `on_unload` |
| `ao_compat` | `plugins/default/ao_compat.py` | Injects synthetic AP to prevent blind restart | `on_wifi_update` |

**Config defaults** (`defaults.toml`): all 4 disabled by default.
**AO overlay** (`angryoxide-v5.toml`): all 4 enabled.
Each plugin checks `bettercap.disabled` at setup time — if False, the plugin no-ops.

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
│  [AO STATUS]  (0,0) — top bar left (replaces PWND)        │  Y=0
│  [FRIEND FACE]  (0,92)   [FRIEND NAME]  (40,94)         │  Y=92-94
├──────────────────────────────────────────────────────────┤  Y=108 (line2)
│  PWND 0 (00)  (0,109)                     AUTO (222,112) │  Y=109/112
└──────────────────────────────────────────────────────────┘
```

### All Indicators by Zone

**Top Bar (Y=0..13) — Mode-independent, always visible:**

| Element | Key | Position | Font | Source | Shows in |
|---------|-----|----------|------|--------|----------|
| Channel | `channel` | (0, 0) | Bold+Medium | Core | PWN: "CH 06" / AO: replaced with "FW 0" (firmware crash count) |
| APs | `aps` | (28, 0) | Bold+Medium | Core | PWN: "APS 5 (18)" / AO: replaced with AO channel list "1,6,11" |
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

### AO Indicator States

The `angryoxide` element at (75, 0) in the top bar shows AO process health at a glance:

| Display | Meaning | When |
|---------|---------|------|
| `AO: 0/297 \| 0m \| CH:1,6,11` | AO running, 0 session / 297 total, 0 min uptime, channels 1,6,11 | Normal startup |
| `AO: 5/302 \| 1h23m \| CH:AH` | AO running, 5 session / 302 total, 1h 23m uptime, autohunt | Normal with autohunt |
| `AO: off` | AO process not started | Plugin loaded but AO binary not launched yet |
| `AO: ERR` | AO permanently stopped | Crash count exceeded `max_crashes` (default 10). Manual reset needed via `/plugins/angryoxide/reset` webhook. |
| *(empty)* | Hidden | PWN mode — indicator set to `''` |

Format: `AO: {session}/{total} | {uptime} | CH:{channels}`

- **session**: handshakes captured by AO this session (resets on restart)
- **total**: total unique handshakes on disk (from `utils.total_unique_handshakes()`)
- **uptime**: AO process uptime as `Ns`, `Nm`, `NhNm` depending on duration
- **channels**: AO channel list from runtime state (e.g., `1,6,11`) or `AH` for autohunt

**PWND element is hidden in AO mode** — both the label and value are suppressed.
The AO indicator replaces it with a more informative format. In PWN mode, PWND
renders normally as `"N (total) [hostname]"`.

### Bottom Bar Replacements in AO Mode

In AO mode, the CH and AP elements show useless data (`*` and `0`) because pwnagotchi
isn't scanning — AO handles it. These elements are repurposed:

| Position | PWN mode | AO mode | Data source |
|----------|----------|---------|-------------|
| (0, 109) | `CH 06` (current channel) | `CRASH:0` (firmware crash count) | `self._fw_crash_count` — climbs when firmware faults detected |
| (40, 109) | `AP 5 (18)` (AP count) | **hidden** (channels moved to AO indicator in top bar) | — |
| (85, 109) | conn status | conn status (unchanged) | bt-tether plugin |
| (120, 109) | `BT-` / `BT C` | unchanged | bt-tether plugin |
| (155, 109) | `CHG100%` | unchanged | pisugarx plugin |
| (222, 112) | `AUTO` | unchanged | core (repositioned by angryoxide plugin) |

**Crash counter:**
- Shows `CRASH:0` when stable (no firmware crashes)
- Shows `CRASH:3` when crashes detected — user should check web dashboard
- Resets when AO plugin resets crash state

**AO channels:** Moved to AO indicator in top bar as `CH:1,6,11` or `CH:AH` (autohunt).
The AP slot in the bottom bar is hidden in AO mode.

### Cross-Mode Indicator Hiding

The angryoxide plugin actively manages indicator visibility in `on_ui_update()`:

**When AO mode is active:**
- Hides: `name`, `walkby`, `blitz`, `walkby_status` (set to `''`)
- Moves off-screen: `shakes`, `channel`, `aps`, `display-password` (position set to (300, 300), outside the 250x122 display — blanking doesn't work because bettercap rewrites them after plugin runs)
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

## PiSugar 3 Button Controls

The PiSugar 3 battery HAT has a single physical button that supports three press types.
These are configured via the PiSugar daemon and mapped to oxigotchi actions:

| Press Type | Action | Display Feedback |
|------------|--------|-----------------|
| **Single press** | Toggle Bluetooth tethering ON/OFF | BT indicator changes: `BT C` (connected) / `BT -` (off) |
| **Double press** | Toggle AUTO/MANU mode | Mode indicator changes: `AUTO` / `MANU` at (222, 112) |
| **Long press** | Toggle AO/PWN mode | Full mode switch — bull faces ↔ Korean faces, ~20s restart |

The standalone bt-tether daemon (decoupled from pwnagotchi's bt-tether plugin) handles
Bluetooth connectivity. Single-press toggles the daemon without restarting pwnagotchi.

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

### tweak_view.json (NOT ACTIVE — Future Enhancement)

The file `/etc/pwnagotchi/custom-plugins/tweak_view.json` exists on the Pi with VSS-style
position overrides, but **no VSS plugin is loaded** so these overrides have no effect.
All element positions are determined by the hardcoded `waveshare2in13_V4.py` layout.

If a VSS plugin is added in the future, it would move elements to a denser layout
(PWND to top-left, CH/AP/BT/BAT/MODE to bottom bar with Small fonts). Until then,
the positions documented in the master element table above are the actual active positions.

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
