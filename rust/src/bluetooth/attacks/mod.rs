//! BT attack configuration, type enums, and rage-level filtering.
//!
//! Defines the 6 attack types (4 auto, 2 manual-only), rage levels, and the
//! [`BtAttackConfig`] struct that drives the offensive BT mode.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

pub mod att_fuzz;
pub mod ble_adv;
pub mod hci;
pub mod knob;
pub mod l2cap_conn_flood;
pub mod l2cap_fuzz;
pub mod smp;
pub mod target;
pub mod l2cap_socket;
pub mod vendor;
pub mod scan;

// ---------------------------------------------------------------------------
// BtAttackType — the 8 enum variants (6 active in ALL, 2 retired)
// ---------------------------------------------------------------------------

/// Individual BT attack types that can be toggled on/off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BtAttackType {
    SmpDowngrade,
    Knob,
    BleAdvInjection,
    L2capFuzz,
    L2capConnFlood,
    AttGattFuzz,
    VendorCmdUnlock,
}

impl BtAttackType {
    /// All 7 active variants in canonical order.
    /// SmpMitm and BleConnHijack are retired — removed from auto scheduling.
    pub const ALL: [BtAttackType; 7] = [
        BtAttackType::SmpDowngrade,
        BtAttackType::Knob,
        BtAttackType::BleAdvInjection,
        BtAttackType::L2capFuzz,
        BtAttackType::L2capConnFlood,
        BtAttackType::AttGattFuzz,
        BtAttackType::VendorCmdUnlock,
    ];

    /// Human-readable short name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SmpDowngrade => "smp_downgrade",
            Self::Knob => "knob",
            Self::BleAdvInjection => "ble_adv_injection",
            Self::L2capFuzz => "l2cap_fuzz",
            Self::L2capConnFlood => "l2cap_conn_flood",
            Self::AttGattFuzz => "att_gatt_fuzz",
            Self::VendorCmdUnlock => "vendor_cmd_unlock",
        }
    }

    /// Whether this attack requires a patchram (HCD) swap.
    pub fn requires_patchram(self) -> bool {
        matches!(self, Self::Knob | Self::VendorCmdUnlock)
    }

    /// Whether this attack targets BLE (Low Energy) connections.
    pub fn is_ble(self) -> bool {
        matches!(
            self,
            Self::SmpDowngrade
                | Self::BleAdvInjection
                | Self::AttGattFuzz
        )
    }

    /// Whether this attack targets BR/EDR (classic) connections.
    pub fn is_classic(self) -> bool {
        matches!(self, Self::Knob | Self::L2capFuzz | Self::L2capConnFlood)
    }

    /// Whether this attack runs automatically via TargetSelector each epoch.
    pub fn is_auto(self) -> bool {
        matches!(self, Self::SmpDowngrade | Self::Knob | Self::L2capFuzz | Self::L2capConnFlood | Self::AttGattFuzz)
    }

    /// Whether this attack can be launched manually against a specific device.
    pub fn is_manual(self) -> bool {
        matches!(self, Self::Knob | Self::BleAdvInjection | Self::L2capFuzz | Self::L2capConnFlood | Self::AttGattFuzz | Self::VendorCmdUnlock)
    }

    /// Minimum rage level required to activate this attack.
    pub fn min_rage_level(self) -> BtRageLevel {
        match self {
            // Low: passive diagnostics only (targets own controller, not external devices)
            Self::VendorCmdUnlock => BtRageLevel::Low,
            // Medium: active attacks that target external devices
            Self::SmpDowngrade | Self::Knob | Self::BleAdvInjection | Self::L2capFuzz | Self::AttGattFuzz => BtRageLevel::Medium,
            // High: disruptive flood attacks
            Self::L2capConnFlood => BtRageLevel::High,
        }
    }
}

// ---------------------------------------------------------------------------
// BtRageLevel
// ---------------------------------------------------------------------------

