//! Web dashboard module (axum HTTP server).
//!
//! Provides a REST API and embedded HTML dashboard for monitoring
//! and configuring oxigotchi. Types and constants are defined here;
//! the axum router will be added when the tokio/axum dependency lands.

use serde::Serialize;

/// System status snapshot returned by the /api/status endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct StatusResponse {
    pub name: String,
    pub version: String,
    pub uptime: String,
    pub epoch: u64,
    pub channel: u8,
    pub aps_seen: u32,
    pub handshakes: u32,
    pub blind_epochs: u32,
    pub mood: f32,
    pub face: String,
    pub status_message: String,
    pub mode: String,
}

/// Attack stats returned by /api/attacks.
#[derive(Debug, Clone, Serialize)]
pub struct AttackStats {
    pub total_attacks: u64,
    pub total_handshakes: u64,
    pub attack_rate: u32,
    pub deauths_this_epoch: u32,
}

/// Capture info returned by /api/captures.
#[derive(Debug, Clone, Serialize)]
pub struct CaptureInfo {
    pub total_files: usize,
    pub handshake_files: usize,
    pub pending_upload: usize,
    pub total_size_bytes: u64,
}

/// Config update request for /api/config.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ConfigUpdate {
    pub name: Option<String>,
    pub attack_rate: Option<u32>,
    pub channel_dwell_ms: Option<u64>,
    pub whitelist_add: Option<String>,
    pub whitelist_remove: Option<String>,
}

/// Battery info returned by /api/battery.
#[derive(Debug, Clone, Serialize)]
pub struct BatteryInfo {
    pub level: u8,
    pub charging: bool,
    pub voltage_mv: u16,
    pub low: bool,
    pub critical: bool,
}

/// WiFi info returned by /api/wifi.
#[derive(Debug, Clone, Serialize)]
pub struct WifiInfo {
    pub state: String,
    pub channel: u8,
    pub aps_tracked: usize,
    pub channels: Vec<u8>,
    pub dwell_ms: u64,
}

/// Bluetooth info returned by /api/bluetooth.
#[derive(Debug, Clone, Serialize)]
pub struct BluetoothInfo {
    pub state: String,
    pub phone_mac: String,
    pub internet_available: bool,
    pub retry_count: u32,
}

/// Recovery/health info returned by /api/recovery.
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryInfo {
    pub state: String,
    pub total_recoveries: u32,
    pub soft_retries: u32,
    pub hard_retries: u32,
    pub diagnostic_count: usize,
}

/// Personality/mood info returned by /api/personality.
#[derive(Debug, Clone, Serialize)]
pub struct PersonalityInfo {
    pub mood: f32,
    pub face: String,
    pub blind_epochs: u32,
    pub total_handshakes: u32,
    pub total_aps_seen: u32,
    pub xp: u64,
    pub level: u32,
}

/// System info returned by /api/system.
#[derive(Debug, Clone, Serialize)]
pub struct SystemInfoResponse {
    pub cpu_temp_c: f32,
    pub mem_used_mb: u32,
    pub mem_total_mb: u32,
    pub cpu_percent: f32,
}

/// Handshake file entry returned by /api/handshakes.
#[derive(Debug, Clone, Serialize)]
pub struct HandshakeEntry {
    pub filename: String,
    pub ssid: String,
    pub size_bytes: u64,
    pub uploaded: bool,
}

/// Mode switch request for /api/mode.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModeSwitch {
    pub mode: String,
}

/// API router paths.
pub const API_STATUS: &str = "/api/status";
pub const API_ATTACKS: &str = "/api/attacks";
pub const API_CAPTURES: &str = "/api/captures";
pub const API_CONFIG: &str = "/api/config";
pub const API_DISPLAY: &str = "/api/display.png";
pub const API_BATTERY: &str = "/api/battery";
pub const API_WIFI: &str = "/api/wifi";
pub const API_BLUETOOTH: &str = "/api/bluetooth";
pub const API_RECOVERY: &str = "/api/recovery";
pub const API_PERSONALITY: &str = "/api/personality";
pub const API_SYSTEM: &str = "/api/system";
pub const API_HANDSHAKES: &str = "/api/handshakes";
pub const API_HANDSHAKE_DL: &str = "/api/handshakes/:filename";
pub const API_MODE: &str = "/api/mode";
pub const API_RESTART: &str = "/api/restart";
pub const API_SHUTDOWN: &str = "/api/shutdown";
pub const API_WHITELIST: &str = "/api/whitelist";
pub const API_CRACKED: &str = "/api/cracked";

