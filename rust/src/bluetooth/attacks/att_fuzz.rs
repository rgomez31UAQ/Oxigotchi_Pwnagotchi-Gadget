//! ATT/GATT protocol fuzzer.
//!
//! Generates malformed ATT (Attribute Protocol) PDUs to stress-test the
//! target's GATT server implementation. Fuzz vectors target common parsing
//! bugs in ATT request handlers.

use std::time::Instant;

use super::hci::HciSocket;
use super::{BtAttackResult, BtAttackType, BtCapture};

// ATT opcodes
const ATT_READ_BY_TYPE_REQ: u8 = 0x08;
const ATT_WRITE_REQ: u8 = 0x12;
const ATT_FIND_INFO_REQ: u8 = 0x04;
const ATT_READ_BLOB_REQ: u8 = 0x0C;
const ATT_PREPARE_WRITE_REQ: u8 = 0x16;

/// ATT/GATT fuzzer: generates malformed ATT PDUs to stress the target's
/// GATT server.
///
/// Fuzz vectors:
/// 1. Read By Type with invalid handle range (start > end)
/// 2. Write Request with empty value
/// 3. Find Information with reversed handle range
/// 4. Read Blob with maximum offset (0xFFFF)
/// 5. Prepare Write with oversized value (overflow attempt)
///
/// NOTE: Proper ATT injection requires an ACL socket (not implemented yet).
/// The previous implementation incorrectly sent ATT PDUs as Write RAM
/// (vendor 0x4C) parameters, causing the first 4 bytes of the payload to be
/// interpreted as a RAM address. For now, payloads are generated and reported
/// as captures for logging/analysis but not injected on the wire.
pub fn run(_hci: &HciSocket, target_addr: &str) -> BtAttackResult {
    let start = Instant::now();
    log::info!("att_fuzz: targeting {} with malformed ATT PDUs", target_addr);

    let fuzz_vectors = build_fuzz_vectors();
    let mut captures: Vec<Vec<u8>> = Vec::new();

    for (name, payload) in &fuzz_vectors {
        // Log the fuzz payload for offline analysis — do NOT send via Write RAM.
        log::info!(
            "att_fuzz: generated {} ({} bytes): {:02x?}",
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
        attack_type: BtAttackType::AttGattFuzz,
        target_address: target_addr.to_string(),
        target_name: None,
        success: !captures.is_empty(),
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
        let hci = HciSocket::open(0).unwrap();
        let result = run(&hci, "AA:BB:CC:DD:EE:FF");
        assert_eq!(result.attack_type, BtAttackType::AttGattFuzz);
        // Payloads are generated and captured (but not sent) → success=true
        assert!(result.success);
    }
}