/// How aggressive the BT attack engine should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BtRageLevel {
    Low = 0,
    Medium = 1,
    High = 2,
}

impl BtRageLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" | "med" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

impl Default for BtRageLevel {
    fn default() -> Self {
        Self::Medium
    }
}

// ---------------------------------------------------------------------------
// BtScanMode
// ---------------------------------------------------------------------------

/// Which scan types to run each BT epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BtScanMode {
    Ble,
    Classic,
    Both,
}

impl Default for BtScanMode {
    fn default() -> Self {
        Self::Both
    }
}

impl BtScanMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ble => "ble",
            Self::Classic => "classic",
            Self::Both => "both",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ble" => Some(Self::Ble),
            "classic" => Some(Self::Classic),
            "both" => Some(Self::Both),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// BtAttackConfig
// ---------------------------------------------------------------------------

/// Full configuration for the BT offensive mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtAttackConfig {
    /// Master switch for the attack subsystem.
    #[serde(default)]
    pub enabled: bool,

    /// Current rage level — filters which attacks can fire.
    #[serde(default)]
    pub rage_level: BtRageLevel,

    /// Which scan types to run: Ble, Classic, or Both (alternating).
    #[serde(default)]
    pub scan_mode: BtScanMode,

    // -- attack toggles (ble_adv_injection and vendor_cmd_unlock removed — manual-only) --
    #[serde(default = "default_true")]
    pub smp_downgrade: bool,
    #[serde(default = "default_true")]
    pub knob: bool,
    #[serde(default)]
    pub l2cap_fuzz: bool,
    #[serde(default)]
    pub l2cap_conn_flood: bool,
    #[serde(default)]
    pub att_gatt_fuzz: bool,

    // -- Target selection ---------------------------------------------------
    /// Minimum RSSI (dBm) to consider a target. Default: -80.
    #[serde(default = "default_min_rssi")]
    pub min_rssi: i16,
    /// Maximum simultaneous attack sessions. Default: 3.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_attacks: u32,
    /// Seconds a target stays in the active pool. Default: 300.
    #[serde(default = "default_target_ttl")]
    pub target_ttl_secs: u64,
    /// MAC addresses (or prefixes) to never attack.
    #[serde(default)]
    pub whitelist: Vec<String>,

    // -- Paths & XP ---------------------------------------------------------
    /// Directory for pcap / SMP capture output.
    #[serde(default = "default_capture_dir")]
    pub capture_dir: String,
    /// Whether captured handshakes count toward XP.
    #[serde(default = "default_true")]
    pub captures_count_as_xp: bool,
    /// Path to the attack-mode HCD patchram.
    #[serde(default = "default_attack_hcd")]
    pub attack_hcd: String,
    /// Path to the stock (factory) HCD.
    #[serde(default = "default_stock_hcd")]
    pub stock_hcd: String,
    /// Maximum capture directory size in MB. 0 = no rotation.
    #[serde(default = "default_max_capture_mb")]
    pub max_capture_mb: u32,
}

// -- serde default helpers ---------------------------------------------------

fn default_true() -> bool {
    true
}
fn default_min_rssi() -> i16 {
    -80
}
fn default_max_concurrent() -> u32 {
    3
}
fn default_target_ttl() -> u64 {
    300
}
fn default_capture_dir() -> String {
    "/etc/oxigotchi/bt_captures".into()
}
fn default_attack_hcd() -> String {
    "/etc/oxigotchi/bt_attack.hcd".into()
}
fn default_stock_hcd() -> String {
    "/lib/firmware/brcm/BCM43430B0.hcd".into()
}
fn default_max_capture_mb() -> u32 {
    50
}

