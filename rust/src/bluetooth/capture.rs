//! BT capture manager — stores attack results (keys, crash logs, vendor
//! results) to disk.
//!
//! File writing is `#[cfg(target_os = "linux")]` only; on other platforms
//! the counters still update but nothing hits the filesystem.

use crate::bluetooth::attacks::{BtAttackResult, BtCapture};
#[cfg(target_os = "linux")]
use log::{error, info};
use std::path::PathBuf;

/// Manages storage of BT attack capture artifacts.
pub struct BtCaptureManager {
    pub base_dir: PathBuf,
    pub total_keys: u32,
    pub total_crashes: u32,
    pub total_vendor: u32,
    pub total_transcripts: u32,
}

/// Replace ':' with empty string so MACs are safe for filenames.
fn sanitize_mac(mac: &str) -> String {
    mac.replace(':', "")
}

impl BtCaptureManager {
    pub fn new(base_dir: &str) -> Self {
        Self {
            base_dir: PathBuf::from(base_dir),
            total_keys: 0,
            total_crashes: 0,
            total_vendor: 0,
            total_transcripts: 0,
        }
    }

    /// Create capture subdirectories (keys/, pairing/, fuzz/, vendor/).
    /// Linux-only; no-op on other platforms.
    pub fn init_dirs(&self) {
        #[cfg(target_os = "linux")]
        {
            for sub in &["keys", "pairing", "fuzz", "vendor"] {
                let dir = self.base_dir.join(sub);
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    error!("bt_capture: failed to create {}: {}", dir.display(), e);
                } else {
                    info!("bt_capture: ensured dir {}", dir.display());
                }
            }
        }
    }

    /// Store a capture from an attack result to the appropriate subdirectory.
    ///
    /// File naming: `{sanitized_mac}_{timestamp}.{ext}` (or opcode-based for
    /// vendor results).  Increments the per-type counter regardless of
    /// platform; actual file I/O is Linux-only.
    pub fn store(&mut self, result: &BtAttackResult) {
        let capture = match &result.capture {
            Some(c) => c,
            None => return,
        };

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();

        match capture {
            BtCapture::LinkKey { address, key } => {
                self.total_keys += 1;
                let filename = format!(
                    "{}_{}.key",
                    sanitize_mac(address),
                    timestamp
                );
                self.write_file("keys", &filename, key);
            }
            BtCapture::PairingTranscript { address, data } => {
                self.total_transcripts += 1;
                let filename = format!(
                    "{}_{}.bin",
                    sanitize_mac(address),
                    timestamp
                );
                self.write_file("pairing", &filename, data);
            }
            BtCapture::FuzzCrash { address, trigger } => {
                self.total_crashes += 1;
                let filename = format!(
                    "{}_{}.crash",
                    sanitize_mac(address),
                    timestamp
                );
                self.write_file("fuzz", &filename, trigger);
            }
            BtCapture::VendorResult { opcode, response } => {
                self.total_vendor += 1;
                let filename = format!("0x{:04X}_{}.bin", opcode, timestamp);
                self.write_file("vendor", &filename, response);
            }
        }
    }

    /// Total captures across all types.
    pub fn total_captures(&self) -> u32 {
        self.total_keys + self.total_crashes + self.total_vendor + self.total_transcripts
    }

    /// Write bytes to `{base_dir}/{subdir}/{filename}`.
    /// Linux-only; logs errors but never panics.
    #[allow(unused_variables)]
    fn write_file(&self, subdir: &str, filename: &str, data: &[u8]) {
        #[cfg(target_os = "linux")]
        {
            let path = self.base_dir.join(subdir).join(filename);
            if let Err(e) = std::fs::write(&path, data) {
                error!("bt_capture: failed to write {}: {}", path.display(), e);
            } else {
                info!("bt_capture: wrote {}", path.display());
            }
        }
    }

    /// Calculate total size of all files in the capture directory tree (bytes).
    pub fn dir_size_bytes(&self) -> u64 {
        let mut total: u64 = 0;
        for sub in &["keys", "pairing", "fuzz", "vendor"] {
            let dir = self.base_dir.join(sub);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        total += meta.len();
                    }
                }
            }
        }
        total
    }

    /// Check capture directory size and rotate oldest files if over limit.
    /// `max_mb` of 0 disables rotation.
    pub fn rotate_if_needed(&self, max_mb: u32) {
        if max_mb == 0 {
            return;
        }

        let max_bytes = (max_mb as u64) * 1024 * 1024;
        let current = self.dir_size_bytes();
        if current <= max_bytes {
            return;
        }

        let target = max_bytes * 80 / 100;
        let mut freed: u64 = 0;
        let mut removed: u32 = 0;

        let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = Vec::new();
        for sub in &["keys", "pairing", "fuzz", "vendor"] {
            let dir = self.base_dir.join(sub);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        files.push((entry.path(), meta.len(), mtime));
                    }
                }
            }
        }

        files.sort_by_key(|(_, _, t)| *t);

        for (path, size, _) in &files {
            if current - freed <= target {
                break;
            }
            if std::fs::remove_file(path).is_ok() {
                freed += size;
                removed += 1;
            }
        }

        if removed > 0 {
            log::info!(
                "bt_capture: rotated {} files, freed {}MB",
                removed,
                freed / 1024 / 1024
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bluetooth::attacks::{BtAttackResult, BtAttackType, BtCapture};
    use std::time::Instant;

    fn dummy_result(capture: Option<BtCapture>) -> BtAttackResult {
        BtAttackResult {
            attack_type: BtAttackType::Knob,
            target_address: "AA:BB:CC:DD:EE:FF".into(),
            target_name: None,
            success: true,
            capture,
            error: None,
            timestamp: Instant::now(),
        }
    }

    #[test]
    fn test_sanitize_mac() {
        assert_eq!(sanitize_mac("AA:BB:CC:DD:EE:FF"), "AABBCCDDEEFF");
        assert_eq!(sanitize_mac("aabb"), "aabb");
        assert_eq!(sanitize_mac(""), "");
    }

    #[test]
    fn test_new_zeroed() {
        let mgr = BtCaptureManager::new("/tmp/bt_test");
        assert_eq!(mgr.total_keys, 0);
        assert_eq!(mgr.total_crashes, 0);
        assert_eq!(mgr.total_vendor, 0);
        assert_eq!(mgr.total_transcripts, 0);
        assert_eq!(mgr.total_captures(), 0);
    }

    #[test]
    fn test_store_link_key_increments() {
        let mut mgr = BtCaptureManager::new("/tmp/bt_test");
        let result = dummy_result(Some(BtCapture::LinkKey {
            address: "AA:BB:CC:DD:EE:FF".into(),
            key: vec![0xAA; 16],
        }));
        mgr.store(&result);
        assert_eq!(mgr.total_keys, 1);
        assert_eq!(mgr.total_captures(), 1);
    }

    #[test]
    fn test_store_pairing_transcript_increments() {
        let mut mgr = BtCaptureManager::new("/tmp/bt_test");
        let result = dummy_result(Some(BtCapture::PairingTranscript {
            address: "11:22:33:44:55:66".into(),
            data: vec![0x01, 0x02, 0x03],
        }));
        mgr.store(&result);
        assert_eq!(mgr.total_transcripts, 1);
        assert_eq!(mgr.total_captures(), 1);
    }

    #[test]
    fn test_store_fuzz_crash_increments() {
        let mut mgr = BtCaptureManager::new("/tmp/bt_test");
        let result = dummy_result(Some(BtCapture::FuzzCrash {
            address: "DE:AD:BE:EF:00:01".into(),
            trigger: vec![0xFF; 64],
        }));
        mgr.store(&result);
        assert_eq!(mgr.total_crashes, 1);
        assert_eq!(mgr.total_captures(), 1);
    }

    #[test]
    fn test_store_vendor_result_increments() {
        let mut mgr = BtCaptureManager::new("/tmp/bt_test");
        let result = dummy_result(Some(BtCapture::VendorResult {
            opcode: 0xFC01,
            response: vec![0x00, 0x01],
        }));
        mgr.store(&result);
        assert_eq!(mgr.total_vendor, 1);
        assert_eq!(mgr.total_captures(), 1);
    }

    #[test]
    fn test_store_no_capture_is_noop() {
        let mut mgr = BtCaptureManager::new("/tmp/bt_test");
        let result = dummy_result(None);
        mgr.store(&result);
        assert_eq!(mgr.total_captures(), 0);
    }

    #[test]
    fn test_total_captures_sums_all() {
        let mut mgr = BtCaptureManager::new("/tmp/bt_test");
        mgr.total_keys = 3;
        mgr.total_crashes = 2;
        mgr.total_vendor = 5;
        mgr.total_transcripts = 1;
        assert_eq!(mgr.total_captures(), 11);
    }

    #[test]
    fn test_rotate_if_needed_zero_disabled() {
        let mgr = BtCaptureManager::new("/tmp/bt_test_rotate");
        mgr.rotate_if_needed(0);
    }

    #[test]
    fn test_dir_size_bytes_nonexistent() {
        let mgr = BtCaptureManager::new("/tmp/bt_nonexistent_dir_12345");
        assert_eq!(mgr.dir_size_bytes(), 0);
    }
}
