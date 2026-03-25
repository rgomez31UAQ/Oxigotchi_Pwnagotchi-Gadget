//! Capture file management: scanning, hashcat conversion, WPA-SEC upload,
//! dictionary cracking, cleanup/rotation, and auto-backup.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Parse BSSID and SSID from a .22000 companion file.
/// Format: WPA*02*hash*AP_BSSID*CLIENT*SSID_HEX*...
/// Returns ([bssid; 6], ssid_string). Falls back to zeroed BSSID and empty SSID.
fn parse_22000_metadata(pcapng_path: &Path) -> ([u8; 6], String) {
    let companion = pcapng_path.with_extension("22000");
    let content = match std::fs::read_to_string(&companion) {
        Ok(c) => c,
        Err(_) => return ([0; 6], String::new()),
    };
    // Take the first line
    let line = match content.lines().next() {
        Some(l) => l,
        None => return ([0; 6], String::new()),
    };
    let fields: Vec<&str> = line.split('*').collect();
    if fields.len() < 6 {
        return ([0; 6], String::new());
    }
    // Field 3 = AP BSSID (12 hex chars, no colons)
    let bssid = {
        let hex = fields[3];
        if hex.len() == 12 {
            let mut b = [0u8; 6];
            for i in 0..6 {
                b[i] = u8::from_str_radix(&hex[i*2..i*2+2], 16).unwrap_or(0);
            }
            b
        } else {
            [0; 6]
        }
    };
    // Field 5 = SSID (hex-encoded)
    let ssid = {
        let hex = fields[5];
        let bytes: Vec<u8> = (0..hex.len()/2)
            .filter_map(|i| u8::from_str_radix(&hex[i*2..i*2+2], 16).ok())
            .collect();
        String::from_utf8(bytes).unwrap_or_default()
    };
    (bssid, ssid)
}

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
    /// Last modification time (for change detection).
    pub mtime: Option<SystemTime>,
    /// Whether a .22000 companion file exists (verified conversion).
    pub converted: bool,
}

/// Manages capture files on disk.
pub struct CaptureManager {
    /// Base directory for captures.
    pub capture_dir: PathBuf,
    /// Known capture files.
    pub files: Vec<CaptureFile>,
    /// Maximum number of capture files to keep (0 = unlimited).
    pub max_files: usize,
    /// Tracks path -> mtime for change detection.
    tracked_mtimes: HashMap<PathBuf, SystemTime>,
}

impl CaptureManager {
    /// Create a new capture manager rooted at the given directory.
    pub fn new(capture_dir: &str) -> Self {
        Self {
            capture_dir: PathBuf::from(capture_dir),
            files: Vec::new(),
            max_files: 0,
            tracked_mtimes: HashMap::new(),
        }
    }

    /// Create a new capture manager with a file limit.
    pub fn with_max_files(capture_dir: &str, max_files: usize) -> Self {
        Self {
            capture_dir: PathBuf::from(capture_dir),
            files: Vec::new(),
            max_files,
            tracked_mtimes: HashMap::new(),
        }
    }

    /// Generate a capture filename: "{hostname}-YYYY-MM-DD_HH-MM-SS.pcapng"
    pub fn generate_filename(&self, hostname: &str, bssid: &[u8; 6]) -> String {
        let bssid_str: String = bssid
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join("");
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        format!("{hostname}_{bssid_str}_{timestamp}.pcapng")
    }

    /// Generate a hostname-prefixed timestamp filename (no BSSID).
    pub fn generate_timestamp_filename(hostname: &str) -> String {
        let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        format!("{hostname}-{timestamp}.pcapng")
    }

    /// Get the full path for a new capture file.
    pub fn capture_path(&self, filename: &str) -> PathBuf {
        self.capture_dir.join(filename)
    }