impl Default for BtAttackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rage_level: BtRageLevel::default(),
            scan_mode: BtScanMode::default(),
            smp_downgrade: true,
            knob: true,
            l2cap_fuzz: false,
            l2cap_conn_flood: false,
            att_gatt_fuzz: false,
            min_rssi: default_min_rssi(),
            max_concurrent_attacks: default_max_concurrent(),
            target_ttl_secs: default_target_ttl(),
            whitelist: Vec::new(),
            capture_dir: default_capture_dir(),
            captures_count_as_xp: true,
            attack_hcd: default_attack_hcd(),
            stock_hcd: default_stock_hcd(),
            max_capture_mb: default_max_capture_mb(),
        }
    }
}

impl BtAttackConfig {
    /// Returns the toggle state for each of the 5 auto attacks, in order:
    /// [smp_downgrade, knob, l2cap_fuzz, l2cap_conn_flood, att_gatt_fuzz].
    pub fn enabled_toggles(&self) -> [bool; 5] {
        [
            self.smp_downgrade,
            self.knob,
            self.l2cap_fuzz,
            self.l2cap_conn_flood,
            self.att_gatt_fuzz,
        ]
    }

    /// Set the toggle for a specific attack type.
    /// Manual-only attacks (BleAdvInjection, VendorCmdUnlock) are no-ops.
    pub fn set_toggle(&mut self, attack: BtAttackType, enabled: bool) {
        match attack {
            BtAttackType::SmpDowngrade => self.smp_downgrade = enabled,
            BtAttackType::Knob => self.knob = enabled,
            BtAttackType::L2capFuzz => self.l2cap_fuzz = enabled,
            BtAttackType::L2capConnFlood => self.l2cap_conn_flood = enabled,
            BtAttackType::AttGattFuzz => self.att_gatt_fuzz = enabled,
            _ => {} // manual-only attacks have no toggle
        }
    }

    /// Returns the list of auto attack types that are both toggled on **and**
    /// permitted at the current [`rage_level`].
    pub fn active_at_rage_level(&self) -> Vec<BtAttackType> {
        let toggles = self.enabled_toggles();
        let auto_attacks = [
            BtAttackType::SmpDowngrade,
            BtAttackType::Knob,
            BtAttackType::L2capFuzz,
            BtAttackType::L2capConnFlood,
            BtAttackType::AttGattFuzz,
        ];
        auto_attacks
            .iter()
            .zip(toggles.iter())
            .filter(|&(attack, &on)| on && attack.min_rage_level() <= self.rage_level)
            .map(|(attack, _)| *attack)
            .collect()
    }

    /// Check whether a MAC address (or prefix) is in the whitelist.
    pub fn is_whitelisted(&self, mac: &str) -> bool {
        let mac_upper = mac.to_ascii_uppercase();
        self.whitelist
            .iter()
            .any(|entry| mac_upper.starts_with(&entry.to_ascii_uppercase()))
    }
}

// ---------------------------------------------------------------------------
// BtCapture — payload variants captured during attacks
// ---------------------------------------------------------------------------

/// Captured artifact from a BT attack.
#[derive(Debug, Clone)]
pub enum BtCapture {
    LinkKey { address: String, key: Vec<u8> },
    PairingTranscript { address: String, data: Vec<u8> },
    FuzzCrash { address: String, trigger: Vec<u8> },
    VendorResult { opcode: u16, response: Vec<u8> },
}

// ---------------------------------------------------------------------------
// BtAttackResult — outcome of a single attack attempt
// ---------------------------------------------------------------------------

/// Result of a single BT attack attempt.
#[derive(Debug, Clone)]
pub struct BtAttackResult {
    pub attack_type: BtAttackType,
    pub target_address: String,
    pub target_name: Option<String>,
    pub success: bool,
    pub capture: Option<BtCapture>,
    pub error: Option<String>,
    /// Human-readable summary of what happened (e.g. "5/5 sent, no crash").
    pub detail: Option<String>,
    pub timestamp: Instant,
}

// ---------------------------------------------------------------------------
// BtAttackScheduler — manages active attacks, history, and concurrency
// ---------------------------------------------------------------------------

