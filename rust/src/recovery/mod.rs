//! Self-healing and recovery module.
//!
//! Handles WiFi SDIO keepalive, GPIO power cycling, watchdog integration,
//! and boot diagnostics.

use std::time::{Duration, Instant};

/// Recovery states for the WiFi subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    /// Everything is healthy.
    Healthy,
    /// WiFi interface is unresponsive, attempting soft recovery.
    SoftRecovery,
    /// Soft recovery failed, attempting GPIO power cycle.
    HardRecovery,
    /// All recovery attempts exhausted.
    Failed,
}

/// WiFi health check result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthCheck {
    /// Interface is up and responding.
    Ok,
    /// Interface exists but is not responding to ioctls.
    Unresponsive,
    /// Interface has disappeared (firmware crash).
    Missing,
}

/// Configuration for recovery behavior.
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Interval between health checks in seconds.
    pub check_interval_secs: u64,
    /// Number of soft recovery attempts before hard recovery.
    pub max_soft_retries: u32,
    /// Number of hard recovery (GPIO power cycle) attempts before giving up.
    pub max_hard_retries: u32,
    /// Delay after GPIO power-off before power-on (milliseconds).
    pub gpio_cycle_delay_ms: u64,
    /// Whether watchdog is enabled.
    pub watchdog_enabled: bool,
    /// Watchdog timeout in seconds.
    pub watchdog_timeout_secs: u64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 10,
            max_soft_retries: 3,
            max_hard_retries: 2,
            gpio_cycle_delay_ms: 500,
            watchdog_enabled: true,
            watchdog_timeout_secs: 60,
        }
    }
}

/// Recovery manager for WiFi and system health.
pub struct RecoveryManager {
    pub config: RecoveryConfig,
    pub state: RecoveryState,
    pub soft_retry_count: u32,
    pub hard_retry_count: u32,
    pub last_check: Option<Instant>,
    pub last_recovery: Option<Instant>,
    /// Total recoveries performed this session.
    pub total_recoveries: u32,
    /// Boot diagnostics log (in-memory ring buffer).
    pub diagnostics: Vec<DiagnosticEntry>,
    pub max_diagnostics: usize,
}

/// A diagnostic log entry.
#[derive(Debug, Clone)]
pub struct DiagnosticEntry {
    pub timestamp: Instant,
    pub level: DiagLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagLevel {
    Info,
    Warn,
    Error,
}

impl RecoveryManager {
    /// Create a new recovery manager with the given configuration.
    pub fn new(config: RecoveryConfig) -> Self {
        Self {
            config,
            state: RecoveryState::Healthy,
            soft_retry_count: 0,
            hard_retry_count: 0,
            last_check: None,
            last_recovery: None,
            total_recoveries: 0,
            diagnostics: Vec::new(),
            max_diagnostics: 100,
        }
    }

    /// Record a diagnostic message.
    pub fn log(&mut self, level: DiagLevel, message: &str) {
        if self.diagnostics.len() >= self.max_diagnostics {
            self.diagnostics.remove(0);
        }
        self.diagnostics.push(DiagnosticEntry {
            timestamp: Instant::now(),
            level,
            message: message.to_string(),
        });
    }

    /// Check if it's time to perform a health check.
    pub fn should_check(&self) -> bool {
        match self.last_check {
            None => true,
            Some(t) => t.elapsed() >= Duration::from_secs(self.config.check_interval_secs),
        }
    }

