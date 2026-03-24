//! Web dashboard module (axum HTTP server).
//!
//! Provides a REST API and embedded HTML dashboard for monitoring
//! and configuring oxigotchi. The axum router shares DaemonState via
//! Arc<Mutex<DaemonState>>.
//!
//! 21 dashboard cards organized by user-journey (At-a-Glance, Hardware,
//! Hunting, Loot, Connectivity, Status, Management) with vanilla JS auto-refresh.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        FromRef, State, Path as AxumPath,
    },
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;

mod html;
use html::DASHBOARD_HTML;

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
    pub capture_dir: String,
    pub capture_files: usize,
    pub handshake_files: usize,
    pub pending_upload: usize,
    pub total_capture_size: u64,
    pub capture_list: Vec<CaptureEntry>,
    /// Session captures: pcapng files created by AO in tmpfs this session.
    pub session_captures: u32,
    /// Session handshakes: validated captures moved to SD this session.
    pub session_handshakes: u32,

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
    pub autohunt_enabled: bool,

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

    // -- gps --
    pub gpsd_available: bool,

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

    // -- firmware health --
    pub fw_crash_suppress: u32,
    pub fw_hardfault: u32,
    pub fw_health: String,

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
    pub pending_attack_toggle: Option<AttackToggle>,
    pub pending_bt_toggle: Option<bool>,
    pub pending_bt_pair: Option<String>,
    pub bt_scan_results: Vec<BtScanDevice>,
    pub bt_scan_in_progress: bool,
    pub pending_settings: Option<SettingsUpdate>,

    // -- plugins --
    pub plugin_list: Vec<PluginInfo>,
    pub pending_plugin_updates: Vec<PluginUpdate>,

    // -- nearby APs --
    pub ap_list: Vec<ApEntry>,

    // -- whitelist --
    pub whitelist: Vec<WhitelistEntry>,
    pub pending_whitelist_adds: Vec<WhitelistAdd>,
    pub pending_whitelist_removes: Vec<String>,

    // -- channel config --
    pub pending_channel_config: Option<ChannelConfig>,

    // -- wpa-sec --
    pub wpasec_api_key: String,
    pub pending_wpasec_key: Option<String>,

    // -- discord --
    pub discord_webhook_url: String,
    pub discord_enabled: bool,
    pub pending_discord_config: Option<DiscordConfig>,

    // -- radio lock manager --
    pub radio_mode: String,
    pub radio_pid: u32,
    pub pending_radio_request: Option<String>,
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
            capture_dir: "/home/pi/captures".into(),
            capture_files: 0,
            handshake_files: 0,
            pending_upload: 0,
            total_capture_size: 0,
            capture_list: Vec::new(),
            session_captures: 0,
            session_handshakes: 0,
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
            autohunt_enabled: true,
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
            gpsd_available: false,
            xp: 0,
            level: 1,
            cpu_temp_c: 0.0,
            mem_used_mb: 0,
            mem_total_mb: 0,
            disk_used_mb: 0,
            disk_total_mb: 0,
            cpu_percent: 0.0,
            boot_time: Instant::now(),
            fw_crash_suppress: 0,
            fw_hardfault: 0,
            fw_health: "Unknown".into(),
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

            pending_attack_toggle: None,
            pending_bt_toggle: None,
            pending_bt_pair: None,
            bt_scan_results: Vec::new(),
            bt_scan_in_progress: false,
            pending_settings: None,
            plugin_list: Vec::new(),
            pending_plugin_updates: Vec::new(),
            ap_list: Vec::new(),
            whitelist: Vec::new(),
            pending_whitelist_adds: Vec::new(),
            pending_whitelist_removes: Vec::new(),
            pending_channel_config: None,
            wpasec_api_key: String::new(),
            pending_wpasec_key: None,
            discord_webhook_url: String::new(),
            discord_enabled: false,
            pending_discord_config: None,
            radio_mode: "FREE".into(),
            radio_pid: 0,
            pending_radio_request: None,
        }
    }
}

/// Shared state type used by axum handlers.
pub type SharedState = Arc<Mutex<DaemonState>>;

