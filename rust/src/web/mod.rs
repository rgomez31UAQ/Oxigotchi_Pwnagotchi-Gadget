//! Web dashboard module (axum HTTP server).
//!
//! Provides a REST API and embedded HTML dashboard for monitoring
//! and configuring oxigotchi.

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

/// API router paths.
pub const API_STATUS: &str = "/api/status";
pub const API_ATTACKS: &str = "/api/attacks";
pub const API_CAPTURES: &str = "/api/captures";
pub const API_CONFIG: &str = "/api/config";
pub const API_DISPLAY: &str = "/api/display.png";

/// Embedded HTML for the dashboard.
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
        h1 { color: #e94560; }
        #display { image-rendering: pixelated; width: 500px; border: 1px solid #333; }
        .refresh { color: #666; font-size: 10px; }
    </style>
</head>
<body>
    <h1 id="name">oxigotchi</h1>
    <div class="card">
        <div class="face" id="face">(O_O)</div>
        <div style="text-align:center" id="status">Loading...</div>
    </div>
    <div class="card">
        <div class="stat"><div class="label">CH</div><div class="value" id="ch">-</div></div>
        <div class="stat"><div class="label">APS</div><div class="value" id="aps">-</div></div>
        <div class="stat"><div class="label">PWND</div><div class="value" id="pwnd">-</div></div>
        <div class="stat"><div class="label">EPOCH</div><div class="value" id="epoch">-</div></div>
        <div class="stat"><div class="label">UPTIME</div><div class="value" id="uptime">-</div></div>
        <div class="stat"><div class="label">MOOD</div><div class="value" id="mood">-</div></div>
    </div>
    <div class="card">
        <h3>E-Ink Preview</h3>
        <img id="display" src="/api/display.png" alt="display preview">
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
                    document.getElementById('display').src = '/api/display.png?' + Date.now();
                })
                .catch(console.error);
        }
        update();
        setInterval(update, 5000);
    </script>
</body>
</html>
"#;

/// Build a StatusResponse from current state.
pub fn build_status(
    name: &str,
    uptime: &str,
    epoch: u64,
    channel: u8,
    aps_seen: u32,
    handshakes: u32,
    blind_epochs: u32,
    mood: f32,
    face: &str,
    status_message: &str,
    mode: &str,
) -> StatusResponse {
    StatusResponse {
        name: name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime: uptime.to_string(),
        epoch,
        channel,
        aps_seen,
        handshakes,
        blind_epochs,
        mood,
        face: face.to_string(),
        status_message: status_message.to_string(),
        mode: mode.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_status() {
        let status = build_status(
            "oxi", "00:01:23", 42, 6, 10, 3, 2, 0.75, "(^_^)", "Having fun!", "AO",
        );
        assert_eq!(status.name, "oxi");
        assert_eq!(status.epoch, 42);
        assert_eq!(status.channel, 6);
        assert_eq!(status.handshakes, 3);
        assert!(!status.version.is_empty());
    }

    #[test]
    fn test_status_serializes() {
        let status = build_status(
            "oxi", "00:00:00", 0, 1, 0, 0, 0, 0.5, "(O_O)", "Booting", "AO",
        );
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"name\":\"oxi\""));
        assert!(json.contains("\"epoch\":0"));
    }

    #[test]
    fn test_api_paths() {
        assert_eq!(API_STATUS, "/api/status");
        assert_eq!(API_ATTACKS, "/api/attacks");
        assert_eq!(API_CAPTURES, "/api/captures");
        assert_eq!(API_CONFIG, "/api/config");
        assert_eq!(API_DISPLAY, "/api/display.png");
    }

    #[test]
    fn test_dashboard_html_contains_elements() {
        assert!(DASHBOARD_HTML.contains("<title>oxigotchi</title>"));
        assert!(DASHBOARD_HTML.contains("/api/status"));
        assert!(DASHBOARD_HTML.contains("/api/display.png"));
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
