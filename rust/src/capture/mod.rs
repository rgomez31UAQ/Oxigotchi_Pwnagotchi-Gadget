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

    pub fn pending(&self) -> usize {
        self.queue.len()
    }
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
}