/// Combined axum state: daemon shared state + WebSocket broadcast sender.
#[derive(Clone)]
pub struct AppState {
    pub shared: SharedState,
    pub ws_tx: broadcast::Sender<String>,
}

impl FromRef<AppState> for SharedState {
    fn from_ref(app: &AppState) -> Self {
        app.shared.clone()
    }
}

// ---------------------------------------------------------------------------
// WebSocket full-state snapshot (all dashboard data in one JSON push)
// ---------------------------------------------------------------------------

/// Combined snapshot of all dashboard-visible state, broadcast over WebSocket.
/// Field names intentionally match the individual REST API responses so the
/// frontend `updateAllCards()` can consume the same keys.
#[derive(Debug, Clone, Serialize)]
struct WsSnapshot {
    // -- status --
    name: String,
    version: String,
    uptime: String,
    epoch: u64,
    channel: u8,
    aps_seen: u32,
    handshakes: u32,
    blind_epochs: u32,
    mood: f32,
    face: String,
    status_message: String,
    mode: String,
    // -- battery --
    battery: BatteryInfo,
    // -- attacks --
    attacks: AttackStats,
    // -- wifi --
    wifi: WifiInfo,
    // -- bluetooth --
    bluetooth: BluetoothInfo,
    // -- personality --
    personality: PersonalityInfo,
    // -- system --
    system: SystemInfoSnapshot,
    // -- recovery + health --
    recovery: RecoveryInfo,
    health: HealthResponse,
    // -- captures --
    captures: CaptureInfo,
    // -- cracked --
    cracked: Vec<CrackedEntry>,
    // -- aps --
    aps: Vec<ApEntry>,
    // -- whitelist --
    whitelist: Vec<WhitelistEntry>,
    // -- plugins --
    plugins: Vec<PluginInfo>,
    // -- radio --
    radio: RadioResponse,
}

/// System info snapshot for WS (uses cached values, not live reads).
#[derive(Debug, Clone, Serialize)]
struct SystemInfoSnapshot {
    cpu_temp_c: f32,
    mem_used_mb: u32,
    mem_total_mb: u32,
    disk_used_mb: u32,
    disk_total_mb: u32,
    cpu_percent: f32,
    uptime_secs: u64,
}

/// Build a full WsSnapshot from the current DaemonState.
fn build_ws_snapshot(s: &DaemonState) -> WsSnapshot {
    WsSnapshot {
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
        battery: BatteryInfo {
            level: s.battery_level,
            charging: s.battery_charging,
            voltage_mv: s.battery_voltage_mv,
            low: s.battery_low,
            critical: s.battery_critical,
            available: s.battery_available,
        },
        attacks: AttackStats {
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
        },
        wifi: WifiInfo {
            state: s.wifi_state.clone(),
            channel: s.channel,
            aps_tracked: s.wifi_aps_tracked,
            channels: s.wifi_channels.clone(),
            dwell_ms: s.wifi_dwell_ms,
            autohunt_enabled: s.autohunt_enabled,
        },
        bluetooth: BluetoothInfo {
            connected: s.bt_connected,
            state: s.bt_state.clone(),
            device_name: s.bt_device_name.clone(),
            ip: s.bt_ip.clone(),
            phone_mac: s.bt_phone_mac.clone(),
            internet_available: s.bt_internet_available,
            retry_count: s.bt_retry_count,
        },
        personality: PersonalityInfo {
            mood: s.mood,
            face: s.face.clone(),
            blind_epochs: s.blind_epochs,
            total_handshakes: s.handshakes,
            total_aps_seen: s.aps_seen,
            xp: s.xp,
            level: s.level,
        },
        system: SystemInfoSnapshot {
            cpu_temp_c: s.cpu_temp_c,
            mem_used_mb: s.mem_used_mb,
            mem_total_mb: s.mem_total_mb,
            disk_used_mb: s.disk_used_mb,
            disk_total_mb: s.disk_total_mb,
            cpu_percent: s.cpu_percent,
            uptime_secs: s.boot_time.elapsed().as_secs(),
        },
        recovery: RecoveryInfo {
            state: s.recovery_state.clone(),
            total_recoveries: s.recovery_total,
            soft_retries: s.recovery_soft_retries,
            hard_retries: s.recovery_hard_retries,
            last_recovery: s.recovery_last_str.clone(),
            diagnostic_count: 0,
            fw_crash_suppress: s.fw_crash_suppress,
            fw_hardfault: s.fw_hardfault,
            fw_health: s.fw_health.clone(),
        },
        health: HealthResponse {
            wifi_state: s.wifi_state.clone(),
            battery_level: s.battery_level,
            battery_charging: s.battery_charging,
            battery_available: s.battery_available,
            uptime_secs: s.boot_time.elapsed().as_secs(),
            ao_state: s.ao_state.clone(),
            ao_pid: s.ao_pid,
            ao_crash_count: s.ao_crash_count,
            ao_uptime: s.ao_uptime.clone(),
            gpsd_available: s.gpsd_available,
        },
        captures: CaptureInfo {
            total_files: s.capture_files,
            handshake_files: s.handshake_files,
            pending_upload: s.pending_upload,
            total_size_bytes: s.total_capture_size,
            session_captures: s.session_captures,
            session_handshakes: s.session_handshakes,
            files: s.capture_list.clone(),
        },
        cracked: s.cracked.clone(),
        aps: s.ap_list.clone(),
        whitelist: s.whitelist.clone(),
        plugins: s.plugin_list.clone(),
        radio: RadioResponse {
            mode: s.radio_mode.clone(),
            pid: s.radio_pid,
            owner: "daemon".into(),
        },
    }
}

