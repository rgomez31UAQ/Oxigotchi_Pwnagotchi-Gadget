//! Web dashboard module (axum HTTP server).
//!
//! Provides a REST API and embedded HTML dashboard for monitoring
//! and configuring oxigotchi. The axum router shares DaemonState via
//! Arc<Mutex<DaemonState>>.
//!
//! All 15 dashboard cards from the Python angryoxide.py plugin are
//! replicated here with htmx auto-refresh and the same dark theme.

use axum::{
    extract::State,
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Shared daemon state (the web server reads/writes this via Arc<Mutex>)
// ---------------------------------------------------------------------------

/// Snapshot of all daemon state that the web server needs access to.
pub struct DaemonState {
    // -- status --
    pub name: String,
    pub uptime_str: String,
    pub epoch: u64,
    pub channel: u8,
    pub aps_seen: u32,
    pub handshakes: u32,
    pub blind_epochs: u32,
    pub mood: f32,
    pub face: String,
    pub status_message: String,
    pub mode: String,

    // -- attacks --
    pub total_attacks: u64,
    pub total_handshakes_attacks: u64,
    pub attack_rate: u32,
    pub deauths_this_epoch: u32,
    /// Per-attack-type toggles: deauth, pmkid, csa, disassoc, anon_reassoc, rogue_m2
    pub attack_deauth: bool,
    pub attack_pmkid: bool,
    pub attack_csa: bool,
    pub attack_disassoc: bool,
    pub attack_anon_reassoc: bool,
    pub attack_rogue_m2: bool,

    // -- captures --
    pub capture_files: usize,
    pub handshake_files: usize,
    pub pending_upload: usize,
    pub total_capture_size: u64,
    pub capture_list: Vec<CaptureEntry>,

    // -- battery --
    pub battery_level: u8,
    pub battery_charging: bool,
    pub battery_voltage_mv: u16,
    pub battery_low: bool,
    pub battery_critical: bool,
    pub battery_available: bool,

    // -- wifi --
    pub wifi_state: String,
    pub wifi_aps_tracked: usize,
    pub wifi_channels: Vec<u8>,
    pub wifi_dwell_ms: u64,

    // -- bluetooth --
    pub bt_state: String,
    pub bt_connected: bool,
    pub bt_device_name: String,
    pub bt_ip: String,
    pub bt_phone_mac: String,
    pub bt_internet_available: bool,
    pub bt_retry_count: u32,

    // -- ao --
    pub ao_state: String,
    pub ao_pid: u32,
    pub ao_crash_count: u32,
    pub ao_uptime: String,

    // -- personality / XP --
    pub xp: u64,
    pub level: u32,

    // -- system info --
    pub cpu_temp_c: f32,
    pub mem_used_mb: u32,
    pub mem_total_mb: u32,
    pub disk_used_mb: u32,
    pub disk_total_mb: u32,
    pub cpu_percent: f32,
    pub boot_time: Instant,

    // -- recovery --
    pub recovery_state: String,
    pub recovery_total: u32,
    pub recovery_soft_retries: u32,
    pub recovery_hard_retries: u32,
    pub recovery_last_str: String,

    // -- cracked passwords --
    pub cracked: Vec<CrackedEntry>,

    // -- display framebuffer snapshot (250x122, 1-bit packed, MSB first) --
    pub screen_width: u32,
    pub screen_height: u32,
    pub screen_bytes: Vec<u8>,

    // -- action requests from web -> daemon --
    pub pending_mode_switch: Option<String>,
    pub pending_rate_change: Option<u32>,
    pub pending_restart: bool,
    pub pending_shutdown: bool,
    pub pending_pwnagotchi_restart: bool,
    pub pending_attack_toggle: Option<AttackToggle>,
    pub pending_bt_toggle: Option<bool>,
}

impl DaemonState {
    /// Create a default state for startup.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            uptime_str: "00:00:00".into(),
            epoch: 0,
            channel: 0,
            aps_seen: 0,
            handshakes: 0,
            blind_epochs: 0,
            mood: 0.5,
            face: "(O_O)".into(),
            status_message: "Booting...".into(),
            mode: "AO".into(),
            total_attacks: 0,
            total_handshakes_attacks: 0,
            attack_rate: 1,
            deauths_this_epoch: 0,
            attack_deauth: true,
            attack_pmkid: true,
            attack_csa: true,
            attack_disassoc: true,
            attack_anon_reassoc: true,
            attack_rogue_m2: true,
            capture_files: 0,
            handshake_files: 0,
            pending_upload: 0,
            total_capture_size: 0,
            capture_list: Vec::new(),
            battery_level: 100,
            battery_charging: false,
            battery_voltage_mv: 4200,
            battery_low: false,
            battery_critical: false,
            battery_available: false,
            wifi_state: "Down".into(),
            wifi_aps_tracked: 0,
            wifi_channels: vec![1, 6, 11],
            wifi_dwell_ms: 2000,
            bt_state: "Off".into(),
            bt_connected: false,
            bt_device_name: String::new(),
            bt_ip: String::new(),
            bt_phone_mac: String::new(),
            bt_internet_available: false,
            bt_retry_count: 0,
            ao_state: "STOPPED".into(),
            ao_pid: 0,
            ao_crash_count: 0,
            ao_uptime: "N/A".into(),
            xp: 0,
            level: 1,
            cpu_temp_c: 0.0,
            mem_used_mb: 0,
            mem_total_mb: 0,
            disk_used_mb: 0,
            disk_total_mb: 0,
            cpu_percent: 0.0,
            boot_time: Instant::now(),
            recovery_state: "Healthy".into(),
            recovery_total: 0,
            recovery_soft_retries: 0,
            recovery_hard_retries: 0,
            recovery_last_str: "never".into(),
            cracked: Vec::new(),
            screen_width: 250,
            screen_height: 122,
            screen_bytes: Vec::new(),
            pending_mode_switch: None,
            pending_rate_change: None,
            pending_restart: false,
            pending_shutdown: false,
            pending_pwnagotchi_restart: false,
            pending_attack_toggle: None,
            pending_bt_toggle: None,
        }
    }
}

/// Shared state type used by axum handlers.
pub type SharedState = Arc<Mutex<DaemonState>>;

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

/// System status snapshot returned by /api/status.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Attack stats returned by GET /api/attacks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackStats {
    pub total_attacks: u64,
    pub total_handshakes: u64,
    pub attack_rate: u32,
    pub deauths_this_epoch: u32,
    pub deauth: bool,
    pub pmkid: bool,
    pub csa: bool,
    pub disassoc: bool,
    pub anon_reassoc: bool,
    pub rogue_m2: bool,
}

/// Attack toggle request for POST /api/attacks.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AttackToggle {
    #[serde(default)]
    pub deauth: Option<bool>,
    #[serde(default)]
    pub pmkid: Option<bool>,
    #[serde(default)]
    pub csa: Option<bool>,
    #[serde(default)]
    pub disassoc: Option<bool>,
    #[serde(default)]
    pub anon_reassoc: Option<bool>,
    #[serde(default)]
    pub rogue_m2: Option<bool>,
}

/// Capture info returned by /api/captures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureInfo {
    pub total_files: usize,
    pub handshake_files: usize,
    pub pending_upload: usize,
    pub total_size_bytes: u64,
    pub files: Vec<CaptureEntry>,
}

/// A single capture file entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureEntry {
    pub filename: String,
    pub size_bytes: u64,
}

/// Health response returned by /api/health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub wifi_state: String,
    pub battery_level: u8,
    pub battery_charging: bool,
    pub battery_available: bool,
    pub uptime_secs: u64,
    pub ao_state: String,
    pub ao_pid: u32,
    pub ao_crash_count: u32,
    pub ao_uptime: String,
}

/// Mode switch request for POST /api/mode.
#[derive(Debug, Clone, Deserialize)]
pub struct ModeSwitch {
    pub mode: String,
}

/// Rate change request for POST /api/rate.
#[derive(Debug, Clone, Deserialize)]
pub struct RateChange {
    pub rate: u32,
}

/// Generic action response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResponse {
    pub ok: bool,
    pub message: String,
}

/// Config update request for /api/config.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigUpdate {
    pub name: Option<String>,
    pub attack_rate: Option<u32>,
    pub channel_dwell_ms: Option<u64>,
    pub whitelist_add: Option<String>,
    pub whitelist_remove: Option<String>,
}

/// Battery info returned by /api/battery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    pub level: u8,
    pub charging: bool,
    pub voltage_mv: u16,
    pub low: bool,
    pub critical: bool,
    pub available: bool,
}

/// WiFi info returned by /api/wifi.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiInfo {
    pub state: String,
    pub channel: u8,
    pub aps_tracked: usize,
    pub channels: Vec<u8>,
    pub dwell_ms: u64,
}

/// Bluetooth info returned by /api/bluetooth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BluetoothInfo {
    pub connected: bool,
    pub state: String,
    pub device_name: String,
    pub ip: String,
    pub phone_mac: String,
    pub internet_available: bool,
    pub retry_count: u32,
}

/// Bluetooth visibility toggle request.
#[derive(Debug, Clone, Deserialize)]
pub struct BtVisibilityToggle {
    pub visible: bool,
}

/// Recovery/health info returned by /api/recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryInfo {
    pub state: String,
    pub total_recoveries: u32,
    pub soft_retries: u32,
    pub hard_retries: u32,
    pub last_recovery: String,
    pub diagnostic_count: usize,
}

/// Personality/mood info returned by /api/personality.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfoResponse {
    pub cpu_temp_c: f32,
    pub mem_used_mb: u32,
    pub mem_total_mb: u32,
    pub disk_used_mb: u32,
    pub disk_total_mb: u32,
    pub cpu_percent: f32,
    pub uptime_secs: u64,
}

