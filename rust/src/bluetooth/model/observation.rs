use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BtTransport {
    Ble,
    Classic,
    Dual,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BtCategory {
    Phone,
    Audio,
    Computer,
    IoT,
    Peripheral,
    Wearable,
    Unknown,
}

impl BtCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            BtCategory::Phone => "phone",
            BtCategory::Audio => "audio",
            BtCategory::Computer => "computer",
            BtCategory::IoT => "iot",
            BtCategory::Peripheral => "peripheral",
            BtCategory::Wearable => "wearable",
            BtCategory::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BtDeviceAttackState {
    Untouched,
    Targeted,
    Attacking,
    Captured,
    Failed,
}

impl Default for BtDeviceAttackState {
    fn default() -> Self {
        BtDeviceAttackState::Untouched
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtDeviceObservation {
    pub id: String,
    pub address: String,
    pub address_type: Option<String>,
    pub transport: BtTransport,
    pub name: Option<String>,
    pub rssi: Option<i16>,
    pub rssi_best: Option<i16>,
    pub category: BtCategory,
    pub services: Vec<String>,
    pub manufacturer: Option<String>,
    pub first_seen: DateTime<Utc>,
    pub ts: DateTime<Utc>,
    pub seen_count: u32,
    pub attack_state: BtDeviceAttackState,
    /// Which attack was last attempted (e.g. "SmpDowngrade", "Knob").
    #[serde(default)]
    pub last_attack: Option<String>,
    /// Short result detail (e.g. "Connection timeout", "Key captured").
    #[serde(default)]
    pub last_attack_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BtDiscoveryObservation {
    ScanStarted,
    ScanStopped,
    DeviceSeen(BtDeviceObservation),
    DeviceLost { id: String, ts: DateTime<Utc> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BtControllerObservation {
    ControllerPresent {
        ts: DateTime<Utc>,
    },
    ControllerMissing {
        ts: DateTime<Utc>,
    },
    ProbeResult {
        probe_name: String,
        ok: bool,
        detail: Option<String>,
        ts: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RfObservation {
    WifiScanStarted {
        ts: DateTime<Utc>,
    },
    WifiScanStopped {
        ts: DateTime<Utc>,
    },
    WifiError {
        detail: Option<String>,
        ts: DateTime<Utc>,
    },
    BtDiscoveryStarted {
        ts: DateTime<Utc>,
    },
    BtDiscoveryStopped {
        ts: DateTime<Utc>,
    },
}