/// Broadcast the current state to all connected WebSocket clients.
/// Called by the daemon after each `sync_to_web()`.
pub fn broadcast_state(shared: &SharedState, ws_tx: &broadcast::Sender<String>) {
    let s = shared.lock().unwrap();
    let snapshot = build_ws_snapshot(&s);
    drop(s); // release mutex before serializing
    if let Ok(json) = serde_json::to_string(&snapshot) {
        let _ = ws_tx.send(json); // ignore error if no subscribers
    }
}

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
    pub session_captures: u32,
    pub session_handshakes: u32,
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
    pub gpsd_available: bool,
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

/// Radio status response for GET /api/radio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioResponse {
    pub mode: String,
    pub pid: u32,
    pub owner: String,
}

/// Radio request for POST /api/radio.
#[derive(Debug, Clone, Deserialize)]
pub struct RadioRequest {
    pub request: String,
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
    pub autohunt_enabled: bool,
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

/// BT scan result entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtScanDevice {
    pub mac: String,
    pub name: String,
}

/// BT pair request.
#[derive(Debug, Clone, Deserialize)]
pub struct BtPairRequest {
    pub mac: String,
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
    pub fw_crash_suppress: u32,
    pub fw_hardfault: u32,
    pub fw_health: String,
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

/// Plugin info for the web API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub author: String,
    pub tag: String,
    pub enabled: bool,
    pub x: i32,
    pub y: i32,
}

/// A plugin config update from the web dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUpdate {
    pub name: String,
    pub enabled: Option<bool>,
    pub x: Option<i32>,
    pub y: Option<i32>,
}

/// A nearby access point entry returned by /api/aps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApEntry {
    pub bssid: String,
    pub ssid: String,
    pub rssi: i16,
    pub channel: u8,
    pub clients: u32,
    pub has_handshake: bool,
}

/// A whitelist entry returned by /api/whitelist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhitelistEntry {
    pub value: String,
    pub entry_type: String,
}

/// Request body for POST /api/whitelist/add.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhitelistAdd {
    pub value: String,
    pub entry_type: String,
}

/// Request body for POST /api/whitelist/remove.
#[derive(Debug, Clone, Deserialize)]
pub struct WhitelistRemove {
    pub value: String,
}

/// Channel configuration request for POST /api/channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub channels: Option<Vec<u8>>,
    pub dwell_ms: Option<u64>,
    #[serde(default)]
    pub autohunt: Option<bool>,
}

