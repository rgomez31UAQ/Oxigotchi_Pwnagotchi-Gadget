//! Target selection for BT attacks.
//!
//! Scores and ranks (device, attack) pairs based on RSSI, novelty,
//! and transport compatibility. The epoch loop calls [`TargetSelector::select`]
//! each round to decide what to attack next.

use crate::bluetooth::attacks::{BtAttackConfig, BtAttackType};
use crate::bluetooth::model::observation::{BtDeviceAttackState, BtDeviceObservation, BtTransport};

/// A single (device, attack) pair ready for dispatch.
#[derive(Debug, Clone)]
pub struct AttackTarget {
    pub device_id: String,
    pub device_address: String,
    pub device_name: Option<String>,
    pub attack: BtAttackType,
    pub priority: i32,
}

pub struct TargetSelector;

impl TargetSelector {
    /// Select up to `max` targets from the device list for the given active attacks.
    ///
    /// Selection logic:
    /// 1. Skip whitelisted devices
    /// 2. Skip devices below `config.min_rssi`
    /// 3. Skip devices with attack_state `Attacking` or `Captured`
    /// 4. For each remaining device, check which active attacks are applicable
    ///    (BLE attacks for BLE/Dual, Classic attacks for Classic/Dual,
    ///     VendorCmdUnlock for any transport)
    /// 5. Score each (device, attack) pair:
    ///    - RSSI component: `(rssi + 127).clamp(0, 127)` (maps -127..0 dBm to 0..127)
    ///    - Novelty bonus: +50 for `Untouched` devices
    ///    - Named bonus: +10 if device has a name
    /// 6. Sort by priority descending, take `max`
    pub fn select(
        devices: &[&BtDeviceObservation],
        active_attacks: &[BtAttackType],
        config: &BtAttackConfig,
        max: usize,
    ) -> Vec<AttackTarget> {
        let mut candidates: Vec<AttackTarget> = Vec::new();

        for device in devices {
            // 1. Skip whitelisted
            if config.is_whitelisted(&device.address) {
                continue;
            }

            // 2. Skip below min_rssi
            let rssi = device.rssi.unwrap_or(-127);
            if rssi < config.min_rssi {
                continue;
            }

            // 3. Skip Attacking or Captured
            match device.attack_state {
                BtDeviceAttackState::Attacking | BtDeviceAttackState::Captured => continue,
                _ => {}
            }

            // 4. For each active attack, check transport compatibility
            for &attack in active_attacks {
                if !is_attack_applicable(attack, device.transport) {
                    continue;
                }

                // 5. Score
                let rssi_score = (rssi as i32 + 127).clamp(0, 127);
                let novelty_bonus = if device.attack_state == BtDeviceAttackState::Untouched {
                    50
                } else {
                    0
                };
                let named_bonus = if device.name.is_some() { 10 } else { 0 };
                let priority = rssi_score + novelty_bonus + named_bonus;

                candidates.push(AttackTarget {
                    device_id: device.id.clone(),
                    device_address: device.address.clone(),
                    device_name: device.name.clone(),
                    attack,
                    priority,
                });
            }
        }

        // 6. Sort descending by priority, take max
        candidates.sort_by(|a, b| b.priority.cmp(&a.priority));
        candidates.truncate(max);
        candidates
    }
}

