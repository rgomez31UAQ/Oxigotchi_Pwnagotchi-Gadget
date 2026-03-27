//! HCD file validation and firmware path constants for BCM43430B0.
//!
//! The kernel's btbcm driver loads firmware from two search paths.
//! Both must be updated when deploying a custom HCD.

use std::path::Path;

/// Minimum valid HCD file size in bytes.
const HCD_MIN_SIZE: u64 = 100;
/// Maximum valid HCD file size in bytes.
const HCD_MAX_SIZE: u64 = 200_000;

// --- Firmware paths (btbcm checks both) ---

/// Primary firmware path (Broadcom naming).
pub const FIRMWARE_BRCM: &str = "/lib/firmware/brcm/BCM43430B0.hcd";
/// Secondary firmware path (Synaptics naming).
pub const FIRMWARE_SYNAPTICS: &str = "/lib/firmware/synaptics/SYN43430B0.hcd";
/// Backup for brcm path.
pub const FIRMWARE_BRCM_BACKUP: &str = "/lib/firmware/brcm/BCM43430B0.hcd.orig";
/// Backup for synaptics path.
pub const FIRMWARE_SYNAPTICS_BACKUP: &str = "/lib/firmware/synaptics/SYN43430B0.hcd.orig";

/// Validate that an HCD file exists and has a reasonable size.
pub fn validate_hcd(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(format!("HCD file not found: {path}"));
    }
    let meta = p.metadata().map_err(|e| format!("cannot stat HCD file: {e}"))?;
    let size = meta.len();
    if size < HCD_MIN_SIZE {
        return Err(format!(
            "HCD file too small ({size} bytes, min {HCD_MIN_SIZE}): {path}"
        ));
    }
    if size > HCD_MAX_SIZE {
        return Err(format!(
            "HCD file too large ({size} bytes, max {HCD_MAX_SIZE}): {path}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firmware_paths() {
        assert!(FIRMWARE_BRCM.ends_with("BCM43430B0.hcd"));
        assert!(FIRMWARE_SYNAPTICS.ends_with("SYN43430B0.hcd"));
        assert!(FIRMWARE_BRCM_BACKUP.ends_with(".orig"));
        assert!(FIRMWARE_SYNAPTICS_BACKUP.ends_with(".orig"));
    }

    #[test]
    fn test_validate_hcd_missing() {
        let result = validate_hcd("/nonexistent/file.hcd");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
