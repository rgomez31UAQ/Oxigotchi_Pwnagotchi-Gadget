# Oxigotchi Bull Face Catalog

28 expressive bull faces for the Waveshare 2.13" V4 e-ink display (250x122, 1-bit).

## Art Specifications

| Property | Value |
|----------|-------|
| Format | 1-bit PNG (mode `1`), no grayscale |
| Dimensions | 120 x 66 pixels |
| Display area | Position (0, 16) on 250x122 screen |
| Color | Black art on white background (inverted to white-on-black by `ui.invert = true`) |
| Style | High-contrast vector/logo, thick outlines, sports mascot bull head |
| Metadata | PNG tEXt chunks: Title, Description, Keywords, Trigger, Author |

## Face Catalog by Category

### Core Lifecycle

| # | File | Description | Keywords | Trigger | Mood |
|---|------|-------------|----------|---------|------|
| 1 | `awake.png` | Bull head with coffee mug, alert eyes | coffee, morning, alert, ready | Boot, wake from idle | -- |
| 2 | `sleep.png` | Full bull body lying down, Zzz floating | sleeping, resting, full body | 2am-5am quiet hours | -- |
| 3 | `shutdown.png` | Bull lying peacefully under moon and stars | night, moon, stars, peaceful | System shutdown | -- |
| 4 | `broken.png` | Bull with bandages/cracks, hearts | damaged, hearts, hurt, cracked | System error, recovery exhausted | -- |

### Directional (Scanning)

Alternates each step in the wait loop based on mood.

| # | File | Description | Keywords | Trigger | Mood |
|---|------|-------------|----------|---------|------|
| 5 | `look_r.png` | Bull head facing right, neutral | profile, right, watching | Scanning right channels | < 0.5 |
| 6 | `look_l.png` | Bull head facing left, neutral | profile, left, watching | Scanning left channels | < 0.5 |
| 7 | `look_r_happy.png` | Bull head facing right, slight smile | profile, right, happy | AP found while scanning right | >= 0.5 |
| 8 | `look_l_happy.png` | Bull head facing left, smile, wink | profile, left, happy, wink | AP found while scanning left | >= 0.5 |

### Attack Cycle (Per-Epoch Events)

| # | File | Description | Keywords | Trigger | Mood |
|---|------|-------------|----------|---------|------|
| 9 | `intense.png` | Bull snorting steam from nostrils, focused | snorting, steam, puffs, determined | Active attack streak / PMKID assoc | -- |
| 10 | `cool.png` | Bull wearing sunglasses, confident smirk | sunglasses, chill, confident, swag | Capture variety cycle, night mode / deauth | -- |
| 11 | `happy.png` | Grinning bull, wide smile, friendly | smiling, cheerful, content | Handshake captured | -- |
| 12 | `sad.png` | Bull with tears, droopy eyes, rain cloud | crying, tearful, cloudy, down | 41+ idle epochs, low mood / AP disappeared | -- |
| 13 | `smart.png` | Bull with glasses, lightbulb above head | glasses, idea, lightbulb, thinking | 50th capture milestone / optimal channel | -- |

### Mood States (AI Epoch Boundary)

| # | File | Description | Keywords | Trigger | Mood Threshold |
|---|------|-------------|----------|---------|----------------|
| 14 | `excited.png` | Bull roaring/bellowing, mouth wide open | roaring, yelling, pumped, wild | Many captures quickly, milestone | active >= 5 epochs |
| 15 | `bored.png` | Bull chewing grass/straw, side profile | grazing, idle, chewing, relaxed | No activity 0-10 epochs, SAFE mode | inactive >= 25 epochs |
| 16 | `angry.png` | Raging bull with electric sparks/lightning | furious, electric, storm, rage | 31-40 idle epochs | inactive >= 2x sad threshold |
| 17 | `lonely.png` | Bull lying down alone, moon/stars | alone, nighttime, lying down | 11-20 idle epochs | stale recon, no peers |
| 18 | `grateful.png` | Bull with bow/ribbon, gentle smile, eyes closed | bow, ribbon, thankful, gentle | 100th capture, capture variety cycle | active + good friend network |
| 19 | `motivated.png` | Bull charging with steam puffs | charging, steam, running, driven | Sunrise greeting, level up | AI reward > 0 |
| 20 | `demotivated.png` | Bull head hanging low, defeated | head down, tired, given up | Very low mood (<0.1) | AI reward < 0 |

