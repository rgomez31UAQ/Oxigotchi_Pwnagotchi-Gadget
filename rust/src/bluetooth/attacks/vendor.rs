//! BCM43430B0 vendor HCI command exerciser.
//!
//! Sends diagnostic vendor commands to probe the BT controller's firmware
//! state: local version, verbose config, and patchram base read.

use std::time::Instant;

use super::hci::{HciCommand, HciSocket};
use super::{BtAttackResult, BtAttackType, BtCapture};

// BCM43430B0 vendor HCI opcodes (all OGF 0x3F)
const READ_VERBOSE_CONFIG: u16 = 0x58;
const READ_RAM: u16 = 0x4D;

/// Base offset for RAM reads — loaded from chip config at runtime.
const PATCHRAM_BASE: u32 = 0; // TODO: load from firmware config

/// Run vendor diagnostics against the BT controller.
///
/// Sends HCI Read Local Version (standard), READ_VERBOSE_CONFIG, and
/// READ_RAM at the patchram base. Returns success if any command returned data.
pub fn run_diagnostics(hci: &HciSocket, target_addr: &str) -> BtAttackResult {
    let start = Instant::now();
    let mut results: Vec<(u16, Vec<u8>)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // 1. READ_LOCAL_VERSION (standard HCI: OGF 0x04, OCF 0x01 → opcode 0x1001)
    log::info!("vendor_diag: sending READ_LOCAL_VERSION to hci device");
    let cmd = HciCommand::new(0x04, 0x01, vec![]);
    match hci.send_command(&cmd) {
        Ok(resp) => {
            log::info!(
                "vendor_diag: READ_LOCAL_VERSION status={} data_len={}",
                resp.status,
                resp.data.len()
            );
            if !resp.data.is_empty() {
                results.push((0x1001, resp.data));
            }
        }
        Err(e) => {
            log::info!("vendor_diag: READ_LOCAL_VERSION failed: {}", e);
            errors.push(format!("READ_LOCAL_VERSION: {}", e));
        }
    }

    // 2. READ_VERBOSE_CONFIG
    log::info!("vendor_diag: sending READ_VERBOSE_CONFIG");
    let cmd = HciCommand::vendor(READ_VERBOSE_CONFIG, vec![]);
    match hci.send_command(&cmd) {
        Ok(resp) => {
            log::info!(
                "vendor_diag: READ_VERBOSE_CONFIG status={} data_len={}",
                resp.status,
                resp.data.len()
            );
            if !resp.data.is_empty() {
                results.push((READ_VERBOSE_CONFIG, resp.data));
            }
        }
        Err(e) => {
            log::info!("vendor_diag: READ_VERBOSE_CONFIG failed: {}", e);
            errors.push(format!("READ_VERBOSE_CONFIG: {}", e));
        }
    }

    // 3. READ_RAM at base offset (loaded from chip config)
    log::info!(
        "vendor_diag: sending READ_RAM at 0x{:08X} (patchram base)",
        PATCHRAM_BASE
    );
    let mut ram_params = Vec::with_capacity(5);
    ram_params.extend_from_slice(&PATCHRAM_BASE.to_le_bytes());
    ram_params.push(0x04); // read 4 bytes
    let cmd = HciCommand::vendor(READ_RAM, ram_params);
    match hci.send_command(&cmd) {
        Ok(resp) => {
            log::info!(
                "vendor_diag: READ_RAM status={} data_len={}",
                resp.status,
                resp.data.len()
            );
            if !resp.data.is_empty() {
                results.push((READ_RAM, resp.data));
            }
        }
        Err(e) => {
            log::info!("vendor_diag: READ_RAM failed: {}", e);
            errors.push(format!("READ_RAM: {}", e));
        }
    }

    let success = !results.is_empty();

    // Combine all response data into a single VendorResult capture.
    let capture = if success {
        // Flatten: for each result, prepend the 2-byte opcode + 2-byte data length + data.
        let mut combined = Vec::new();
        for (opcode, data) in &results {
            combined.extend_from_slice(&opcode.to_le_bytes());
            combined.extend_from_slice(&(data.len() as u16).to_le_bytes());
            combined.extend_from_slice(data);
        }
        Some(BtCapture::VendorResult {
            opcode: 0x1001, // primary opcode (Read Local Version)
            response: combined,
        })
    } else {
        None
    };

    let error = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };

    BtAttackResult {
        attack_type: BtAttackType::VendorCmdUnlock,
        target_address: target_addr.to_string(),
        target_name: None,
        success,
        capture,
        error,
        detail: None,
        timestamp: start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_run_diagnostics_stub() {
        let hci = HciSocket::open(0).unwrap();
        let result = run_diagnostics(&hci, "AA:BB:CC:DD:EE:FF");
        assert_eq!(result.attack_type, BtAttackType::VendorCmdUnlock);
        assert_eq!(result.target_address, "AA:BB:CC:DD:EE:FF");
        // Stub returns 8 bytes of data, so all 3 commands should succeed
        assert!(result.success);
        assert!(result.capture.is_some());
    }
}