/// Handshake file entry returned by /api/handshakes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeEntry {
    pub filename: String,
    pub ssid: String,
    pub size_bytes: u64,
    pub uploaded: bool,
}

/// A cracked password entry returned by /api/cracked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrackedEntry {
    pub ssid: String,
    pub bssid: String,
    pub password: String,
}

// ---------------------------------------------------------------------------
// API route constants
// ---------------------------------------------------------------------------

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
pub const API_HEALTH: &str = "/api/health";
pub const API_RATE: &str = "/api/rate";

// ---------------------------------------------------------------------------
// StatusParams helper (used by main.rs to build StatusResponse)
// ---------------------------------------------------------------------------

/// Parameters for building a [`StatusResponse`].
pub struct StatusParams<'a> {
    pub name: &'a str,
    pub uptime: &'a str,
    pub epoch: u64,
    pub channel: u8,
    pub aps_seen: u32,
    pub handshakes: u32,
    pub blind_epochs: u32,
    pub mood: f32,
    pub face: &'a str,
    pub status_message: &'a str,
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

// ---------------------------------------------------------------------------
// System info helpers (read from /proc on Linux, stubs elsewhere)
// ---------------------------------------------------------------------------

/// Read CPU temperature from /sys/class/thermal on Linux.
fn read_cpu_temp() -> f32 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp") {
            if let Ok(millideg) = content.trim().parse::<f32>() {
                return millideg / 1000.0;
            }
        }
    }
    0.0
}

/// Read memory info from /proc/meminfo on Linux.
fn read_mem_info() -> (u32, u32) {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            let mut total_kb: u64 = 0;
            let mut available_kb: u64 = 0;
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    total_kb = line.split_whitespace().nth(1)
                        .and_then(|s| s.parse().ok()).unwrap_or(0);
                } else if line.starts_with("MemAvailable:") {
                    available_kb = line.split_whitespace().nth(1)
                        .and_then(|s| s.parse().ok()).unwrap_or(0);
                }
            }
            let total_mb = (total_kb / 1024) as u32;
            let used_mb = ((total_kb.saturating_sub(available_kb)) / 1024) as u32;
            return (used_mb, total_mb);
        }
    }
    (0, 0)
}

/// Read disk usage for the root partition.
fn read_disk_info() -> (u32, u32) {
    #[cfg(target_os = "linux")]
    {
        // Use statvfs via libc
        unsafe {
            let path = std::ffi::CString::new("/").unwrap();
            let mut stat: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(path.as_ptr(), &mut stat) == 0 {
                let total = (stat.f_blocks as u64 * stat.f_frsize as u64) / (1024 * 1024);
                let avail = (stat.f_bavail as u64 * stat.f_frsize as u64) / (1024 * 1024);
                return ((total - avail) as u32, total as u32);
            }
        }
    }
    (0, 0)
}

// ---------------------------------------------------------------------------
// Axum route handlers
// ---------------------------------------------------------------------------

/// GET / -> dashboard HTML
async fn dashboard_handler() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

/// GET /api/status -> JSON status
async fn status_handler(State(state): State<SharedState>) -> Json<StatusResponse> {
    let s = state.lock().unwrap();
    Json(StatusResponse {
        name: s.name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime: s.uptime_str.clone(),
        epoch: s.epoch,
        channel: s.channel,
        aps_seen: s.aps_seen,
        handshakes: s.handshakes,
        blind_epochs: s.blind_epochs,
        mood: s.mood,
        face: s.face.clone(),
        status_message: s.status_message.clone(),
        mode: s.mode.clone(),
    })
}

/// GET /api/captures -> JSON capture list
async fn captures_handler(State(state): State<SharedState>) -> Json<CaptureInfo> {
    let s = state.lock().unwrap();
    Json(CaptureInfo {
        total_files: s.capture_files,
        handshake_files: s.handshake_files,
        pending_upload: s.pending_upload,
        total_size_bytes: s.total_capture_size,
        files: s.capture_list.clone(),
    })
}

/// GET /api/health -> JSON system health
async fn health_handler(State(state): State<SharedState>) -> Json<HealthResponse> {
    let s = state.lock().unwrap();
    Json(HealthResponse {
        wifi_state: s.wifi_state.clone(),
        battery_level: s.battery_level,
        battery_charging: s.battery_charging,
        battery_available: s.battery_available,
        uptime_secs: s.boot_time.elapsed().as_secs(),
        ao_state: s.ao_state.clone(),
        ao_pid: s.ao_pid,
        ao_crash_count: s.ao_crash_count,
        ao_uptime: s.ao_uptime.clone(),
    })
}

/// GET /api/battery -> JSON battery info
async fn battery_handler(State(state): State<SharedState>) -> Json<BatteryInfo> {
    let s = state.lock().unwrap();
    Json(BatteryInfo {
        level: s.battery_level,
        charging: s.battery_charging,
        voltage_mv: s.battery_voltage_mv,
        low: s.battery_low,
        critical: s.battery_critical,
        available: s.battery_available,
    })
}

/// GET /api/wifi -> JSON wifi info
async fn wifi_handler(State(state): State<SharedState>) -> Json<WifiInfo> {
    let s = state.lock().unwrap();
    Json(WifiInfo {
        state: s.wifi_state.clone(),
        channel: s.channel,
        aps_tracked: s.wifi_aps_tracked,
        channels: s.wifi_channels.clone(),
        dwell_ms: s.wifi_dwell_ms,
    })
}

/// GET /api/bluetooth -> JSON bluetooth info
async fn bluetooth_handler(State(state): State<SharedState>) -> Json<BluetoothInfo> {
    let s = state.lock().unwrap();
    Json(BluetoothInfo {
        connected: s.bt_connected,
        state: s.bt_state.clone(),
        device_name: s.bt_device_name.clone(),
        ip: s.bt_ip.clone(),
        phone_mac: s.bt_phone_mac.clone(),
        internet_available: s.bt_internet_available,
        retry_count: s.bt_retry_count,
    })
}

/// POST /api/bluetooth -> toggle bluetooth visibility
async fn bluetooth_toggle_handler(
    State(state): State<SharedState>,
    Json(body): Json<BtVisibilityToggle>,
) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    s.pending_bt_toggle = Some(body.visible);
    Json(ActionResponse {
        ok: true,
        message: format!("Bluetooth visibility {} queued", if body.visible { "ON" } else { "OFF" }),
    })
}

/// GET /api/personality -> JSON personality/mood info
async fn personality_handler(State(state): State<SharedState>) -> Json<PersonalityInfo> {
    let s = state.lock().unwrap();
    Json(PersonalityInfo {
        mood: s.mood,
        face: s.face.clone(),
        blind_epochs: s.blind_epochs,
        total_handshakes: s.handshakes,
        total_aps_seen: s.aps_seen,
        xp: s.xp,
        level: s.level,
    })
}

/// GET /api/system -> JSON system info (CPU, memory, disk)
async fn system_handler(State(state): State<SharedState>) -> Json<SystemInfoResponse> {
    // Read live system info where available
    let cpu_temp = read_cpu_temp();
    let (mem_used, mem_total) = read_mem_info();
    let (disk_used, disk_total) = read_disk_info();
    let s = state.lock().unwrap();
    Json(SystemInfoResponse {
        cpu_temp_c: if cpu_temp > 0.0 { cpu_temp } else { s.cpu_temp_c },
        mem_used_mb: if mem_total > 0 { mem_used } else { s.mem_used_mb },
        mem_total_mb: if mem_total > 0 { mem_total } else { s.mem_total_mb },
        disk_used_mb: if disk_total > 0 { disk_used } else { s.disk_used_mb },
        disk_total_mb: if disk_total > 0 { disk_total } else { s.disk_total_mb },
        cpu_percent: s.cpu_percent,
        uptime_secs: s.boot_time.elapsed().as_secs(),
    })
}

/// GET /api/attacks -> JSON attack stats + toggles
async fn attacks_get_handler(State(state): State<SharedState>) -> Json<AttackStats> {
    let s = state.lock().unwrap();
    Json(AttackStats {
        total_attacks: s.total_attacks,
        total_handshakes: s.total_handshakes_attacks,
        attack_rate: s.attack_rate,
        deauths_this_epoch: s.deauths_this_epoch,
        deauth: s.attack_deauth,
        pmkid: s.attack_pmkid,
        csa: s.attack_csa,
        disassoc: s.attack_disassoc,
        anon_reassoc: s.attack_anon_reassoc,
        rogue_m2: s.attack_rogue_m2,
    })
}

/// POST /api/attacks -> toggle attack types
async fn attacks_post_handler(
    State(state): State<SharedState>,
    Json(body): Json<AttackToggle>,
) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    // Apply toggles immediately to state; daemon will pick them up
    if let Some(v) = body.deauth { s.attack_deauth = v; }
    if let Some(v) = body.pmkid { s.attack_pmkid = v; }
    if let Some(v) = body.csa { s.attack_csa = v; }
    if let Some(v) = body.disassoc { s.attack_disassoc = v; }
    if let Some(v) = body.anon_reassoc { s.attack_anon_reassoc = v; }
    if let Some(v) = body.rogue_m2 { s.attack_rogue_m2 = v; }
    s.pending_attack_toggle = Some(body);
    Json(ActionResponse {
        ok: true,
        message: "Attack toggles updated".into(),
    })
}

