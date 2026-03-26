//! BT attack configuration, type enums, and rage-level filtering.
//!
//! Defines the 8 toggleable attack types, rage levels, and the
//! [`BtAttackConfig`] struct that drives the offensive BT mode.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// BtAttackType — the 8 attack variants
// ---------------------------------------------------------------------------

/// Individual BT attack types that can be toggled on/off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BtAttackType {
    SmpDowngrade,
    SmpMitm,
    Knob,
    BleAdvInjection,
    BleConnHijack,
    L2capFuzz,
    AttGattFuzz,
    VendorCmdUnlock,
}

impl BtAttackType {
    /// All 8 variants in canonical order.
    pub const ALL: [BtAttackType; 8] = [
        BtAttackType::SmpDowngrade,
        BtAttackType::SmpMitm,
        BtAttackType::Knob,
        BtAttackType::BleAdvInjection,
        BtAttackType::BleConnHijack,
        BtAttackType::L2capFuzz,
        BtAttackType::AttGattFuzz,
        BtAttackType::VendorCmdUnlock,
    ];

    /// Human-readable short name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SmpDowngrade => "smp_downgrade",
            Self::SmpMitm => "smp_mitm",
            Self::Knob => "knob",
            Self::BleAdvInjection => "ble_adv_injection",
            Self::BleConnHijack => "ble_conn_hijack",
            Self::L2capFuzz => "l2cap_fuzz",
            Self::AttGattFuzz => "att_gatt_fuzz",
            Self::VendorCmdUnlock => "vendor_cmd_unlock",
        }
    }

    /// Whether this attack requires a patchram (HCD) swap.
    pub fn requires_patchram(self) -> bool {
        matches!(
            self,
            Self::SmpDowngrade
                | Self::SmpMitm
                | Self::Knob
                | Self::VendorCmdUnlock
        )
    }

    /// Whether this attack targets BLE (Low Energy) connections.
    pub fn is_ble(self) -> bool {
        matches!(
            self,
            Self::BleAdvInjection | Self::BleConnHijack | Self::AttGattFuzz
        )
    }

    /// Whether this attack targets BR/EDR (classic) connections.
    pub fn is_classic(self) -> bool {
        matches!(
            self,
            Self::SmpDowngrade | Self::SmpMitm | Self::Knob | Self::L2capFuzz
        )
    }

    /// Minimum rage level required to activate this attack.
    fn min_rage_level(self) -> BtRageLevel {
        match self {
            // Low: safe/passive-ish attacks
            Self::SmpDowngrade | Self::Knob | Self::VendorCmdUnlock => BtRageLevel::Low,
            // Medium: active injection / fuzzing
            Self::BleAdvInjection | Self::L2capFuzz | Self::AttGattFuzz => BtRageLevel::Medium,
            // High: aggressive / disruptive
            Self::SmpMitm | Self::BleConnHijack => BtRageLevel::High,
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
        Self::Low
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

    // -- 8 attack toggles --------------------------------------------------
    #[serde(default = "default_true")]
    pub smp_downgrade: bool,
    #[serde(default)]
    pub smp_mitm: bool,
    #[serde(default = "default_true")]
    pub knob: bool,
    #[serde(default)]
    pub ble_adv_injection: bool,
    #[serde(default)]
    pub ble_conn_hijack: bool,
    #[serde(default)]
    pub l2cap_fuzz: bool,
    #[serde(default)]
    pub att_gatt_fuzz: bool,
    #[serde(default = "default_true")]
    pub vendor_cmd_unlock: bool,

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
    "/lib/firmware/brcm/SYN43430B0.hcd".into()
}

impl Default for BtAttackConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rage_level: BtRageLevel::default(),
            smp_downgrade: true,
            smp_mitm: false,
            knob: true,
            ble_adv_injection: false,
            ble_conn_hijack: false,
            l2cap_fuzz: false,
            att_gatt_fuzz: false,
            vendor_cmd_unlock: true,
            min_rssi: default_min_rssi(),
            max_concurrent_attacks: default_max_concurrent(),
            target_ttl_secs: default_target_ttl(),
            whitelist: Vec::new(),
            capture_dir: default_capture_dir(),
            captures_count_as_xp: true,
            attack_hcd: default_attack_hcd(),
            stock_hcd: default_stock_hcd(),
        }
    }
}

impl BtAttackConfig {
    /// Returns the toggle state for each of the 8 attacks, in
    /// [`BtAttackType::ALL`] order.
    pub fn enabled_toggles(&self) -> [bool; 8] {
        [
            self.smp_downgrade,
            self.smp_mitm,
            self.knob,
            self.ble_adv_injection,
            self.ble_conn_hijack,
            self.l2cap_fuzz,
            self.att_gatt_fuzz,
            self.vendor_cmd_unlock,
        ]
    }

