//! HCD file validation and hciattach command builders for BCM43436B0.

use std::path::Path;

/// Minimum valid HCD file size in bytes.
const HCD_MIN_SIZE: u64 = 100;
/// Maximum valid HCD file size in bytes.
const HCD_MAX_SIZE: u64 = 200_000;

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

/// Build hciattach arguments for loading an HCD file on UART.
///
/// Produces: `/dev/ttyS0 bcm43xx 3000000 flow -b <hcd_path>`
pub fn build_hciattach_args(hcd_path: &str) -> Vec<String> {
    vec![
        "/dev/ttyS0".into(),
        "bcm43xx".into(),
        "3000000".into(),
        "flow".into(),
        "-b".into(),
        hcd_path.into(),
    ]
}

/// Build hciconfig arguments to bring hci0 up.
pub fn build_hci_up_args() -> Vec<String> {
    vec!["hci0".into(), "up".into()]
}

/// Build hciconfig arguments to bring hci0 down.
pub fn build_hci_down_args() -> Vec<String> {
    vec!["hci0".into(), "down".into()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hciattach_args() {
        let args = build_hciattach_args("/lib/firmware/attack.hcd");
        assert_eq!(args.len(), 6);
        assert_eq!(args[0], "/dev/ttyS0");
        assert_eq!(args[1], "bcm43xx");
        assert_eq!(args[2], "3000000");
        assert_eq!(args[3], "flow");
        assert_eq!(args[4], "-b");
        assert_eq!(args[5], "/lib/firmware/attack.hcd");
    }

    #[test]
    fn test_hci_up_args() {
        let args = build_hci_up_args();
        assert_eq!(args, vec!["hci0", "up"]);
    }

    #[test]
    fn test_hci_down_args() {
        let args = build_hci_down_args();
        assert_eq!(args, vec!["hci0", "down"]);
    }

    #[test]
    fn test_validate_hcd_missing() {
        let result = validate_hcd("/nonexistent/file.hcd");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
