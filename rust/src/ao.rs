//! AngryOxide subprocess management.
//!
//! Spawns, monitors, stops, and restarts the angryoxide binary.

use log::{error, info, warn};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// AO process state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AoState {
    /// Not started yet.
    Stopped,
    /// Running normally.
    Running,
    /// Crashed, awaiting restart.
    Crashed,
    /// Permanently stopped after too many crashes.
    Failed,
}

/// Configuration for the AO subprocess.
#[derive(Debug, Clone)]
pub struct AoConfig {
    /// Path to the angryoxide binary.
    pub binary: String,
    /// WiFi interface to use.
    pub interface: String,
    /// Output directory for captures.
    pub output_dir: String,
    /// Attack rate (1 = safe for BCM43436B0, 2+ crashes).
    pub rate: u32,
    /// Channel dwell time in seconds.
    pub dwell: u32,
    /// Whether to skip interface setup (--no-setup).
    pub no_setup: bool,
    /// Run in headless mode.
    pub headless: bool,
    /// Maximum crash count before giving up.
    pub max_crashes: u32,
    /// Base backoff seconds for exponential restart delay.
    pub base_backoff_secs: u64,
}

impl Default for AoConfig {
    fn default() -> Self {
        Self {
            binary: "/usr/local/bin/angryoxide".into(),
            interface: "wlan0mon".into(),
            output_dir: "/home/pi/handshakes/".into(),
            rate: 1,
            dwell: 5,
            no_setup: true,
            headless: true,
            max_crashes: 10,
            base_backoff_secs: 5,
        }
    }
}

/// Manages the angryoxide child process.
pub struct AoManager {
    pub config: AoConfig,
    pub state: AoState,
    /// Child process handle (only on unix-like systems at runtime).
    #[cfg(unix)]
    process: Option<std::process::Child>,
    /// PID of the running process (0 if not running).
    pub pid: u32,
    /// Total crash count this session.
    pub crash_count: u32,
    /// Consecutive stable epochs since last crash.
    pub stable_epochs: u32,
    /// Last crash time.
    pub last_crash: Option<Instant>,
    /// Start time of the current run.
    pub start_time: Option<Instant>,
    /// Next allowed restart time (for exponential backoff).
    next_restart: Option<Instant>,
    /// AP count parsed from AO stdout (shared with reader thread).
    pub ao_ap_count: Arc<AtomicU32>,
    /// Current channel parsed from AO stdout (shared with reader thread).
    pub ao_channel: Arc<AtomicU32>,
    /// Signals the reader thread to stop.
    pub shutdown_flag: Arc<AtomicBool>,
    /// Whether gpsd was detected at startup.
    pub gpsd_detected: bool,
}

impl AoManager {
    /// Create a new AO manager with the given config.
    pub fn new(config: AoConfig) -> Self {
        Self {
            config,
            state: AoState::Stopped,
            #[cfg(unix)]
            process: None,
            pid: 0,
            crash_count: 0,
            stable_epochs: 0,
            last_crash: None,
            start_time: None,
            next_restart: None,
            ao_ap_count: Arc::new(AtomicU32::new(0)),
            ao_channel: Arc::new(AtomicU32::new(0)),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            gpsd_detected: false,
        }
    }

    /// Get the current AP count parsed from AO stdout.
    pub fn ap_count(&self) -> u32 {
        self.ao_ap_count.load(Ordering::Relaxed)
    }

    /// Get the current channel parsed from AO stdout.
    pub fn channel(&self) -> u32 {
        self.ao_channel.load(Ordering::Relaxed)
    }

