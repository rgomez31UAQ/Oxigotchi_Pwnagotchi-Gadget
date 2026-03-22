//! AngryOxide subprocess management.
//!
//! Spawns, monitors, stops, and restarts the angryoxide binary.

use log::{error, info, warn};
use std::path::Path;
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
        }
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

            match std::process::Command::new(&self.config.binary)
                .args(&args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    self.pid = child.id();
                    info!("AO started with PID {}", self.pid);
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
}