/// GET /api/recovery -> JSON recovery info
async fn recovery_handler(State(state): State<SharedState>) -> Json<RecoveryInfo> {
    let s = state.lock().unwrap();
    Json(RecoveryInfo {
        state: s.recovery_state.clone(),
        total_recoveries: s.recovery_total,
        soft_retries: s.recovery_soft_retries,
        hard_retries: s.recovery_hard_retries,
        last_recovery: s.recovery_last_str.clone(),
        diagnostic_count: 0,
    })
}

/// GET /api/cracked -> JSON list of cracked passwords
async fn cracked_handler(State(state): State<SharedState>) -> Json<Vec<CrackedEntry>> {
    let s = state.lock().unwrap();
    Json(s.cracked.clone())
}

/// POST /api/mode -> switch mode
async fn mode_handler(
    State(state): State<SharedState>,
    Json(body): Json<ModeSwitch>,
) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    let new_mode = if body.mode == "toggle" {
        if s.mode == "AO" { "PWN".to_string() } else { "AO".to_string() }
    } else {
        body.mode.to_uppercase()
    };
    s.pending_mode_switch = Some(new_mode.clone());
    Json(ActionResponse {
        ok: true,
        message: format!("Mode switch to {} queued", new_mode),
    })
}

/// POST /api/rate -> change attack rate
async fn rate_handler(
    State(state): State<SharedState>,
    Json(body): Json<RateChange>,
) -> Json<ActionResponse> {
    let rate = body.rate.clamp(1, 3);
    let mut s = state.lock().unwrap();
    s.pending_rate_change = Some(rate);
    Json(ActionResponse {
        ok: true,
        message: format!("Rate change to {} queued", rate),
    })
}

/// POST /api/restart -> restart AO
async fn restart_handler(State(state): State<SharedState>) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    s.pending_restart = true;
    Json(ActionResponse {
        ok: true,
        message: "AO restart queued".into(),
    })
}

/// POST /api/shutdown -> system shutdown
async fn shutdown_handler(State(state): State<SharedState>) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    s.pending_shutdown = true;
    Json(ActionResponse {
        ok: true,
        message: "System shutdown queued".into(),
    })
}

// ---------------------------------------------------------------------------
// Display framebuffer endpoint
// ---------------------------------------------------------------------------

/// GET /api/display.png -> 1-bit BMP of the current e-ink framebuffer.
/// Returns a 250x122 monochrome BMP image.
async fn display_handler(
    State(state): State<SharedState>,
) -> axum::response::Response<axum::body::Body> {
    use axum::http::{header, StatusCode};

    let s = state.lock().unwrap();
    let w = s.screen_width;
    let h = s.screen_height;
    let fb = s.screen_bytes.clone();
    drop(s);

    if fb.is_empty() {
        return axum::response::Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(axum::body::Body::from("no framebuffer yet"))
            .unwrap();
    }

    // Build a 1-bit BMP (monochrome, uncompressed).
    // BMP stores rows bottom-to-top, each row padded to 4-byte boundary.
    let fb_stride = ((w + 7) / 8) as usize; // 32 bytes per row in our framebuffer
    let bmp_stride = ((w + 31) / 32 * 4) as usize; // 32 bytes (250 bits -> 32 bytes, already 4-byte aligned)
    let pixel_data_size = bmp_stride * h as usize;
    let file_header_size = 14u32;
    let dib_header_size = 40u32;
    let color_table_size = 8u32; // 2 entries * 4 bytes each
    let pixel_offset = file_header_size + dib_header_size + color_table_size;
    let file_size = pixel_offset + pixel_data_size as u32;

    let mut bmp = Vec::with_capacity(file_size as usize);

    // File header (14 bytes)
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&[0u8; 4]); // reserved
    bmp.extend_from_slice(&pixel_offset.to_le_bytes());

    // DIB header (BITMAPINFOHEADER, 40 bytes)
    bmp.extend_from_slice(&dib_header_size.to_le_bytes());
    bmp.extend_from_slice(&(w as i32).to_le_bytes());
    bmp.extend_from_slice(&(h as i32).to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes()); // planes
    bmp.extend_from_slice(&1u16.to_le_bytes()); // bits per pixel
    bmp.extend_from_slice(&0u32.to_le_bytes()); // compression (none)
    bmp.extend_from_slice(&(pixel_data_size as u32).to_le_bytes());
    bmp.extend_from_slice(&2835i32.to_le_bytes()); // h resolution (72 DPI)
    bmp.extend_from_slice(&2835i32.to_le_bytes()); // v resolution
    bmp.extend_from_slice(&0u32.to_le_bytes()); // colors used
    bmp.extend_from_slice(&0u32.to_le_bytes()); // important colors

    // Color table: index 0 = black (0x00), index 1 = white (0xFF)
    // In our framebuffer: bit 1 = black (On), bit 0 = white (Off)
    // BMP: palette[0] for bit=0, palette[1] for bit=1
    // So palette[0] = white, palette[1] = black
    bmp.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0x00]); // palette[0] = white (bit 0 = Off)
    bmp.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // palette[1] = black (bit 1 = On)

    // Pixel data: BMP is bottom-to-top, our framebuffer is top-to-bottom
    for row in (0..h as usize).rev() {
        let fb_row_start = row * fb_stride;
        let fb_row_end = fb_row_start + fb_stride;
        if fb_row_end <= fb.len() {
            bmp.extend_from_slice(&fb[fb_row_start..fb_row_end]);
        } else {
            bmp.extend_from_slice(&vec![0u8; bmp_stride]);
        }
        // Pad to bmp_stride if needed
        if fb_stride < bmp_stride {
            bmp.extend_from_slice(&vec![0u8; bmp_stride - fb_stride]);
        }
    }

    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/bmp")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(axum::body::Body::from(bmp))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the axum router with all routes, sharing daemon state.
pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/", get(dashboard_handler))
        .route(API_STATUS, get(status_handler))
        .route(API_CAPTURES, get(captures_handler))
        .route(API_HEALTH, get(health_handler))
        .route(API_BATTERY, get(battery_handler))
        .route(API_WIFI, get(wifi_handler))
        .route(API_BLUETOOTH, get(bluetooth_handler).post(bluetooth_toggle_handler))
        .route(API_PERSONALITY, get(personality_handler))
        .route(API_SYSTEM, get(system_handler))
        .route(API_ATTACKS, get(attacks_get_handler).post(attacks_post_handler))
        .route(API_RECOVERY, get(recovery_handler))
        .route(API_CRACKED, get(cracked_handler))
        .route(API_MODE, post(mode_handler))
        .route(API_RATE, post(rate_handler))
        .route(API_RESTART, post(restart_handler))
        .route(API_SHUTDOWN, post(shutdown_handler))
        .route(API_DISPLAY, get(display_handler))
        .with_state(state)
}

/// Start the axum web server on 0.0.0.0:8080.
/// This function is async and should be spawned as a tokio task.
pub async fn start_server(state: SharedState) {
    let app = build_router(state);
    let listener = match tokio::net::TcpListener::bind("0.0.0.0:8080").await {
        Ok(l) => l,
        Err(e) => {
            log::error!("failed to bind web server on 0.0.0.0:8080: {e}");
            return;
        }
    };
    log::info!("web dashboard listening on http://0.0.0.0:8080");
    if let Err(e) = axum::serve(listener, app).await {
        log::error!("web server error: {e}");
    }
}

// ---------------------------------------------------------------------------
// Embedded dashboard HTML — 15 cards, htmx auto-refresh, dark theme
// ---------------------------------------------------------------------------