    /// Build the command-line arguments for angryoxide.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = vec![
            "--interface".into(),
            self.config.interface.clone(),
            "--output".into(),
            self.config.output_dir.clone(),
            "--rate".into(),
            self.config.rate.to_string(),
            "--dwell".into(),
            self.config.dwell.to_string(),
        ];
        if self.config.headless {
            args.push("--headless".into());
        }
        if self.config.no_setup {
            args.push("--no-setup".into());
        }
        if self.gpsd_detected {
            args.push("--gpsd".into());
        }
        args
    }

    /// Start the AO subprocess.
    pub fn start(&mut self) -> Result<(), String> {
        if self.state == AoState::Running {
            return Ok(()); // already running
        }
        if self.state == AoState::Failed {
            return Err("AO permanently stopped after too many crashes".into());
        }

        // Probe gpsd availability
        self.gpsd_detected = gpsd_available();
        if self.gpsd_detected {
            info!("gpsd detected at 127.0.0.1:2947, enabling --gpsd");
        }

        // Check if binary exists
        if !Path::new(&self.config.binary).exists() {
            // On non-Pi (dev), just log and simulate
            #[cfg(not(unix))]
            {
                info!("AO binary not found (dev mode), simulating start");
                self.state = AoState::Running;
                self.pid = 0;
                self.start_time = Some(Instant::now());
                return Ok(());
            }
            #[cfg(unix)]
            return Err(format!("AO binary not found: {}", self.config.binary));
        }

        #[cfg(unix)]
        {
            let args = self.build_args();
            info!("starting AO: {} {}", self.config.binary, args.join(" "));

            // Reset shutdown flag before spawning
            self.shutdown_flag.store(false, Ordering::Relaxed);
            self.ao_ap_count.store(0, Ordering::Relaxed);

            match std::process::Command::new(&self.config.binary)
                .args(&args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(mut child) => {
                    self.pid = child.id();
                    info!("AO started with PID {}", self.pid);

                    // Take stdout and spawn reader thread (JoinHandle intentionally
                    // discarded — thread exits on stdout EOF when child dies)
                    if let Some(stdout) = child.stdout.take() {
                        let ap_count = Arc::clone(&self.ao_ap_count);
                        let channel = Arc::clone(&self.ao_channel);
                        let shutdown = Arc::clone(&self.shutdown_flag);
                        if let Err(e) = std::thread::Builder::new()
                            .name("ao-stdout-reader".into())
                            .spawn(move || {
                                ao_stdout_reader(stdout, ap_count, channel, shutdown);
                            })
                        {
                            error!("failed to spawn AO stdout reader: {e}");
                        }
                    }

                    self.process = Some(child);
                    self.state = AoState::Running;
                    self.start_time = Some(Instant::now());
                    Ok(())
                }
                Err(e) => {
                    error!("failed to start AO: {e}");
                    Err(format!("failed to start AO: {e}"))
                }
            }
        }

        #[cfg(not(unix))]
        {
            // Windows/dev stub: simulate successful start
            info!("AO start (stub, non-unix platform)");
            self.state = AoState::Running;
            self.pid = 0;
            self.start_time = Some(Instant::now());
            Ok(())
        }
    }

    /// Stop the AO subprocess.
    pub fn stop(&mut self) {
        if self.state == AoState::Stopped || self.state == AoState::Failed {
            return;
        }
        self.shutdown_flag.store(true, Ordering::Relaxed);
        self.ao_ap_count.store(0, Ordering::Relaxed);

        #[cfg(unix)]
        {
            if let Some(mut child) = self.process.take() {
                info!("stopping AO (PID {})", self.pid);
                // Send SIGTERM (15)
                let _ = signal_process(child.id(), 15);
                // Wait up to 10s for graceful exit
                match wait_timeout(&mut child, Duration::from_secs(10)) {
                    Ok(_) => info!("AO stopped gracefully"),
                    Err(_) => {
                        warn!("AO did not stop gracefully, sending SIGKILL");
                        let _ = signal_process(child.id(), 9);
                        let _ = child.wait();
                    }
                }
            }
        }

        self.state = AoState::Stopped;
        self.pid = 0;
        self.start_time = None;
        info!("AO stopped");
    }

    /// Restart AO (stop then start).
    pub fn restart(&mut self) -> Result<(), String> {
        self.stop();
        self.start()
    }

    /// Check if AO process is still alive; detect crashes.
    /// Returns true if AO crashed and was detected.
    pub fn check_health(&mut self) -> bool {
        if self.state != AoState::Running {
            return false;
        }

        #[cfg(unix)]
        {
            if let Some(ref mut child) = self.process {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        warn!("AO process exited with status: {status}");
                        self.process = None;
                        self.on_crash();
                        return true;
                    }
                    Ok(None) => {} // still running
                    Err(e) => {
                        error!("error checking AO process: {e}");
                    }
                }
            }
        }

        false
    }

    /// Handle a crash: increment counter, compute backoff.
    fn on_crash(&mut self) {
        self.crash_count += 1;
        self.stable_epochs = 0;
        self.last_crash = Some(Instant::now());
        self.pid = 0;
        self.start_time = None;

        if self.crash_count >= self.config.max_crashes {
            error!(
                "AO reached max crash count ({}), stopping permanently",
                self.config.max_crashes
            );
            self.state = AoState::Failed;
        } else {
            let backoff = self.backoff_seconds();
            warn!(
                "AO crash #{}, will restart in {}s",
                self.crash_count, backoff
            );
            self.next_restart = Some(Instant::now() + Duration::from_secs(backoff));
            self.state = AoState::Crashed;
        }
    }

    /// Calculate exponential backoff: min(base * 2^(crashes-1), 300).
    fn backoff_seconds(&self) -> u64 {
        let exp = self.config.base_backoff_secs
            * 2u64.saturating_pow(self.crash_count.saturating_sub(1));
        exp.min(300)
    }

    /// Try to restart if we're in crashed state and backoff has elapsed.
    /// Returns true if a restart was attempted.
    pub fn try_auto_restart(&mut self) -> bool {
        if self.state != AoState::Crashed {
            return false;
        }
        if let Some(restart_at) = self.next_restart {
            if Instant::now() >= restart_at {
                info!("auto-restarting AO after backoff");
                match self.start() {
                    Ok(()) => {
                        self.next_restart = None;
                        return true;
                    }
                    Err(e) => {
                        error!("auto-restart failed: {e}");
                        return false;
                    }
                }
            }
        }
        false
    }

    /// Record a stable epoch (no crash). Resets crash counter after enough stable epochs.
    pub fn record_stable_epoch(&mut self) {
        if self.state == AoState::Running {
            self.stable_epochs += 1;
            // After 10 stable epochs, reset crash counter
            if self.stable_epochs >= 10 && self.crash_count > 0 {
                info!(
                    "AO stable for {} epochs, resetting crash counter",
                    self.stable_epochs
                );
                self.crash_count = 0;
            }
        }
    }

    /// Get AO uptime in seconds (0 if not running).
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.map(|t| t.elapsed().as_secs()).unwrap_or(0)
    }

    /// Get AO uptime as a formatted string.
    pub fn uptime_str(&self) -> String {
        match self.start_time {
            Some(t) => {
                let secs = t.elapsed().as_secs();
                let h = secs / 3600;
                let m = (secs % 3600) / 60;
                let s = secs % 60;
                format!("{h:02}:{m:02}:{s:02}")
            }
            None => "N/A".into(),
        }
    }

    /// Set the attack rate.
    pub fn set_rate(&mut self, rate: u32) {
        self.config.rate = rate.clamp(1, 3);
        info!("AO rate set to {}", self.config.rate);
    }

    /// Current state as a display string.
    pub fn state_str(&self) -> &'static str {
        match self.state {
            AoState::Stopped => "STOPPED",
            AoState::Running => "RUNNING",
            AoState::Crashed => "CRASHED",
            AoState::Failed => "FAILED",
        }
    }

    /// Reset the crash counter and allow restarts again.
    pub fn reset(&mut self) {
        self.crash_count = 0;
        self.stable_epochs = 0;
        if self.state == AoState::Failed {
            self.state = AoState::Stopped;
        }
        info!("AO crash counter reset");
    }
}