    /// Register a capture file.
    pub fn register(&mut self, file: CaptureFile) {
        if let Some(mtime) = file.mtime {
            self.tracked_mtimes.insert(file.path.clone(), mtime);
        }
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

    /// Count verified captures (those with .22000 companion files).
    pub fn verified_count(&self) -> usize {
        self.files.iter().filter(|f| f.converted).count()
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

    /// Mark a capture as converted (has .22000 companion).
    pub fn mark_converted(&mut self, path: &Path) {
        if let Some(f) = self.files.iter_mut().find(|f| f.path == path) {
            f.converted = true;
            f.has_handshake = true;
        }
    }

    /// Check if a .22000 companion file exists for a given pcapng path.
    pub fn has_22000_companion(pcapng_path: &Path) -> bool {
        let companion = pcapng_path.with_extension("22000");
        companion.exists()
    }

    /// Scan the capture directory for existing pcapng files.
    /// Detects new files, modified files, and checks for .22000 companions.
    /// Returns (new_count, modified_count).
    pub fn scan_directory(&mut self) -> Result<usize, std::io::Error> {
        let dir = self.capture_dir.clone();
        if !dir.exists() {
            return Ok(0);
        }
        let mut count = 0;
        let mut seen_paths: Vec<PathBuf> = Vec::new();

        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "pcapng") {
                let meta = entry.metadata()?;
                let mtime = meta.modified().ok();
                let converted = Self::has_22000_companion(&path);

                seen_paths.push(path.clone());

                // Check if already tracked
                let already_tracked = self.files.iter().any(|f| f.path == path);
                if already_tracked {
                    // Check for modification
                    if let Some(new_mtime) = mtime {
                        let changed = self
                            .tracked_mtimes
                            .get(&path)
                            .is_none_or(|old| *old != new_mtime);
                        if changed {
                            // Update the existing entry
                            if let Some(f) = self.files.iter_mut().find(|f| f.path == path) {
                                f.size = meta.len();
                                f.mtime = Some(new_mtime);
                                f.converted = converted;
                                if converted {
                                    f.has_handshake = true;
                                }
                            }
                            self.tracked_mtimes.insert(path, new_mtime);
                        }
                    }
                } else {
                    // New file — extract BSSID from .22000 companion if it exists
                    let (bssid, ssid) = parse_22000_metadata(&path);
                    self.files.push(CaptureFile {
                        path: path.clone(),
                        ssid,
                        bssid,
                        has_handshake: converted,
                        uploaded: false,
                        size: meta.len(),
                        mtime,
                        converted,
                    });
                    if let Some(mt) = mtime {
                        self.tracked_mtimes.insert(path, mt);
                    }
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Remove oldest captures if over the file limit. Returns number removed.
    /// Never deletes verified (.22000 companion / converted) captures.
    pub fn cleanup(&mut self) -> usize {
        if self.max_files == 0 || self.files.len() <= self.max_files {
            return 0;
        }
        let to_remove = self.files.len() - self.max_files;

        // Collect indices of non-verified files, oldest first
        let removable_indices: Vec<usize> = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.converted)
            .map(|(i, _)| i)
            .take(to_remove)
            .collect();

        // Remove in reverse order to preserve indices
        let mut removed_count = 0;
        for &idx in removable_indices.iter().rev() {
            let f = self.files.remove(idx);
            let _ = std::fs::remove_file(&f.path);
            self.tracked_mtimes.remove(&f.path);
            removed_count += 1;
        }
        removed_count
    }

    /// Get a list of pcapng files that haven't been converted yet.
    pub fn unconverted_files(&self) -> Vec<&CaptureFile> {
        self.files.iter().filter(|f| !f.converted).collect()
    }

    /// Get a list of converted files that haven't been uploaded yet.
    pub fn uploadable_files(&self) -> Vec<&CaptureFile> {
        self.files
            .iter()
            .filter(|f| f.converted && !f.uploaded)
            .collect()
    }

    /// Find a capture file by path.
    pub fn find_by_path(&self, path: &Path) -> Option<&CaptureFile> {
        self.files.iter().find(|f| f.path == path)
    }
}

// ---------------------------------------------------------------------------
// Upload queue for WPA-SEC integration
// ---------------------------------------------------------------------------

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
// Hashcat conversion: pcapng -> .22000 via hcxpcapngtool
// ---------------------------------------------------------------------------

/// Result of a hashcat conversion attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum ConversionResult {
    /// Successfully converted, output file at given path.
    Success(PathBuf),
    /// hcxpcapngtool is not installed.
    ToolNotFound,
    /// Conversion ran but produced no output (no handshakes in capture).
    NoHandshakes,
    /// Conversion failed with an error message.
    Failed(String),
}

/// Check if hcxpcapngtool is available on the system.
#[cfg(unix)]
pub fn hcxpcapngtool_available() -> bool {
    std::process::Command::new("which")
        .arg("hcxpcapngtool")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(not(unix))]
pub fn hcxpcapngtool_available() -> bool {
    false
}

/// Convert a .pcapng file to .22000 format using hcxpcapngtool.
/// Output file is placed alongside the input with a .22000 extension.
#[cfg(unix)]
pub fn convert_to_22000(pcapng_path: &Path) -> ConversionResult {
    if !hcxpcapngtool_available() {
        return ConversionResult::ToolNotFound;
    }

    let output_path = pcapng_path.with_extension("22000");

    let result = std::process::Command::new("hcxpcapngtool")
        .arg("-o")
        .arg(&output_path)
        .arg(pcapng_path)
        .output();

    match result {
        Ok(output) => {
            if output.status.success() && output_path.exists() {
                // Check the output file is non-empty
                match std::fs::metadata(&output_path) {
                    Ok(meta) if meta.len() > 0 => ConversionResult::Success(output_path),
                    _ => {
                        let _ = std::fs::remove_file(&output_path);
                        ConversionResult::NoHandshakes
                    }
                }
            } else if output_path.exists() {
                // Tool succeeded but output may be empty
                match std::fs::metadata(&output_path) {
                    Ok(meta) if meta.len() > 0 => ConversionResult::Success(output_path),
                    _ => {
                        let _ = std::fs::remove_file(&output_path);
                        ConversionResult::NoHandshakes
                    }
                }
            } else {
                ConversionResult::NoHandshakes
            }
        }
        Err(e) => ConversionResult::Failed(format!("hcxpcapngtool exec error: {e}")),
    }
}

#[cfg(not(unix))]
pub fn convert_to_22000(_pcapng_path: &Path) -> ConversionResult {
    ConversionResult::ToolNotFound
}

/// Batch-convert all unconverted pcapng files. Returns (success, no_handshakes, failed).
pub fn batch_convert(manager: &mut CaptureManager) -> (usize, usize, usize) {
    let unconverted: Vec<PathBuf> = manager
        .unconverted_files()
        .iter()
        .map(|f| f.path.clone())
        .collect();

    let mut success = 0;
    let mut no_hs = 0;
    let mut failed = 0;

    for path in unconverted {
        match convert_to_22000(&path) {
            ConversionResult::Success(_) => {
                manager.mark_converted(&path);
                success += 1;
            }
            ConversionResult::NoHandshakes => {
                no_hs += 1;
            }
            ConversionResult::ToolNotFound => {
                // Stop trying if tool is not found
                return (success, no_hs, failed);
            }
            ConversionResult::Failed(_) => {
                failed += 1;
            }
        }
    }
    (success, no_hs, failed)
}

// ---------------------------------------------------------------------------
// WPA-SEC upload integration
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
    /// Maximum retry attempts per file.
    pub max_retries: u32,
}

impl Default for WpaSecConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            url: "https://wpa-sec.stanev.org".into(),
            enabled: false,
            max_retries: 3,
        }
    }
}

/// Result of a WPA-SEC upload attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum UploadResult {
    /// Upload succeeded.
    Success,
    /// API key not configured.
    NoApiKey,
    /// Uploads disabled in config.
    Disabled,
    /// curl is not available.
    CurlNotFound,
    /// Upload failed with error.
    Failed(String),
}

/// Check if curl is available on the system.
#[cfg(unix)]
pub fn curl_available() -> bool {
    std::process::Command::new("which")
        .arg("curl")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(not(unix))]
pub fn curl_available() -> bool {
    false
}

/// Upload a .22000 file to wpa-sec.stanev.org using curl.
///
/// POST to {url}/?submit with file field "file" and cookie "key={api_key}".
#[cfg(unix)]
pub fn upload_to_wpasec(path: &Path, config: &WpaSecConfig) -> Result<(), String> {
    if !config.enabled {
        return Err("WPA-SEC uploads disabled".into());
    }
    if config.api_key.is_empty() {
        return Err("WPA-SEC API key not configured".into());
    }
    if !curl_available() {
        return Err("curl not found".into());
    }
    if !path.exists() {
        return Err(format!("file not found: {}", path.display()));
    }

    let url = format!("{}/?submit", config.url);

    // Retry with exponential backoff
    let mut last_err = String::new();
    for attempt in 0..config.max_retries {
        if attempt > 0 {
            let delay = std::time::Duration::from_secs(1 << attempt);
            std::thread::sleep(delay);
        }

        let result = std::process::Command::new("curl")
            .arg("-s")
            .arg("-S")
            .arg("--fail")
            .arg("-b")
            .arg(format!("key={}", config.api_key))
            .arg("-F")
            .arg(format!("file=@{}", path.display()))
            .arg(&url)
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    return Ok(());
                }
                last_err = String::from_utf8_lossy(&output.stderr).to_string();
            }
            Err(e) => {
                last_err = format!("curl exec error: {e}");
            }
        }
    }
    Err(format!(
        "upload failed after {} attempts: {}",
        config.max_retries, last_err
    ))
}