### Social

| # | File | Description | Keywords | Trigger | Mood |
|---|------|-------------|----------|---------|------|
| 21 | `friend.png` | Two bull heads touching noses, hearts | pair, kissing, hearts, love, peer | Pwngrid peer discovered | -- |

### Data Transfer

| # | File | Description | Keywords | Trigger | Mood |
|---|------|-------------|----------|---------|------|
| 22 | `upload.png` | Bull surrounded by binary 0s and 1s | binary, data, matrix, digital | WPA-SEC upload in progress | -- |
| 23 | `debug.png` | Bull with monocle/goggles, examining | monocle, inspection, steampunk | Boot diagnostics | -- |

### System / Edge Cases (Oxigotchi Plugin Extensions)

These faces are not part of stock pwnagotchi. They are set by the angryoxide plugin via `_face()`.

| # | File | Description | Keywords | Trigger | Detection |
|---|------|-------------|----------|---------|-----------|
| 24 | `wifi_down.png` | Bull tangled in cables, WiFi X symbol | tangled, wires, broken wifi | WiFi interface lost | `blind_for >= mon_max_blind_epochs` |
| 25 | `fw_crash.png` | Bull electrocuted, lightning bolts, dazed | shocked, zapped, sparks, crash | Firmware crash detected | `-110` in dmesg |
| 26 | `ao_crashed.png` | Mushroom cloud explosion, smoke columns | explosion, nuclear, smoke, disaster | AO process crashed | `_check_health()` finds dead process |
| 27 | `battery_low.png` | Bull roaring/yawning, low energy | low energy, warning, depleted | Battery <20% | PiSugar `battery_level` |
| 28 | `battery_critical.png` | Bull collapsed, empty battery icon | dead, collapsed, empty battery | Battery <5% | PiSugar `battery_level` |

## Adding New Faces

### Naming Convention

- Lowercase filename matching the Face enum variant: `triumphant.png`, `smug.png`
- Underscore-separated for multi-word names: `night_owl.png`, `battery_low.png`

### Size and Format

1. Start with high-contrast black & white source art (any resolution)
2. Run through `process_for_eink.py`:
   - Auto-crop whitespace
   - Resize to 120x66 (LANCZOS, maintain aspect ratio)
   - Center-pad on white canvas
   - Threshold to 1-bit (no dithering)
3. Add PNG tEXt metadata via `add_metadata.py` (Title, Description, Keywords, Trigger, Author)

### Registering in the Plugin

1. **Add PNG** to `faces/eink/` (this repo) and deploy to Pi at `/etc/pwnagotchi/custom-plugins/faces/`
2. **Add config** entry in `angryoxide-v5.toml`:
   ```toml
   [ui.faces]
   new_face = ["/etc/pwnagotchi/custom-plugins/faces/new_face.png"]
   ```
3. **Add trigger** in the angryoxide plugin (`angryoxide_v2.py`):
   - For stock pwnagotchi states: map to existing `on_*()` hooks
   - For custom states: use `_face('new_face')` in the appropriate detection logic
4. **Update this catalog** with the new face's row

### Processing Pipeline

```
Source art (any size)     process_for_eink.py     eink/        add_metadata.py
  new_face.png       -->  crop/resize/1-bit  -->  new_face.png  -->  + tEXt metadata
```

## Planned Faces

22 additional bull faces are planned to reach the target of 50. See [BULL_FACE_IDEAS.md](../docs/BULL_FACE_IDEAS.md) for the full list including:

- **Capture Reactions**: triumphant, smug, surprised
- **Environmental**: rainy, sunny, night_owl
- **Activity**: eating, running, sniffing, headbutt
- **Social**: waving, flexing, dancing
- **System**: sweating, plugged_in, yawning, dizzy, skeptical, thinking, celebrating, sneaking

## File Locations

| What | Local (repo) | Pi |
|------|-------------|-----|
| E-ink PNGs | `faces/eink/*.png` | `/etc/pwnagotchi/custom-plugins/faces/` |
| Processing script | `faces/process_for_eink.py` | -- |
| Metadata script | `faces/add_metadata.py` | -- |
| Config overlay | `angryoxide-v5.toml` | `/etc/pwnagotchi/conf.d/angryoxide-v5.toml` |
| Plugin | `angryoxide_v2.py` | `/etc/pwnagotchi/custom-plugins/angryoxide.py` |