/// Check if an attack type is applicable to a given transport.
///
/// - BLE attacks apply to BLE and Dual devices
/// - Classic attacks apply to Classic and Dual devices
/// - VendorCmdUnlock (neither BLE nor Classic) applies to any device
fn is_attack_applicable(attack: BtAttackType, transport: BtTransport) -> bool {
    if attack.is_ble() {
        matches!(transport, BtTransport::Ble | BtTransport::Dual)
    } else if attack.is_classic() {
        matches!(transport, BtTransport::Classic | BtTransport::Dual)
    } else {
        // VendorCmdUnlock — controller-level, works on any transport
        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::bluetooth::model::observation::BtCategory;

    fn make_device(
        id: &str,
        address: &str,
        name: Option<&str>,
        rssi: i16,
        transport: BtTransport,
        attack_state: BtDeviceAttackState,
    ) -> BtDeviceObservation {
        let now = Utc::now();
        BtDeviceObservation {
            id: id.into(),
            address: address.into(),
            address_type: None,
            transport,
            name: name.map(|s| s.into()),
            rssi: Some(rssi),
            rssi_best: Some(rssi),
            category: BtCategory::Unknown,
            services: Vec::new(),
            manufacturer: None,
            first_seen: now,
            ts: now,
            seen_count: 1,
            attack_state,
            last_attack: None,
            last_attack_detail: None,
            name_resolve_attempted: false,
            connectable: true,
        }
    }

    fn default_config() -> BtAttackConfig {
        BtAttackConfig {
            min_rssi: -80,
            ..BtAttackConfig::default()
        }
    }

    #[test]
    fn test_basic_selection() {
        let dev = make_device(
            "d1", "AA:BB:CC:DD:EE:FF", Some("Phone"), -50,
            BtTransport::Ble, BtDeviceAttackState::Untouched,
        );
        let devices = vec![&dev];
        let attacks = vec![BtAttackType::BleAdvInjection];
        let config = default_config();

        let targets = TargetSelector::select(&devices, &attacks, &config, 5);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].device_id, "d1");
        assert_eq!(targets[0].attack, BtAttackType::BleAdvInjection);
        // RSSI(-50 + 127 = 77) + novelty(50) + named(10) = 137
        assert_eq!(targets[0].priority, 137);
    }

    #[test]
    fn test_skips_whitelisted() {
        let dev = make_device(
            "d1", "AA:BB:CC:DD:EE:FF", None, -50,
            BtTransport::Ble, BtDeviceAttackState::Untouched,
        );
        let devices = vec![&dev];
        let attacks = vec![BtAttackType::BleAdvInjection];
        let mut config = default_config();
        config.whitelist = vec!["AA:BB:CC".into()];

        let targets = TargetSelector::select(&devices, &attacks, &config, 5);
        assert!(targets.is_empty());
    }

    #[test]
    fn test_skips_below_min_rssi() {
        let dev = make_device(
            "d1", "AA:BB:CC:DD:EE:FF", None, -90,
            BtTransport::Ble, BtDeviceAttackState::Untouched,
        );
        let devices = vec![&dev];
        let attacks = vec![BtAttackType::BleAdvInjection];
        let config = default_config(); // min_rssi = -80

        let targets = TargetSelector::select(&devices, &attacks, &config, 5);
        assert!(targets.is_empty());
    }

    #[test]
    fn test_skips_attacking_and_captured() {
        let d1 = make_device(
            "d1", "AA:BB:CC:DD:EE:01", None, -50,
            BtTransport::Ble, BtDeviceAttackState::Attacking,
        );
        let d2 = make_device(
            "d2", "AA:BB:CC:DD:EE:02", None, -50,
            BtTransport::Ble, BtDeviceAttackState::Captured,
        );
        let d3 = make_device(
            "d3", "AA:BB:CC:DD:EE:03", None, -50,
            BtTransport::Ble, BtDeviceAttackState::Failed,
        );
        let devices = vec![&d1, &d2, &d3];
        let attacks = vec![BtAttackType::BleAdvInjection];
        let config = default_config();

        let targets = TargetSelector::select(&devices, &attacks, &config, 5);
        // Only d3 (Failed) should pass — Attacking and Captured are skipped
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].device_id, "d3");
    }

    #[test]
    fn test_transport_filtering() {
        let ble_dev = make_device(
            "d1", "11:11:11:11:11:11", None, -50,
            BtTransport::Ble, BtDeviceAttackState::Untouched,
        );
        let classic_dev = make_device(
            "d2", "22:22:22:22:22:22", None, -50,
            BtTransport::Classic, BtDeviceAttackState::Untouched,
        );
        let dual_dev = make_device(
            "d3", "33:33:33:33:33:33", None, -50,
            BtTransport::Dual, BtDeviceAttackState::Untouched,
        );
        let devices = vec![&ble_dev, &classic_dev, &dual_dev];
        let config = default_config();

        // BLE attack — should hit BLE + Dual, not Classic
        let ble_attacks = vec![BtAttackType::BleAdvInjection];
        let targets = TargetSelector::select(&devices, &ble_attacks, &config, 10);
        let ids: Vec<&str> = targets.iter().map(|t| t.device_id.as_str()).collect();
        assert!(ids.contains(&"d1")); // BLE
        assert!(!ids.contains(&"d2")); // Classic — skip
        assert!(ids.contains(&"d3")); // Dual

        // Classic attack — should hit Classic + Dual, not BLE
        let classic_attacks = vec![BtAttackType::Knob];
        let targets = TargetSelector::select(&devices, &classic_attacks, &config, 10);
        let ids: Vec<&str> = targets.iter().map(|t| t.device_id.as_str()).collect();
        assert!(!ids.contains(&"d1")); // BLE — skip
        assert!(ids.contains(&"d2")); // Classic
        assert!(ids.contains(&"d3")); // Dual
    }

    #[test]
    fn test_vendor_cmd_applies_to_all() {
        let ble_dev = make_device(
            "d1", "11:11:11:11:11:11", None, -50,
            BtTransport::Ble, BtDeviceAttackState::Untouched,
        );
        let classic_dev = make_device(
            "d2", "22:22:22:22:22:22", None, -50,
            BtTransport::Classic, BtDeviceAttackState::Untouched,
        );
        let unknown_dev = make_device(
            "d3", "33:33:33:33:33:33", None, -50,
            BtTransport::Unknown, BtDeviceAttackState::Untouched,
        );
        let devices = vec![&ble_dev, &classic_dev, &unknown_dev];
        let attacks = vec![BtAttackType::VendorCmdUnlock];
        let config = default_config();

        let targets = TargetSelector::select(&devices, &attacks, &config, 10);
        assert_eq!(targets.len(), 3); // all transports
    }

    #[test]
    fn test_scoring_and_ordering() {
        // Farther device but untouched + named
        let d1 = make_device(
            "far_named", "11:11:11:11:11:11", Some("MyPhone"), -70,
            BtTransport::Ble, BtDeviceAttackState::Untouched,
        );
        // Closer device, already targeted, no name
        let d2 = make_device(
            "close_targeted", "22:22:22:22:22:22", None, -30,
            BtTransport::Ble, BtDeviceAttackState::Targeted,
        );
        let devices = vec![&d1, &d2];
        let attacks = vec![BtAttackType::BleAdvInjection];
        let config = default_config();

        let targets = TargetSelector::select(&devices, &attacks, &config, 10);
        assert_eq!(targets.len(), 2);
        // d1: (-70+127=57) + 50(untouched) + 10(named) = 117
        // d2: (-30+127=97) + 0(targeted) + 0(unnamed) = 97
        assert_eq!(targets[0].device_id, "far_named");
        assert_eq!(targets[0].priority, 117);
        assert_eq!(targets[1].device_id, "close_targeted");
        assert_eq!(targets[1].priority, 97);
    }

    #[test]
    fn test_max_limits_results() {
        let d1 = make_device(
            "d1", "11:11:11:11:11:11", None, -50,
            BtTransport::Ble, BtDeviceAttackState::Untouched,
        );
        let d2 = make_device(
            "d2", "22:22:22:22:22:22", None, -50,
            BtTransport::Ble, BtDeviceAttackState::Untouched,
        );
        let d3 = make_device(
            "d3", "33:33:33:33:33:33", None, -50,
            BtTransport::Ble, BtDeviceAttackState::Untouched,
        );
        let devices = vec![&d1, &d2, &d3];
        let attacks = vec![BtAttackType::BleAdvInjection];
        let config = default_config();

        let targets = TargetSelector::select(&devices, &attacks, &config, 2);
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn test_no_rssi_treated_as_worst() {
        let mut dev = make_device(
            "d1", "AA:BB:CC:DD:EE:FF", None, -50,
            BtTransport::Ble, BtDeviceAttackState::Untouched,
        );
        dev.rssi = None; // no RSSI -> defaults to -127
        let devices = vec![&dev];
        let attacks = vec![BtAttackType::BleAdvInjection];
        let config = default_config(); // min_rssi = -80

        // -127 < -80, so should be skipped
        let targets = TargetSelector::select(&devices, &attacks, &config, 5);
        assert!(targets.is_empty());
    }

    #[test]
    fn test_empty_inputs() {
        let config = default_config();
        // No devices
        let targets = TargetSelector::select(&[], &[BtAttackType::Knob], &config, 5);
        assert!(targets.is_empty());

        // No attacks
        let dev = make_device(
            "d1", "AA:BB:CC:DD:EE:FF", None, -50,
            BtTransport::Ble, BtDeviceAttackState::Untouched,
        );
        let targets = TargetSelector::select(&[&dev], &[], &config, 5);
        assert!(targets.is_empty());
    }

    #[test]
    fn test_multiple_attacks_per_device() {
        let dev = make_device(
            "d1", "AA:BB:CC:DD:EE:FF", None, -50,
            BtTransport::Dual, BtDeviceAttackState::Untouched,
        );
        let devices = vec![&dev];
        // A BLE attack + a Classic attack — both applicable to Dual
        let attacks = vec![BtAttackType::BleAdvInjection, BtAttackType::Knob];
        let config = default_config();

        let targets = TargetSelector::select(&devices, &attacks, &config, 10);
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn test_is_attack_applicable() {
        // BLE attacks
        assert!(is_attack_applicable(BtAttackType::BleAdvInjection, BtTransport::Ble));
        assert!(is_attack_applicable(BtAttackType::BleAdvInjection, BtTransport::Dual));
        assert!(!is_attack_applicable(BtAttackType::BleAdvInjection, BtTransport::Classic));
        assert!(!is_attack_applicable(BtAttackType::BleAdvInjection, BtTransport::Unknown));

        // Classic attacks
        assert!(is_attack_applicable(BtAttackType::Knob, BtTransport::Classic));
        assert!(is_attack_applicable(BtAttackType::Knob, BtTransport::Dual));
        assert!(!is_attack_applicable(BtAttackType::Knob, BtTransport::Ble));

        // VendorCmdUnlock — applies to everything
        assert!(is_attack_applicable(BtAttackType::VendorCmdUnlock, BtTransport::Ble));
        assert!(is_attack_applicable(BtAttackType::VendorCmdUnlock, BtTransport::Classic));
        assert!(is_attack_applicable(BtAttackType::VendorCmdUnlock, BtTransport::Dual));
        assert!(is_attack_applicable(BtAttackType::VendorCmdUnlock, BtTransport::Unknown));
    }
}