/// WPA-SEC config response returned by GET /api/wpasec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WpaSecResponse {
    pub api_key: String,
    pub enabled: bool,
}

/// WPA-SEC config update request for POST /api/wpasec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WpaSecUpdate {
    pub api_key: String,
}

/// Discord webhook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    pub webhook_url: String,
    pub enabled: bool,
}

/// Discord config response returned by GET /api/discord.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordResponse {
    pub webhook_url: String,
    pub enabled: bool,
}

/// Logs response returned by GET /api/logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsResponse {
    pub lines: Vec<String>,
}

/// Settings update request for POST /api/settings.
#[derive(Debug, Clone, Deserialize)]
pub struct SettingsUpdate {
    pub name: Option<String>,
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
pub const API_APS: &str = "/api/aps";
pub const API_WHITELIST_ADD: &str = "/api/whitelist/add";
pub const API_WHITELIST_REMOVE: &str = "/api/whitelist/remove";
pub const API_CHANNELS: &str = "/api/channels";
pub const API_LOGS: &str = "/api/logs";
pub const API_WPASEC: &str = "/api/wpasec";
pub const API_DISCORD: &str = "/api/discord";
pub const API_DOWNLOAD_SINGLE: &str = "/api/download/:filename";
pub const API_RESTART_PI: &str = "/api/restart-pi";
pub const API_RESTART_SSH: &str = "/api/restart-ssh";
pub const API_RESTART_PWN: &str = "/api/restart-pwn";
pub const API_SETTINGS: &str = "/api/settings";
pub const API_BT_SCAN: &str = "/api/bluetooth/scan";
pub const API_BT_PAIR: &str = "/api/bluetooth/pair";
pub const API_RADIO: &str = "/api/radio";

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
        session_captures: s.session_captures,
        session_handshakes: s.session_handshakes,
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
        gpsd_available: s.gpsd_available,
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
        autohunt_enabled: s.autohunt_enabled,
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
        fw_crash_suppress: s.fw_crash_suppress,
        fw_hardfault: s.fw_hardfault,
        fw_health: s.fw_health.clone(),
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
    let mode = body.mode.to_uppercase();
    s.pending_mode_switch = Some(mode.clone());
    Json(ActionResponse {
        ok: true,
        message: format!("Mode switch to {} queued", mode),
    })
}

/// GET /api/radio -> current radio lock status
async fn radio_get_handler(State(state): State<SharedState>) -> Json<RadioResponse> {
    let s = state.lock().unwrap();
    Json(RadioResponse {
        mode: s.radio_mode.clone(),
        pid: s.radio_pid,
        owner: "daemon".into(),
    })
}

/// POST /api/radio -> request a radio mode switch
async fn radio_post_handler(
    State(state): State<SharedState>,
    Json(body): Json<RadioRequest>,
) -> Json<ActionResponse> {
    let request = body.request.to_uppercase();
    match request.as_str() {
        "WIFI" | "BT" | "FREE" => {
            let mut s = state.lock().unwrap();
            s.pending_radio_request = Some(request.clone());
            Json(ActionResponse {
                ok: true,
                message: format!("Radio mode switch to {} queued", request),
            })
        }
        _ => Json(ActionResponse {
            ok: false,
            message: format!("Invalid radio mode: {}. Use WIFI, BT, or FREE", request),
        }),
    }
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

/// GET /api/plugins -> JSON list of all plugins
async fn plugins_get_handler(State(state): State<SharedState>) -> Json<Vec<PluginInfo>> {
    let s = state.lock().unwrap();
    Json(s.plugin_list.clone())
}

/// POST /api/plugins -> update plugin configs (array of updates)
async fn plugins_post_handler(
    State(state): State<SharedState>,
    Json(body): Json<Vec<PluginUpdate>>,
) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    s.pending_plugin_updates.extend(body);
    Json(ActionResponse {
        ok: true,
        message: "Plugin updates queued".into(),
    })
}