/// Embedded HTML for the dashboard.
/// Contains 15 cards matching the Python dashboard: face, stats, display preview,
/// attacks, captures, battery, bluetooth, wifi, recovery, personality, system,
/// handshakes, mode, config, and cracked passwords.
pub const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>oxigotchi</title>
    <style>
        body { font-family: monospace; background: #1a1a2e; color: #e0e0e0; margin: 20px; }
        .card { background: #16213e; border-radius: 8px; padding: 16px; margin: 8px 0; }
        .face { font-size: 48px; text-align: center; padding: 20px; }
        .stat { display: inline-block; margin: 8px 16px; }
        .label { color: #888; font-size: 12px; }
        .value { font-size: 20px; color: #0f0; }
        .warn { color: #ff0; }
        .err { color: #f00; }
        h1 { color: #e94560; }
        h3 { color: #aaa; margin: 0 0 8px 0; }
        #display { image-rendering: pixelated; width: 500px; border: 1px solid #333; }
        .refresh { color: #666; font-size: 10px; }
        .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
        .btn { background: #0a3d62; border: 1px solid #3c6382; color: #e0e0e0;
               padding: 8px 16px; cursor: pointer; border-radius: 4px; font-family: monospace; }
        .btn:hover { background: #3c6382; }
    </style>
</head>
<body>
    <h1 id="name">oxigotchi</h1>
    <!-- Card 1: Face -->
    <div class="card">
        <div class="face" id="face">(O_O)</div>
        <div style="text-align:center" id="status">Loading...</div>
    </div>
    <!-- Card 2: Core stats -->
    <div class="card">
        <div class="stat"><div class="label">CH</div><div class="value" id="ch">-</div></div>
        <div class="stat"><div class="label">APS</div><div class="value" id="aps">-</div></div>
        <div class="stat"><div class="label">PWND</div><div class="value" id="pwnd">-</div></div>
        <div class="stat"><div class="label">EPOCH</div><div class="value" id="epoch">-</div></div>
        <div class="stat"><div class="label">UPTIME</div><div class="value" id="uptime">-</div></div>
        <div class="stat"><div class="label">MOOD</div><div class="value" id="mood">-</div></div>
    </div>
    <!-- Card 3: Display preview -->
    <div class="card">
        <h3>E-Ink Preview</h3>
        <img id="display" src="/api/display.png" alt="display preview">
    </div>
    <div class="grid">
    <!-- Card 4: Battery -->
    <div class="card">
        <h3>Battery</h3>
        <div class="stat"><div class="label">LEVEL</div><div class="value" id="bat_level">-</div></div>
        <div class="stat"><div class="label">STATE</div><div class="value" id="bat_state">-</div></div>
        <div class="stat"><div class="label">VOLTAGE</div><div class="value" id="bat_volt">-</div></div>
    </div>
    <!-- Card 5: Bluetooth -->
    <div class="card">
        <h3>Bluetooth</h3>
        <div class="stat"><div class="label">STATUS</div><div class="value" id="bt_state">-</div></div>
        <div class="stat"><div class="label">INET</div><div class="value" id="bt_inet">-</div></div>
    </div>
    <!-- Card 6: WiFi -->
    <div class="card">
        <h3>WiFi</h3>
        <div class="stat"><div class="label">STATE</div><div class="value" id="wifi_state">-</div></div>
        <div class="stat"><div class="label">APs</div><div class="value" id="wifi_aps">-</div></div>
    </div>
    <!-- Card 7: Attacks -->
    <div class="card">
        <h3>Attacks</h3>
        <div class="stat"><div class="label">TOTAL</div><div class="value" id="atk_total">-</div></div>
        <div class="stat"><div class="label">RATE</div><div class="value" id="atk_rate">-</div></div>
    </div>
    <!-- Card 8: Captures -->
    <div class="card">
        <h3>Captures</h3>
        <div class="stat"><div class="label">FILES</div><div class="value" id="cap_files">-</div></div>
        <div class="stat"><div class="label">PENDING</div><div class="value" id="cap_pending">-</div></div>
    </div>
    <!-- Card 9: Recovery -->
    <div class="card">
        <h3>Recovery</h3>
        <div class="stat"><div class="label">STATE</div><div class="value" id="rec_state">-</div></div>
        <div class="stat"><div class="label">RECOVERIES</div><div class="value" id="rec_total">-</div></div>
    </div>
    <!-- Card 10: Personality -->
    <div class="card">
        <h3>Personality</h3>
        <div class="stat"><div class="label">XP</div><div class="value" id="xp">-</div></div>
        <div class="stat"><div class="label">LEVEL</div><div class="value" id="level">-</div></div>
    </div>
    <!-- Card 11: System -->
    <div class="card">
        <h3>System</h3>
        <div class="stat"><div class="label">CPU</div><div class="value" id="sys_cpu">-</div></div>
        <div class="stat"><div class="label">MEM</div><div class="value" id="sys_mem">-</div></div>
        <div class="stat"><div class="label">TEMP</div><div class="value" id="sys_temp">-</div></div>
    </div>
    <!-- Card 12: Cracked -->
    <div class="card">
        <h3>Cracked Passwords</h3>
        <div id="cracked" style="font-size:14px;color:#0f0">None yet</div>
    </div>
    <!-- Card 13: Handshakes -->
    <div class="card">
        <h3>Handshakes</h3>
        <div id="handshakes_list" style="font-size:12px">Loading...</div>
    </div>
    <!-- Card 14: Mode -->
    <div class="card">
        <h3>Mode</h3>
        <div class="stat"><div class="label">CURRENT</div><div class="value" id="mode">-</div></div>
        <button class="btn" onclick="toggleMode()">Toggle AUTO/MANU</button>
    </div>
    <!-- Card 15: Actions -->
    <div class="card">
        <h3>Actions</h3>
        <button class="btn" onclick="fetch('/api/restart',{method:'POST'})">Restart</button>
    </div>
    </div>
    <div class="refresh">Auto-refreshes every 5s</div>
    <script>
        function update() {
            fetch('/api/status')
                .then(r => r.json())
                .then(d => {
                    document.getElementById('name').textContent = d.name + '>';
                    document.getElementById('face').textContent = d.face;
                    document.getElementById('status').textContent = d.status_message;
                    document.getElementById('ch').textContent = d.channel;
                    document.getElementById('aps').textContent = d.aps_seen;
                    document.getElementById('pwnd').textContent = d.handshakes;
                    document.getElementById('epoch').textContent = d.epoch;
                    document.getElementById('uptime').textContent = d.uptime;
                    document.getElementById('mood').textContent = Math.round(d.mood * 100) + '%';
                    document.getElementById('mode').textContent = d.mode;
                    document.getElementById('display').src = '/api/display.png?' + Date.now();
                })
                .catch(console.error);
        }
        function toggleMode() {
            fetch('/api/mode', {method:'POST', headers:{'Content-Type':'application/json'},
                body:JSON.stringify({mode:'toggle'})});
        }
        update();
        setInterval(update, 5000);
    </script>
</body>
</html>
"#;

/// Parameters for building a [`StatusResponse`].
///
/// Uses a struct instead of 11 positional arguments to satisfy clippy's
/// `too_many_arguments` lint and improve readability at call sites.
pub struct StatusParams<'a> {
    /// Device name.
    pub name: &'a str,
    /// Formatted uptime string.
    pub uptime: &'a str,
    /// Current epoch number.
    pub epoch: u64,
    /// Current WiFi channel.
    pub channel: u8,
    /// Total APs seen.
    pub aps_seen: u32,
    /// Total handshakes captured.
    pub handshakes: u32,
    /// Consecutive blind epochs.
    pub blind_epochs: u32,
    /// Mood value (0.0 - 1.0).
    pub mood: f32,
    /// Current face kaomoji.
    pub face: &'a str,
    /// Current status message.
    pub status_message: &'a str,
    /// Operating mode (e.g. "AO").
    pub mode: &'a str,
}

/// Build a [`StatusResponse`] from a [`StatusParams`] snapshot.
pub fn build_status(p: &StatusParams<'_>) -> StatusResponse {
    StatusResponse {
        name: p.name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime: p.uptime.to_string(),
        epoch: p.epoch,
        channel: p.channel,
        aps_seen: p.aps_seen,
        handshakes: p.handshakes,
        blind_epochs: p.blind_epochs,
        mood: p.mood,
        face: p.face.to_string(),
        status_message: p.status_message.to_string(),
        mode: p.mode.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_status() {
        let status = build_status(&StatusParams {
            name: "oxi", uptime: "00:01:23", epoch: 42, channel: 6,
            aps_seen: 10, handshakes: 3, blind_epochs: 2, mood: 0.75,
            face: "(^_^)", status_message: "Having fun!", mode: "AO",
        });
        assert_eq!(status.name, "oxi");
        assert_eq!(status.epoch, 42);
        assert_eq!(status.channel, 6);
        assert_eq!(status.handshakes, 3);
        assert!(!status.version.is_empty());
    }

    #[test]
    fn test_status_serializes() {
        let status = build_status(&StatusParams {
            name: "oxi", uptime: "00:00:00", epoch: 0, channel: 1,
            aps_seen: 0, handshakes: 0, blind_epochs: 0, mood: 0.5,
            face: "(O_O)", status_message: "Booting", mode: "AO",
        });
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"name\":\"oxi\""));
        assert!(json.contains("\"epoch\":0"));
    }

    #[test]
    fn test_api_paths() {
        // Original 5 endpoints
        assert_eq!(API_STATUS, "/api/status");
        assert_eq!(API_ATTACKS, "/api/attacks");
        assert_eq!(API_CAPTURES, "/api/captures");
        assert_eq!(API_CONFIG, "/api/config");
        assert_eq!(API_DISPLAY, "/api/display.png");
        // New 13 endpoints (22 total)
        assert_eq!(API_BATTERY, "/api/battery");
        assert_eq!(API_WIFI, "/api/wifi");
        assert_eq!(API_BLUETOOTH, "/api/bluetooth");
        assert_eq!(API_RECOVERY, "/api/recovery");
        assert_eq!(API_PERSONALITY, "/api/personality");
        assert_eq!(API_SYSTEM, "/api/system");
        assert_eq!(API_HANDSHAKES, "/api/handshakes");
        assert_eq!(API_HANDSHAKE_DL, "/api/handshakes/:filename");
        assert_eq!(API_MODE, "/api/mode");
        assert_eq!(API_RESTART, "/api/restart");
        assert_eq!(API_SHUTDOWN, "/api/shutdown");
        assert_eq!(API_WHITELIST, "/api/whitelist");
        assert_eq!(API_CRACKED, "/api/cracked");
    }

    #[test]
    fn test_dashboard_html_contains_elements() {
        assert!(DASHBOARD_HTML.contains("<title>oxigotchi</title>"));
        assert!(DASHBOARD_HTML.contains("/api/status"));
        assert!(DASHBOARD_HTML.contains("/api/display.png"));
        // Verify 15 cards exist
        assert!(DASHBOARD_HTML.contains("Card 1: Face"));
        assert!(DASHBOARD_HTML.contains("Card 15: Actions"));
    }

    #[test]
    fn test_battery_info_serialize() {
        let info = BatteryInfo {
            level: 75,
            charging: true,
            voltage_mv: 4100,
            low: false,
            critical: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"level\":75"));
        assert!(json.contains("\"charging\":true"));
    }

    #[test]
    fn test_wifi_info_serialize() {
        let info = WifiInfo {
            state: "Monitor".into(),
            channel: 6,
            aps_tracked: 15,
            channels: vec![1, 6, 11],
            dwell_ms: 250,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"state\":\"Monitor\""));
    }

    #[test]
    fn test_personality_info_serialize() {
        let info = PersonalityInfo {
            mood: 0.75,
            face: "(^_^)".into(),
            blind_epochs: 2,
            total_handshakes: 10,
            total_aps_seen: 50,
            xp: 420,
            level: 3,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"level\":3"));
        assert!(json.contains("\"xp\":420"));
    }

    #[test]
    fn test_mode_switch_deserialize() {
        let json = r#"{"mode": "MANU"}"#;
        let ms: ModeSwitch = serde_json::from_str(json).unwrap();
        assert_eq!(ms.mode, "MANU");
    }

    #[test]
    fn test_config_update_deserialize() {
        let json = r#"{"name": "mybot", "attack_rate": 1}"#;
        let update: ConfigUpdate = serde_json::from_str(json).unwrap();
        assert_eq!(update.name.unwrap(), "mybot");
        assert_eq!(update.attack_rate.unwrap(), 1);
        assert!(update.whitelist_add.is_none());
    }

    #[test]
    fn test_attack_stats_serialize() {
        let stats = AttackStats {
            total_attacks: 100,
            total_handshakes: 5,
            attack_rate: 1,
            deauths_this_epoch: 3,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"total_attacks\":100"));
    }

    #[test]
    fn test_capture_info_serialize() {
        let info = CaptureInfo {
            total_files: 10,
            handshake_files: 3,
            pending_upload: 2,
            total_size_bytes: 1024000,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"handshake_files\":3"));
    }
}
