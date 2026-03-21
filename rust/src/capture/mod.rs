//! Capture file management: naming, counting, dedup, and upload queue.

use std::path::{Path, PathBuf};

/// Metadata for a captured handshake file.
#[derive(Debug, Clone)]
pub struct CaptureFile {
    /// Full path to the pcapng file.
    pub path: PathBuf,
    /// SSID of the captured network (if known).
    pub ssid: String,
    /// BSSID of the captured network.
    pub bssid: [u8; 6],
    /// Whether a valid handshake/PMKID was found.
    pub has_handshake: bool,
    /// Whether this capture has been uploaded to wpa-sec.
    pub uploaded: bool,
    /// File size in bytes.
    pub size: u64,
}

/// Manages capture files on disk.
pub struct CaptureManager {
    /// Base directory for captures.
    pub capture_dir: PathBuf,
    /// Known capture files.
    pub files: Vec<CaptureFile>,
    /// Maximum number of capture files to keep (0 = unlimited).
    pub max_files: usize,
}

impl CaptureManager {
    /// Create a new capture manager rooted at the given directory.
    pub fn new(capture_dir: &str) -> Self {
        Self {
            capture_dir: PathBuf::from(capture_dir),
            files: Vec::new(),
            max_files: 0,
        }
    }

    /// Generate a capture filename: "{hostname}_{bssid}_{timestamp}.pcapng"
    pub fn generate_filename(&self, hostname: &str, bssid: &[u8; 6]) -> String {
        let bssid_str: String = bssid
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join("");
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        format!("{hostname}_{bssid_str}_{timestamp}.pcapng")
    }

    /// Get the full path for a new capture file.
    pub fn capture_path(&self, filename: &str) -> PathBuf {
        self.capture_dir.join(filename)
    }

    /// Register a capture file.
    pub fn register(&mut self, file: CaptureFile) {
        self.files.push(file);
    }

    /// Count total captures.
    pub fn count(&self) -> usize {
        self.files.len()
    }

    /// Count captures with valid handshakes.
    pub fn handshake_count(&self) -> usize {
        self.files.iter().filter(|f| f.has_handshake).count()
    }

    /// Count captures pending upload.
    pub fn pending_upload_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.has_handshake && !f.uploaded)
            .count()
    }

    /// Get total size of all captures.
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// Mark a capture as uploaded.
    pub fn mark_uploaded(&mut self, path: &Path) {
        if let Some(f) = self.files.iter_mut().find(|f| f.path == path) {
            f.uploaded = true;
        }
    }

    /// Scan the capture directory for existing pcapng files.
    /// Stub implementation — real version would parse pcapng headers.
    pub fn scan_directory(&mut self) -> Result<usize, std::io::Error> {
        let dir = &self.capture_dir;
        if !dir.exists() {
            return Ok(0);
        }
        let mut count = 0;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "pcapng") {
                let meta = entry.metadata()?;
                self.files.push(CaptureFile {
                    path,
                    ssid: String::new(),
                    bssid: [0; 6],
                    has_handshake: false,
                    uploaded: false,
                    size: meta.len(),
                });
                count += 1;
            }
        }
        Ok(count)
    }

    /// Remove oldest captures if over the file limit. Returns number removed.
    pub fn cleanup(&mut self) -> usize {
        if self.max_files == 0 || self.files.len() <= self.max_files {
            return 0;
        }
        let to_remove = self.files.len() - self.max_files;
        // Remove oldest (first in the list) captures
        let removed: Vec<_> = self.files.drain(..to_remove).collect();
        for f in &removed {
            let _ = std::fs::remove_file(&f.path);
        }
        removed.len()
    }
}

/// Upload queue for WPA-SEC integration.
#[derive(Debug, Default)]
pub struct UploadQueue {
    /// Paths of files queued for upload.
    pub queue: Vec<PathBuf>,
    /// Total uploads completed this session.
    pub completed: u32,
    /// Total upload failures this session.
    pub failures: u32,
}

impl UploadQueue {
    /// Create an empty upload queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file to the upload queue.
    pub fn enqueue(&mut self, path: PathBuf) {
        if !self.queue.contains(&path) {
            self.queue.push(path);
        }
    }

    /// Get the next file to upload (FIFO).
    pub fn next(&mut self) -> Option<PathBuf> {
        if self.queue.is_empty() {
            None
        } else {
            Some(self.queue.remove(0))
        }
    }

    /// Record a successful upload.
    pub fn record_success(&mut self) {
        self.completed += 1;
    }