/// Maximum attack results kept in the rolling history.
const MAX_HISTORY: usize = 100;

/// Schedules and tracks BT attack sessions.
pub struct BtAttackScheduler {
    pub config: BtAttackConfig,
    pub total_attacks: u64,
    pub total_captures: u64,
    /// device_id -> currently running attack type
    pub active_attacks: HashMap<String, BtAttackType>,
    /// Bounded rolling history of completed attack results.
    pub attack_history: Vec<BtAttackResult>,
}

impl BtAttackScheduler {
    /// Create a new scheduler with the given config.
    pub fn new(config: BtAttackConfig) -> Self {
        Self {
            config,
            total_attacks: 0,
            total_captures: 0,
            active_attacks: HashMap::new(),
            attack_history: Vec::new(),
        }
    }

    /// Attack types that are both toggled on and permitted at the current rage level.
    pub fn active_attack_types(&self) -> Vec<BtAttackType> {
        self.config.active_at_rage_level()
    }

    /// Record a completed attack result.
    ///
    /// Increments counters and appends to bounded history (oldest evicted
    /// when full). The caller should call [`remove_active`] before this
    /// to clear the device from the active set.
    pub fn record(&mut self, result: BtAttackResult) {
        self.total_attacks += 1;
        if result.capture.is_some() {
            self.total_captures += 1;
        }
        if self.attack_history.len() >= MAX_HISTORY {
            self.attack_history.remove(0);
        }
        self.attack_history.push(result);
    }

    /// Mark a device as having an active attack.
    pub fn mark_active(&mut self, device_id: &str, attack: BtAttackType) {
        self.active_attacks.insert(device_id.to_string(), attack);
    }

    /// Remove a device from the active attack set (call when attack completes).
    pub fn remove_active(&mut self, device_id: &str) {
        self.active_attacks.remove(device_id);
    }

    /// Whether we have capacity for another concurrent attack.
    pub fn can_attack(&self) -> bool {
        (self.active_attacks.len() as u32) < self.config.max_concurrent_attacks
    }