impl Default for AoManager {
    fn default() -> Self {
        Self::new(AoConfig::default())
    }
}

/// Check if gpsd is reachable at localhost:2947 (TCP connect with 1s timeout).
#[cfg(unix)]
pub fn gpsd_available() -> bool {
    use std::net::TcpStream;
    TcpStream::connect_timeout(
        &"127.0.0.1:2947".parse().unwrap(),
        Duration::from_secs(1),
    )
    .is_ok()
}

/// gpsd is not available on non-unix platforms.
#[cfg(not(unix))]
pub fn gpsd_available() -> bool {
    false
}

// Unix-only helpers for process signaling
#[cfg(unix)]
fn signal_process(pid: u32, sig: i32) -> Result<(), String> {
    let ret = unsafe { libc::kill(pid as i32, sig) };
    if ret == 0 {
        Ok(())
    } else {
        Err(format!("kill({pid}, {sig}) failed: {ret}"))
    }
}

#[cfg(unix)]
fn wait_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<std::process::ExitStatus, String> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    return Err("timeout".into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("wait error: {e}")),
        }
    }
}

/// Parse a line of AO stdout for AP/target count.
/// Returns Some(count) if the line contains a target/AP count, None otherwise.
/// Matches patterns like "Targets: 5", "APs: 12", "Access Points: 8" (case-insensitive).
pub fn parse_ao_line(line: &str) -> Option<u32> {
    let lower = line.to_ascii_lowercase();

    // Find keyword position (longer matches first to avoid ambiguity)
    let keyword_end = if let Some(pos) = lower.find("access points") {
        pos + "access points".len()
    } else if let Some(pos) = lower.find("targets") {
        pos + "targets".len()
    } else if let Some(pos) = lower.find("target") {
        pos + "target".len()
    } else if let Some(pos) = lower.find("aps") {
        pos + "aps".len()
    } else {
        return None;
    };

    // Skip separator characters (whitespace, :, -, =)
    let rest = &line[keyword_end..];
    let rest = rest.trim_start_matches(|c: char| c == ':' || c == '-' || c == '=' || c.is_whitespace());

    // Extract digits
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Extract a 12-hex-char BSSID from a line (e.g., "[a0ab1bce191f]" or "a0ab1bce191f").
fn extract_bssid(line: &str) -> Option<String> {
    // Look for 12 consecutive hex chars (MAC without colons)
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 12 <= bytes.len() {
        let candidate = &line[i..i + 12];
        if candidate.chars().all(|c| c.is_ascii_hexdigit()) {
            // Avoid matching timestamps or other long hex strings
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_hexdigit();
            let after_ok = i + 12 >= bytes.len() || !bytes[i + 12].is_ascii_hexdigit();
            if before_ok && after_ok {
                return Some(candidate.to_ascii_lowercase());
            }
        }
        i += 1;
    }
    None
}

/// Reader thread: reads AO stdout line-by-line, counts unique BSSIDs as APs.
#[cfg(unix)]
fn ao_stdout_reader(
    stdout: std::process::ChildStdout,
    ap_count: Arc<AtomicU32>,
    ao_channel: Arc<AtomicU32>,
    shutdown: Arc<AtomicBool>,
) {
    use std::collections::HashSet;
    use std::io::{BufRead, BufReader};

    let reader = BufReader::new(stdout);
    let mut seen_bssids: HashSet<String> = HashSet::new();

    for line_result in reader.lines() {
        if shutdown.load(Ordering::Relaxed) {
            info!("AO stdout reader: shutdown signaled");
            break;
        }
        match line_result {
            Ok(line) => {
                // Try explicit AP count from status line first
                if let Some(count) = parse_ao_line(&line) {
                    ap_count.store(count, Ordering::Relaxed);
                }

                // Parse channel from status lines: "Channel: 6"
                if let Some(pos) = line.find("Channel:") {
                    let rest = line[pos + 8..].trim();
                    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(ch) = digits.parse::<u32>() {
                        ao_channel.store(ch, Ordering::Relaxed);
                    }
                }

                // Track unique BSSIDs from attack/info lines
                if line.contains("Retrieval")
                    || line.contains("Disassoc")
                    || line.contains("Deauth")
                    || line.contains("PMKID")
                    || line.contains("Association")
                {
                    if let Some(bssid) = extract_bssid(&line) {
                        seen_bssids.insert(bssid);
                        ap_count.store(seen_bssids.len() as u32, Ordering::Relaxed);
                    }
                }

                // Log capture events
                let lower = line.to_ascii_lowercase();
                if lower.contains("handshake")
                    || lower.contains("pmkid")
                    || lower.contains("captured")
                    || lower.contains("hash")
                {
                    info!("AO capture event: {}", &line[..line.len().min(120)]);
                }
            }
            Err(e) => {
                warn!("AO stdout read error: {e}");
                break;
            }
        }
    }
    info!("AO stdout reader thread exiting");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ao_config_default() {
        let cfg = AoConfig::default();
        assert_eq!(cfg.rate, 1);
        assert_eq!(cfg.dwell, 5);
        assert!(cfg.headless);
        assert!(cfg.no_setup);
    }

    #[test]
    fn test_ao_manager_new() {
        let ao = AoManager::default();
        assert_eq!(ao.state, AoState::Stopped);
        assert_eq!(ao.pid, 0);
        assert_eq!(ao.crash_count, 0);
    }

    #[test]
    fn test_build_args() {
        let ao = AoManager::default();
        let args = ao.build_args();
        assert!(args.contains(&"--interface".to_string()));
        assert!(args.contains(&"wlan0mon".to_string()));
        assert!(args.contains(&"--headless".to_string()));
        assert!(args.contains(&"--no-setup".to_string()));
        assert!(args.contains(&"--rate".to_string()));
        assert!(args.contains(&"1".to_string()));
    }

    #[test]
    fn test_state_str() {
        let mut ao = AoManager::default();
        assert_eq!(ao.state_str(), "STOPPED");
        ao.state = AoState::Running;
        assert_eq!(ao.state_str(), "RUNNING");
        ao.state = AoState::Crashed;
        assert_eq!(ao.state_str(), "CRASHED");
    }

    #[test]
    fn test_backoff_calculation() {
        let mut ao = AoManager::default();
        ao.crash_count = 1;
        assert_eq!(ao.backoff_seconds(), 5);
        ao.crash_count = 2;
        assert_eq!(ao.backoff_seconds(), 10);
        ao.crash_count = 3;
        assert_eq!(ao.backoff_seconds(), 20);
        ao.crash_count = 10;
        assert_eq!(ao.backoff_seconds(), 300); // capped
    }

    #[test]
    fn test_set_rate() {
        let mut ao = AoManager::default();
        ao.set_rate(2);
        assert_eq!(ao.config.rate, 2);
        ao.set_rate(0); // clamp to 1
        assert_eq!(ao.config.rate, 1);
        ao.set_rate(5); // clamp to 3
        assert_eq!(ao.config.rate, 3);
    }

    #[test]
    fn test_reset() {
        let mut ao = AoManager::default();
        ao.crash_count = 5;
        ao.state = AoState::Failed;
        ao.reset();
        assert_eq!(ao.crash_count, 0);
        assert_eq!(ao.state, AoState::Stopped);
    }

    #[test]
    fn test_record_stable_epoch() {
        let mut ao = AoManager::default();
        ao.state = AoState::Running;
        ao.crash_count = 3;
        for _ in 0..10 {
            ao.record_stable_epoch();
        }
        assert_eq!(ao.crash_count, 0);
    }

    #[test]
    fn test_uptime_str_not_running() {
        let ao = AoManager::default();
        assert_eq!(ao.uptime_str(), "N/A");
    }

    #[test]
    fn test_uptime_str_running() {
        let mut ao = AoManager::default();
        ao.start_time = Some(Instant::now());
        let s = ao.uptime_str();
        assert_eq!(s.len(), 8);
        assert!(s.starts_with("00:00:0"));
    }

    #[test]
    fn test_on_crash_increments() {
        let mut ao = AoManager::default();
        ao.state = AoState::Running;
        ao.on_crash();
        assert_eq!(ao.crash_count, 1);
        assert_eq!(ao.state, AoState::Crashed);
        assert!(ao.next_restart.is_some());
    }

    #[test]
    fn test_on_crash_max_reached() {
        let mut ao = AoManager::new(AoConfig {
            max_crashes: 2,
            ..Default::default()
        });
        ao.state = AoState::Running;
        ao.on_crash(); // crash 1
        assert_eq!(ao.state, AoState::Crashed);
        ao.state = AoState::Running;
        ao.on_crash(); // crash 2 = max
        assert_eq!(ao.state, AoState::Failed);
    }

    #[test]
    fn test_try_auto_restart_not_crashed() {
        let mut ao = AoManager::default();
        ao.state = AoState::Stopped;
        assert!(!ao.try_auto_restart());
    }

    #[test]
    fn test_check_health_stopped() {
        let mut ao = AoManager::default();
        assert!(!ao.check_health());
    }

    #[test]
    fn test_stop_when_stopped() {
        let mut ao = AoManager::default();
        ao.stop(); // should not panic
        assert_eq!(ao.state, AoState::Stopped);
    }

    #[test]
    fn test_start_failed_state() {
        let mut ao = AoManager::default();
        ao.state = AoState::Failed;
        let result = ao.start();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_ao_line_targets() {
        assert_eq!(parse_ao_line("Targets: 5"), Some(5));
        assert_eq!(parse_ao_line("Targets: 0"), Some(0));
        assert_eq!(parse_ao_line("Targets: 42"), Some(42));
    }

    #[test]
    fn test_parse_ao_line_aps() {
        assert_eq!(parse_ao_line("APs: 12"), Some(12));
        assert_eq!(parse_ao_line("APs=3"), Some(3));
        assert_eq!(parse_ao_line("APs-7"), Some(7));
    }

    #[test]
    fn test_parse_ao_line_access_points() {
        assert_eq!(parse_ao_line("Access Points: 8"), Some(8));
        assert_eq!(parse_ao_line("access points: 15"), Some(15));
    }

    #[test]
    fn test_parse_ao_line_case_insensitive() {
        assert_eq!(parse_ao_line("TARGETS: 10"), Some(10));
        assert_eq!(parse_ao_line("targets: 3"), Some(3));
        assert_eq!(parse_ao_line("aps: 7"), Some(7));
    }

    #[test]
    fn test_parse_ao_line_no_match() {
        assert_eq!(parse_ao_line(""), None);
        assert_eq!(parse_ao_line("some random log line"), None);
        assert_eq!(parse_ao_line("Frames: 1234 | Rate: 50"), None);
        assert_eq!(parse_ao_line("Status :: 14:23:01 | AA:BB:CC"), None);
    }

    #[test]
    fn test_parse_ao_line_no_digits_after_separator() {
        assert_eq!(parse_ao_line("Targets: "), None);
        assert_eq!(parse_ao_line("APs: abc"), None);
        assert_eq!(parse_ao_line("Targets:"), None);
    }

    #[test]
    fn test_parse_ao_line_embedded_in_status() {
        assert_eq!(parse_ao_line("2026-03-22 14:00:00 UTC | [Status] | Targets: 9"), Some(9));
        assert_eq!(parse_ao_line("[INFO] APs: 20"), Some(20));
    }

    #[test]
    fn test_ap_count_default() {
        let ao = AoManager::default();
        assert_eq!(ao.ap_count(), 0);
    }

    #[test]
    fn test_ap_count_read_write() {
        let ao = AoManager::default();
        ao.ao_ap_count.store(42, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(ao.ap_count(), 42);
    }

    #[test]
    fn test_shutdown_flag_default() {
        let ao = AoManager::default();
        assert!(!ao.shutdown_flag.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn test_stop_sets_shutdown_and_resets_count() {
        let mut ao = AoManager::default();
        ao.state = AoState::Running;
        ao.ao_ap_count.store(15, std::sync::atomic::Ordering::Relaxed);
        ao.stop();
        assert!(ao.shutdown_flag.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(ao.ao_ap_count.load(std::sync::atomic::Ordering::Relaxed), 0);
    }
}