    /// Record a failed upload.
    pub fn record_failure(&mut self, path: PathBuf) {
        self.failures += 1;
        // Re-queue on failure for retry
        self.enqueue(path);
    }

    /// Number of files still waiting to be uploaded.
    pub fn pending(&self) -> usize {
        self.queue.len()
    }
}

// ---------------------------------------------------------------------------
// WPA-SEC upload integration (Python: wpa-sec plugin → capture/wpasec.rs)
// ---------------------------------------------------------------------------

/// WPA-SEC upload client configuration.
#[derive(Debug, Clone)]
pub struct WpaSecConfig {
    /// WPA-SEC API key.
    pub api_key: String,
    /// Upload endpoint URL.
    pub url: String,
    /// Whether uploads are enabled.
    pub enabled: bool,
}

impl Default for WpaSecConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            url: "https://wpa-sec.stanev.org".into(),
            enabled: false,
        }
    }
}

/// Upload a capture file to wpa-sec.stanev.org.
///
/// TODO: Implement HTTP multipart upload using ureq or reqwest crate.
/// The Python plugin POSTs the pcapng file to /upload with the API key header.
pub fn upload_to_wpasec(_path: &Path, _config: &WpaSecConfig) -> Result<(), String> {
    // TODO: implement HTTP POST upload
    // POST {url}/?submit with file field "file" and cookie "key={api_key}"
    Err("WPA-SEC upload not yet implemented".into())
}

// ---------------------------------------------------------------------------
// Quick dictionary attack (Python: better_quickdic.py → capture/quickdic.rs)
// ---------------------------------------------------------------------------

/// Run a quick offline dictionary attack on a captured handshake.
///
/// TODO: Implement using hashcat or aircrack-ng subprocess, or native Rust
/// PBKDF2-SHA1 with a small built-in wordlist.
pub fn quick_dictionary_attack(_pcapng_path: &Path, _wordlist_path: &Path) -> Result<Option<String>, String> {
    // TODO: parse .pcapng → extract PMKID/4-way → try wordlist
    // Returns Some(password) on success, None if no match
    Err("Quick dictionary attack not yet implemented".into())
}

// ---------------------------------------------------------------------------
// Cracked password display (Python: display-password.py → capture/cracked.rs)
// ---------------------------------------------------------------------------

/// A cracked WiFi password entry.
#[derive(Debug, Clone)]
pub struct CrackedPassword {
    /// SSID of the cracked network.
    pub ssid: String,
    /// The cracked password.
    pub password: String,
    /// BSSID of the network.
    pub bssid: [u8; 6],
}

/// Storage for cracked passwords, displayed on the e-ink screen.
#[derive(Debug, Default)]
pub struct CrackedPasswordStore {
    pub passwords: Vec<CrackedPassword>,
}

impl CrackedPasswordStore {
    /// Create an empty cracked-password store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a cracked password.
    pub fn add(&mut self, ssid: &str, password: &str, bssid: [u8; 6]) {
        self.passwords.push(CrackedPassword {
            ssid: ssid.to_string(),
            password: password.to_string(),
            bssid,
        });
    }

    /// Get the most recently cracked password for display.
    pub fn latest(&self) -> Option<&CrackedPassword> {
        self.passwords.last()
    }

    /// Format for e-ink display: "SSID: pass****"
    pub fn display_str(&self) -> String {
        match self.latest() {
            Some(cp) => {
                let masked = if cp.password.len() > 4 {
                    format!("{}****", &cp.password[..4])
                } else {
                    cp.password.clone()
                };
                format!("{}: {}", cp.ssid, masked)
            }
            None => String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Handshake download via web (Python: handshakes-dl.py → capture/download.rs)
// ---------------------------------------------------------------------------

/// List capture files available for download via the web dashboard.
///
/// TODO: Integrate with the axum web server to serve files from capture_dir.
pub fn list_downloadable_captures(capture_dir: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    if !capture_dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    let entries = std::fs::read_dir(capture_dir)
        .map_err(|e| format!("Failed to read {}: {e}", capture_dir.display()))?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "pcapng" || ext == "pcap") {
            files.push(path);
        }
    }
    Ok(files)
}

// ---------------------------------------------------------------------------
// Auto-backup (Python: auto-backup plugin)
// ---------------------------------------------------------------------------

/// Auto-backup configuration.
#[derive(Debug, Clone)]
pub struct AutoBackupConfig {
    /// Whether auto-backup is enabled.
    pub enabled: bool,
    /// Backup interval in seconds.
    pub interval_secs: u64,
    /// Backup destination path.
    pub dest_dir: std::path::PathBuf,
}

impl Default for AutoBackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 3600,
            dest_dir: std::path::PathBuf::from("/home/pi/backups"),
        }
    }
}