    /// Number of currently active attack sessions.
    pub fn active_count(&self) -> u32 {
        self.active_attacks.len() as u32
    }

}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let cfg = BtAttackConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.rage_level, BtRageLevel::Medium);
        assert!(cfg.smp_downgrade);
        assert!(cfg.knob);
        assert!(!cfg.l2cap_fuzz);
        assert!(!cfg.att_gatt_fuzz);
        assert_eq!(cfg.min_rssi, -80);
        assert_eq!(cfg.max_concurrent_attacks, 3);
        assert_eq!(cfg.target_ttl_secs, 300);
        assert!(cfg.whitelist.is_empty());
        assert_eq!(cfg.capture_dir, "/etc/oxigotchi/bt_captures");
        assert!(cfg.captures_count_as_xp);
    }

    #[test]
    fn test_enabled_toggles_order() {
        let cfg = BtAttackConfig::default();
        let t = cfg.enabled_toggles();
        assert_eq!(t, [true, true, false, false, false]); // smp=true, knob=true, l2cap_fuzz=false, l2cap_conn_flood=false, att=false
    }

    #[test]
    fn test_set_toggle() {
        let mut cfg = BtAttackConfig::default();
        cfg.set_toggle(BtAttackType::SmpDowngrade, false);
        assert!(!cfg.smp_downgrade);
        cfg.set_toggle(BtAttackType::L2capFuzz, true);
        assert!(cfg.l2cap_fuzz);
        // Manual-only attacks are no-ops
        cfg.set_toggle(BtAttackType::BleAdvInjection, true);
        // No field to check — it's a no-op
    }

    #[test]
    fn test_active_at_rage_low() {
        let mut cfg = BtAttackConfig::default();
        cfg.rage_level = BtRageLevel::Low;
        let active = cfg.active_at_rage_level();
        assert!(active.is_empty(), "No auto attacks at Low rage (all auto attacks require Medium)");
    }

    #[test]
    fn test_active_at_rage_medium() {
        let cfg = BtAttackConfig::default(); // rage = Medium (new default)
        let active = cfg.active_at_rage_level();
        assert_eq!(active.len(), 2);
        assert!(active.contains(&BtAttackType::SmpDowngrade));
        assert!(active.contains(&BtAttackType::Knob));
    }

    #[test]
    fn test_active_at_rage_high() {
        let mut cfg = BtAttackConfig::default();
        cfg.rage_level = BtRageLevel::High;
        cfg.l2cap_fuzz = true;
        cfg.l2cap_conn_flood = true;
        cfg.att_gatt_fuzz = true;
        let active = cfg.active_at_rage_level();
        assert_eq!(active.len(), 5);
    }

    #[test]
    fn test_whitelist_matching() {
        let mut cfg = BtAttackConfig::default();
        cfg.whitelist = vec!["AA:BB:CC".into(), "11:22:33:44:55:66".into()];
        assert!(cfg.is_whitelisted("AA:BB:CC:DD:EE:FF"));
        assert!(cfg.is_whitelisted("aa:bb:cc:dd:ee:ff")); // case-insensitive
        assert!(cfg.is_whitelisted("11:22:33:44:55:66"));
        assert!(!cfg.is_whitelisted("FF:FF:FF:FF:FF:FF"));
    }

    #[test]
    fn test_attack_type_properties() {
        // SMP uses LE HCI commands — it's BLE, not Classic, and no patchram
        assert!(BtAttackType::SmpDowngrade.is_ble());
        assert!(!BtAttackType::SmpDowngrade.is_classic());
        assert!(!BtAttackType::SmpDowngrade.requires_patchram());

        assert!(BtAttackType::BleAdvInjection.is_ble());
        assert!(!BtAttackType::BleAdvInjection.is_classic());
        assert!(!BtAttackType::BleAdvInjection.requires_patchram());

        // KNOB is BR/EDR classic and needs patchram
        assert!(BtAttackType::Knob.is_classic());
        assert!(!BtAttackType::Knob.is_ble());
        assert!(BtAttackType::Knob.requires_patchram());

        // L2capConnFlood is classic, no patchram
        assert!(BtAttackType::L2capConnFlood.is_classic());
        assert!(!BtAttackType::L2capConnFlood.is_ble());
        assert!(!BtAttackType::L2capConnFlood.requires_patchram());

        assert!(BtAttackType::VendorCmdUnlock.requires_patchram());
        assert!(!BtAttackType::VendorCmdUnlock.is_ble());
        assert!(!BtAttackType::VendorCmdUnlock.is_classic());
    }

    #[test]
    fn test_rage_level_str_roundtrip() {
        for level in [BtRageLevel::Low, BtRageLevel::Medium, BtRageLevel::High] {
            let s = level.as_str();
            assert_eq!(BtRageLevel::from_str(s), Some(level));
        }
        assert_eq!(BtRageLevel::from_str("med"), Some(BtRageLevel::Medium));
        assert_eq!(BtRageLevel::from_str("unknown"), None);
    }

    #[test]
    fn test_attack_type_as_str() {
        assert_eq!(BtAttackType::SmpDowngrade.as_str(), "smp_downgrade");
        assert_eq!(BtAttackType::AttGattFuzz.as_str(), "att_gatt_fuzz");
    }

    #[test]
    fn test_all_variants_count() {
        assert_eq!(BtAttackType::ALL.len(), 7);
    }

    #[test]
    fn test_serde_roundtrip() {
        let cfg = BtAttackConfig::default();
        let toml_str = toml::to_string(&cfg).unwrap();
        let parsed: BtAttackConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.enabled_toggles(), cfg.enabled_toggles());
        assert_eq!(parsed.rage_level, cfg.rage_level);
        assert_eq!(parsed.min_rssi, cfg.min_rssi);
    }

    #[test]
    fn test_toml_parsing() {
        let toml = r#"
enabled = true
rage_level = "High"
smp_downgrade = false
knob = true
l2cap_fuzz = true
att_gatt_fuzz = true
min_rssi = -70
max_concurrent_attacks = 5
target_ttl_secs = 600
whitelist = ["AA:BB:CC"]
capture_dir = "/tmp/bt_caps"
captures_count_as_xp = false
attack_hcd = "/tmp/attack.hcd"
stock_hcd = "/tmp/stock.hcd"
"#;
        let cfg: BtAttackConfig = toml::from_str(toml).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.rage_level, BtRageLevel::High);
        assert!(!cfg.smp_downgrade);
        assert!(cfg.knob);
        assert_eq!(cfg.min_rssi, -70);
        assert_eq!(cfg.max_concurrent_attacks, 5);
        assert_eq!(cfg.whitelist, vec!["AA:BB:CC"]);
        assert_eq!(cfg.capture_dir, "/tmp/bt_caps");
        assert!(!cfg.captures_count_as_xp);
    }

    // -----------------------------------------------------------------------
    // BtAttackScheduler tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_scheduler_new() {
        let sched = BtAttackScheduler::new(BtAttackConfig::default());
        assert_eq!(sched.total_attacks, 0);
        assert_eq!(sched.total_captures, 0);
        assert!(sched.active_attacks.is_empty());
        assert!(sched.attack_history.is_empty());
    }

    #[test]
    fn test_scheduler_active_types_delegates() {
        let sched = BtAttackScheduler::new(BtAttackConfig::default());
        let types = sched.active_attack_types();
        // Default config: rage=Medium, auto attacks on=[smp_downgrade, knob]
        assert_eq!(types.len(), 2);
        assert!(types.contains(&BtAttackType::SmpDowngrade));
        assert!(types.contains(&BtAttackType::Knob));
    }

    #[test]
    fn test_scheduler_mark_active_and_can_attack() {
        let mut cfg = BtAttackConfig::default();
        cfg.max_concurrent_attacks = 2;
        let mut sched = BtAttackScheduler::new(cfg);

        assert!(sched.can_attack());
        assert_eq!(sched.active_count(), 0);

        sched.mark_active("d1", BtAttackType::Knob);
        assert!(sched.can_attack());
        assert_eq!(sched.active_count(), 1);

        sched.mark_active("d2", BtAttackType::SmpDowngrade);
        assert!(!sched.can_attack()); // at capacity
        assert_eq!(sched.active_count(), 2);
    }

    #[test]
    fn test_scheduler_remove_active() {
        let mut sched = BtAttackScheduler::new(BtAttackConfig::default());
        sched.mark_active("d1", BtAttackType::Knob);
        assert_eq!(sched.active_count(), 1);

        sched.remove_active("d1");
        assert_eq!(sched.active_count(), 0);
        assert!(sched.can_attack());
    }

    #[test]
    fn test_scheduler_record_increments() {
        let mut sched = BtAttackScheduler::new(BtAttackConfig::default());

        // Record without capture
        sched.record(BtAttackResult {
            attack_type: BtAttackType::Knob,
            target_address: "AA:BB:CC:DD:EE:FF".into(),
            target_name: None,
            success: false,
            capture: None,
            error: Some("timeout".into()),
            detail: None,
            timestamp: Instant::now(),
        });
        assert_eq!(sched.total_attacks, 1);
        assert_eq!(sched.total_captures, 0);
        assert_eq!(sched.attack_history.len(), 1);

        // Record with capture
        sched.record(BtAttackResult {
            attack_type: BtAttackType::SmpDowngrade,
            target_address: "11:22:33:44:55:66".into(),
            target_name: Some("Phone".into()),
            success: true,
            capture: Some(BtCapture::LinkKey {
                address: "11:22:33:44:55:66".into(),
                key: vec![0xAA; 16],
            }),
            error: None,
            detail: None,
            timestamp: Instant::now(),
        });
        assert_eq!(sched.total_attacks, 2);
        assert_eq!(sched.total_captures, 1);
        assert_eq!(sched.attack_history.len(), 2);
    }

    #[test]
    fn test_scheduler_history_bounded() {
        let mut sched = BtAttackScheduler::new(BtAttackConfig::default());
        for i in 0..150 {
            sched.record(BtAttackResult {
                attack_type: BtAttackType::L2capFuzz,
                target_address: format!("AA:BB:CC:DD:{:02X}:{:02X}", i / 256, i % 256),
                target_name: None,
                success: false,
                capture: None,
                error: None,
                detail: None,
                timestamp: Instant::now(),
            });
        }
        assert_eq!(sched.attack_history.len(), MAX_HISTORY); // capped at 100
        assert_eq!(sched.total_attacks, 150); // counter still tracks all
    }

    #[test]
    fn test_scheduler_mark_active_overwrites() {
        let mut sched = BtAttackScheduler::new(BtAttackConfig::default());
        sched.mark_active("d1", BtAttackType::Knob);
        sched.mark_active("d1", BtAttackType::SmpDowngrade);
        assert_eq!(sched.active_count(), 1);
        assert_eq!(
            sched.active_attacks.get("d1"),
            Some(&BtAttackType::SmpDowngrade)
        );
    }

    #[test]
    fn test_is_auto() {
        assert!(BtAttackType::SmpDowngrade.is_auto());
        assert!(BtAttackType::Knob.is_auto());
        assert!(BtAttackType::L2capFuzz.is_auto());
        assert!(BtAttackType::L2capConnFlood.is_auto());
        assert!(BtAttackType::AttGattFuzz.is_auto());
        assert!(!BtAttackType::BleAdvInjection.is_auto());
        assert!(!BtAttackType::VendorCmdUnlock.is_auto());
    }

    #[test]
    fn test_is_manual() {
        assert!(BtAttackType::Knob.is_manual());
        assert!(BtAttackType::BleAdvInjection.is_manual());
        assert!(BtAttackType::L2capFuzz.is_manual());
        assert!(BtAttackType::L2capConnFlood.is_manual());
        assert!(BtAttackType::AttGattFuzz.is_manual());
        assert!(BtAttackType::VendorCmdUnlock.is_manual());
        assert!(!BtAttackType::SmpDowngrade.is_manual());
    }

    #[test]
    fn test_min_rage_level_is_pub() {
        assert_eq!(BtAttackType::VendorCmdUnlock.min_rage_level(), BtRageLevel::Low);
        assert_eq!(BtAttackType::Knob.min_rage_level(), BtRageLevel::Medium);
        assert_eq!(BtAttackType::L2capConnFlood.min_rage_level(), BtRageLevel::High);
    }

    #[test]
    fn test_scan_mode_default_is_both() {
        let cfg = BtAttackConfig::default();
        assert_eq!(cfg.scan_mode, BtScanMode::Both);
    }

    #[test]
    fn test_scan_mode_serde_roundtrip() {
        for mode in [BtScanMode::Ble, BtScanMode::Classic, BtScanMode::Both] {
            let s = mode.as_str();
            assert_eq!(BtScanMode::from_str(s), Some(mode));
        }
        assert_eq!(BtScanMode::from_str("garbage"), None);
    }

    #[test]
    fn test_scan_mode_serde_default_missing_field() {
        // Simulate a config.toml that doesn't have scan_mode at all
        let toml = r#"
enabled = true
rage_level = "Medium"
"#;
        let cfg: BtAttackConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.scan_mode, BtScanMode::Both);
    }
}