/// GET /api/aps -> JSON list of nearby access points (sorted by RSSI)
async fn aps_handler(State(state): State<SharedState>) -> Json<Vec<ApEntry>> {
    let s = state.lock().unwrap();
    let mut aps = s.ap_list.clone();
    aps.sort_by(|a, b| a.rssi.cmp(&b.rssi)); // strongest (least negative) last; JS will display strongest first
    Json(aps)
}

/// GET /api/whitelist -> JSON list of whitelist entries
async fn whitelist_get_handler(State(state): State<SharedState>) -> Json<Vec<WhitelistEntry>> {
    let s = state.lock().unwrap();
    Json(s.whitelist.clone())
}

/// POST /api/whitelist/add -> add a whitelist entry
async fn whitelist_add_handler(
    State(state): State<SharedState>,
    Json(body): Json<WhitelistAdd>,
) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    s.pending_whitelist_adds.push(body);
    Json(ActionResponse {
        ok: true,
        message: "Whitelist add queued".into(),
    })
}

/// POST /api/whitelist/remove -> remove a whitelist entry
async fn whitelist_remove_handler(
    State(state): State<SharedState>,
    Json(body): Json<WhitelistRemove>,
) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    s.pending_whitelist_removes.push(body.value);
    Json(ActionResponse {
        ok: true,
        message: "Whitelist remove queued".into(),
    })
}

/// POST /api/channels -> update channel configuration
async fn channels_handler(
    State(state): State<SharedState>,
    Json(body): Json<ChannelConfig>,
) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    s.pending_channel_config = Some(body);
    Json(ActionResponse {
        ok: true,
        message: "Channel config update queued".into(),
    })
}

/// GET /api/logs -> last 50 lines of journalctl output
async fn logs_handler() -> Json<LogsResponse> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = std::process::Command::new("journalctl")
            .args(["-u", "rusty-oxigotchi", "-n", "50", "--no-pager"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
            return Json(LogsResponse { lines });
        }
    }
    Json(LogsResponse { lines: Vec::new() })
}

// ---------------------------------------------------------------------------
// WPA-SEC endpoints
// ---------------------------------------------------------------------------

/// GET /api/wpasec -> JSON wpa-sec config (key masked)
async fn wpasec_get_handler(State(state): State<SharedState>) -> Json<WpaSecResponse> {
    let s = state.lock().unwrap();
    let key = &s.wpasec_api_key;
    let masked = if key.len() > 4 {
        format!("{}****", &key[..4])
    } else if !key.is_empty() {
        "****".into()
    } else {
        String::new()
    };
    Json(WpaSecResponse {
        api_key: masked,
        enabled: !s.wpasec_api_key.is_empty(),
    })
}

/// POST /api/wpasec -> set wpa-sec API key
async fn wpasec_post_handler(
    State(state): State<SharedState>,
    Json(body): Json<WpaSecUpdate>,
) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    s.pending_wpasec_key = Some(body.api_key);
    Json(ActionResponse {
        ok: true,
        message: "WPA-SEC key update queued".into(),
    })
}

// ---------------------------------------------------------------------------
// Discord webhook endpoints
// ---------------------------------------------------------------------------

/// GET /api/discord -> JSON discord config
async fn discord_get_handler(State(state): State<SharedState>) -> Json<DiscordResponse> {
    let s = state.lock().unwrap();
    let url = &s.discord_webhook_url;
    let masked = if url.len() > 30 {
        format!("{}****", &url[..30])
    } else if !url.is_empty() {
        "****".into()
    } else {
        String::new()
    };
    Json(DiscordResponse {
        webhook_url: masked,
        enabled: s.discord_enabled,
    })
}

/// POST /api/discord -> set discord config
async fn discord_post_handler(
    State(state): State<SharedState>,
    Json(body): Json<DiscordConfig>,
) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    s.pending_discord_config = Some(body);
    Json(ActionResponse {
        ok: true,
        message: "Discord config update queued".into(),
    })
}

// ---------------------------------------------------------------------------
// Single capture download endpoint
// ---------------------------------------------------------------------------