    /// Process a health check result and advance the recovery state machine.
    pub fn process_health(&mut self, check: HealthCheck) -> RecoveryAction {
        self.last_check = Some(Instant::now());

        match (self.state, check) {
            (_, HealthCheck::Ok) => {
                if self.state != RecoveryState::Healthy {
                    self.log(DiagLevel::Info, "WiFi recovered, back to healthy state");
                    self.total_recoveries += 1;
                }
                self.state = RecoveryState::Healthy;
                self.soft_retry_count = 0;
                self.hard_retry_count = 0;
                RecoveryAction::None
            }
            (RecoveryState::Healthy, HealthCheck::Unresponsive) => {
                self.state = RecoveryState::SoftRecovery;
                self.soft_retry_count = 1;
                self.log(DiagLevel::Warn, "WiFi unresponsive, starting soft recovery");
                RecoveryAction::SoftRecover
            }
            (RecoveryState::Healthy, HealthCheck::Missing) => {
                self.state = RecoveryState::HardRecovery;
                self.hard_retry_count = 1;
                self.log(DiagLevel::Error, "WiFi interface missing, starting hard recovery");
                RecoveryAction::HardRecover
            }
            (RecoveryState::SoftRecovery, _) => {
                self.soft_retry_count += 1;
                if self.soft_retry_count > self.config.max_soft_retries {
                    self.state = RecoveryState::HardRecovery;
                    self.hard_retry_count = 1;
                    self.log(DiagLevel::Error, "Soft recovery exhausted, escalating to hard recovery");
                    RecoveryAction::HardRecover
                } else {
                    self.log(
                        DiagLevel::Warn,
                        &format!(
                            "Soft recovery attempt {}/{}",
                            self.soft_retry_count, self.config.max_soft_retries
                        ),
                    );
                    RecoveryAction::SoftRecover
                }
            }
            (RecoveryState::HardRecovery, _) => {
                self.hard_retry_count += 1;
                if self.hard_retry_count > self.config.max_hard_retries {
                    self.state = RecoveryState::Failed;
                    self.log(DiagLevel::Error, "All recovery attempts exhausted");
                    RecoveryAction::GiveUp
                } else {
                    self.log(
                        DiagLevel::Error,
                        &format!(
                            "Hard recovery attempt {}/{}",
                            self.hard_retry_count, self.config.max_hard_retries
                        ),
                    );
                    RecoveryAction::HardRecover
                }
            }
            (RecoveryState::Failed, _) => RecoveryAction::GiveUp,
        }
    }

    /// Get the number of diagnostic entries.
    pub fn diagnostic_count(&self) -> usize {
        self.diagnostics.len()
    }

    /// Get diagnostics of a given level.
    pub fn diagnostics_by_level(&self, level: DiagLevel) -> Vec<&DiagnosticEntry> {
        self.diagnostics
            .iter()
            .filter(|d| d.level == level)
            .collect()
    }
}

impl Default for RecoveryManager {
    fn default() -> Self {
        Self::new(RecoveryConfig::default())
    }
}

/// Action the recovery manager recommends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    /// No action needed.
    None,
    /// Perform soft recovery (rmmod/modprobe or interface restart).
    SoftRecover,
    /// Perform hard recovery (GPIO WL_REG_ON power cycle).
    HardRecover,
    /// All recovery attempts failed.
    GiveUp,
}

// ---------------------------------------------------------------------------
// GPIO WL_REG_ON power cycle (actual hardware recovery)
// ---------------------------------------------------------------------------

/// BCM GPIO pin for WL_REG_ON (WiFi chip power control).
/// On Pi Zero 2W / BCM43436B0, this is GPIO 41.
pub const WL_REG_ON_PIN: u8 = 41;

/// Perform a hardware power cycle of the WiFi chip by toggling WL_REG_ON.
///
/// Sequence: pull LOW (power off) → wait → pull HIGH (power on) → wait for
/// chip to re-enumerate on SDIO bus.
///
/// On non-aarch64 platforms this is a no-op stub.
pub fn gpio_power_cycle_wifi(delay_ms: u64) -> Result<(), String> {
    #[cfg(target_arch = "aarch64")]
    {
        use rppal::gpio::Gpio;
        use std::thread;
        use std::time::Duration;

        let gpio = Gpio::new().map_err(|e| format!("GPIO init: {e}"))?;
        let mut pin = gpio
            .get(WL_REG_ON_PIN)
            .map_err(|e| format!("WL_REG_ON pin {WL_REG_ON_PIN}: {e}"))?
            .into_output();

        // Power off
        pin.set_low();
        thread::sleep(Duration::from_millis(delay_ms));

        // Power on
        pin.set_high();
        // Wait for chip to re-enumerate
        thread::sleep(Duration::from_millis(2000));

        Ok(())
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        let _ = delay_ms;
        log::debug!("gpio_power_cycle_wifi: no-op on non-Pi platform");
        Ok(())
    }
}

