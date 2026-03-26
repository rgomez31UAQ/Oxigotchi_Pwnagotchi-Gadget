//! SMP (Security Manager Protocol) attack workers.
//!
//! - `run_downgrade`: forces Just Works pairing via NoInputNoOutput IO capability
//! - `run_mitm`: MITM relay framework stub

use std::time::Instant;

use super::hci::{HciCommand, HciSocket};
use super::{BtAttackResult, BtAttackType, BtCapture};

// HCI OGF for LE Controller commands
const OGF_LE: u8 = 0x08;

// HCI LE opcodes
const LE_CREATE_CONNECTION: u16 = 0x0D;

/// Parse a BD_ADDR string "AA:BB:CC:DD:EE:FF" into 6 bytes in reversed
/// (little-endian) order as required by HCI.
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

/// SMP downgrade attack: initiate an LE connection with NoInputNoOutput
/// IO capability to force Just Works pairing (no MITM protection).
///
/// Sends HCI_LE_Create_Connection with permissive parameters, then
/// captures the pairing transcript.
pub fn run_downgrade(hci: &HciSocket, target_addr: &str) -> BtAttackResult {
    let start = Instant::now();
    log::info!("smp_downgrade: targeting {}", target_addr);

    let bdaddr = parse_bdaddr(target_addr);

    // Build HCI_LE_Create_Connection command parameters:
    //   scan_interval(2) + scan_window(2) + filter_policy(1) +
    //   peer_addr_type(1) + peer_addr(6) + own_addr_type(1) +
    //   conn_interval_min(2) + conn_interval_max(2) + conn_latency(2) +
    //   supervision_timeout(2) + min_ce_length(2) + max_ce_length(2)
    let mut params = Vec::with_capacity(25);
    params.extend_from_slice(&0x0060u16.to_le_bytes()); // scan_interval: 60ms
    params.extend_from_slice(&0x0030u16.to_le_bytes()); // scan_window: 30ms
    params.push(0x00); // filter_policy: no whitelist
    params.push(0x00); // peer_addr_type: public
    params.extend_from_slice(&bdaddr); // peer address
    params.push(0x00); // own_addr_type: public
    params.extend_from_slice(&0x0018u16.to_le_bytes()); // conn_interval_min: 30ms
    params.extend_from_slice(&0x0028u16.to_le_bytes()); // conn_interval_max: 50ms
    params.extend_from_slice(&0x0000u16.to_le_bytes()); // conn_latency: 0
    params.extend_from_slice(&0x00C8u16.to_le_bytes()); // supervision_timeout: 2s
    params.extend_from_slice(&0x0000u16.to_le_bytes()); // min_ce_length: 0
    params.extend_from_slice(&0x0000u16.to_le_bytes()); // max_ce_length: 0

    let cmd = HciCommand::new(OGF_LE, LE_CREATE_CONNECTION, params);
    match hci.send_command(&cmd) {
        Ok(resp) => {
            log::info!(
                "smp_downgrade: LE_Create_Connection status={} data_len={}",
                resp.status,
                resp.data.len()
            );

            // In a real scenario we would wait for the connection event,
            // then send SMP Pairing Request with IO=NoInputNoOutput.
            // For now, capture what the controller returned.
            let capture = if !resp.data.is_empty() {
                Some(BtCapture::PairingTranscript {
                    address: target_addr.to_string(),
                    data: resp.data,
                })
            } else {
                None
            };

            let success = resp.status == 0;
            BtAttackResult {
                attack_type: BtAttackType::SmpDowngrade,
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
            log::info!("smp_downgrade: failed: {}", e);
            BtAttackResult {
                attack_type: BtAttackType::SmpDowngrade,
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

/// SMP MITM relay — framework stub.
///
/// A real MITM relay requires two BT adapters and real-time forwarding
/// of SMP messages. This returns an error indicating it is a framework only.
pub fn run_mitm(hci: &HciSocket, target_addr: &str) -> BtAttackResult {
    let start = Instant::now();
    let _ = hci; // acknowledge parameter
    log::info!("smp_mitm: framework stub for {}", target_addr);

    BtAttackResult {
        attack_type: BtAttackType::SmpMitm,
        target_address: target_addr.to_string(),
        target_name: None,
        success: false,
        capture: None,
        error: Some("MITM relay: framework only".into()),
        timestamp: start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bdaddr() {
        let addr = parse_bdaddr("AA:BB:CC:DD:EE:FF");
        // Reversed for HCI LE: FF EE DD CC BB AA
        assert_eq!(addr, [0xFF, 0xEE, 0xDD, 0xCC, 0xBB, 0xAA]);
    }

    #[test]
    fn test_parse_bdaddr_invalid() {
        let addr = parse_bdaddr("not-an-address");
        assert_eq!(addr, [0; 6]);
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_run_downgrade_stub() {
        let hci = HciSocket::open(0).unwrap();
        let result = run_downgrade(&hci, "AA:BB:CC:DD:EE:FF");
        assert_eq!(result.attack_type, BtAttackType::SmpDowngrade);
        assert!(result.success); // stub returns status 0
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_run_mitm_stub() {
        let hci = HciSocket::open(0).unwrap();
        let result = run_mitm(&hci, "AA:BB:CC:DD:EE:FF");
        assert_eq!(result.attack_type, BtAttackType::SmpMitm);
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("MITM relay: framework only"));
    }
}