/// Perform a backup of captures and config to the backup directory.
///
/// TODO: Implement file copy with timestamp-based naming.
pub fn auto_backup(
    _capture_dir: &Path,
    _config_path: &Path,
    _backup_config: &AutoBackupConfig,
) -> Result<usize, String> {
    // TODO: copy captures + config to dest_dir/YYYYMMDD_HHMMSS/
    Err("Auto-backup not yet implemented".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_filename() {
        let cm = CaptureManager::new("/tmp/captures");
        let bssid = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let name = cm.generate_filename("oxigotchi", &bssid);
        assert!(name.starts_with("oxigotchi_aabbccddeeff_"));
        assert!(name.ends_with(".pcapng"));
    }

    #[test]
    fn test_capture_path() {
        let cm = CaptureManager::new("/tmp/captures");
        let path = cm.capture_path("test.pcapng");
        assert_eq!(path, PathBuf::from("/tmp/captures/test.pcapng"));
    }

    #[test]
    fn test_register_and_count() {
        let mut cm = CaptureManager::new("/tmp/captures");
        assert_eq!(cm.count(), 0);
        cm.register(CaptureFile {
            path: PathBuf::from("/tmp/test1.pcapng"),
            ssid: "Net1".into(),
            bssid: [0; 6],
            has_handshake: true,
            uploaded: false,
            size: 1024,
        });
        cm.register(CaptureFile {
            path: PathBuf::from("/tmp/test2.pcapng"),
            ssid: "Net2".into(),
            bssid: [0; 6],
            has_handshake: false,
            uploaded: false,
            size: 512,
        });
        assert_eq!(cm.count(), 2);
        assert_eq!(cm.handshake_count(), 1);
        assert_eq!(cm.pending_upload_count(), 1);
        assert_eq!(cm.total_size(), 1536);
    }

    #[test]
    fn test_mark_uploaded() {
        let mut cm = CaptureManager::new("/tmp/captures");
        let path = PathBuf::from("/tmp/test.pcapng");
        cm.register(CaptureFile {
            path: path.clone(),
            ssid: "Net".into(),
            bssid: [0; 6],
            has_handshake: true,
            uploaded: false,
            size: 100,
        });
        assert_eq!(cm.pending_upload_count(), 1);
        cm.mark_uploaded(&path);
        assert_eq!(cm.pending_upload_count(), 0);
    }

    #[test]
    fn test_upload_queue_fifo() {
        let mut q = UploadQueue::new();
        q.enqueue(PathBuf::from("a.pcapng"));
        q.enqueue(PathBuf::from("b.pcapng"));
        assert_eq!(q.pending(), 2);

        assert_eq!(q.next().unwrap(), PathBuf::from("a.pcapng"));
        assert_eq!(q.next().unwrap(), PathBuf::from("b.pcapng"));
        assert!(q.next().is_none());
    }

    #[test]
    fn test_upload_queue_dedup() {
        let mut q = UploadQueue::new();
        q.enqueue(PathBuf::from("a.pcapng"));
        q.enqueue(PathBuf::from("a.pcapng")); // duplicate
        assert_eq!(q.pending(), 1);
    }

    #[test]
    fn test_upload_queue_retry_on_failure() {
        let mut q = UploadQueue::new();
        q.enqueue(PathBuf::from("a.pcapng"));
        let path = q.next().unwrap();
        q.record_failure(path);
        assert_eq!(q.pending(), 1); // Re-queued
        assert_eq!(q.failures, 1);
    }

    #[test]
    fn test_cleanup_respects_limit() {
        let mut cm = CaptureManager::new("/tmp/nonexistent");
        cm.max_files = 2;
        for i in 0..5 {
            cm.register(CaptureFile {
                path: PathBuf::from(format!("/tmp/nonexistent/{i}.pcapng")),
                ssid: format!("Net{i}"),
                bssid: [0; 6],
                has_handshake: false,
                uploaded: false,
                size: 100,
            });
        }
        let removed = cm.cleanup();
        assert_eq!(removed, 3);
        assert_eq!(cm.count(), 2);
    }

    // ---- WPA-SEC config tests ----

    #[test]
    fn test_wpasec_config_default() {
        let cfg = WpaSecConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.api_key.is_empty());
        assert!(cfg.url.contains("wpa-sec"));
    }

    #[test]
    fn test_upload_to_wpasec_not_implemented() {
        let cfg = WpaSecConfig::default();
        let result = upload_to_wpasec(Path::new("/tmp/test.pcapng"), &cfg);
        assert!(result.is_err());
    }

    // ---- Cracked password tests ----

    #[test]
    fn test_cracked_password_store() {
        let mut store = CrackedPasswordStore::new();
        assert!(store.latest().is_none());
        assert!(store.display_str().is_empty());

        store.add("TestNet", "password123", [0; 6]);
        assert_eq!(store.latest().unwrap().ssid, "TestNet");
        assert_eq!(store.display_str(), "TestNet: pass****");
    }

    #[test]
    fn test_cracked_password_short() {
        let mut store = CrackedPasswordStore::new();
        store.add("Net", "abc", [0; 6]);
        // Short password shown as-is
        assert_eq!(store.display_str(), "Net: abc");
    }

    // ---- Auto-backup tests ----

    #[test]
    fn test_auto_backup_config_default() {
        let cfg = AutoBackupConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.interval_secs, 3600);
    }

    #[test]
    fn test_auto_backup_not_implemented() {
        let cfg = AutoBackupConfig::default();
        let result = auto_backup(Path::new("/tmp"), Path::new("/tmp/cfg"), &cfg);
        assert!(result.is_err());
    }

    // ---- Edge case tests ----

    #[test]
    fn test_capture_manager_zero_captures() {
        let cm = CaptureManager::new("/nonexistent");
        assert_eq!(cm.count(), 0);
        assert_eq!(cm.handshake_count(), 0);
        assert_eq!(cm.pending_upload_count(), 0);
        assert_eq!(cm.total_size(), 0);
    }

    #[test]
    fn test_capture_manager_many_captures() {
        let mut cm = CaptureManager::new("/tmp/captures");
        for i in 0..1000 {
            cm.register(CaptureFile {
                path: PathBuf::from(format!("/tmp/captures/{i}.pcapng")),
                ssid: format!("Net{i}"),
                bssid: [0, 0, 0, 0, (i >> 8) as u8, (i & 0xFF) as u8],
                has_handshake: i % 3 == 0,
                uploaded: i % 6 == 0,
                size: 1024,
            });
        }
        assert_eq!(cm.count(), 1000);
        // Every 3rd file has a handshake (i % 3 == 0): 0,3,6,...,999 => 334 files
        assert_eq!(cm.handshake_count(), 334);
        // Pending = has_handshake && !uploaded: i%3==0 && i%6!=0 => i%3==0 && i%6!=0
        // i%6==0 implies i%3==0, so pending = (i%3==0) - (i%6==0) = 334 - 167 = 167
        assert_eq!(cm.pending_upload_count(), 167);
        assert_eq!(cm.total_size(), 1000 * 1024);
    }

    #[test]
    fn test_cleanup_unlimited() {
        let mut cm = CaptureManager::new("/tmp/nonexistent");
        cm.max_files = 0; // unlimited
        for i in 0..10 {
            cm.register(CaptureFile {
                path: PathBuf::from(format!("/tmp/nonexistent/{i}.pcapng")),
                ssid: String::new(),
                bssid: [0; 6],
                has_handshake: false,
                uploaded: false,
                size: 100,
            });
        }
        let removed = cm.cleanup();
        assert_eq!(removed, 0);
        assert_eq!(cm.count(), 10);
    }

    #[test]
    fn test_cleanup_at_limit() {
        let mut cm = CaptureManager::new("/tmp/nonexistent");
        cm.max_files = 5;
        for i in 0..5 {
            cm.register(CaptureFile {
                path: PathBuf::from(format!("/tmp/nonexistent/{i}.pcapng")),
                ssid: String::new(),
                bssid: [0; 6],
                has_handshake: false,
                uploaded: false,
                size: 100,
            });
        }
        let removed = cm.cleanup();
        assert_eq!(removed, 0); // exactly at limit, not over
    }

    #[test]
    fn test_mark_uploaded_nonexistent_path() {
        let mut cm = CaptureManager::new("/tmp/captures");
        cm.register(CaptureFile {
            path: PathBuf::from("/tmp/captures/a.pcapng"),
            ssid: "Net".into(),
            bssid: [0; 6],
            has_handshake: true,
            uploaded: false,
            size: 100,
        });
        // Marking a non-existent path does nothing
        cm.mark_uploaded(Path::new("/tmp/captures/nonexistent.pcapng"));
        assert_eq!(cm.pending_upload_count(), 1);
    }
}