/// GET /api/download/:filename -> serve a single capture file
async fn download_single_handler(
    AxumPath(filename): AxumPath<String>,
    State(state): State<SharedState>,
) -> axum::response::Response<axum::body::Body> {
    use axum::http::{header, StatusCode};

    let capture_dir = {
        let s = state.lock().unwrap();
        s.capture_dir.clone()
    };

    // Sanitize filename: reject path traversal
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return axum::response::Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(axum::body::Body::from("invalid filename"))
            .unwrap();
    }

    let path = std::path::Path::new(&capture_dir).join(&filename);
    if !path.exists() || !path.is_file() {
        return axum::response::Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("file not found"))
            .unwrap();
    }

    match std::fs::read(&path) {
        Ok(data) => {
            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", filename),
                )
                .body(axum::body::Body::from(data))
                .unwrap()
        }
        Err(_) => {
            axum::response::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from("failed to read file"))
                .unwrap()
        }
    }
}

// ---------------------------------------------------------------------------
// System control endpoints
// ---------------------------------------------------------------------------

/// POST /api/restart-pi -> reboot the Pi
async fn restart_pi_handler() -> Json<ActionResponse> {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("sudo")
            .arg("reboot")
            .spawn();
    }
    Json(ActionResponse {
        ok: true,
        message: "Pi reboot initiated".into(),
    })
}

/// POST /api/restart-ssh -> restart SSH service
async fn restart_ssh_handler() -> Json<ActionResponse> {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("sudo")
            .args(["systemctl", "restart", "ssh"])
            .spawn();
    }
    Json(ActionResponse {
        ok: true,
        message: "SSH restart initiated".into(),
    })
}

/// POST /api/bluetooth/scan -> trigger BT scan, return cached results
async fn bt_scan_handler(State(state): State<SharedState>) -> Json<Vec<BtScanDevice>> {
    let mut s = state.lock().unwrap();
    if s.bt_scan_in_progress {
        // Return current cached results while scan is in progress
        return Json(s.bt_scan_results.clone());
    }
    // Signal main loop to run a scan
    s.bt_scan_in_progress = true;
    s.bt_scan_results.clear();
    Json(Vec::new())
}

/// GET /api/bluetooth/scan -> get cached scan results
async fn bt_scan_results_handler(State(state): State<SharedState>) -> Json<Vec<BtScanDevice>> {
    let s = state.lock().unwrap();
    Json(s.bt_scan_results.clone())
}

/// POST /api/bluetooth/pair -> pair with a device by MAC
async fn bt_pair_handler(
    State(state): State<SharedState>,
    Json(body): Json<BtPairRequest>,
) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    s.pending_bt_pair = Some(body.mac.clone());
    Json(ActionResponse {
        ok: true,
        message: format!("Pairing with {} queued", body.mac),
    })
}

/// POST /api/settings -> update device settings (name, etc.)
async fn settings_handler(
    State(state): State<SharedState>,
    Json(body): Json<SettingsUpdate>,
) -> Json<ActionResponse> {
    let mut s = state.lock().unwrap();
    s.pending_settings = Some(body);
    Json(ActionResponse {
        ok: true,
        message: "Settings update queued".into(),
    })
}

/// POST /api/restart-pwn -> restart the oxigotchi service itself
async fn restart_pwn_handler() -> Json<ActionResponse> {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("sudo")
            .args(["systemctl", "restart", "rusty-oxigotchi"])
            .spawn();
    }
    Json(ActionResponse {
        ok: true,
        message: "Oxigotchi restart initiated".into(),
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
/// GET /api/download/all -> ZIP of all capture files
async fn download_zip_handler(State(state): State<SharedState>) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use std::io::Write;

    let capture_dir = {
        let s = state.lock().unwrap();
        s.capture_dir.clone()
    };

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        if let Ok(entries) = std::fs::read_dir(&capture_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let name = path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if let Ok(data) = std::fs::read(&path) {
                        let _ = zip.start_file(&name, options);
                        let _ = zip.write_all(&data);
                    }
                }
            }
        }
        let _ = zip.finish();
    }

    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"captures.zip\"")
        .body(axum::body::Body::from(buf.into_inner()))
        .unwrap()
}

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

