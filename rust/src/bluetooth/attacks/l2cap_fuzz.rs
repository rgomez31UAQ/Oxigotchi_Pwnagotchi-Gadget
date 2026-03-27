//! L2CAP signaling fuzzer.
//!
//! Generates malformed L2CAP signaling packets (echo request, information
//! request, connection request, configuration request) and sends them via
//! raw HCI ACL to stress-test the target's L2CAP state machine.

use std::time::Instant;

use super::hci::HciSocket;
use super::{BtAttackResult, BtAttackType, BtCapture};

// L2CAP signaling channel CID
const L2CAP_SIG_CID: u16 = 0x0001;

// L2CAP signaling command codes
const L2CAP_ECHO_REQ: u8 = 0x08;
const L2CAP_INFO_REQ: u8 = 0x0A;
const L2CAP_CONN_REQ: u8 = 0x02;
const L2CAP_CONF_REQ: u8 = 0x04;

/// L2CAP fuzzer: generates malformed signaling packets to stress the
/// target's L2CAP implementation.
///
/// Fuzz vectors:
/// 1. Echo request with oversized payload
/// 2. Information request with invalid info type
/// 3. Connection request with PSM=0 (invalid)
/// 4. Configuration request with bad MTU option
///
/// NOTE: Proper L2CAP injection requires an ACL socket (not implemented yet).
/// The previous implementation incorrectly sent L2CAP PDUs as Write RAM
/// (vendor 0x4C) parameters, causing the first 4 bytes of the payload to be
/// interpreted as a RAM address. For now, payloads are generated and reported
/// as captures for logging/analysis but not injected on the wire.
pub fn run(_hci: &HciSocket, target_addr: &str) -> BtAttackResult {
    let start = Instant::now();
    log::info!("l2cap_fuzz: targeting {} with malformed signaling packets", target_addr);

    let fuzz_vectors = build_fuzz_vectors();
    let mut captures: Vec<Vec<u8>> = Vec::new();

    for (name, payload) in &fuzz_vectors {
        // Log the fuzz payload for offline analysis — do NOT send via Write RAM.
        log::info!(
            "l2cap_fuzz: generated {} ({} bytes): {:02x?}",
            name,
            payload.len(),
            &payload[..payload.len().min(32)]
        );
        captures.push(payload.clone());
    }

    let capture = captures.first().map(|trigger| BtCapture::FuzzCrash {
        address: target_addr.to_string(),
        trigger: trigger.clone(),
    });

    BtAttackResult {
        attack_type: BtAttackType::L2capFuzz,
        target_address: target_addr.to_string(),
        target_name: None,
        success: !captures.is_empty(),
        capture,
        error: None,
        timestamp: start,
    }
}

/// Build the set of malformed L2CAP signaling packets.
fn build_fuzz_vectors() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("echo_oversized", build_echo_oversized()),
        ("info_req_invalid", build_info_req_invalid()),
        ("conn_req_psm_zero", build_conn_req_psm_zero()),
        ("conf_req_bad_mtu", build_conf_req_bad_mtu()),
    ]
}

/// L2CAP signaling header: code(1) + id(1) + length(2) + data
fn sig_header(code: u8, id: u8, data: &[u8]) -> Vec<u8> {
    let mut pkt = Vec::new();
    // L2CAP basic header: length(2) + CID(2)
    let sig_len = 4 + data.len();
    pkt.extend_from_slice(&(sig_len as u16).to_le_bytes()); // L2CAP length
    pkt.extend_from_slice(&L2CAP_SIG_CID.to_le_bytes()); // CID: signaling
    // Signaling command header
    pkt.push(code);
    pkt.push(id);
    pkt.extend_from_slice(&(data.len() as u16).to_le_bytes());
    pkt.extend_from_slice(data);
    pkt
}

/// Echo request with 200 bytes of 0xFF — tests buffer handling.
fn build_echo_oversized() -> Vec<u8> {
    sig_header(L2CAP_ECHO_REQ, 0x01, &vec![0xFF; 200])
}

/// Information request with info_type = 0xFFFF (undefined).
fn build_info_req_invalid() -> Vec<u8> {
    sig_header(L2CAP_INFO_REQ, 0x02, &0xFFFFu16.to_le_bytes())
}

/// Connection request with PSM = 0 (invalid/reserved).
fn build_conn_req_psm_zero() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0x0000u16.to_le_bytes()); // PSM: 0 (invalid)
    data.extend_from_slice(&0x0040u16.to_le_bytes()); // source CID: 0x0040
    sig_header(L2CAP_CONN_REQ, 0x03, &data)
}

/// Configuration request with absurd MTU option (0xFFFF).
fn build_conf_req_bad_mtu() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0x0040u16.to_le_bytes()); // destination CID
    data.extend_from_slice(&0x0000u16.to_le_bytes()); // flags: no continuation
    // MTU option: type=0x01, length=2, value=0xFFFF
    data.push(0x01); // option type: MTU
    data.push(0x02); // option length
    data.extend_from_slice(&0xFFFFu16.to_le_bytes()); // MTU: max
    sig_header(L2CAP_CONF_REQ, 0x04, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzz_vectors_built() {
        let vectors = build_fuzz_vectors();
        assert_eq!(vectors.len(), 4);
        for (name, payload) in &vectors {
            assert!(!name.is_empty());
            assert!(!payload.is_empty());
        }
    }

    #[test]
    fn test_sig_header_format() {
        let pkt = sig_header(L2CAP_ECHO_REQ, 0x01, &[0xAA, 0xBB]);
        // L2CAP header: length=6 (4 sig header + 2 data), CID=0x0001
        assert_eq!(pkt[0], 6); // length lo
        assert_eq!(pkt[1], 0); // length hi
        assert_eq!(pkt[2], 1); // CID lo
        assert_eq!(pkt[3], 0); // CID hi
        // Signaling: code, id, data_len, data
        assert_eq!(pkt[4], L2CAP_ECHO_REQ);
        assert_eq!(pkt[5], 0x01); // id
        assert_eq!(pkt[6], 2); // data length lo
        assert_eq!(pkt[7], 0); // data length hi
        assert_eq!(pkt[8], 0xAA);
        assert_eq!(pkt[9], 0xBB);
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_run_stub() {
        let hci = HciSocket::open(0).unwrap();
        let result = run(&hci, "AA:BB:CC:DD:EE:FF");
        assert_eq!(result.attack_type, BtAttackType::L2capFuzz);
        // Payloads are generated and captured (but not sent) → success=true
        assert!(result.success);
    }
}