pub const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0, user-scalable=no">
<title>oxigotchi</title>
<script src="https://unpkg.com/htmx.org@1.9.10"></script>
<style>
*{box-sizing:border-box;margin:0;padding:0}
body{background:#1a1a2e;color:#e0e0e0;font-family:'SF Mono','Fira Code','Cascadia Code',monospace;font-size:14px;padding:12px;max-width:600px;margin:0 auto}
h1{color:#00d4aa;font-size:20px;text-align:center;margin-bottom:16px;letter-spacing:1px}
.card{background:#16213e;border-radius:12px;padding:16px;margin-bottom:12px}
.card-title{color:#00d4aa;font-size:15px;font-weight:bold;margin-bottom:12px;padding-bottom:8px;border-bottom:1px solid #0f3460}
.face{font-size:48px;text-align:center;padding:20px;color:#e0e0e0}
.status-grid{display:grid;grid-template-columns:1fr 1fr;gap:6px 16px}
.status-grid .label{color:#888;font-size:12px}
.status-grid .value{color:#e0e0e0;font-size:13px;font-weight:bold}
.stat-row{display:flex;flex-wrap:wrap;gap:8px}
.stat{text-align:center;flex:1;min-width:60px}
.stat .label{color:#888;font-size:11px}
.stat .value{color:#00d4aa;font-size:18px;font-weight:bold}
.health-row{display:flex;flex-wrap:wrap;gap:10px;margin-bottom:4px}
.health-item{display:flex;align-items:center;gap:6px;font-size:13px}
.dot{width:10px;height:10px;border-radius:50%;display:inline-block}
.dot-green{background:#00d4aa}
.dot-red{background:#e94560}
.dot-gray{background:#555}
.dot-yellow{background:#f0c040}
.toggle-row{display:flex;align-items:center;justify-content:space-between;padding:10px 0;border-bottom:1px solid #0f3460}
.toggle-row:last-child{border-bottom:none}
.toggle-info{flex:1;margin-right:12px}
.toggle-label{font-size:14px;font-weight:bold;color:#e0e0e0}
.toggle-desc{font-size:11px;color:#888;margin-top:2px}
.switch{position:relative;width:50px;height:28px;flex-shrink:0}
.switch input{opacity:0;width:0;height:0}
.slider{position:absolute;cursor:pointer;top:0;left:0;right:0;bottom:0;background:#555;border-radius:28px;transition:.25s}
.slider:before{position:absolute;content:"";height:22px;width:22px;left:3px;bottom:3px;background:#fff;border-radius:50%;transition:.25s}
input:checked+.slider{background:#00d4aa}
input:checked+.slider:before{transform:translateX(22px)}
.rate-btns{display:flex;gap:8px;margin-top:8px}
.rate-btn{flex:1;padding:14px 0;border:2px solid #0f3460;border-radius:10px;background:transparent;color:#e0e0e0;font-size:18px;font-weight:bold;font-family:inherit;cursor:pointer;text-align:center;transition:.2s}
.rate-btn.active{background:#0f3460;color:#00d4aa;border-color:#00d4aa}
.rate-btn.risky{border-color:#e67e22;color:#e67e22}
.rate-btn.risky.active{background:#5a3000;color:#e67e22;border-color:#e67e22}
.rate-btn:active{transform:scale(0.95)}
.mode-btns{display:flex;gap:8px;margin-top:8px}
.mode-btn{flex:1;padding:14px 0;border:2px solid #0f3460;border-radius:10px;background:transparent;color:#e0e0e0;font-size:16px;font-weight:bold;font-family:inherit;cursor:pointer;text-align:center;transition:.2s}
.mode-btn.active{background:#00d4aa;color:#1a1a2e;border-color:#00d4aa}
.mode-btn:active{transform:scale(0.95)}
.action-btns{display:flex;flex-wrap:wrap;gap:8px}
.action-btn{flex:1;min-width:100px;padding:14px 8px;border:none;border-radius:10px;font-family:inherit;font-size:13px;font-weight:bold;cursor:pointer;text-align:center;transition:.2s}
.action-btn:active{transform:scale(0.95)}
.btn-restart{background:#0f3460;color:#00d4aa}
.btn-stop{background:#e94560;color:#fff}
.btn-warn{background:#f0c040;color:#1a1a2e}
.captures-list{max-height:200px;overflow-y:auto;margin-top:8px}
.capture-item{font-size:12px;color:#aaa;padding:4px 0;border-bottom:1px solid #0f346033}
.capture-item:last-child{border-bottom:none}
.toast{position:fixed;bottom:20px;left:50%;transform:translateX(-50%);background:#00d4aa;color:#1a1a2e;padding:10px 20px;border-radius:8px;font-size:13px;font-weight:bold;opacity:0;transition:opacity .3s;pointer-events:none;z-index:999}
.toast.show{opacity:1}
.progress-bar{height:6px;background:#0f3460;border-radius:3px;overflow:hidden;margin-top:4px}
.progress-fill{height:100%;background:#00d4aa;border-radius:3px;transition:width .3s}
.grid-2{display:grid;grid-template-columns:1fr 1fr;gap:8px}
.sub{color:#888;font-size:11px;margin-bottom:8px}
</style>
</head>
<body>
<h1>Oxigotchi Dashboard</h1>
<div style="text-align:center;color:#888;font-size:11px;margin:-12px 0 14px">Rusty Oxigotchi &mdash; WiFi capture bull</div>

<!-- 1. Face display -->
<div class="card" id="card-face">
<div class="face" id="face">(O_O)</div>
<div style="text-align:center;color:#888" id="status-msg">Loading...</div>
</div>

<!-- 2. Core stats -->
<div class="card" id="card-stats">
<div class="card-title">Core Stats</div>
<div class="stat-row">
<div class="stat"><div class="label">CH</div><div class="value" id="s-ch">-</div></div>
<div class="stat"><div class="label">APS</div><div class="value" id="s-aps">-</div></div>
<div class="stat"><div class="label">PWND</div><div class="value" id="s-pwnd">-</div></div>
<div class="stat"><div class="label">EPOCH</div><div class="value" id="s-epoch">-</div></div>
<div class="stat"><div class="label">UPTIME</div><div class="value" id="s-uptime">-</div></div>
<div class="stat"><div class="label">RATE</div><div class="value" id="s-rate">-</div></div>
</div>
</div>

<!-- 3. E-ink preview -->
<div class="card" id="card-eink" style="text-align:center">
<div class="card-title">Live Display</div>
<div style="padding:8px;background:#fff;display:inline-block;border-radius:4px"><img id="eink-img" src="/api/display.png" alt="e-ink" style="width:250px;height:122px;image-rendering:pixelated"></div>
</div>

<div class="grid-2">

<!-- 4. Battery -->
<div class="card" id="card-battery">
<div class="card-title">Battery</div>
<div class="status-grid">
<div class="label">Level</div><div class="value" id="bat-level">-</div>
<div class="label">State</div><div class="value" id="bat-state">-</div>
<div class="label">Voltage</div><div class="value" id="bat-voltage">-</div>
</div>
<div class="progress-bar"><div class="progress-fill" id="bat-bar" style="width:0%"></div></div>
</div>

<!-- 5. Bluetooth -->
<div class="card" id="card-bt">
<div class="card-title">Bluetooth</div>
<div class="status-grid">
<div class="label">Status</div><div class="value" id="bt-status">-</div>
<div class="label">Device</div><div class="value" id="bt-device">-</div>
<div class="label">IP</div><div class="value" id="bt-ip">-</div>
</div>
</div>

</div>

<!-- 6. WiFi -->
<div class="card" id="card-wifi">
<div class="card-title">WiFi</div>
<div class="sub">Monitor mode status and channel info.</div>
<div class="status-grid">
<div class="label">State</div><div class="value" id="wifi-state">-</div>
<div class="label">Channel</div><div class="value" id="wifi-ch">-</div>
<div class="label">APs Tracked</div><div class="value" id="wifi-aps">-</div>
<div class="label">Channels</div><div class="value" id="wifi-channels">-</div>
<div class="label">Dwell</div><div class="value" id="wifi-dwell">-</div>
</div>
</div>

<!-- 7. Attack controls -->
<div class="card" id="card-attacks">
<div class="card-title">Attack Types</div>
<div style="color:#00d4aa;font-size:11px;margin-bottom:10px;padding:8px;background:#0f346033;border-radius:6px">All 6 ON is the sweet spot &mdash; they complement each other.</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Deauth</div><div class="toggle-desc">Kick clients to capture reconnection handshakes</div></div>
<label class="switch"><input type="checkbox" id="atk-deauth" checked onchange="toggleAttack('deauth',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">PMKID</div><div class="toggle-desc">Grab router password hashes without clients</div></div>
<label class="switch"><input type="checkbox" id="atk-pmkid" checked onchange="toggleAttack('pmkid',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">CSA</div><div class="toggle-desc">Trick clients into switching channels</div></div>
<label class="switch"><input type="checkbox" id="atk-csa" checked onchange="toggleAttack('csa',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Disassociation</div><div class="toggle-desc">Catches clients that resist deauth</div></div>
<label class="switch"><input type="checkbox" id="atk-disassoc" checked onchange="toggleAttack('disassoc',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Anon Reassoc</div><div class="toggle-desc">Capture PMKID from stubborn routers</div></div>
<label class="switch"><input type="checkbox" id="atk-anon_reassoc" checked onchange="toggleAttack('anon_reassoc',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Rogue M2</div><div class="toggle-desc">Fake AP trick for handshakes</div></div>
<label class="switch"><input type="checkbox" id="atk-rogue_m2" checked onchange="toggleAttack('rogue_m2',this.checked)"><span class="slider"></span></label>
</div>

<div style="margin-top:12px;padding-top:10px;border-top:1px solid #0f3460">
<div style="font-size:12px;color:#888;margin-bottom:4px">Attack Rate</div>
<div class="sub">Rate 1 is max safe for BCM43436B0. Higher rates cause firmware crashes.</div>
<div class="rate-btns">
<button class="rate-btn active" id="rate-1" onclick="setRate(1)">1<br><span style="font-size:10px;font-weight:normal;color:#888">Safe</span></button>
<button class="rate-btn risky" id="rate-2" onclick="setRate(2)">2<br><span style="font-size:10px;font-weight:normal">Risky</span></button>
<button class="rate-btn risky" id="rate-3" onclick="setRate(3)">3<br><span style="font-size:10px;font-weight:normal">Danger</span></button>
</div>
</div>
</div>

<!-- 8. Capture list -->
<div class="card" id="card-captures">
<div class="card-title">Recent Captures</div>
<div class="sub">Validated capture files. Click to download.</div>
<div class="status-grid" style="margin-bottom:8px">
<div class="label">Total Files</div><div class="value" id="cap-total">-</div>
<div class="label">Handshakes</div><div class="value" id="cap-hs">-</div>
<div class="label">Pending Upload</div><div class="value" id="cap-pending">-</div>
<div class="label">Total Size</div><div class="value" id="cap-size">-</div>
</div>
<div class="captures-list" id="cap-list"><div style="color:#555;font-size:12px">Loading...</div></div>
</div>

<!-- 9. Recovery status -->
<div class="card" id="card-recovery">
<div class="card-title">Recovery Status</div>
<div class="sub">WiFi and firmware crash recovery tracking.</div>
<div class="health-row" style="margin-bottom:8px">
<div class="health-item"><span class="dot dot-gray" id="h-wifi"></span>WiFi</div>
<div class="health-item"><span class="dot dot-gray" id="h-ao"></span>AO</div>
<div class="health-item"><span class="dot dot-gray" id="h-recovery"></span>Recovery</div>
</div>
<div class="status-grid">
<div class="label">State</div><div class="value" id="rec-state">-</div>
<div class="label">Crashes</div><div class="value" id="rec-crashes">-</div>
<div class="label">Recoveries</div><div class="value" id="rec-total">-</div>
<div class="label">Last Recovery</div><div class="value" id="rec-last">-</div>
<div class="label">AO PID</div><div class="value" id="rec-pid">-</div>
<div class="label">AO Uptime</div><div class="value" id="rec-ao-up">-</div>
</div>
</div>

<!-- 10. Personality -->
<div class="card" id="card-personality">
<div class="card-title">Personality</div>
<div class="sub">Mood, experience, and level progression.</div>
<div class="status-grid">
<div class="label">Mood</div><div class="value" id="p-mood">-</div>
<div class="label">Face</div><div class="value" id="p-face">-</div>
<div class="label">XP</div><div class="value" id="p-xp">-</div>
<div class="label">Level</div><div class="value" id="p-level">-</div>
<div class="label">Blind Epochs</div><div class="value" id="p-blind">-</div>
</div>
<div class="progress-bar" style="margin-top:8px"><div class="progress-fill" id="mood-bar" style="width:50%"></div></div>
</div>

<!-- 11. System info -->
<div class="card" id="card-system">
<div class="card-title">System Info</div>
<div class="sub">Hardware stats from the Pi.</div>
<div class="status-grid">
<div class="label">CPU Temp</div><div class="value" id="sys-temp">-</div>
<div class="label">CPU Usage</div><div class="value" id="sys-cpu">-</div>
<div class="label">Memory</div><div class="value" id="sys-mem">-</div>
<div class="label">Disk</div><div class="value" id="sys-disk">-</div>
<div class="label">Sys Uptime</div><div class="value" id="sys-uptime">-</div>
</div>
</div>

<!-- 12. Cracked passwords -->
<div class="card" id="card-cracked">
<div class="card-title">Cracked Passwords</div>
<div class="sub">Passwords cracked from captured handshakes.</div>
<div id="cracked-list"><div style="color:#555;font-size:12px">No cracked passwords yet</div></div>
</div>

<!-- 13. Handshake download -->
<div class="card" id="card-download">
<div class="card-title">Download Captures</div>
<div class="sub">Download all captures as a ZIP archive.</div>
<div class="action-btns">
<!-- TODO: implement /api/handshakes/download.zip endpoint -->
<button class="action-btn btn-restart" onclick="toast('ZIP download not yet implemented')">Download All (ZIP)</button>
</div>
</div>

<!-- 14. Mode switch -->
<div class="card" id="card-mode">
<div class="card-title">Mode</div>
<div class="sub">AO Mode = AngryOxide attacks. PWN Mode = stock bettercap. Switching takes ~90s.</div>
<div class="mode-btns">
<button class="mode-btn active" id="mode-ao" onclick="switchMode('AO')">AO Mode</button>
<button class="mode-btn" id="mode-pwn" onclick="switchMode('PWN')">PWN Mode</button>
</div>
</div>

<!-- 15. Actions -->
<div class="card" id="card-actions">
<div class="card-title">Actions</div>
<div class="sub">Restart applies config changes. Shutdown powers off the Pi.</div>
<div class="action-btns">
<button class="action-btn btn-restart" onclick="restartAO()">Restart AO</button>
<button class="action-btn btn-stop" onclick="if(confirm('Shut down the Pi?'))doShutdown()">Shutdown Pi</button>
<button class="action-btn btn-warn" onclick="if(confirm('Restart pwnagotchi?'))restartPwn()">Restart Pwn</button>
</div>
</div>

<div style="text-align:center;color:#555;font-size:10px;margin-top:8px">Auto-refreshes every 5s &bull; Rusty Oxigotchi</div>

<div class="toast" id="toast"></div>

<script>
function api(method, path, body) {
    var opts = {method: method, headers: {'Content-Type':'application/json'}};
    if (body) opts.body = JSON.stringify(body);
    return fetch(path, opts).then(function(r){return r.json()}).catch(function(e){console.error('API:',path,e)});
}
function toast(msg) {
    var t = document.getElementById('toast');
    t.textContent = msg;
    t.classList.add('show');
    setTimeout(function(){t.classList.remove('show')}, 1500);
}
function fmtUptime(secs) {
    if (!secs && secs !== 0) return '--';
    var h = Math.floor(secs/3600), m = Math.floor((secs%3600)/60), s = secs%60;
    return String(h).padStart(2,'0')+':'+String(m).padStart(2,'0')+':'+String(s).padStart(2,'0');
}
function fmtBytes(b) {
    if (b < 1024) return b + ' B';
    if (b < 1048576) return (b/1024).toFixed(1) + ' KB';
    return (b/1048576).toFixed(1) + ' MB';
}
function esc(s) { var d = document.createElement('div'); d.textContent = s; return d.innerHTML; }

// --- Refresh functions ---

function refreshStatus() {
    api('GET', '/api/status').then(function(d) {
        if (!d) return;
        document.getElementById('face').textContent = d.face;
        document.getElementById('status-msg').textContent = d.status_message;
        document.getElementById('s-ch').textContent = d.channel;
        document.getElementById('s-aps').textContent = d.aps_seen;
        document.getElementById('s-pwnd').textContent = d.handshakes;
        document.getElementById('s-epoch').textContent = d.epoch;
        document.getElementById('s-uptime').textContent = d.uptime;
        // Mode buttons
        document.getElementById('mode-ao').classList.toggle('active', d.mode === 'AO');
        document.getElementById('mode-pwn').classList.toggle('active', d.mode === 'PWN');
    });
}

function refreshBattery() {
    api('GET', '/api/battery').then(function(d) {
        if (!d) return;
        if (d.available) {
            document.getElementById('bat-level').textContent = d.level + '%';
            document.getElementById('bat-level').style.color = d.critical ? '#e94560' : (d.low ? '#f0c040' : '#00d4aa');
            document.getElementById('bat-state').textContent = d.charging ? 'Charging' : 'Discharging';
            document.getElementById('bat-voltage').textContent = (d.voltage_mv / 1000).toFixed(2) + 'V';
            document.getElementById('bat-bar').style.width = d.level + '%';
            document.getElementById('bat-bar').style.background = d.critical ? '#e94560' : (d.low ? '#f0c040' : '#00d4aa');
        } else {
            document.getElementById('bat-level').textContent = 'N/A';
            document.getElementById('bat-state').textContent = 'Not detected';
            document.getElementById('bat-voltage').textContent = '-';
        }
    });
}

function refreshBluetooth() {
    api('GET', '/api/bluetooth').then(function(d) {
        if (!d) return;
        document.getElementById('bt-status').textContent = d.connected ? 'Connected' : d.state;
        document.getElementById('bt-status').style.color = d.connected ? '#00d4aa' : '#888';
        document.getElementById('bt-device').textContent = d.device_name || '-';
        document.getElementById('bt-ip').textContent = d.ip || '-';
    });
}

function refreshWifi() {
    api('GET', '/api/wifi').then(function(d) {
        if (!d) return;
        document.getElementById('wifi-state').textContent = d.state;
        document.getElementById('wifi-state').style.color = d.state === 'Monitor' ? '#00d4aa' : '#e94560';
        document.getElementById('wifi-ch').textContent = d.channel;
        document.getElementById('wifi-aps').textContent = d.aps_tracked;
        document.getElementById('wifi-channels').textContent = d.channels.join(', ') || '-';
        document.getElementById('wifi-dwell').textContent = d.dwell_ms + 'ms';
    });
}

function refreshAttacks() {
    api('GET', '/api/attacks').then(function(d) {
        if (!d) return;
        document.getElementById('s-rate').textContent = d.attack_rate;
        ['deauth','pmkid','csa','disassoc','anon_reassoc','rogue_m2'].forEach(function(k) {
            var cb = document.getElementById('atk-'+k);
            if (cb) cb.checked = d[k];
        });
        [1,2,3].forEach(function(n) {
            document.getElementById('rate-'+n).classList.toggle('active', n === d.attack_rate);
        });
    });
}

function refreshCaptures() {
    api('GET', '/api/captures').then(function(d) {
        if (!d) return;
        document.getElementById('cap-total').textContent = d.total_files;
        document.getElementById('cap-hs').textContent = d.handshake_files;
        document.getElementById('cap-pending').textContent = d.pending_upload;
        document.getElementById('cap-size').textContent = fmtBytes(d.total_size_bytes);
        var el = document.getElementById('cap-list');
        if (!d.files || !d.files.length) {
            el.innerHTML = '<div style="color:#555;font-size:12px">No captures yet</div>';
            return;
        }
        el.innerHTML = d.files.map(function(f) {
            return '<div class="capture-item">' + esc(f.filename) + ' <span style="color:#555">(' + fmtBytes(f.size_bytes) + ')</span></div>';
        }).join('');
    });
}

function refreshRecovery() {
    api('GET', '/api/recovery').then(function(d) {
        if (!d) return;
        document.getElementById('rec-state').textContent = d.state;
        document.getElementById('rec-state').style.color = d.state === 'Healthy' ? '#00d4aa' : '#f0c040';
        document.getElementById('rec-total').textContent = d.total_recoveries;
        document.getElementById('rec-last').textContent = d.last_recovery;
    });
    api('GET', '/api/health').then(function(d) {
        if (!d) return;
        document.getElementById('rec-crashes').textContent = d.ao_crash_count;
        document.getElementById('rec-crashes').style.color = d.ao_crash_count > 0 ? '#f0c040' : '#e0e0e0';
        document.getElementById('rec-pid').textContent = d.ao_pid || '-';
        document.getElementById('rec-ao-up').textContent = d.ao_uptime;
        // Health dots
        var wdot = document.getElementById('h-wifi');
        wdot.className = 'dot ' + (d.wifi_state === 'Monitor' ? 'dot-green' : 'dot-red');
        var adot = document.getElementById('h-ao');
        adot.className = 'dot ' + (d.ao_state === 'RUNNING' ? 'dot-green' : 'dot-red');
        var rdot = document.getElementById('h-recovery');
        rdot.className = 'dot ' + (d.ao_crash_count === 0 ? 'dot-green' : 'dot-yellow');
        document.getElementById('sys-uptime').textContent = fmtUptime(d.uptime_secs);
    });
}

function refreshPersonality() {
    api('GET', '/api/personality').then(function(d) {
        if (!d) return;
        document.getElementById('p-mood').textContent = Math.round(d.mood * 100) + '%';
        document.getElementById('p-face').textContent = d.face;
        document.getElementById('p-xp').textContent = d.xp;
        document.getElementById('p-level').textContent = d.level;
        document.getElementById('p-blind').textContent = d.blind_epochs;
        document.getElementById('mood-bar').style.width = Math.round(d.mood * 100) + '%';
        var moodColor = d.mood > 0.7 ? '#00d4aa' : (d.mood > 0.3 ? '#f0c040' : '#e94560');
        document.getElementById('mood-bar').style.background = moodColor;
    });
}

function refreshSystem() {
    api('GET', '/api/system').then(function(d) {
        if (!d) return;
        document.getElementById('sys-temp').textContent = d.cpu_temp_c > 0 ? d.cpu_temp_c.toFixed(1) + '\u00B0C' : '-';
        document.getElementById('sys-temp').style.color = d.cpu_temp_c > 70 ? '#e94560' : (d.cpu_temp_c > 55 ? '#f0c040' : '#00d4aa');
        document.getElementById('sys-cpu').textContent = d.cpu_percent > 0 ? d.cpu_percent.toFixed(0) + '%' : '-';
        document.getElementById('sys-mem').textContent = d.mem_total_mb > 0 ? d.mem_used_mb + '/' + d.mem_total_mb + ' MB' : '-';
        document.getElementById('sys-disk').textContent = d.disk_total_mb > 0 ? d.disk_used_mb + '/' + d.disk_total_mb + ' MB' : '-';
    });
}

function refreshCracked() {
    api('GET', '/api/cracked').then(function(list) {
        var el = document.getElementById('cracked-list');
        if (!list || !list.length) {
            el.innerHTML = '<div style="color:#555;font-size:12px">No cracked passwords yet</div>';
            return;
        }
        el.innerHTML = list.map(function(c) {
            return '<div style="padding:4px 0;border-bottom:1px solid #0f346022">' +
                '<span style="color:#00d4aa;font-weight:bold">' + esc(c.ssid || c.bssid) + '</span>' +
                (c.bssid ? ' <span style="color:#666;font-size:10px">[' + esc(c.bssid) + ']</span>' : '') +
                '<br><span style="color:#f0c040;font-family:monospace;font-size:12px">' + esc(c.password) + '</span></div>';
        }).join('');
    });
}

// --- Action functions ---

function toggleAttack(name, val) {
    var data = {};
    data[name] = val;
    api('POST', '/api/attacks', data).then(function() {
        toast('Attack ' + name + (val ? ' ON' : ' OFF'));
    });
}
function setRate(r) {
    api('POST', '/api/rate', {rate: r}).then(function() {
        [1,2,3].forEach(function(n) {
            document.getElementById('rate-'+n).classList.toggle('active', n === r);
        });
        toast('Rate set to ' + r);
    });
}
function switchMode(mode) {
    toast('Switching to ' + mode + '...');
    api('POST', '/api/mode', {mode: mode}).then(function(r) {
        if (r && r.ok) toast(r.message);
    });
}
function restartAO() {
    api('POST', '/api/restart', {}).then(function(r) {
        toast(r && r.message ? r.message : 'Restart queued');
    });
}
function doShutdown() {
    api('POST', '/api/shutdown', {}).then(function(r) {
        toast(r && r.message ? r.message : 'Shutdown queued');
    });
}
function restartPwn() {
    api('POST', '/api/restart', {}).then(function(r) {
        toast('Pwnagotchi restart queued');
    });
}

// --- Initial load & auto-refresh ---
refreshStatus();
setTimeout(refreshBattery, 500);
setTimeout(refreshBluetooth, 1000);
setTimeout(refreshWifi, 1500);
setTimeout(refreshAttacks, 2000);
setTimeout(refreshCaptures, 2500);
setTimeout(refreshRecovery, 3000);
setTimeout(refreshPersonality, 3500);
setTimeout(refreshSystem, 4000);
setTimeout(refreshCracked, 4500);

setInterval(refreshStatus, 5000);
setInterval(refreshBattery, 15000);
setInterval(refreshBluetooth, 15000);
setInterval(refreshWifi, 5000);
setInterval(refreshAttacks, 10000);
setInterval(refreshCaptures, 30000);
setInterval(refreshRecovery, 15000);
setInterval(refreshPersonality, 10000);
setInterval(refreshSystem, 15000);
setInterval(refreshCracked, 60000);
setInterval(function(){ document.getElementById('eink-img').src='/api/display.png?t='+Date.now(); }, 5000);
</script>
</body>
</html>
"##;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Helper: create a SharedState with test defaults.
    fn test_state() -> SharedState {
        Arc::new(Mutex::new(DaemonState::new("testbot")))
    }

    /// Helper: build the router with test state.
    fn test_router() -> (Router, SharedState) {
        let state = test_state();
        let router = build_router(state.clone());
        (router, state)
    }

    /// Helper: make a GET request and return (status, body_string).
    async fn get(router: &Router, path: &str) -> (u16, String) {
        let req = axum::http::Request::builder()
            .uri(path)
            .body(Body::empty())
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();
        let status = resp.status().as_u16();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&body).to_string())
    }

    /// Helper: make a POST request with JSON body and return (status, body_string).
    async fn post_json(router: &Router, path: &str, json: &str) -> (u16, String) {
        let req = axum::http::Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(json.to_string()))
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();
        let status = resp.status().as_u16();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&body).to_string())
    }

    // === Serialization tests (keep existing ones) ===

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
        assert_eq!(API_STATUS, "/api/status");
        assert_eq!(API_ATTACKS, "/api/attacks");
        assert_eq!(API_CAPTURES, "/api/captures");
        assert_eq!(API_CONFIG, "/api/config");
        assert_eq!(API_DISPLAY, "/api/display.png");
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
        assert_eq!(API_HEALTH, "/api/health");
        assert_eq!(API_RATE, "/api/rate");
    }

    #[test]
    fn test_battery_info_serialize() {
        let info = BatteryInfo {
            level: 75, charging: true, voltage_mv: 4100,
            low: false, critical: false, available: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"level\":75"));
        assert!(json.contains("\"charging\":true"));
        assert!(json.contains("\"available\":true"));
    }

    #[test]
    fn test_wifi_info_serialize() {
        let info = WifiInfo {
            state: "Monitor".into(), channel: 6,
            aps_tracked: 15, channels: vec![1, 6, 11], dwell_ms: 250,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"state\":\"Monitor\""));
        assert!(json.contains("\"aps_tracked\":15"));
    }

    #[test]
    fn test_bluetooth_info_serialize() {
        let info = BluetoothInfo {
            connected: true, state: "Connected".into(),
            device_name: "Phone".into(), ip: "10.0.0.1".into(),
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            internet_available: true, retry_count: 0,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"connected\":true"));
        assert!(json.contains("\"device_name\":\"Phone\""));
    }

    #[test]
    fn test_personality_info_serialize() {
        let info = PersonalityInfo {
            mood: 0.75, face: "(^_^)".into(), blind_epochs: 2,
            total_handshakes: 10, total_aps_seen: 50, xp: 420, level: 3,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"level\":3"));
        assert!(json.contains("\"xp\":420"));
    }

    #[test]
    fn test_system_info_serialize() {
        let info = SystemInfoResponse {
            cpu_temp_c: 45.2, mem_used_mb: 200, mem_total_mb: 512,
            disk_used_mb: 3000, disk_total_mb: 16000,
            cpu_percent: 35.0, uptime_secs: 7200,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"cpu_temp_c\":45.2"));
        assert!(json.contains("\"disk_used_mb\":3000"));
        assert!(json.contains("\"uptime_secs\":7200"));
    }

    #[test]
    fn test_attack_stats_serialize() {
        let stats = AttackStats {
            total_attacks: 100, total_handshakes: 5,
            attack_rate: 1, deauths_this_epoch: 3,
            deauth: true, pmkid: true, csa: false,
            disassoc: true, anon_reassoc: true, rogue_m2: false,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"total_attacks\":100"));
        assert!(json.contains("\"deauth\":true"));
        assert!(json.contains("\"csa\":false"));
    }

    #[test]
    fn test_attack_toggle_deserialize() {
        let json = r#"{"deauth": false, "pmkid": true}"#;
        let toggle: AttackToggle = serde_json::from_str(json).unwrap();
        assert_eq!(toggle.deauth, Some(false));
        assert_eq!(toggle.pmkid, Some(true));
        assert_eq!(toggle.csa, None);
    }

    #[test]
    fn test_cracked_entry_serialize() {
        let entry = CrackedEntry {
            ssid: "MyWifi".into(), bssid: "AA:BB:CC:DD:EE:FF".into(),
            password: "hunter2".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"ssid\":\"MyWifi\""));
        assert!(json.contains("\"password\":\"hunter2\""));
    }

    #[test]
    fn test_recovery_info_serialize() {
        let info = RecoveryInfo {
            state: "Healthy".into(), total_recoveries: 2,
            soft_retries: 1, hard_retries: 1,
            last_recovery: "5m ago".into(), diagnostic_count: 3,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"state\":\"Healthy\""));
        assert!(json.contains("\"total_recoveries\":2"));
        assert!(json.contains("\"last_recovery\":\"5m ago\""));
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
    fn test_capture_info_serialize() {
        let info = CaptureInfo {
            total_files: 10, handshake_files: 3,
            pending_upload: 2, total_size_bytes: 1024000, files: vec![],
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"handshake_files\":3"));
    }

    #[test]
    fn test_health_response_serialize() {
        let health = HealthResponse {
            wifi_state: "Monitor".into(), battery_level: 80,
            battery_charging: false, battery_available: true,
            uptime_secs: 3600, ao_state: "RUNNING".into(),
            ao_pid: 1234, ao_crash_count: 0, ao_uptime: "01:00:00".into(),
        };
        let json = serde_json::to_string(&health).unwrap();
        assert!(json.contains("\"ao_pid\":1234"));
    }

    #[test]
    fn test_rate_change_deserialize() {
        let json = r#"{"rate": 2}"#;
        let rc: RateChange = serde_json::from_str(json).unwrap();
        assert_eq!(rc.rate, 2);
    }

    #[test]
    fn test_action_response_serialize() {
        let resp = ActionResponse { ok: true, message: "done".into() };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"ok\":true"));
    }

    #[test]
    fn test_daemon_state_new() {
        let ds = DaemonState::new("testbot");
        assert_eq!(ds.name, "testbot");
        assert_eq!(ds.mode, "AO");
        assert_eq!(ds.epoch, 0);
        assert!(!ds.pending_restart);
        assert!(!ds.pending_shutdown);
        assert!(ds.attack_deauth);
        assert!(ds.attack_pmkid);
        assert!(ds.attack_csa);
        assert!(ds.attack_disassoc);
        assert!(ds.attack_anon_reassoc);
        assert!(ds.attack_rogue_m2);
        assert_eq!(ds.xp, 0);
        assert_eq!(ds.level, 1);
        assert!(ds.cracked.is_empty());
        assert_eq!(ds.bt_state, "Off");
        assert!(!ds.bt_connected);
    }

    #[test]
    fn test_build_router_compiles() {
        let state = test_state();
        let _router = build_router(state);
    }

    // === Dashboard HTML tests ===

    #[test]
    fn test_dashboard_html_contains_all_cards() {
        assert!(DASHBOARD_HTML.contains("<title>oxigotchi</title>"));
        assert!(DASHBOARD_HTML.contains("card-face"), "missing face card");
        assert!(DASHBOARD_HTML.contains("card-stats"), "missing core stats card");
        assert!(DASHBOARD_HTML.contains("card-eink"), "missing e-ink card");
        assert!(DASHBOARD_HTML.contains("card-battery"), "missing battery card");
        assert!(DASHBOARD_HTML.contains("card-bt"), "missing bluetooth card");
        assert!(DASHBOARD_HTML.contains("card-wifi"), "missing wifi card");
        assert!(DASHBOARD_HTML.contains("card-attacks"), "missing attacks card");
        assert!(DASHBOARD_HTML.contains("card-captures"), "missing captures card");
        assert!(DASHBOARD_HTML.contains("card-recovery"), "missing recovery card");
        assert!(DASHBOARD_HTML.contains("card-personality"), "missing personality card");
        assert!(DASHBOARD_HTML.contains("card-system"), "missing system card");
        assert!(DASHBOARD_HTML.contains("card-cracked"), "missing cracked card");
        assert!(DASHBOARD_HTML.contains("card-download"), "missing download card");
        assert!(DASHBOARD_HTML.contains("card-mode"), "missing mode card");
        assert!(DASHBOARD_HTML.contains("card-actions"), "missing actions card");
    }

    #[test]
    fn test_dashboard_html_has_all_api_calls() {
        assert!(DASHBOARD_HTML.contains("/api/status"), "missing /api/status");
        assert!(DASHBOARD_HTML.contains("/api/battery"), "missing /api/battery");
        assert!(DASHBOARD_HTML.contains("/api/bluetooth"), "missing /api/bluetooth");
        assert!(DASHBOARD_HTML.contains("/api/wifi"), "missing /api/wifi");
        assert!(DASHBOARD_HTML.contains("/api/attacks"), "missing /api/attacks");
        assert!(DASHBOARD_HTML.contains("/api/captures"), "missing /api/captures");
        assert!(DASHBOARD_HTML.contains("/api/recovery"), "missing /api/recovery");
        assert!(DASHBOARD_HTML.contains("/api/personality"), "missing /api/personality");
        assert!(DASHBOARD_HTML.contains("/api/system"), "missing /api/system");
        assert!(DASHBOARD_HTML.contains("/api/cracked"), "missing /api/cracked");
        assert!(DASHBOARD_HTML.contains("/api/health"), "missing /api/health");
        assert!(DASHBOARD_HTML.contains("/api/mode"), "missing /api/mode");
        assert!(DASHBOARD_HTML.contains("/api/rate"), "missing /api/rate");
        assert!(DASHBOARD_HTML.contains("/api/restart"), "missing /api/restart");
        assert!(DASHBOARD_HTML.contains("/api/shutdown"), "missing /api/shutdown");
    }

    #[test]
    fn test_dashboard_html_has_attack_toggles() {
        assert!(DASHBOARD_HTML.contains("atk-deauth"));
        assert!(DASHBOARD_HTML.contains("atk-pmkid"));
        assert!(DASHBOARD_HTML.contains("atk-csa"));
        assert!(DASHBOARD_HTML.contains("atk-disassoc"));
        assert!(DASHBOARD_HTML.contains("atk-anon_reassoc"));
        assert!(DASHBOARD_HTML.contains("atk-rogue_m2"));
        assert!(DASHBOARD_HTML.contains("toggleAttack"));
    }

    #[test]
    fn test_dashboard_html_has_rate_buttons() {
        assert!(DASHBOARD_HTML.contains("rate-1"));
        assert!(DASHBOARD_HTML.contains("rate-2"));
        assert!(DASHBOARD_HTML.contains("rate-3"));
        assert!(DASHBOARD_HTML.contains("setRate"));
    }

    #[test]
    fn test_dashboard_html_dark_theme() {
        assert!(DASHBOARD_HTML.contains("#1a1a2e"), "missing background color");
        assert!(DASHBOARD_HTML.contains("#00d4aa"), "missing accent color");
        assert!(DASHBOARD_HTML.contains("#16213e"), "missing card background");
    }

    #[test]
    fn test_dashboard_html_auto_refresh() {
        assert!(DASHBOARD_HTML.contains("setInterval(refreshStatus, 5000)"));
        assert!(DASHBOARD_HTML.contains("setInterval(refreshBattery, 15000)"));
        assert!(DASHBOARD_HTML.contains("setInterval(refreshWifi, 5000)"));
    }

    // === HTTP endpoint integration tests ===

    #[tokio::test]
    async fn test_get_dashboard_returns_html() {
        let (router, _) = test_router();
        let (status, body) = get(&router, "/").await;
        assert_eq!(status, 200);
        assert!(body.contains("<!DOCTYPE html>"));
        assert!(body.contains("<title>oxigotchi</title>"));
    }

    #[tokio::test]
    async fn test_get_status_json() {
        let (router, state) = test_router();
        {
            let mut s = state.lock().unwrap();
            s.epoch = 42;
            s.channel = 6;
            s.face = "(^_^)".into();
        }
        let (status, body) = get(&router, "/api/status").await;
        assert_eq!(status, 200);
        let resp: StatusResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(resp.name, "testbot");
        assert_eq!(resp.epoch, 42);
        assert_eq!(resp.channel, 6);
        assert_eq!(resp.face, "(^_^)");
    }

    #[tokio::test]
    async fn test_get_battery_json() {
        let (router, state) = test_router();
        {
            let mut s = state.lock().unwrap();
            s.battery_level = 73;
            s.battery_charging = true;
            s.battery_available = true;
            s.battery_voltage_mv = 4050;
        }
        let (status, body) = get(&router, "/api/battery").await;
        assert_eq!(status, 200);
        let resp: BatteryInfo = serde_json::from_str(&body).unwrap();
        assert_eq!(resp.level, 73);
        assert!(resp.charging);
        assert!(resp.available);
        assert_eq!(resp.voltage_mv, 4050);
    }

    #[tokio::test]
    async fn test_get_wifi_json() {
        let (router, state) = test_router();
        {
            let mut s = state.lock().unwrap();
            s.wifi_state = "Monitor".into();
            s.channel = 11;
            s.wifi_aps_tracked = 25;
            s.wifi_channels = vec![1, 6, 11];
            s.wifi_dwell_ms = 2000;
        }
        let (status, body) = get(&router, "/api/wifi").await;
        assert_eq!(status, 200);
        let resp: WifiInfo = serde_json::from_str(&body).unwrap();
        assert_eq!(resp.state, "Monitor");
        assert_eq!(resp.channel, 11);
        assert_eq!(resp.aps_tracked, 25);
        assert_eq!(resp.channels, vec![1, 6, 11]);
        assert_eq!(resp.dwell_ms, 2000);
    }

    #[tokio::test]
    async fn test_get_bluetooth_json() {
        let (router, state) = test_router();
        {
            let mut s = state.lock().unwrap();
            s.bt_connected = true;
            s.bt_state = "Connected".into();
            s.bt_device_name = "Pixel 7".into();
            s.bt_ip = "10.0.0.2".into();
        }
        let (status, body) = get(&router, "/api/bluetooth").await;
        assert_eq!(status, 200);
        let resp: BluetoothInfo = serde_json::from_str(&body).unwrap();
        assert!(resp.connected);
        assert_eq!(resp.device_name, "Pixel 7");
        assert_eq!(resp.ip, "10.0.0.2");
    }

    #[tokio::test]
    async fn test_get_personality_json() {
        let (router, state) = test_router();
        {
            let mut s = state.lock().unwrap();
            s.mood = 0.85;
            s.face = "(^_^)".into();
            s.xp = 420;
            s.level = 3;
            s.blind_epochs = 2;
            s.handshakes = 10;
            s.aps_seen = 50;
        }
        let (status, body) = get(&router, "/api/personality").await;
        assert_eq!(status, 200);
        let resp: PersonalityInfo = serde_json::from_str(&body).unwrap();
        assert_eq!(resp.mood, 0.85);
        assert_eq!(resp.xp, 420);
        assert_eq!(resp.level, 3);
        assert_eq!(resp.total_handshakes, 10);
        assert_eq!(resp.total_aps_seen, 50);
    }

    #[tokio::test]
    async fn test_get_system_json() {
        let (router, state) = test_router();
        {
            let mut s = state.lock().unwrap();
            s.cpu_temp_c = 42.5;
            s.mem_used_mb = 200;
            s.mem_total_mb = 512;
            s.disk_used_mb = 3000;
            s.disk_total_mb = 16000;
        }
        let (status, body) = get(&router, "/api/system").await;
        assert_eq!(status, 200);
        let resp: SystemInfoResponse = serde_json::from_str(&body).unwrap();
        // On non-Linux, the live reads return 0, so fallback values are used
        assert!(resp.uptime_secs < 5); // just-created state
    }

    #[tokio::test]
    async fn test_get_attacks_json() {
        let (router, state) = test_router();
        {
            let mut s = state.lock().unwrap();
            s.total_attacks = 100;
            s.attack_deauth = true;
            s.attack_csa = false;
        }
        let (status, body) = get(&router, "/api/attacks").await;
        assert_eq!(status, 200);
        let resp: AttackStats = serde_json::from_str(&body).unwrap();
        assert_eq!(resp.total_attacks, 100);
        assert!(resp.deauth);
        assert!(!resp.csa);
    }

    #[tokio::test]
    async fn test_post_attacks_toggle() {
        let (router, state) = test_router();
        let (status, body) = post_json(&router, "/api/attacks",
            r#"{"deauth": false, "csa": true}"#).await;
        assert_eq!(status, 200);
        let resp: ActionResponse = serde_json::from_str(&body).unwrap();
        assert!(resp.ok);
        let s = state.lock().unwrap();
        assert!(!s.attack_deauth);
        assert!(s.attack_csa);
        assert!(s.pending_attack_toggle.is_some());
    }

    #[tokio::test]
    async fn test_get_captures_json() {
        let (router, state) = test_router();
        {
            let mut s = state.lock().unwrap();
            s.capture_files = 5;
            s.handshake_files = 2;
            s.capture_list = vec![
                CaptureEntry { filename: "test.pcapng".into(), size_bytes: 1024 },
            ];
        }
        let (status, body) = get(&router, "/api/captures").await;
        assert_eq!(status, 200);
        let resp: CaptureInfo = serde_json::from_str(&body).unwrap();
        assert_eq!(resp.total_files, 5);
        assert_eq!(resp.handshake_files, 2);
        assert_eq!(resp.files.len(), 1);
        assert_eq!(resp.files[0].filename, "test.pcapng");
    }

    #[tokio::test]
    async fn test_get_health_json() {
        let (router, state) = test_router();
        {
            let mut s = state.lock().unwrap();
            s.wifi_state = "Monitor".into();
            s.ao_state = "RUNNING".into();
            s.ao_pid = 1234;
        }
        let (status, body) = get(&router, "/api/health").await;
        assert_eq!(status, 200);
        let resp: HealthResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(resp.wifi_state, "Monitor");
        assert_eq!(resp.ao_state, "RUNNING");
        assert_eq!(resp.ao_pid, 1234);
    }

    #[tokio::test]
    async fn test_get_recovery_json() {
        let (router, state) = test_router();
        {
            let mut s = state.lock().unwrap();
            s.recovery_state = "Recovering".into();
            s.recovery_total = 3;
            s.recovery_last_str = "2m ago".into();
        }
        let (status, body) = get(&router, "/api/recovery").await;
        assert_eq!(status, 200);
        let resp: RecoveryInfo = serde_json::from_str(&body).unwrap();
        assert_eq!(resp.state, "Recovering");
        assert_eq!(resp.total_recoveries, 3);
        assert_eq!(resp.last_recovery, "2m ago");
    }

    #[tokio::test]
    async fn test_get_cracked_empty() {
        let (router, _) = test_router();
        let (status, body) = get(&router, "/api/cracked").await;
        assert_eq!(status, 200);
        let resp: Vec<CrackedEntry> = serde_json::from_str(&body).unwrap();
        assert!(resp.is_empty());
    }

    #[tokio::test]
    async fn test_get_cracked_with_entries() {
        let (router, state) = test_router();
        {
            let mut s = state.lock().unwrap();
            s.cracked.push(CrackedEntry {
                ssid: "MyWifi".into(),
                bssid: "AA:BB:CC:DD:EE:FF".into(),
                password: "hunter2".into(),
            });
        }
        let (status, body) = get(&router, "/api/cracked").await;
        assert_eq!(status, 200);
        let resp: Vec<CrackedEntry> = serde_json::from_str(&body).unwrap();
        assert_eq!(resp.len(), 1);
        assert_eq!(resp[0].ssid, "MyWifi");
        assert_eq!(resp[0].password, "hunter2");
    }

    #[tokio::test]
    async fn test_post_mode_toggle() {
        let (router, state) = test_router();
        let (status, body) = post_json(&router, "/api/mode",
            r#"{"mode": "toggle"}"#).await;
        assert_eq!(status, 200);
        let resp: ActionResponse = serde_json::from_str(&body).unwrap();
        assert!(resp.ok);
        assert!(resp.message.contains("PWN")); // default is AO, toggle -> PWN
        let s = state.lock().unwrap();
        assert_eq!(s.pending_mode_switch.as_deref(), Some("PWN"));
    }

    #[tokio::test]
    async fn test_post_mode_explicit() {
        let (router, state) = test_router();
        let (status, _) = post_json(&router, "/api/mode",
            r#"{"mode": "pwn"}"#).await;
        assert_eq!(status, 200);
        let s = state.lock().unwrap();
        assert_eq!(s.pending_mode_switch.as_deref(), Some("PWN"));
    }

    #[tokio::test]
    async fn test_post_rate_clamps() {
        let (router, state) = test_router();
        // Rate 5 should clamp to 3
        let (status, body) = post_json(&router, "/api/rate",
            r#"{"rate": 5}"#).await;
        assert_eq!(status, 200);
        let resp: ActionResponse = serde_json::from_str(&body).unwrap();
        assert!(resp.ok);
        assert!(resp.message.contains("3"));
        let s = state.lock().unwrap();
        assert_eq!(s.pending_rate_change, Some(3));
    }

    #[tokio::test]
    async fn test_post_rate_valid() {
        let (router, state) = test_router();
        let (status, _) = post_json(&router, "/api/rate",
            r#"{"rate": 2}"#).await;
        assert_eq!(status, 200);
        let s = state.lock().unwrap();
        assert_eq!(s.pending_rate_change, Some(2));
    }

    #[tokio::test]
    async fn test_post_restart() {
        let (router, state) = test_router();
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/restart")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        let s = state.lock().unwrap();
        assert!(s.pending_restart);
    }

    #[tokio::test]
    async fn test_post_shutdown() {
        let (router, state) = test_router();
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/shutdown")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let action: ActionResponse = serde_json::from_slice(&body).unwrap();
        assert!(action.ok);
        assert!(action.message.contains("shutdown"));
        let s = state.lock().unwrap();
        assert!(s.pending_shutdown);
    }

    #[tokio::test]
    async fn test_post_bluetooth_toggle() {
        let (router, state) = test_router();
        let (status, body) = post_json(&router, "/api/bluetooth",
            r#"{"visible": true}"#).await;
        assert_eq!(status, 200);
        let resp: ActionResponse = serde_json::from_str(&body).unwrap();
        assert!(resp.ok);
        let s = state.lock().unwrap();
        assert_eq!(s.pending_bt_toggle, Some(true));
    }

    #[tokio::test]
    async fn test_404_unknown_route() {
        let (router, _) = test_router();
        let (status, _) = get(&router, "/api/nonexistent").await;
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn test_all_get_endpoints_200() {
        let (router, _) = test_router();
        let endpoints = [
            "/", "/api/status", "/api/captures", "/api/health",
            "/api/battery", "/api/wifi", "/api/bluetooth",
            "/api/personality", "/api/system", "/api/attacks",
            "/api/recovery", "/api/cracked",
        ];
        for endpoint in endpoints {
            let (status, _) = get(&router, endpoint).await;
            assert_eq!(status, 200, "GET {} returned {}", endpoint, status);
        }
    }

    #[tokio::test]
    async fn test_state_independence() {
        // Two separate routers should have independent state
        let (router1, state1) = test_router();
        let (router2, _state2) = test_router();
        {
            let mut s = state1.lock().unwrap();
            s.epoch = 999;
        }
        let (_, body1) = get(&router1, "/api/status").await;
        let (_, body2) = get(&router2, "/api/status").await;
        let resp1: StatusResponse = serde_json::from_str(&body1).unwrap();
        let resp2: StatusResponse = serde_json::from_str(&body2).unwrap();
        assert_eq!(resp1.epoch, 999);
        assert_eq!(resp2.epoch, 0);
    }
}