/// GET /ws -> WebSocket upgrade for live state push.
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(app): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, app.ws_tx.subscribe()))
}

/// Handle a single WebSocket connection: forward broadcast messages to the client.
async fn handle_ws(mut socket: WebSocket, mut rx: broadcast::Receiver<String>) {
    loop {
        tokio::select! {
            // Server push: broadcast state to client
            result = rx.recv() => {
                match result {
                    Ok(json) => {
                        if socket.send(Message::Text(json)).await.is_err() {
                            break; // client disconnected
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::debug!("ws client lagged by {n} messages");
                        // continue — next recv() will get the latest
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Client message (ping/pong or close)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {} // ignore other messages
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// Create a broadcast channel for WebSocket live updates.
/// Returns the sender (for the daemon to broadcast) and a receiver (dropped immediately).
/// The sender is passed to `build_router` and stored by the daemon.
pub fn create_ws_broadcast() -> broadcast::Sender<String> {
    let (tx, _) = broadcast::channel::<String>(16);
    tx
}

/// Build the axum router with all routes, sharing daemon state.
pub fn build_router(state: SharedState, ws_tx: broadcast::Sender<String>) -> Router {
    let app = AppState {
        shared: state,
        ws_tx,
    };
    Router::new()
        .route("/", get(dashboard_handler))
        .route("/ws", get(ws_handler))
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
        .route("/api/download/all", get(download_zip_handler))
        .route("/api/plugins", get(plugins_get_handler).post(plugins_post_handler))
        .route(API_APS, get(aps_handler))
        .route(API_WHITELIST, get(whitelist_get_handler))
        .route(API_WHITELIST_ADD, post(whitelist_add_handler))
        .route(API_WHITELIST_REMOVE, post(whitelist_remove_handler))
        .route(API_CHANNELS, post(channels_handler))
        .route(API_LOGS, get(logs_handler))
        .route(API_WPASEC, get(wpasec_get_handler).post(wpasec_post_handler))
        .route(API_DISCORD, get(discord_get_handler).post(discord_post_handler))
        .route(API_DOWNLOAD_SINGLE, get(download_single_handler))
        .route(API_RESTART_PI, post(restart_pi_handler))
        .route(API_RESTART_SSH, post(restart_ssh_handler))
        .route(API_RESTART_PWN, post(restart_pwn_handler))
        .route(API_SETTINGS, post(settings_handler))
        .route(API_BT_SCAN, get(bt_scan_results_handler).post(bt_scan_handler))
        .route(API_BT_PAIR, post(bt_pair_handler))
        .route(API_RADIO, get(radio_get_handler).post(radio_post_handler))
        .with_state(app)
}

/// Start the axum web server on 0.0.0.0:8080.
/// This function is async and should be spawned as a tokio task.
pub async fn start_server(state: SharedState, ws_tx: broadcast::Sender<String>) {
    let app = build_router(state, ws_tx);
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
        let ws_tx = create_ws_broadcast();
        let router = build_router(state.clone(), ws_tx);
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
        assert_eq!(API_APS, "/api/aps");
        assert_eq!(API_WHITELIST_ADD, "/api/whitelist/add");
        assert_eq!(API_WHITELIST_REMOVE, "/api/whitelist/remove");
        assert_eq!(API_CHANNELS, "/api/channels");
        assert_eq!(API_LOGS, "/api/logs");
        assert_eq!(API_WPASEC, "/api/wpasec");
        assert_eq!(API_DISCORD, "/api/discord");
        assert_eq!(API_DOWNLOAD_SINGLE, "/api/download/:filename");
        assert_eq!(API_RESTART_PI, "/api/restart-pi");
        assert_eq!(API_RESTART_SSH, "/api/restart-ssh");
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
            autohunt_enabled: true,
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
            fw_crash_suppress: 0, fw_hardfault: 0,
            fw_health: "Unknown".into(),
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
            pending_upload: 2, total_size_bytes: 1024000,
            session_captures: 5, session_handshakes: 2,
            files: vec![],
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
            gpsd_available: true,
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
        assert!(ds.wpasec_api_key.is_empty());
        assert!(ds.pending_wpasec_key.is_none());
        assert!(ds.discord_webhook_url.is_empty());
        assert!(!ds.discord_enabled);
        assert!(ds.pending_discord_config.is_none());
    }

    #[test]
    fn test_build_router_compiles() {
        let state = test_state();
        let ws_tx = create_ws_broadcast();
        let _router = build_router(state, ws_tx);
    }

    // === Dashboard HTML tests ===

    #[test]
    fn test_dashboard_html_contains_all_cards() {
        assert!(DASHBOARD_HTML.contains("<title>oxigotchi</title>"));
        // face card removed — e-ink preview replaces it
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
        // download card merged into captures card
        assert!(DASHBOARD_HTML.contains("card-mode"), "missing mode card");
        assert!(DASHBOARD_HTML.contains("card-actions"), "missing actions card");
        assert!(DASHBOARD_HTML.contains("card-plugins"), "missing plugins card");
        assert!(DASHBOARD_HTML.contains("card-aps"), "missing APs card");
        assert!(DASHBOARD_HTML.contains("card-whitelist"), "missing whitelist card");
        assert!(DASHBOARD_HTML.contains("card-channels"), "missing channels card");
        assert!(DASHBOARD_HTML.contains("card-logs"), "missing logs card");
        assert!(DASHBOARD_HTML.contains("card-wpasec"), "missing wpasec card");
        assert!(DASHBOARD_HTML.contains("card-discord"), "missing discord card");
        assert!(DASHBOARD_HTML.contains("card-settings"), "missing settings card");
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
        assert!(DASHBOARD_HTML.contains("/api/plugins"), "missing /api/plugins");
        assert!(DASHBOARD_HTML.contains("/api/aps"), "missing /api/aps");
        assert!(DASHBOARD_HTML.contains("/api/whitelist"), "missing /api/whitelist");
        assert!(DASHBOARD_HTML.contains("/api/channels"), "missing /api/channels");
        assert!(DASHBOARD_HTML.contains("/api/logs"), "missing /api/logs");
        assert!(DASHBOARD_HTML.contains("/api/wpasec"), "missing /api/wpasec");
        assert!(DASHBOARD_HTML.contains("/api/discord"), "missing /api/discord");
        assert!(DASHBOARD_HTML.contains("/api/restart-pi"), "missing /api/restart-pi");
        assert!(DASHBOARD_HTML.contains("/api/restart-ssh"), "missing /api/restart-ssh");
        assert!(DASHBOARD_HTML.contains("/api/download/"), "missing /api/download");
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
        assert!(resp.message.contains("TOGGLE")); // toggle passed through to daemon
        let s = state.lock().unwrap();
        assert_eq!(s.pending_mode_switch.as_deref(), Some("TOGGLE"));
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
            "/api/recovery", "/api/cracked", "/api/plugins",
            "/api/aps", "/api/whitelist", "/api/logs",
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

    #[tokio::test]
    async fn test_download_zip_endpoint_exists() {
        let state = test_state();
        let ws_tx = create_ws_broadcast();
        let router = build_router(state, ws_tx);
        let (status, _body) = get(&router, "/api/download/all").await;
        // get() returns (u16, String); 200 = OK with empty zip for nonexistent capture dir
        assert_eq!(status, 200);
    }

    #[tokio::test]
    async fn test_plugins_get_empty() {
        let (router, _state) = test_router();
        let (status, body) = get(&router, "/api/plugins").await;
        assert_eq!(status, 200);
        let plugins: Vec<PluginInfo> = serde_json::from_str(&body).unwrap();
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn test_plugins_post_queues_update() {
        let (router, state) = test_router();
        let (status, _body) = post_json(&router, "/api/plugins", r#"[{"name":"uptime","x":100,"y":50}]"#).await;
        assert_eq!(status, 200);
        let s = state.lock().unwrap();
        assert_eq!(s.pending_plugin_updates.len(), 1);
        assert_eq!(s.pending_plugin_updates[0].name, "uptime");
    }
}
