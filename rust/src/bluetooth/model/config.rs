use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BtMode {
    Off,
    Passive,
    Telemetry,
    Lab,
    Attack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtFeatureConfig {
    pub enabled: bool,
    pub mode: BtMode,
    pub discovery: BtDiscoveryConfig,
    pub controller: BtControllerConfig,
    pub coex: BtCoexConfig,
    pub ui: BtUiConfig,
    pub storage: BtStorageConfig,
}

impl Default for BtFeatureConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: BtMode::Passive,
            discovery: BtDiscoveryConfig::default(),
            controller: BtControllerConfig::default(),
            coex: BtCoexConfig::default(),
            ui: BtUiConfig::default(),
            storage: BtStorageConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtDiscoveryConfig {
    pub ble: bool,
    pub classic: bool,
    pub cache_limit: usize,
    pub summary_interval_sec: u64,
}

impl Default for BtDiscoveryConfig {
    fn default() -> Self {
        Self {
            ble: true,
            classic: true,
            cache_limit: 256,
            summary_interval_sec: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtControllerConfig {
    pub telemetry_enabled: bool,
    pub snapshot_interval_sec: u64,
    pub health_probe_interval_sec: u64,
}

impl Default for BtControllerConfig {
    fn default() -> Self {
        Self {
            telemetry_enabled: true,
            snapshot_interval_sec: 120,
            health_probe_interval_sec: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtCoexConfig {
    pub enabled: bool,
    pub overlap_window_ms: u64,
    pub contention_threshold: u32,
}

impl Default for BtCoexConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            overlap_window_ms: 2000,
            contention_threshold: 70,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtUiConfig {
    pub dashboard_enabled: bool,
    pub eink_summary_enabled: bool,
    pub eink_min_interval_sec: u64,
}

impl Default for BtUiConfig {
    fn default() -> Self {
        Self {
            dashboard_enabled: true,
            eink_summary_enabled: true,
            eink_min_interval_sec: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtStorageConfig {
    pub device_history_limit: usize,
    pub retain_controller_snapshots: bool,
}

impl Default for BtStorageConfig {
    fn default() -> Self {
        Self {
            device_history_limit: 1000,
            retain_controller_snapshots: true,
        }
    }
}
