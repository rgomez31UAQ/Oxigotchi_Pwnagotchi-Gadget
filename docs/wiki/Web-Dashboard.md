# Web Dashboard

← [Back to Wiki Home](Home)

---

```
http://10.0.0.2:8080
```

Full control from your phone or laptop. Auto-refreshes every 5-30 seconds. Dark theme, mobile-friendly, built with Tailwind CSS and served by the Rust daemon via axum.

## Dashboard Cards

The dashboard has 23 live cards organized by user journey:

| Card | Description |
|------|-------------|
| **Face Display** | Current bull face and mood — matches the e-ink display |
| **Core Stats** | Handshakes captured, APs seen, epoch count, uptime |
| **Live E-ink Preview** | Real-time rendering of what the e-ink display shows |
| **Battery** | PiSugar 3 charge level, charging state, estimated runtime |
| **Bluetooth** | BT state, discovered devices, BT aggression level, mode toggle (RAGE/BT/SAFE) |
| **Phone Tethering** | Scan for phones, pair via D-Bus, disconnect, forget devices, passkey display |
| **WiFi** | Monitor mode status, current channel, interface state |
| **Attack Toggles** | Per-attack-type enable/disable (deauth, PMKID, CSA, disassoc, anon reassoc, rogue M2) |
| **RAGE Slider** | 3-level aggression preset (Chill/Hunt/RAGE) — one slider controls rate, dwell, and channels |
| **Smart Skip** | Toggle to skip APs you already have handshakes for |
| **Recent Captures** | Latest handshakes with timestamps and AP names |
| **Per-File Downloads** | Download individual .pcapng or .22000 files |
| **Cracked Passwords** | Passwords returned from WPA-SEC cloud cracking |
| **Recovery Status** | WiFi firmware health, crash count, recovery state |
| **Personality/XP** | Current level, XP progress, mood score |
| **System Info** | CPU temp, memory, SD card usage, uptime |
| **Mode Switch** | RAGE/BT/SAFE mode buttons |
| **System Controls** | Restart AO, shutdown Pi, restart SSH |
| **Plugins** | Installed Lua plugins, enable/disable toggle |
| **Nearby Networks** | APs currently visible to AngryOxide |
| **Whitelist** | Managed list of networks to skip during attacks |
| **Channel Config** | 13 toggleable channel buttons, dwell slider, autohunt mode |
| **WPA-SEC** | API key input, upload status, auto-upload toggle |
| **Discord** | Webhook URL, notification settings |
| **Live Logs** | Real-time daemon and AO log output |

## Features

- **Auto-refresh** — Cards update every 5-30 seconds depending on data volatility
- **Dark theme** — Easy on the eyes, designed for field use
- **Mobile-friendly** — Responsive layout works on phone screens
- **Tailwind CSS** — Clean, consistent styling
- **Embedded** — HTML/CSS/JS served directly from the Rust binary, no external dependencies
- **Authentication** — Basic auth (default: `changeme`/`changeme`)

## API Endpoints

The dashboard is backed by a REST API. Key endpoints:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/status` | GET | Full system status (mode, epoch, captures, uptime, personality) |
| `/api/qpu` | GET | RF classification stats (frame rates, BSSIDs, dominant class) |
| `/api/captures` | GET | List of capture files with metadata |
| `/api/config` | GET/POST | Read or update daemon configuration |
| `/api/attacks` | POST | Toggle individual attack types |
| `/api/mode` | POST | Switch between RAGE, BT, and SAFE modes |
| `/api/whitelist` | GET/POST | Manage network whitelist |
| `/api/channels` | POST | Update channel configuration |