/// Fetch cracked passwords from wpa-sec.stanev.org.
/// Returns lines in format: BSSID:SSID:PASSWORD
#[cfg(unix)]
pub fn fetch_cracked_from_wpasec(config: &WpaSecConfig) -> Vec<(String, String, String)> {
    if config.api_key.is_empty() || !config.enabled {
        return Vec::new();
    }
    let url = format!("{}/?api&dl=1", config.url);
    let result = std::process::Command::new("curl")
        .arg("-s")
        .arg("-b")
        .arg(format!("key={}", config.api_key))
        .arg(&url)
        .output();
    match result {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            text.lines()
                .filter_map(|line| {
                    let parts: Vec<&str> = line.splitn(3, ':').collect();
                    if parts.len() == 3 && !parts[2].is_empty() {
                        Some((parts[0].to_string(), parts[1].to_string(), parts[2].to_string()))
                    } else {
                        None
                    }
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(not(unix))]
pub fn fetch_cracked_from_wpasec(_config: &WpaSecConfig) -> Vec<(String, String, String)> {
    Vec::new()
}

#[cfg(not(unix))]
pub fn upload_to_wpasec(_path: &Path, _config: &WpaSecConfig) -> Result<(), String> {
    Err("WPA-SEC upload requires Unix (curl)".into())
}

/// Upload all pending .22000 files from the capture manager.
/// Returns (uploaded_count, failed_count).
pub fn upload_all_pending(
    manager: &mut CaptureManager,
    config: &WpaSecConfig,
    queue: &mut UploadQueue,
) -> (usize, usize) {
    if !config.enabled || config.api_key.is_empty() {
        return (0, 0);
    }

    // Enqueue any uploadable files not already in queue
    let uploadable: Vec<PathBuf> = manager
        .uploadable_files()
        .iter()
        .map(|f| f.path.with_extension("22000"))
        .collect();
    for p in uploadable {
        queue.enqueue(p);
    }

    let mut uploaded = 0;
    let mut failed = 0;

    while let Some(path) = queue.next() {
        match upload_to_wpasec(&path, config) {
            Ok(()) => {
                queue.record_success();
                // Mark the original pcapng as uploaded
                let pcapng_path = path.with_extension("pcapng");
                manager.mark_uploaded(&pcapng_path);
                uploaded += 1;
            }
            Err(_) => {
                queue.record_failure(path);
                failed += 1;
                // Stop after first failure to avoid hammering the server
                break;
            }
        }
    }
    (uploaded, failed)
}

// ---------------------------------------------------------------------------
// Quick dictionary attack via aircrack-ng
// ---------------------------------------------------------------------------

/// Check if aircrack-ng is available on the system.
#[cfg(unix)]
pub fn aircrack_available() -> bool {
    std::process::Command::new("which")
        .arg("aircrack-ng")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(not(unix))]
pub fn aircrack_available() -> bool {
    false
}

/// Result of a dictionary attack attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum DictAttackResult {
    /// Password found.
    Cracked(String),
    /// No password found in wordlist.
    NotFound,
    /// aircrack-ng not installed.
    ToolNotFound,
    /// Attack failed with an error.
    Failed(String),
}

/// Run a quick offline dictionary attack on a captured handshake using aircrack-ng.
///
/// Parses aircrack-ng output looking for "KEY FOUND! [ ... ]".
#[cfg(unix)]
pub fn quick_dictionary_attack(
    pcapng_path: &Path,
    wordlist_path: &Path,
) -> Result<Option<String>, String> {
    if !aircrack_available() {
        return Err("aircrack-ng not found".into());
    }
    if !pcapng_path.exists() {
        return Err(format!("capture file not found: {}", pcapng_path.display()));
    }
    if !wordlist_path.exists() {
        return Err(format!("wordlist not found: {}", wordlist_path.display()));
    }

    let result = std::process::Command::new("aircrack-ng")
        .arg("-w")
        .arg(wordlist_path)
        .arg("-q") // quiet mode
        .arg("-l")
        .arg("/dev/stdout") // write key to stdout
        .arg(pcapng_path)
        .output();

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse output for cracked password
            if let Some(password) = parse_aircrack_output(&stdout) {
                Ok(Some(password))
            } else {
                Ok(None)
            }
        }
        Err(e) => Err(format!("aircrack-ng exec error: {e}")),
    }
}

#[cfg(not(unix))]
pub fn quick_dictionary_attack(
    _pcapng_path: &Path,
    _wordlist_path: &Path,
) -> Result<Option<String>, String> {
    Err("dictionary attack requires Unix (aircrack-ng)".into())
}