    /// Set the toggle for a specific attack type.
    pub fn set_toggle(&mut self, attack: BtAttackType, enabled: bool) {
        match attack {
            BtAttackType::SmpDowngrade => self.smp_downgrade = enabled,
            BtAttackType::SmpMitm => self.smp_mitm = enabled,
            BtAttackType::Knob => self.knob = enabled,
            BtAttackType::BleAdvInjection => self.ble_adv_injection = enabled,
            BtAttackType::BleConnHijack => self.ble_conn_hijack = enabled,
            BtAttackType::L2capFuzz => self.l2cap_fuzz = enabled,
            BtAttackType::AttGattFuzz => self.att_gatt_fuzz = enabled,
            BtAttackType::VendorCmdUnlock => self.vendor_cmd_unlock = enabled,
        }
    }

    /// Returns the list of attack types that are both toggled on **and**
    /// permitted at the current [`rage_level`].
    pub fn active_at_rage_level(&self) -> Vec<BtAttackType> {
        let toggles = self.enabled_toggles();
        BtAttackType::ALL
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let cfg = BtAttackConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.rage_level, BtRageLevel::Low);
        assert!(cfg.smp_downgrade);
        assert!(!cfg.smp_mitm);
        assert!(cfg.knob);
        assert!(!cfg.ble_adv_injection);
        assert!(!cfg.ble_conn_hijack);
        assert!(!cfg.l2cap_fuzz);
        assert!(!cfg.att_gatt_fuzz);
        assert!(cfg.vendor_cmd_unlock);
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
        // smp_downgrade=true, smp_mitm=false, knob=true, ...
        assert_eq!(t, [true, false, true, false, false, false, false, true]);
    }

    #[test]
    fn test_set_toggle() {
        let mut cfg = BtAttackConfig::default();
        cfg.set_toggle(BtAttackType::SmpMitm, true);
        assert!(cfg.smp_mitm);
        cfg.set_toggle(BtAttackType::Knob, false);
        assert!(!cfg.knob);
    }

    #[test]
    fn test_active_at_rage_low() {
        let cfg = BtAttackConfig::default(); // rage = Low
        let active = cfg.active_at_rage_level();
        // Only Low-level attacks that are also toggled on
        assert!(active.contains(&BtAttackType::SmpDowngrade));
        assert!(active.contains(&BtAttackType::Knob));
        assert!(active.contains(&BtAttackType::VendorCmdUnlock));
        assert_eq!(active.len(), 3);
    }

    #[test]
    fn test_active_at_rage_medium() {
        let mut cfg = BtAttackConfig::default();
        cfg.rage_level = BtRageLevel::Medium;
        cfg.ble_adv_injection = true;
        let active = cfg.active_at_rage_level();
        // Low-level (3 on) + Medium-level ble_adv_injection
        assert!(active.contains(&BtAttackType::BleAdvInjection));
        assert_eq!(active.len(), 4);
    }

    #[test]
    fn test_active_at_rage_high() {
        let mut cfg = BtAttackConfig::default();
        cfg.rage_level = BtRageLevel::High;
        cfg.smp_mitm = true;
        cfg.ble_conn_hijack = true;
        let active = cfg.active_at_rage_level();
        // All toggled-on attacks are allowed at High
        assert_eq!(active.len(), 5); // 3 default + mitm + hijack
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
        assert!(BtAttackType::SmpDowngrade.is_classic());
        assert!(!BtAttackType::SmpDowngrade.is_ble());
        assert!(BtAttackType::SmpDowngrade.requires_patchram());

        assert!(BtAttackType::BleAdvInjection.is_ble());
        assert!(!BtAttackType::BleAdvInjection.is_classic());
        assert!(!BtAttackType::BleAdvInjection.requires_patchram());

        assert!(BtAttackType::VendorCmdUnlock.requires_patchram());
        assert!(!BtAttackType::VendorCmdUnlock.is_ble());
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
        assert_eq!(BtAttackType::ALL.len(), 8);
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
smp_mitm = true
knob = true
ble_adv_injection = true
ble_conn_hijack = true
l2cap_fuzz = true
att_gatt_fuzz = true
vendor_cmd_unlock = false
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
        assert!(cfg.smp_mitm);
        assert_eq!(cfg.min_rssi, -70);
        assert_eq!(cfg.max_concurrent_attacks, 5);
        assert_eq!(cfg.whitelist, vec!["AA:BB:CC"]);
        assert_eq!(cfg.capture_dir, "/tmp/bt_caps");
        assert!(!cfg.captures_count_as_xp);
    }
}