/// Watchdog manager -- pings `/dev/watchdog` to prevent system reset.
pub struct Watchdog {
    /// Whether the hardware watchdog is enabled.
    pub enabled: bool,
    pub timeout_secs: u64,
    pub last_ping: Option<Instant>,
}

impl Watchdog {
    /// Create a new watchdog with the given enabled state and timeout.
    pub fn new(enabled: bool, timeout_secs: u64) -> Self {
        Self {
            enabled,
            timeout_secs,
            last_ping: None,
        }
    }

    /// Check if we need to ping the watchdog.
    pub fn needs_ping(&self) -> bool {
        if !self.enabled {
            return false;
        }
        match self.last_ping {
            None => true,
            Some(t) => t.elapsed() >= Duration::from_secs(self.timeout_secs / 2),
        }
    }

    /// Record a watchdog ping.
    pub fn ping(&mut self) {
        self.last_ping = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_healthy_on_ok() {
        let mut rm = RecoveryManager::default();
        let action = rm.process_health(HealthCheck::Ok);
        assert_eq!(action, RecoveryAction::None);
        assert_eq!(rm.state, RecoveryState::Healthy);
    }

    #[test]
    fn test_unresponsive_triggers_soft() {
        let mut rm = RecoveryManager::default();
        let action = rm.process_health(HealthCheck::Unresponsive);
        assert_eq!(action, RecoveryAction::SoftRecover);
        assert_eq!(rm.state, RecoveryState::SoftRecovery);
    }

    #[test]
    fn test_missing_triggers_hard() {
        let mut rm = RecoveryManager::default();
        let action = rm.process_health(HealthCheck::Missing);
        assert_eq!(action, RecoveryAction::HardRecover);
        assert_eq!(rm.state, RecoveryState::HardRecovery);
    }

    #[test]
    fn test_soft_exhaustion_escalates() {
        let mut rm = RecoveryManager::new(RecoveryConfig {
            max_soft_retries: 2,
            ..Default::default()
        });
        rm.process_health(HealthCheck::Unresponsive); // soft 1
        rm.process_health(HealthCheck::Unresponsive); // soft 2
        let action = rm.process_health(HealthCheck::Unresponsive); // soft exhausted -> hard
        assert_eq!(action, RecoveryAction::HardRecover);
        assert_eq!(rm.state, RecoveryState::HardRecovery);
    }

    #[test]
    fn test_hard_exhaustion_gives_up() {
        let mut rm = RecoveryManager::new(RecoveryConfig {
            max_soft_retries: 1,
            max_hard_retries: 1,
            ..Default::default()
        });
        rm.process_health(HealthCheck::Unresponsive); // soft 1
        rm.process_health(HealthCheck::Unresponsive); // soft exhausted -> hard 1
        let action = rm.process_health(HealthCheck::Unresponsive); // hard exhausted -> give up
        assert_eq!(action, RecoveryAction::GiveUp);
        assert_eq!(rm.state, RecoveryState::Failed);
    }

    #[test]
    fn test_recovery_resets_on_ok() {
        let mut rm = RecoveryManager::default();
        rm.process_health(HealthCheck::Unresponsive);
        assert_eq!(rm.state, RecoveryState::SoftRecovery);
        let action = rm.process_health(HealthCheck::Ok);
        assert_eq!(action, RecoveryAction::None);
        assert_eq!(rm.state, RecoveryState::Healthy);
        assert_eq!(rm.total_recoveries, 1);
    }

    #[test]
    fn test_diagnostics_ring_buffer() {
        let mut rm = RecoveryManager::new(RecoveryConfig::default());
        rm.max_diagnostics = 3;
        rm.log(DiagLevel::Info, "msg1");
        rm.log(DiagLevel::Info, "msg2");
        rm.log(DiagLevel::Info, "msg3");
        rm.log(DiagLevel::Warn, "msg4"); // Should evict msg1
        assert_eq!(rm.diagnostic_count(), 3);
        assert_eq!(rm.diagnostics[0].message, "msg2");
    }

    #[test]
    fn test_diagnostics_by_level() {
        let mut rm = RecoveryManager::default();
        rm.log(DiagLevel::Info, "info");
        rm.log(DiagLevel::Warn, "warn");
        rm.log(DiagLevel::Error, "error");
        assert_eq!(rm.diagnostics_by_level(DiagLevel::Warn).len(), 1);
    }

    #[test]
    fn test_should_check_initially() {
        let rm = RecoveryManager::default();
        assert!(rm.should_check());
    }

    #[test]
    fn test_should_check_after_interval() {
        let mut rm = RecoveryManager::default();
        rm.last_check = Some(Instant::now());
        assert!(!rm.should_check());

        rm.last_check = Some(Instant::now() - Duration::from_secs(100));
        assert!(rm.should_check());
    }

    #[test]
    fn test_watchdog_needs_ping() {
        let wd = Watchdog::new(true, 60);
        assert!(wd.needs_ping());

        let mut wd2 = Watchdog::new(true, 60);
        wd2.ping();
        assert!(!wd2.needs_ping());
    }

    #[test]
    fn test_watchdog_disabled() {
        let wd = Watchdog::new(false, 60);
        assert!(!wd.needs_ping());
    }

    #[test]
    fn test_wl_reg_on_pin_constant() {
        assert_eq!(WL_REG_ON_PIN, 41);
    }

    #[test]
    fn test_gpio_power_cycle_stub() {
        // On non-Pi platforms, should succeed as a no-op
        let result = gpio_power_cycle_wifi(500);
        assert!(result.is_ok());
    }

    #[test]
    fn test_recovery_after_max_retries_stays_failed() {
        let mut rm = RecoveryManager::new(RecoveryConfig {
            max_soft_retries: 1,
            max_hard_retries: 1,
            ..Default::default()
        });
        rm.process_health(HealthCheck::Unresponsive); // soft 1
        rm.process_health(HealthCheck::Unresponsive); // soft exhausted -> hard 1
        rm.process_health(HealthCheck::Unresponsive); // hard exhausted -> give up

        // Further unresponsive checks should keep returning GiveUp
        let action = rm.process_health(HealthCheck::Unresponsive);
        assert_eq!(action, RecoveryAction::GiveUp);
        assert_eq!(rm.state, RecoveryState::Failed);

        // But if wifi recovers, we should reset to Healthy
        let action = rm.process_health(HealthCheck::Ok);
        assert_eq!(action, RecoveryAction::None);
        assert_eq!(rm.state, RecoveryState::Healthy);
    }

    #[test]
    fn test_watchdog_needs_ping_after_timeout() {
        let mut wd = Watchdog::new(true, 60);
        // Simulate a ping from 31+ seconds ago (half of 60)
        wd.last_ping = Some(Instant::now() - Duration::from_secs(31));
        assert!(wd.needs_ping());
    }

    #[test]
    fn test_diagnostics_empty() {
        let rm = RecoveryManager::default();
        assert_eq!(rm.diagnostic_count(), 0);
        assert!(rm.diagnostics_by_level(DiagLevel::Error).is_empty());
    }

    #[test]
    fn test_diagnostics_overflow() {
        let mut rm = RecoveryManager::default();
        rm.max_diagnostics = 5;
        for i in 0..20 {
            rm.log(DiagLevel::Info, &format!("msg{i}"));
        }
        assert_eq!(rm.diagnostic_count(), 5);
        // Should have messages 15-19
        assert_eq!(rm.diagnostics[0].message, "msg15");
        assert_eq!(rm.diagnostics[4].message, "msg19");
    }
}