/// Parse aircrack-ng output for a cracked password.
/// Looks for "KEY FOUND! [ password ]" pattern.
pub fn parse_aircrack_output(output: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        // Pattern: "KEY FOUND! [ thepassword ]"
        if let Some(rest) = trimmed.strip_prefix("KEY FOUND!") {
            let rest = rest.trim();
            if let Some(inner) = rest.strip_prefix('[') {
                if let Some(password) = inner.strip_suffix(']') {
                    let password = password.trim();
                    if !password.is_empty() {
                        return Some(password.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Run dictionary attacks on all unconverted/uncracked captures.
/// Returns the list of newly cracked (path, password) pairs.
pub fn attack_all_captures(
    manager: &CaptureManager,
    wordlist_path: &Path,
    store: &mut CrackedPasswordStore,
) -> Vec<(PathBuf, String)> {
    let mut cracked = Vec::new();
    let already_cracked: Vec<String> = store
        .passwords
        .iter()
        .map(|p| p.ssid.clone())
        .collect();

    for file in &manager.files {
        // Skip files already cracked (by SSID match) or already uploaded
        if !file.ssid.is_empty() && already_cracked.contains(&file.ssid) {
            continue;
        }

        match quick_dictionary_attack(&file.path, wordlist_path) {
            Ok(Some(password)) => {
                store.add(&file.ssid, &password, file.bssid);
                cracked.push((file.path.clone(), password));
            }
            Ok(None) => {} // not found
            Err(_) => {}   // tool not available or error
        }
    }
    cracked
}

// ---------------------------------------------------------------------------
// Cracked password display
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
        // Don't add duplicates
        if self
            .passwords
            .iter()
            .any(|p| p.ssid == ssid && p.password == password)
        {
            return;
        }
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

    /// Total number of cracked passwords.
    pub fn count(&self) -> usize {
        self.passwords.len()
    }

    /// Save cracked passwords to a file (one per line: SSID:password).
    pub fn save_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
        let mut content = String::new();
        for p in &self.passwords {
            content.push_str(&format!("{}:{}\n", p.ssid, p.password));
        }
        std::fs::write(path, content)
    }

    /// Load cracked passwords from a file (one per line: SSID:password).
    pub fn load_from_file(&mut self, path: &Path) -> Result<usize, std::io::Error> {
        if !path.exists() {
            return Ok(0);
        }
        let content = std::fs::read_to_string(path)?;
        let mut count = 0;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((ssid, password)) = line.split_once(':') {
                self.add(ssid, password, [0; 6]);
                count += 1;
            }
        }
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Handshake download via web
// ---------------------------------------------------------------------------

/// List capture files available for download via the web dashboard.
pub fn list_downloadable_captures(capture_dir: &Path) -> Result<Vec<PathBuf>, String> {
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
// Auto-backup
// ---------------------------------------------------------------------------

/// Auto-backup configuration.
#[derive(Debug, Clone)]
pub struct AutoBackupConfig {
    /// Whether auto-backup is enabled.
    pub enabled: bool,
    /// Backup interval in seconds.
    pub interval_secs: u64,
    /// Backup destination path.
    pub dest_dir: PathBuf,
    /// Last backup time.
    pub last_backup: Option<std::time::Instant>,
}

impl Default for AutoBackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 3600,
            dest_dir: PathBuf::from("/home/pi/backups"),
            last_backup: None,
        }
    }
}

impl AutoBackupConfig {
    /// Check if a backup is due based on interval.
    pub fn is_due(&self) -> bool {
        if !self.enabled {
            return false;
        }
        match self.last_backup {
            None => true,
            Some(last) => last.elapsed().as_secs() >= self.interval_secs,
        }
    }

    /// Record that a backup was just performed.
    pub fn record_backup(&mut self) {
        self.last_backup = Some(std::time::Instant::now());
    }
}

/// Perform a backup of captures to the backup directory using tar.
///
/// Creates a tarball at dest_dir/captures_YYYYMMDD_HHMMSS.tar.gz.
#[cfg(unix)]
pub fn auto_backup(
    capture_dir: &Path,
    _config_path: &Path,
    backup_config: &AutoBackupConfig,
) -> Result<usize, String> {
    if !backup_config.enabled {
        return Err("auto-backup disabled".into());
    }
    if !capture_dir.exists() {
        return Err(format!(
            "capture dir does not exist: {}",
            capture_dir.display()
        ));
    }

    // Ensure backup destination exists
    std::fs::create_dir_all(&backup_config.dest_dir)
        .map_err(|e| format!("failed to create backup dir: {e}"))?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let tarball = backup_config
        .dest_dir
        .join(format!("captures_{timestamp}.tar.gz"));

    let result = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&tarball)
        .arg("-C")
        .arg(
            capture_dir
                .parent()
                .unwrap_or(Path::new("/")),
        )
        .arg(
            capture_dir
                .file_name()
                .unwrap_or(std::ffi::OsStr::new("captures")),
        )
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                // Count files that were backed up
                let count = std::fs::read_dir(capture_dir)
                    .map(|entries| {
                        entries
                            .filter_map(|e| e.ok())
                            .filter(|e| {
                                e.path()
                                    .extension()
                                    .is_some_and(|ext| ext == "pcapng" || ext == "22000")
                            })
                            .count()
                    })
                    .unwrap_or(0);
                Ok(count)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("tar failed: {stderr}"))
            }
        }
        Err(e) => Err(format!("tar exec error: {e}")),
    }
}

#[cfg(not(unix))]
pub fn auto_backup(
    _capture_dir: &Path,
    _config_path: &Path,
    _backup_config: &AutoBackupConfig,
) -> Result<usize, String> {
    Err("auto-backup requires Unix (tar)".into())
}

// ---------------------------------------------------------------------------
// tmpfs capture pipeline: validate in RAM, move handshakes to SD
// ---------------------------------------------------------------------------

/// Move validated captures (.pcapng + .22000) from tmpfs to permanent storage.
/// Delete junk (pcapng files that produced no .22000 and are older than 60s) from tmpfs.
/// Returns (moved_count, deleted_count).
#[cfg(unix)]
pub fn move_validated_captures(
    tmpfs_dir: &Path,
    permanent_dir: &Path,
    manager: &mut CaptureManager,
) -> (usize, usize) {
    use std::fs;
    let mut moved = 0;
    let mut deleted = 0;

    // Ensure permanent dir exists
    let _ = fs::create_dir_all(permanent_dir);

    let entries: Vec<_> = match fs::read_dir(tmpfs_dir) {
        Ok(e) => e.filter_map(|e| e.ok()).collect(),
        Err(_) => return (0, 0),
    };

    for entry in &entries {
        let path = entry.path();
        if path.extension().map(|e| e == "pcapng").unwrap_or(false) {
            let companion = path.with_extension("22000");
            if companion.exists() && companion.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                // Valid handshake — move both files to permanent storage
                let pcapng_dest = permanent_dir.join(path.file_name().unwrap());
                let companion_dest = permanent_dir.join(companion.file_name().unwrap());
                if fs::copy(&path, &pcapng_dest).is_ok() {
                    let _ = fs::copy(&companion, &companion_dest);
                    let _ = fs::remove_file(&path);
                    let _ = fs::remove_file(&companion);
                    moved += 1;
                    log::info!("capture: moved validated {} to SD",
                        path.file_name().unwrap().to_string_lossy());
                }
            } else {
                // Check if conversion was attempted (file is old enough).
                // Only delete if file hasn't been modified in last 60 seconds
                // (give batch_convert time to process it first).
                if let Ok(meta) = path.metadata() {
                    let age = meta.modified().ok()
                        .and_then(|m| m.elapsed().ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    if age > 60 {
                        let _ = fs::remove_file(&path);
                        deleted += 1;
                    }
                }
            }
        }
    }

    // Also clean up any orphaned .22000 files in tmpfs
    for entry in &entries {
        let path = entry.path();
        if path.extension().map(|e| e == "22000").unwrap_or(false) {
            if !path.with_extension("pcapng").exists() {
                let _ = fs::remove_file(&path);
            }
        }
    }

    // Re-scan permanent dir to pick up newly moved files
    if moved > 0 {
        let _ = manager.scan_directory();
    }

    (moved, deleted)
}

#[cfg(not(unix))]
pub fn move_validated_captures(
    _tmpfs_dir: &Path,
    _permanent_dir: &Path,
    _manager: &mut CaptureManager,
) -> (usize, usize) {
    (0, 0)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper: create a temp directory with some fake pcapng files.
    fn make_temp_captures(prefix: &str, count: usize) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxigotchi_test_{prefix}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for i in 0..count {
            let name = format!("capture_{i}.pcapng");
            fs::write(dir.join(&name), format!("fake pcapng data {i}")).unwrap();
        }
        dir
    }

    /// Helper: clean up temp directory.
    fn cleanup_temp(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    // ---- Generate filename tests ----

    #[test]
    fn test_generate_filename() {
        let cm = CaptureManager::new("/tmp/captures");
        let bssid = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let name = cm.generate_filename("oxigotchi", &bssid);
        assert!(name.starts_with("oxigotchi_aabbccddeeff_"));
        assert!(name.ends_with(".pcapng"));
    }

    #[test]
    fn test_generate_timestamp_filename() {
        let name = CaptureManager::generate_timestamp_filename("oxigotchi");
        assert!(name.starts_with("oxigotchi-"));
        assert!(name.ends_with(".pcapng"));
        // Format: oxigotchi-YYYY-MM-DD_HH-MM-SS.pcapng
        assert!(name.len() > 30, "filename too short: {name}");
    }

    #[test]
    fn test_capture_path() {
        let cm = CaptureManager::new("/tmp/captures");
        let path = cm.capture_path("test.pcapng");
        assert_eq!(path, PathBuf::from("/tmp/captures/test.pcapng"));
    }

    // ---- Register and count tests ----

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
            mtime: None,
            converted: false,
        });
        cm.register(CaptureFile {
            path: PathBuf::from("/tmp/test2.pcapng"),
            ssid: "Net2".into(),
            bssid: [0; 6],
            has_handshake: false,
            uploaded: false,
            size: 512,
            mtime: None,
            converted: false,
        });
        assert_eq!(cm.count(), 2);
        assert_eq!(cm.handshake_count(), 1);
        assert_eq!(cm.pending_upload_count(), 1);
        assert_eq!(cm.total_size(), 1536);
    }

    #[test]
    fn test_verified_count() {
        let mut cm = CaptureManager::new("/tmp/captures");
        cm.register(CaptureFile {
            path: PathBuf::from("/tmp/a.pcapng"),
            ssid: "A".into(),
            bssid: [0; 6],
            has_handshake: true,
            uploaded: false,
            size: 100,
            mtime: None,
            converted: true,
        });
        cm.register(CaptureFile {
            path: PathBuf::from("/tmp/b.pcapng"),
            ssid: "B".into(),
            bssid: [0; 6],
            has_handshake: false,
            uploaded: false,
            size: 100,
            mtime: None,
            converted: false,
        });
        assert_eq!(cm.verified_count(), 1);
    }

    // ---- Mark uploaded / converted tests ----

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
            mtime: None,
            converted: true,
        });
        assert_eq!(cm.pending_upload_count(), 1);
        cm.mark_uploaded(&path);
        assert_eq!(cm.pending_upload_count(), 0);
    }

    #[test]
    fn test_mark_converted() {
        let mut cm = CaptureManager::new("/tmp/captures");
        let path = PathBuf::from("/tmp/test.pcapng");
        cm.register(CaptureFile {
            path: path.clone(),
            ssid: "Net".into(),
            bssid: [0; 6],
            has_handshake: false,
            uploaded: false,
            size: 100,
            mtime: None,
            converted: false,
        });
        assert!(!cm.files[0].converted);
        assert!(!cm.files[0].has_handshake);

        cm.mark_converted(&path);
        assert!(cm.files[0].converted);
        assert!(cm.files[0].has_handshake);
    }

    // ---- Directory scanning tests ----

    #[test]
    fn test_scan_directory_empty() {
        let dir = make_temp_captures("scan_empty", 0);
        let mut cm = CaptureManager::new(dir.to_str().unwrap());
        let count = cm.scan_directory().unwrap();
        assert_eq!(count, 0);
        assert_eq!(cm.count(), 0);
        cleanup_temp(&dir);
    }

    #[test]
    fn test_scan_directory_nonexistent() {
        let mut cm = CaptureManager::new("/nonexistent/path/captures");
        let count = cm.scan_directory().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_scan_directory_with_files() {
        let dir = make_temp_captures("scan_files", 3);
        let mut cm = CaptureManager::new(dir.to_str().unwrap());
        let count = cm.scan_directory().unwrap();
        assert_eq!(count, 3);
        assert_eq!(cm.count(), 3);

        // All files should have size > 0
        for f in &cm.files {
            assert!(f.size > 0);
            assert!(f.mtime.is_some());
        }
        cleanup_temp(&dir);
    }

    #[test]
    fn test_scan_directory_ignores_non_pcapng() {
        let dir = make_temp_captures("scan_ignore", 2);
        // Add a non-pcapng file
        fs::write(dir.join("notes.txt"), "not a capture").unwrap();
        fs::write(dir.join("data.json"), "{}").unwrap();

        let mut cm = CaptureManager::new(dir.to_str().unwrap());
        let count = cm.scan_directory().unwrap();
        assert_eq!(count, 2); // only .pcapng files
        cleanup_temp(&dir);
    }

    #[test]
    fn test_scan_directory_detects_22000_companion() {
        let dir = make_temp_captures("scan_companion", 2);
        // Create a .22000 companion for the first file
        fs::write(dir.join("capture_0.22000"), "WPA*02*hash*data").unwrap();

        let mut cm = CaptureManager::new(dir.to_str().unwrap());
        let count = cm.scan_directory().unwrap();
        assert_eq!(count, 2);

        // One file should be marked as converted
        let converted_count = cm.files.iter().filter(|f| f.converted).count();
        assert_eq!(converted_count, 1);

        // The converted file should also be marked as having a handshake
        let hs_count = cm.handshake_count();
        assert_eq!(hs_count, 1);
        cleanup_temp(&dir);
    }

    #[test]
    fn test_scan_directory_rescan_detects_new() {
        let dir = make_temp_captures("scan_rescan", 2);
        let mut cm = CaptureManager::new(dir.to_str().unwrap());

        let count1 = cm.scan_directory().unwrap();
        assert_eq!(count1, 2);

        // Add a new file
        fs::write(dir.join("capture_new.pcapng"), "new capture data").unwrap();

        let count2 = cm.scan_directory().unwrap();
        assert_eq!(count2, 1); // only the new file
        assert_eq!(cm.count(), 3);
        cleanup_temp(&dir);
    }

    #[test]
    fn test_scan_directory_rescan_no_duplicates() {
        let dir = make_temp_captures("scan_nodup", 2);
        let mut cm = CaptureManager::new(dir.to_str().unwrap());

        cm.scan_directory().unwrap();
        cm.scan_directory().unwrap(); // rescan

        assert_eq!(cm.count(), 2); // no duplicates
        cleanup_temp(&dir);
    }

    // ---- Cleanup / rotation tests ----

    #[test]
    fn test_cleanup_respects_limit() {
        let dir = make_temp_captures("cleanup_limit", 5);
        let mut cm = CaptureManager::new(dir.to_str().unwrap());
        cm.max_files = 2;
        cm.scan_directory().unwrap();
        assert_eq!(cm.count(), 5);

        let removed = cm.cleanup();
        assert_eq!(removed, 3);
        assert_eq!(cm.count(), 2);
        cleanup_temp(&dir);
    }

    #[test]
    fn test_cleanup_never_deletes_converted() {
        let dir = make_temp_captures("cleanup_converted", 5);
        // Make first 3 files "converted"
        for i in 0..3 {
            fs::write(dir.join(format!("capture_{i}.22000")), "hash data").unwrap();
        }

        let mut cm = CaptureManager::new(dir.to_str().unwrap());
        cm.max_files = 2;
        cm.scan_directory().unwrap();
        assert_eq!(cm.count(), 5);
        assert_eq!(cm.verified_count(), 3);

        let removed = cm.cleanup();
        // Should only remove 2 non-converted files (indices 3 and 4)
        assert_eq!(removed, 2);
        assert_eq!(cm.count(), 3);
        // All remaining should be converted
        assert_eq!(cm.verified_count(), 3);
        cleanup_temp(&dir);
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
                mtime: None,
                converted: false,
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
                mtime: None,
                converted: false,
            });
        }
        let removed = cm.cleanup();
        assert_eq!(removed, 0); // exactly at limit, not over
    }

    #[test]
    fn test_cleanup_deletes_files_from_disk() {
        let dir = make_temp_captures("cleanup_disk", 5);
        let mut cm = CaptureManager::new(dir.to_str().unwrap());
        cm.max_files = 2;
        cm.scan_directory().unwrap();

        cm.cleanup();

        // Only 2 pcapng files should remain on disk
        let remaining: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "pcapng"))
            .collect();
        assert_eq!(remaining.len(), 2);
        cleanup_temp(&dir);
    }

    #[test]
    fn test_with_max_files_constructor() {
        let cm = CaptureManager::with_max_files("/tmp/captures", 100);
        assert_eq!(cm.max_files, 100);
        assert_eq!(cm.count(), 0);
    }

    // ---- Upload queue tests ----

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
    fn test_upload_queue_success_counter() {
        let mut q = UploadQueue::new();
        q.record_success();
        q.record_success();
        assert_eq!(q.completed, 2);
        assert_eq!(q.failures, 0);
    }

    // ---- Mark uploaded with nonexistent path ----

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
            mtime: None,
            converted: true,
        });
        // Marking a non-existent path does nothing
        cm.mark_uploaded(Path::new("/tmp/captures/nonexistent.pcapng"));
        assert_eq!(cm.pending_upload_count(), 1);
    }

    // ---- WPA-SEC config tests ----

    #[test]
    fn test_wpasec_config_default() {
        let cfg = WpaSecConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.api_key.is_empty());
        assert!(cfg.url.contains("wpa-sec"));
        assert_eq!(cfg.max_retries, 3);
    }

    #[test]
    fn test_wpasec_upload_disabled() {
        let cfg = WpaSecConfig::default(); // disabled by default
        let result = upload_to_wpasec(Path::new("/tmp/test.22000"), &cfg);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // On non-unix it says "requires Unix", on unix it says "disabled"
        assert!(
            err.contains("disabled") || err.contains("Unix"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_wpasec_upload_no_api_key() {
        let cfg = WpaSecConfig {
            enabled: true,
            api_key: String::new(),
            ..Default::default()
        };
        let result = upload_to_wpasec(Path::new("/tmp/test.22000"), &cfg);
        assert!(result.is_err());
    }

    // ---- Hashcat conversion tests ----

    #[test]
    fn test_conversion_result_variants() {
        let success = ConversionResult::Success(PathBuf::from("/tmp/test.22000"));
        let not_found = ConversionResult::ToolNotFound;
        let no_hs = ConversionResult::NoHandshakes;
        let failed = ConversionResult::Failed("error".into());

        assert_ne!(success, not_found);
        assert_ne!(no_hs, failed);
    }

    #[test]
    fn test_convert_to_22000_no_tool() {
        // On non-unix or where hcxpcapngtool isn't installed, this returns ToolNotFound
        let result = convert_to_22000(Path::new("/tmp/nonexistent.pcapng"));
        assert!(
            result == ConversionResult::ToolNotFound
                || matches!(result, ConversionResult::Failed(_))
        );
    }

    #[test]
    fn test_has_22000_companion() {
        let dir = std::env::temp_dir().join(format!("oxigotchi_test_companion_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let pcapng = dir.join("test.pcapng");
        fs::write(&pcapng, "fake pcapng").unwrap();

        // No companion yet
        assert!(!CaptureManager::has_22000_companion(&pcapng));

        // Create companion
        fs::write(dir.join("test.22000"), "hash data").unwrap();
        assert!(CaptureManager::has_22000_companion(&pcapng));

        cleanup_temp(&dir);
    }

    #[test]
    fn test_batch_convert_empty() {
        let mut cm = CaptureManager::new("/tmp/nonexistent");
        let (s, n, f) = batch_convert(&mut cm);
        assert_eq!(s, 0);
        assert_eq!(n, 0);
        assert_eq!(f, 0);
    }

    #[test]
    fn test_unconverted_files() {
        let mut cm = CaptureManager::new("/tmp/captures");
        cm.register(CaptureFile {
            path: PathBuf::from("/tmp/a.pcapng"),
            ssid: "A".into(),
            bssid: [0; 6],
            has_handshake: false,
            uploaded: false,
            size: 100,
            mtime: None,
            converted: false,
        });
        cm.register(CaptureFile {
            path: PathBuf::from("/tmp/b.pcapng"),
            ssid: "B".into(),
            bssid: [0; 6],
            has_handshake: true,
            uploaded: false,
            size: 100,
            mtime: None,
            converted: true,
        });
        let unconverted = cm.unconverted_files();
        assert_eq!(unconverted.len(), 1);
        assert_eq!(unconverted[0].path, PathBuf::from("/tmp/a.pcapng"));
    }

    #[test]
    fn test_uploadable_files() {
        let mut cm = CaptureManager::new("/tmp/captures");
        // Converted but not uploaded
        cm.register(CaptureFile {
            path: PathBuf::from("/tmp/a.pcapng"),
            ssid: "A".into(),
            bssid: [0; 6],
            has_handshake: true,
            uploaded: false,
            size: 100,
            mtime: None,
            converted: true,
        });
        // Converted and already uploaded
        cm.register(CaptureFile {
            path: PathBuf::from("/tmp/b.pcapng"),
            ssid: "B".into(),
            bssid: [0; 6],
            has_handshake: true,
            uploaded: true,
            size: 100,
            mtime: None,
            converted: true,
        });
        // Not converted
        cm.register(CaptureFile {
            path: PathBuf::from("/tmp/c.pcapng"),
            ssid: "C".into(),
            bssid: [0; 6],
            has_handshake: false,
            uploaded: false,
            size: 100,
            mtime: None,
            converted: false,
        });
        let uploadable = cm.uploadable_files();
        assert_eq!(uploadable.len(), 1);
        assert_eq!(uploadable[0].path, PathBuf::from("/tmp/a.pcapng"));
    }

    // ---- Quick dictionary attack / aircrack parsing tests ----

    #[test]
    fn test_parse_aircrack_output_found() {
        let output = r#"
                              Aircrack-ng 1.7

      [00:00:01] 12345/100000 keys tested (8743.21 k/s)

      Time left: 10 seconds

                        KEY FOUND! [ password123 ]

      Master Key     : AA BB CC DD EE FF 00 11 22 33 44 55 66 77 88 99
                       AA BB CC DD EE FF 00 11 22 33 44 55 66 77 88 99
        "#;
        let result = parse_aircrack_output(output);
        assert_eq!(result, Some("password123".to_string()));
    }

    #[test]
    fn test_parse_aircrack_output_not_found() {
        let output = r#"
                              Aircrack-ng 1.7

      [00:00:10] 100000/100000 keys tested (10000.00 k/s)

      KEY NOT FOUND

        "#;
        let result = parse_aircrack_output(output);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_aircrack_output_empty() {
        let result = parse_aircrack_output("");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_aircrack_output_complex_password() {
        let output = "KEY FOUND! [ my p@ssw0rd! ]";
        let result = parse_aircrack_output(output);
        assert_eq!(result, Some("my p@ssw0rd!".to_string()));
    }

    #[test]
    fn test_parse_aircrack_output_spaces_in_password() {
        let output = "KEY FOUND! [ hello world ]";
        let result = parse_aircrack_output(output);
        assert_eq!(result, Some("hello world".to_string()));
    }

    #[test]
    fn test_dict_attack_result_variants() {
        let cracked = DictAttackResult::Cracked("pass".into());
        let not_found = DictAttackResult::NotFound;
        let no_tool = DictAttackResult::ToolNotFound;
        let failed = DictAttackResult::Failed("err".into());
        assert_ne!(cracked, not_found);
        assert_ne!(no_tool, failed);
    }

    #[test]
    fn test_quick_dictionary_attack_no_tool() {
        // On non-unix, this returns Err
        let result = quick_dictionary_attack(
            Path::new("/tmp/test.pcapng"),
            Path::new("/tmp/wordlist.txt"),
        );
        assert!(result.is_err());
    }

    // ---- Cracked password store tests ----

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

    #[test]
    fn test_cracked_password_exactly_4_chars() {
        let mut store = CrackedPasswordStore::new();
        store.add("Net", "abcd", [0; 6]);
        assert_eq!(store.display_str(), "Net: abcd");
    }

    #[test]
    fn test_cracked_password_5_chars() {
        let mut store = CrackedPasswordStore::new();
        store.add("Net", "abcde", [0; 6]);
        assert_eq!(store.display_str(), "Net: abcd****");
    }

    #[test]
    fn test_cracked_password_dedup() {
        let mut store = CrackedPasswordStore::new();
        store.add("Net", "pass", [0; 6]);
        store.add("Net", "pass", [0; 6]); // duplicate
        assert_eq!(store.count(), 1);
    }

    #[test]
    fn test_cracked_password_different_ssid() {
        let mut store = CrackedPasswordStore::new();
        store.add("Net1", "pass1", [0; 6]);
        store.add("Net2", "pass2", [0; 6]);
        assert_eq!(store.count(), 2);
        assert_eq!(store.latest().unwrap().ssid, "Net2");
    }

    #[test]
    fn test_cracked_password_save_load() {
        let dir = std::env::temp_dir().join(format!("oxigotchi_test_cracked_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cracked.txt");

        // Save
        let mut store = CrackedPasswordStore::new();
        store.add("HomeWifi", "mypassword", [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        store.add("OfficeNet", "secret123", [0; 6]);
        store.save_to_file(&path).unwrap();

        // Verify file contents
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("HomeWifi:mypassword"));
        assert!(content.contains("OfficeNet:secret123"));

        // Load into new store
        let mut store2 = CrackedPasswordStore::new();
        let count = store2.load_from_file(&path).unwrap();
        assert_eq!(count, 2);
        assert_eq!(store2.count(), 2);

        cleanup_temp(&dir);
    }

    #[test]
    fn test_cracked_password_load_nonexistent() {
        let mut store = CrackedPasswordStore::new();
        let count = store.load_from_file(Path::new("/nonexistent/cracked.txt")).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_cracked_password_load_empty_file() {
        let dir = std::env::temp_dir().join(format!("oxigotchi_test_crackedempty_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cracked.txt");
        fs::write(&path, "").unwrap();

        let mut store = CrackedPasswordStore::new();
        let count = store.load_from_file(&path).unwrap();
        assert_eq!(count, 0);
        assert_eq!(store.count(), 0);

        cleanup_temp(&dir);
    }

    #[test]
    fn test_cracked_password_load_with_blank_lines() {
        let dir = std::env::temp_dir().join(format!("oxigotchi_test_crackedblank_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cracked.txt");
        fs::write(&path, "\nNet1:pass1\n\nNet2:pass2\n\n").unwrap();

        let mut store = CrackedPasswordStore::new();
        let count = store.load_from_file(&path).unwrap();
        assert_eq!(count, 2);

        cleanup_temp(&dir);
    }

    // ---- Auto-backup config tests ----

    #[test]
    fn test_auto_backup_config_default() {
        let cfg = AutoBackupConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.interval_secs, 3600);
        assert!(cfg.last_backup.is_none());
    }

    #[test]
    fn test_auto_backup_is_due_disabled() {
        let cfg = AutoBackupConfig::default();
        assert!(!cfg.is_due()); // disabled
    }

    #[test]
    fn test_auto_backup_is_due_first_time() {
        let cfg = AutoBackupConfig {
            enabled: true,
            interval_secs: 3600,
            dest_dir: PathBuf::from("/tmp/backups"),
            last_backup: None,
        };
        assert!(cfg.is_due()); // never backed up
    }

    #[test]
    fn test_auto_backup_is_due_recent() {
        let cfg = AutoBackupConfig {
            enabled: true,
            interval_secs: 3600,
            dest_dir: PathBuf::from("/tmp/backups"),
            last_backup: Some(std::time::Instant::now()),
        };
        assert!(!cfg.is_due()); // just backed up
    }

    #[test]
    fn test_auto_backup_record() {
        let mut cfg = AutoBackupConfig {
            enabled: true,
            interval_secs: 3600,
            dest_dir: PathBuf::from("/tmp/backups"),
            last_backup: None,
        };
        assert!(cfg.is_due());
        cfg.record_backup();
        assert!(!cfg.is_due());
    }

    #[test]
    fn test_auto_backup_disabled() {
        let cfg = AutoBackupConfig::default();
        let result = auto_backup(Path::new("/tmp"), Path::new("/tmp/cfg"), &cfg);
        assert!(result.is_err());
    }

    // ---- Upload all pending tests ----

    #[test]
    fn test_upload_all_pending_disabled() {
        let mut cm = CaptureManager::new("/tmp/captures");
        let cfg = WpaSecConfig::default();
        let mut queue = UploadQueue::new();
        let (uploaded, failed) = upload_all_pending(&mut cm, &cfg, &mut queue);
        assert_eq!(uploaded, 0);
        assert_eq!(failed, 0);
    }

    #[test]
    fn test_upload_all_pending_no_api_key() {
        let mut cm = CaptureManager::new("/tmp/captures");
        let cfg = WpaSecConfig {
            enabled: true,
            api_key: String::new(),
            ..Default::default()
        };
        let mut queue = UploadQueue::new();
        let (uploaded, failed) = upload_all_pending(&mut cm, &cfg, &mut queue);
        assert_eq!(uploaded, 0);
        assert_eq!(failed, 0);
    }

    // ---- Find by path tests ----

    #[test]
    fn test_find_by_path() {
        let mut cm = CaptureManager::new("/tmp/captures");
        let path = PathBuf::from("/tmp/captures/a.pcapng");
        cm.register(CaptureFile {
            path: path.clone(),
            ssid: "TestNet".into(),
            bssid: [0; 6],
            has_handshake: true,
            uploaded: false,
            size: 100,
            mtime: None,
            converted: false,
        });

        let found = cm.find_by_path(&path);
        assert!(found.is_some());
        assert_eq!(found.unwrap().ssid, "TestNet");

        let not_found = cm.find_by_path(Path::new("/tmp/captures/b.pcapng"));
        assert!(not_found.is_none());
    }

    // ---- List downloadable captures tests ----

    #[test]
    fn test_list_downloadable_nonexistent() {
        let result = list_downloadable_captures(Path::new("/nonexistent/dir"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_list_downloadable_captures() {
        let dir = make_temp_captures("list_dl", 3);
        // Also add a .pcap file
        fs::write(dir.join("old.pcap"), "old format").unwrap();
        // And a non-capture file
        fs::write(dir.join("notes.txt"), "text").unwrap();

        let result = list_downloadable_captures(&dir).unwrap();
        assert_eq!(result.len(), 4); // 3 pcapng + 1 pcap

        cleanup_temp(&dir);
    }

    // ---- Edge case tests ----

    #[test]
    fn test_capture_manager_zero_captures() {
        let cm = CaptureManager::new("/nonexistent");
        assert_eq!(cm.count(), 0);
        assert_eq!(cm.handshake_count(), 0);
        assert_eq!(cm.pending_upload_count(), 0);
        assert_eq!(cm.verified_count(), 0);
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
                mtime: None,
                converted: i % 3 == 0,
            });
        }
        assert_eq!(cm.count(), 1000);
        // Every 3rd file has a handshake (i % 3 == 0): 0,3,6,...,999 => 334 files
        assert_eq!(cm.handshake_count(), 334);
        // Pending = has_handshake && !uploaded: i%3==0 && i%6!=0 => i%3==0 && i%6!=0
        // i%6==0 implies i%3==0, so pending = (i%3==0) - (i%6==0) = 334 - 167 = 167
        assert_eq!(cm.pending_upload_count(), 167);
        assert_eq!(cm.total_size(), 1000 * 1024);
        assert_eq!(cm.verified_count(), 334);
    }

    // ---- Attack all captures tests ----

    #[test]
    fn test_attack_all_captures_empty() {
        let cm = CaptureManager::new("/tmp/captures");
        let mut store = CrackedPasswordStore::new();
        let cracked = attack_all_captures(&cm, Path::new("/tmp/wordlist.txt"), &mut store);
        assert!(cracked.is_empty());
    }

    // ---- Mtime tracking tests ----

    #[test]
    fn test_register_tracks_mtime() {
        let mut cm = CaptureManager::new("/tmp/captures");
        let now = SystemTime::now();
        let path = PathBuf::from("/tmp/captures/test.pcapng");
        cm.register(CaptureFile {
            path: path.clone(),
            ssid: "Net".into(),
            bssid: [0; 6],
            has_handshake: false,
            uploaded: false,
            size: 100,
            mtime: Some(now),
            converted: false,
        });
        assert!(cm.tracked_mtimes.contains_key(&path));
        assert_eq!(cm.tracked_mtimes[&path], now);
    }

    #[test]
    fn test_register_no_mtime() {
        let mut cm = CaptureManager::new("/tmp/captures");
        let path = PathBuf::from("/tmp/captures/test.pcapng");
        cm.register(CaptureFile {
            path: path.clone(),
            ssid: "Net".into(),
            bssid: [0; 6],
            has_handshake: false,
            uploaded: false,
            size: 100,
            mtime: None,
            converted: false,
        });
        assert!(!cm.tracked_mtimes.contains_key(&path));
    }

    // ---- Conversion + upload pipeline integration test ----

    #[test]
    fn test_pipeline_scan_convert_upload_flow() {
        // Simulate the full pipeline without actual tools
        let mut cm = CaptureManager::new("/tmp/captures_pipeline");

        // Register some captures
        for i in 0..5 {
            cm.register(CaptureFile {
                path: PathBuf::from(format!("/tmp/captures_pipeline/cap_{i}.pcapng")),
                ssid: format!("Net{i}"),
                bssid: [0; 6],
                has_handshake: false,
                uploaded: false,
                size: 1024,
                mtime: None,
                converted: false,
            });
        }
        assert_eq!(cm.count(), 5);
        assert_eq!(cm.unconverted_files().len(), 5);
        assert_eq!(cm.uploadable_files().len(), 0);

        // Simulate conversion of 3 files
        for i in 0..3 {
            let path = PathBuf::from(format!("/tmp/captures_pipeline/cap_{i}.pcapng"));
            cm.mark_converted(&path);
        }
        assert_eq!(cm.unconverted_files().len(), 2);
        assert_eq!(cm.uploadable_files().len(), 3);
        assert_eq!(cm.verified_count(), 3);

        // Simulate upload of 2 files
        for i in 0..2 {
            let path = PathBuf::from(format!("/tmp/captures_pipeline/cap_{i}.pcapng"));
            cm.mark_uploaded(&path);
        }
        assert_eq!(cm.uploadable_files().len(), 1);
        assert_eq!(cm.pending_upload_count(), 1);

        // Cleanup should not remove converted files
        cm.max_files = 3;
        let removed = cm.cleanup();
        assert_eq!(removed, 2); // removed the 2 unconverted files
        assert_eq!(cm.count(), 3);
        assert_eq!(cm.verified_count(), 3);
    }
}
