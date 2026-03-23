//! Self-healing and recovery module.
//!
//! Replaces: wifi-watchdog.sh, wifi-recovery.sh, bootlog.sh, wlan_keepalive.c
//!
//! Handles WiFi health monitoring, GPIO power cycling, hardware watchdog,
//! boot diagnostics, and systemd service management.

use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Recovery state machine
// ---------------------------------------------------------------------------

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

/// Action the recovery manager recommends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    /// No action needed.
    None,
    /// Perform soft recovery (rmmod/modprobe or interface restart).
    SoftRecover,
    /// Perform hard recovery (GPIO WL_REG_ON power cycle).
    HardRecover,
    /// All recovery attempts failed -- reboot recommended.
    Reboot,
    /// All recovery attempts failed.
    GiveUp,
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
    /// Cooldown between recovery attempts in seconds.
    pub recovery_cooldown_secs: u64,
    /// Path to boot diagnostics log file.
    pub bootlog_path: String,
    /// Maximum total retries (soft + hard) before triggering reboot.
    pub max_total_retries_before_reboot: u32,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 10,
            max_soft_retries: 3,
            max_hard_retries: 2,
            gpio_cycle_delay_ms: 3000,
            watchdog_enabled: true,
            watchdog_timeout_secs: 60,
            recovery_cooldown_secs: 60,
            bootlog_path: "/boot/firmware/bootlog.txt".to_string(),
            max_total_retries_before_reboot: 5,
        }
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// Severity level for diagnostic entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagLevel {
    Info,
    Warn,
    Error,
}

impl DiagLevel {
    /// Short tag for log formatting.
    pub fn tag(&self) -> &'static str {
        match self {
            DiagLevel::Info => "INFO",
            DiagLevel::Warn => "WARN",
            DiagLevel::Error => "ERROR",
        }
    }
}

/// A diagnostic log entry.
#[derive(Debug, Clone)]
pub struct DiagnosticEntry {
    pub timestamp: Instant,
    /// Wall-clock time string for file output.
    pub wall_time: String,
    pub level: DiagLevel,
    pub message: String,
}

/// In-memory ring buffer for diagnostics (replaces bootlog.sh).
pub struct DiagnosticsBuffer {
    pub entries: Vec<DiagnosticEntry>,
    pub max_entries: usize,
}

impl DiagnosticsBuffer {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::with_capacity(max_entries.min(256)),
            max_entries,
        }
    }

    /// Add an entry, evicting the oldest if at capacity.
    pub fn push(&mut self, level: DiagLevel, message: &str) {
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(DiagnosticEntry {
            timestamp: Instant::now(),
            wall_time: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            level,
            message: message.to_string(),
        });
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Filter entries by level.
    pub fn by_level(&self, level: DiagLevel) -> Vec<&DiagnosticEntry> {
        self.entries.iter().filter(|e| e.level == level).collect()
    }

    /// Format all entries for file output.
    pub fn format_all(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            out.push_str(&format!(
                "{} [{}] {}\n",
                entry.wall_time,
                entry.level.tag(),
                entry.message,
            ));
        }
        out
    }

    /// Write diagnostics to a file (appending).
    pub fn write_to_file(&self, path: &str) -> Result<(), String> {
        use std::fs::OpenOptions;
        use std::io::Write;

        let content = self.format_all();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("open {path}: {e}"))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("write {path}: {e}"))?;
        Ok(())
    }
}

impl Default for DiagnosticsBuffer {
    fn default() -> Self {
        Self::new(100)
    }
}

// ---------------------------------------------------------------------------
// Recovery manager
// ---------------------------------------------------------------------------

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
            wall_time: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
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

    /// Check if cooldown period has elapsed since last recovery.
    pub fn cooldown_active(&self) -> bool {
        match self.last_recovery {
            None => false,
            Some(t) => t.elapsed() < Duration::from_secs(self.config.recovery_cooldown_secs),
        }
    }

    /// Record that a recovery attempt was made (resets cooldown timer).
    pub fn record_recovery(&mut self) {
        self.last_recovery = Some(Instant::now());
        self.total_recoveries += 1;
    }

    /// Check if we have exceeded total retry limit and should reboot.
    pub fn should_reboot(&self) -> bool {
        let total = self.soft_retry_count + self.hard_retry_count;
        total >= self.config.max_total_retries_before_reboot
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
                self.log(
                    DiagLevel::Error,
                    "WiFi interface missing, starting hard recovery",
                );
                RecoveryAction::HardRecover
            }
            (RecoveryState::SoftRecovery, _) => {
                self.soft_retry_count += 1;
                if self.soft_retry_count > self.config.max_soft_retries {
                    self.state = RecoveryState::HardRecovery;
                    self.hard_retry_count = 1;
                    self.log(
                        DiagLevel::Error,
                        "Soft recovery exhausted, escalating to hard recovery",
                    );
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
                    if self.should_reboot() {
                        RecoveryAction::Reboot
                    } else {
                        RecoveryAction::GiveUp
                    }
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

// ---------------------------------------------------------------------------
// WiFi health checking (replaces wifi-watchdog.sh interface checks)
// ---------------------------------------------------------------------------

/// Check if a network interface exists via sysfs.
pub fn interface_exists(name: &str) -> bool {
    #[cfg(unix)]
    {
        std::path::Path::new(&format!("/sys/class/net/{name}")).exists()
    }
    #[cfg(not(unix))]
    {
        let _ = name;
        true
    }
}

/// Probe WiFi health by checking sysfs for wlan0 and wlan0mon.
///
/// Logic (matches wifi-watchdog.sh):
/// - wlan0 missing              -> Missing
/// - wlan0 present, wlan0mon missing -> Unresponsive
/// - both present               -> Ok
pub fn check_wifi_health() -> HealthCheck {
    if !interface_exists("wlan0") {
        HealthCheck::Missing
    } else if !interface_exists("wlan0mon") {
        HealthCheck::Unresponsive
    } else {
        HealthCheck::Ok
    }
}

// ---------------------------------------------------------------------------
// GPIO WL_REG_ON power cycle (replaces wifi-recovery.sh)
// ---------------------------------------------------------------------------

/// BCM GPIO pin for WL_REG_ON (WiFi chip power control).
/// On Pi Zero 2W / BCM43436B0, this is GPIO 41.
pub const WL_REG_ON_PIN: u8 = 41;

/// MMC controller device ID for the Pi Zero 2W SDIO bus.
pub const MMC_DEVICE: &str = "3f300000.mmcnr";
/// Sysfs path for MMC driver bind/unbind.
pub const MMC_DRIVER_PATH: &str = "/sys/bus/platform/drivers/mmc-bcm2835";

/// A step in the GPIO recovery sequence, for testability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpioStep {
    StopService(String),
    ModprobeRemove(String),
    MmcUnbind,
    GpioPinLow(u8),
    Sleep(Duration),
    GpioPinHigh(u8),
    MmcRebind,
    ModprobeLoad(String),
    Verify,
    StartService(String),
}

/// Build the full GPIO power cycle sequence (matches wifi-recovery.sh).
///
/// This is a pure function returning the ordered list of steps.
/// Execution is separate so the sequence is testable without hardware.
pub fn build_gpio_recovery_sequence(delay_ms: u64) -> Vec<GpioStep> {
    vec![
        // Stop services that use WiFi
        GpioStep::StopService("pwnagotchi".into()),
        GpioStep::StopService("bettercap".into()),
        GpioStep::StopService("wlan-keepalive".into()),
        // Unload driver
        GpioStep::ModprobeRemove("brcmfmac".into()),
        GpioStep::Sleep(Duration::from_secs(1)),
        // Unbind MMC controller
        GpioStep::MmcUnbind,
        GpioStep::Sleep(Duration::from_secs(1)),
        // Pull WL_REG_ON low (power off WiFi chip)
        GpioStep::GpioPinLow(WL_REG_ON_PIN),
        GpioStep::Sleep(Duration::from_millis(delay_ms)),
        // Push WL_REG_ON high (power on WiFi chip)
        GpioStep::GpioPinHigh(WL_REG_ON_PIN),
        GpioStep::Sleep(Duration::from_secs(2)),
        // Rebind MMC controller
        GpioStep::MmcRebind,
        GpioStep::Sleep(Duration::from_secs(3)),
        // Reload driver
        GpioStep::ModprobeLoad("brcmfmac".into()),
        GpioStep::Sleep(Duration::from_secs(5)),
        // Verify recovery
        GpioStep::Verify,
        // Restart services
        GpioStep::StartService("wlan-keepalive".into()),
        GpioStep::StartService("bettercap".into()),
        GpioStep::Sleep(Duration::from_secs(3)),
        GpioStep::StartService("pwnagotchi".into()),
    ]
}

/// Execute the full GPIO recovery sequence.
///
/// Returns Ok(true) if wlan0 recovered, Ok(false) if it did not.
/// On non-unix, all system commands are no-ops.
pub fn execute_gpio_recovery(delay_ms: u64) -> Result<bool, String> {
    let steps = build_gpio_recovery_sequence(delay_ms);

    for step in &steps {
        match step {
            GpioStep::StopService(name) => {
                let _ = stop_service(name);
            }
            GpioStep::ModprobeRemove(module) => {
                run_modprobe_remove(module)?;
            }
            GpioStep::MmcUnbind => {
                write_sysfs(&format!("{MMC_DRIVER_PATH}/unbind"), MMC_DEVICE)?;
            }
            GpioStep::GpioPinLow(pin) => {
                gpio_set_pin(*pin, false)?;
            }
            GpioStep::Sleep(dur) => {
                std::thread::sleep(*dur);
            }
            GpioStep::GpioPinHigh(pin) => {
                gpio_set_pin(*pin, true)?;
            }
            GpioStep::MmcRebind => {
                write_sysfs(&format!("{MMC_DRIVER_PATH}/bind"), MMC_DEVICE)?;
            }
            GpioStep::ModprobeLoad(module) => {
                run_modprobe_load(module)?;
            }
            GpioStep::Verify => {
                if interface_exists("wlan0") {
                    log::info!("GPIO recovery: wlan0 is back");
                } else {
                    log::error!("GPIO recovery: wlan0 did not return");
                    return Ok(false);
                }
            }
            GpioStep::StartService(name) => {
                let _ = start_service(name);
            }
        }
    }

    Ok(true)
}

/// Perform a bare hardware power cycle of the WiFi chip (pin toggle only).
///
/// On non-aarch64 platforms this is a no-op stub.
pub fn gpio_power_cycle_wifi(delay_ms: u64) -> Result<(), String> {
    #[cfg(target_arch = "aarch64")]
    {
        use rppal::gpio::Gpio;
        use std::thread;

        let gpio = Gpio::new().map_err(|e| format!("GPIO init: {e}"))?;
        let mut pin = gpio
            .get(WL_REG_ON_PIN)
            .map_err(|e| format!("WL_REG_ON pin {WL_REG_ON_PIN}: {e}"))?
            .into_output();

        pin.set_low();
        thread::sleep(Duration::from_millis(delay_ms));
        pin.set_high();
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

/// Set a GPIO pin high or low.
fn gpio_set_pin(pin: u8, high: bool) -> Result<(), String> {
    #[cfg(target_arch = "aarch64")]
    {
        use rppal::gpio::Gpio;

        let gpio = Gpio::new().map_err(|e| format!("GPIO init: {e}"))?;
        let mut p = gpio
            .get(pin)
            .map_err(|e| format!("GPIO pin {pin}: {e}"))?
            .into_output();
        if high {
            p.set_high();
        } else {
            p.set_low();
        }
        Ok(())
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        let _ = (pin, high);
        log::debug!(
            "gpio_set_pin: pin {} {} (no-op)",
            pin,
            if high { "HIGH" } else { "LOW" }
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// System command helpers
// ---------------------------------------------------------------------------

/// Run `modprobe -r <module>`.
fn run_modprobe_remove(module: &str) -> Result<(), String> {
    run_command("modprobe", &["-r", module])
}

/// Run `modprobe <module>`.
fn run_modprobe_load(module: &str) -> Result<(), String> {
    run_command("modprobe", &[module])
}

// ---------------------------------------------------------------------------
// PSM watchdog counter reset via SDIO RAMRW (nexmon 0x500)
// ---------------------------------------------------------------------------

/// Firmware RAM addresses for watchdog counters (from firmware analysis).
/// Writing zeros to these addresses resets the counters, preventing PSM wedge.
const PSM_COUNTER_ADDR: u32 = 0x0003_F99C;
const DPC_COUNTER_ADDR: u32 = 0x0003_F9A4;
const RSSI_COUNTER_ADDR: u32 = 0x0003_F9A0;

/// Reset PSM/DPC/RSSI watchdog counters via SDIO RAMRW (nexmon 0x500).
///
/// Sends a netlink message to the nexmon kernel module to write zeros
/// to the firmware's watchdog counter addresses. This prevents the
/// ~2.5 hour PSM accumulation that causes firmware degradation.
///
/// Requires: brcmfmac-nexmon DKMS module loaded, wlan0 interface up.
#[cfg(unix)]
pub fn reset_watchdog_counters() -> Result<(), String> {
    // Build netlink messages for each counter
    for (name, addr) in &[
        ("PSM", PSM_COUNTER_ADDR),
        ("DPC", DPC_COUNTER_ADDR),
        ("RSSI", RSSI_COUNTER_ADDR),
    ] {
        // Use the test tool as a subprocess (simpler than raw netlink from Rust)
        let result = std::process::Command::new("python3")
            .args([
                "/usr/local/bin/test_sdio_ramrw.py",
                "write",
                &format!("0x{:X}", addr),
                "00000000",
            ])
            .output();

        match result {
            Ok(output) if output.status.success() => {
                log::debug!("reset {name} counter at 0x{addr:05X}");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log::warn!("reset {name} counter failed: {stderr}");
            }
            Err(e) => {
                return Err(format!("reset {name} counter: {e}"));
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn reset_watchdog_counters() -> Result<(), String> {
    Ok(())
}

/// Write a string to a sysfs path.
fn write_sysfs(path: &str, value: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        std::fs::write(path, value).map_err(|e| format!("write {path}: {e}"))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, value);
        log::debug!("write_sysfs: {path} <- {value} (no-op)");
        Ok(())
    }
}

/// Run an external command, returning Ok on success or Err with stderr.
fn run_command(program: &str, args: &[&str]) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::process::Command;
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("{program}: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("{program} failed: {stderr}"))
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (program, args);
        log::debug!("run_command: {program} {:?} (no-op)", args);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Service management (replaces systemctl calls in shell scripts)
// ---------------------------------------------------------------------------

/// Build a systemctl command arg list for the given action and service.
pub fn build_systemctl_args(action: &str, service: &str) -> Vec<String> {
    vec![
        "systemctl".to_string(),
        action.to_string(),
        service.to_string(),
    ]
}

/// Restart a systemd service.
pub fn restart_service(name: &str) -> Result<(), String> {
    run_command("systemctl", &["restart", name])
}

/// Stop a systemd service.
pub fn stop_service(name: &str) -> Result<(), String> {
    run_command("systemctl", &["stop", name])
}

/// Start a systemd service.
pub fn start_service(name: &str) -> Result<(), String> {
    run_command("systemctl", &["start", name])
}

/// Check if a systemd service is active.
pub fn is_service_active(name: &str) -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
        Command::new("systemctl")
            .args(["is-active", "--quiet", name])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = name;
        false
    }
}

// ---------------------------------------------------------------------------
// Boot diagnostics (replaces bootlog.sh)
// ---------------------------------------------------------------------------

/// Collect boot diagnostic information and return as a formatted string.
///
/// On Unix, runs the same commands as bootlog.sh:
/// - failed services, SSH status, network info, disk space
pub fn collect_boot_diagnostics() -> String {
    #[cfg(unix)]
    {
        use std::process::Command;

        let mut report = String::new();
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        report.push_str(&format!("=== Boot {timestamp} ===\n"));

        if let Ok(out) = Command::new("uptime").output() {
            report.push_str(&format!(
                "Uptime: {}\n",
                String::from_utf8_lossy(&out.stdout)
            ));
        }

        report.push_str("--- Failed Services ---\n");
        if let Ok(out) = Command::new("systemctl")
            .args(["list-units", "--failed"])
            .output()
        {
            report.push_str(&String::from_utf8_lossy(&out.stdout));
        }

        report.push_str("--- SSH ---\n");
        if let Ok(out) = Command::new("systemctl").args(["status", "ssh"]).output() {
            report.push_str(&String::from_utf8_lossy(&out.stdout));
        }

        report.push_str("--- Network ---\n");
        if let Ok(out) = Command::new("ip").args(["addr"]).output() {
            report.push_str(&String::from_utf8_lossy(&out.stdout));
        }

        report.push_str("--- Listening ports ---\n");
        if let Ok(out) = Command::new("ss").args(["-tlnp"]).output() {
            report.push_str(&String::from_utf8_lossy(&out.stdout));
        }

        report.push_str("--- Disk ---\n");
        if let Ok(out) = Command::new("df").args(["-h"]).output() {
            report.push_str(&String::from_utf8_lossy(&out.stdout));
        }

        report.push_str("=== End ===\n");
        report
    }
    #[cfg(not(unix))]
    {
        format!(
            "=== Boot {} ===\n(non-Unix stub)\n=== End ===\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        )
    }
}

/// Write boot diagnostics to the configured log file.
pub fn write_boot_diagnostics(path: &str) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let report = collect_boot_diagnostics();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("open {path}: {e}"))?;
    file.write_all(report.as_bytes())
        .map_err(|e| format!("write {path}: {e}"))?;
    Ok(())
}

/// Self-heal SSH if port 22 is not listening (mirrors bootlog.sh logic).
pub fn heal_ssh() -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::process::Command;

        let output = Command::new("ss")
            .args(["-tln"])
            .output()
            .map_err(|e| format!("ss: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        if !stdout.contains(":22 ") {
            log::warn!("SSH not listening on port 22, attempting heal");
            let _ = Command::new("ssh-keygen").args(["-A"]).output();
            restart_service("ssh")
                .or_else(|_| restart_service("emergency-ssh"))
                .map_err(|e| format!("SSH heal failed: {e}"))?;
            log::info!("SSH healed");
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        log::debug!("heal_ssh: no-op on non-Unix");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Hardware watchdog (replaces watchdog.service)
// ---------------------------------------------------------------------------

/// Watchdog manager -- pings `/dev/watchdog` to prevent system reset.
pub struct Watchdog {
    /// Whether the hardware watchdog is enabled.
    pub enabled: bool,
    pub timeout_secs: u64,
    pub last_ping: Option<Instant>,
    /// File descriptor for /dev/watchdog (Unix only).
    #[cfg(unix)]
    fd: Option<i32>,
}

impl Watchdog {
    /// Create a new watchdog with the given enabled state and timeout.
    pub fn new(enabled: bool, timeout_secs: u64) -> Self {
        Self {
            enabled,
            timeout_secs,
            last_ping: None,
            #[cfg(unix)]
            fd: None,
        }
    }

    /// Open /dev/watchdog. Once opened, the kernel will reboot if we
    /// stop pinging within the timeout.
    pub fn open(&mut self) -> Result<(), String> {
        #[cfg(unix)]
        {
            use std::ffi::CString;

            if !self.enabled {
                return Ok(());
            }
            let path = CString::new("/dev/watchdog").unwrap();
            let fd = unsafe { libc::open(path.as_ptr(), libc::O_WRONLY) };
            if fd < 0 {
                return Err(format!(
                    "/dev/watchdog: {}",
                    std::io::Error::last_os_error()
                ));
            }
            self.fd = Some(fd);
            log::info!("hardware watchdog opened (timeout={}s)", self.timeout_secs);
            Ok(())
        }
        #[cfg(not(unix))]
        {
            log::debug!("watchdog open: no-op on non-Unix");
            Ok(())
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

    /// Write to /dev/watchdog to prevent system reset.
    pub fn ping(&mut self) {
        #[cfg(unix)]
        {
            if let Some(fd) = self.fd {
                let byte: [u8; 1] = [b'V'];
                unsafe {
                    libc::write(fd, byte.as_ptr() as *const libc::c_void, 1);
                }
            }
        }
        self.last_ping = Some(Instant::now());
    }

    /// Cleanly close the watchdog (writing magic close character 'V'
    /// tells the kernel to disable the watchdog timer).
    pub fn close(&mut self) {
        #[cfg(unix)]
        {
            if let Some(fd) = self.fd.take() {
                let byte: [u8; 1] = [b'V'];
                unsafe {
                    libc::write(fd, byte.as_ptr() as *const libc::c_void, 1);
                    libc::close(fd);
                }
                log::info!("hardware watchdog closed cleanly");
            }
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.close();
    }
}

// ---------------------------------------------------------------------------
// System reboot
// ---------------------------------------------------------------------------

/// Trigger a system reboot.
pub fn trigger_reboot() -> Result<(), String> {
    log::error!("triggering system reboot");
    run_command("reboot", &[])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ======= State machine tests =======

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
        let action = rm.process_health(HealthCheck::Unresponsive);
        assert_eq!(action, RecoveryAction::HardRecover);
        assert_eq!(rm.state, RecoveryState::HardRecovery);
    }

    #[test]
    fn test_hard_exhaustion_gives_up() {
        let mut rm = RecoveryManager::new(RecoveryConfig {
            max_soft_retries: 1,
            max_hard_retries: 1,
            max_total_retries_before_reboot: 100,
            ..Default::default()
        });
        rm.process_health(HealthCheck::Unresponsive); // soft 1
        rm.process_health(HealthCheck::Unresponsive); // soft exhausted -> hard 1
        let action = rm.process_health(HealthCheck::Unresponsive);
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
    fn test_recovery_after_max_retries_stays_failed() {
        let mut rm = RecoveryManager::new(RecoveryConfig {
            max_soft_retries: 1,
            max_hard_retries: 1,
            max_total_retries_before_reboot: 100,
            ..Default::default()
        });
        rm.process_health(HealthCheck::Unresponsive); // soft 1
        rm.process_health(HealthCheck::Unresponsive); // soft exhausted -> hard 1
        rm.process_health(HealthCheck::Unresponsive); // hard exhausted -> give up

        let action = rm.process_health(HealthCheck::Unresponsive);
        assert_eq!(action, RecoveryAction::GiveUp);
        assert_eq!(rm.state, RecoveryState::Failed);

        let action = rm.process_health(HealthCheck::Ok);
        assert_eq!(action, RecoveryAction::None);
        assert_eq!(rm.state, RecoveryState::Healthy);
    }

    #[test]
    fn test_max_retries_triggers_reboot() {
        let mut rm = RecoveryManager::new(RecoveryConfig {
            max_soft_retries: 2,
            max_hard_retries: 2,
            max_total_retries_before_reboot: 5,
            ..Default::default()
        });
        // soft 1
        rm.process_health(HealthCheck::Unresponsive);
        // soft 2
        rm.process_health(HealthCheck::Unresponsive);
        // soft exhausted -> hard 1 (soft_retry_count=3, hard=1)
        rm.process_health(HealthCheck::Unresponsive);
        // hard 2 (soft=3, hard=2)
        rm.process_health(HealthCheck::Unresponsive);
        // hard exhausted -> should_reboot (soft=3, hard=3, total=6 >= 5)
        let action = rm.process_health(HealthCheck::Unresponsive);
        assert_eq!(action, RecoveryAction::Reboot);
        assert_eq!(rm.state, RecoveryState::Failed);
    }

    #[test]
    fn test_full_state_machine_cycle() {
        // Healthy -> SoftRecover -> HardRecover -> Failed -> Healthy
        let mut rm = RecoveryManager::new(RecoveryConfig {
            max_soft_retries: 1,
            max_hard_retries: 1,
            max_total_retries_before_reboot: 100,
            ..Default::default()
        });

        assert_eq!(rm.state, RecoveryState::Healthy);

        let action = rm.process_health(HealthCheck::Unresponsive);
        assert_eq!(action, RecoveryAction::SoftRecover);
        assert_eq!(rm.state, RecoveryState::SoftRecovery);

        let action = rm.process_health(HealthCheck::Unresponsive);
        assert_eq!(action, RecoveryAction::HardRecover);
        assert_eq!(rm.state, RecoveryState::HardRecovery);

        let action = rm.process_health(HealthCheck::Unresponsive);
        assert_eq!(action, RecoveryAction::GiveUp);
        assert_eq!(rm.state, RecoveryState::Failed);

        let action = rm.process_health(HealthCheck::Ok);
        assert_eq!(action, RecoveryAction::None);
        assert_eq!(rm.state, RecoveryState::Healthy);
        assert_eq!(rm.total_recoveries, 1);
    }

    // ======= Cooldown tests =======

    #[test]
    fn test_cooldown_initially_inactive() {
        let rm = RecoveryManager::default();
        assert!(!rm.cooldown_active());
    }

    #[test]
    fn test_cooldown_active_after_recovery() {
        let mut rm = RecoveryManager::default();
        rm.record_recovery();
        assert!(rm.cooldown_active());
    }

    #[test]
    fn test_cooldown_expires() {
        let mut rm = RecoveryManager::new(RecoveryConfig {
            recovery_cooldown_secs: 60,
            ..Default::default()
        });
        rm.last_recovery = Some(Instant::now() - Duration::from_secs(61));
        assert!(!rm.cooldown_active());
    }

    #[test]
    fn test_cooldown_still_active_within_window() {
        let mut rm = RecoveryManager::new(RecoveryConfig {
            recovery_cooldown_secs: 60,
            ..Default::default()
        });
        rm.last_recovery = Some(Instant::now() - Duration::from_secs(30));
        assert!(rm.cooldown_active());
    }

    // ======= GPIO sequence tests =======

    #[test]
    fn test_gpio_sequence_correct_pin() {
        let steps = build_gpio_recovery_sequence(3000);
        let low_steps: Vec<_> = steps
            .iter()
            .filter(|s| matches!(s, GpioStep::GpioPinLow(_)))
            .collect();
        let high_steps: Vec<_> = steps
            .iter()
            .filter(|s| matches!(s, GpioStep::GpioPinHigh(_)))
            .collect();

        assert_eq!(low_steps.len(), 1);
        assert_eq!(high_steps.len(), 1);
        assert_eq!(low_steps[0], &GpioStep::GpioPinLow(41));
        assert_eq!(high_steps[0], &GpioStep::GpioPinHigh(41));
    }

    #[test]
    fn test_gpio_sequence_correct_order() {
        let steps = build_gpio_recovery_sequence(3000);

        let modprobe_remove_idx = steps
            .iter()
            .position(|s| matches!(s, GpioStep::ModprobeRemove(_)))
            .unwrap();
        let mmc_unbind_idx = steps
            .iter()
            .position(|s| matches!(s, GpioStep::MmcUnbind))
            .unwrap();
        let pin_low_idx = steps
            .iter()
            .position(|s| matches!(s, GpioStep::GpioPinLow(_)))
            .unwrap();
        let pin_high_idx = steps
            .iter()
            .position(|s| matches!(s, GpioStep::GpioPinHigh(_)))
            .unwrap();
        let mmc_rebind_idx = steps
            .iter()
            .position(|s| matches!(s, GpioStep::MmcRebind))
            .unwrap();
        let modprobe_load_idx = steps
            .iter()
            .position(|s| matches!(s, GpioStep::ModprobeLoad(_)))
            .unwrap();
        let verify_idx = steps
            .iter()
            .position(|s| matches!(s, GpioStep::Verify))
            .unwrap();

        // modprobe -r -> unbind -> LOW -> HIGH -> rebind -> modprobe -> verify
        assert!(modprobe_remove_idx < mmc_unbind_idx);
        assert!(mmc_unbind_idx < pin_low_idx);
        assert!(pin_low_idx < pin_high_idx);
        assert!(pin_high_idx < mmc_rebind_idx);
        assert!(mmc_rebind_idx < modprobe_load_idx);
        assert!(modprobe_load_idx < verify_idx);
    }

    #[test]
    fn test_gpio_sequence_has_sleep_between_low_and_high() {
        let steps = build_gpio_recovery_sequence(3000);
        let pin_low_idx = steps
            .iter()
            .position(|s| matches!(s, GpioStep::GpioPinLow(_)))
            .unwrap();
        let pin_high_idx = steps
            .iter()
            .position(|s| matches!(s, GpioStep::GpioPinHigh(_)))
            .unwrap();

        let has_sleep = steps[pin_low_idx + 1..pin_high_idx]
            .iter()
            .any(|s| matches!(s, GpioStep::Sleep(_)));
        assert!(has_sleep, "must sleep between GPIO LOW and HIGH");
    }

    #[test]
    fn test_gpio_sequence_delay_configurable() {
        let steps_3s = build_gpio_recovery_sequence(3000);
        let steps_5s = build_gpio_recovery_sequence(5000);

        let pin_low_idx_3 = steps_3s
            .iter()
            .position(|s| matches!(s, GpioStep::GpioPinLow(_)))
            .unwrap();
        let sleep_3 = &steps_3s[pin_low_idx_3 + 1];

        let pin_low_idx_5 = steps_5s
            .iter()
            .position(|s| matches!(s, GpioStep::GpioPinLow(_)))
            .unwrap();
        let sleep_5 = &steps_5s[pin_low_idx_5 + 1];

        assert_eq!(*sleep_3, GpioStep::Sleep(Duration::from_millis(3000)));
        assert_eq!(*sleep_5, GpioStep::Sleep(Duration::from_millis(5000)));
    }

    #[test]
    fn test_gpio_sequence_services_stopped_before_modprobe() {
        let steps = build_gpio_recovery_sequence(3000);
        let modprobe_idx = steps
            .iter()
            .position(|s| matches!(s, GpioStep::ModprobeRemove(_)))
            .unwrap();
        let stop_services: Vec<_> = steps[..modprobe_idx]
            .iter()
            .filter(|s| matches!(s, GpioStep::StopService(_)))
            .collect();
        assert!(
            stop_services.len() >= 2,
            "services must be stopped before modprobe -r"
        );
    }

    #[test]
    fn test_gpio_sequence_services_restarted_after_verify() {
        let steps = build_gpio_recovery_sequence(3000);
        let verify_idx = steps
            .iter()
            .position(|s| matches!(s, GpioStep::Verify))
            .unwrap();
        let start_services: Vec<_> = steps[verify_idx..]
            .iter()
            .filter(|s| matches!(s, GpioStep::StartService(_)))
            .collect();
        assert!(
            start_services.len() >= 2,
            "services must be restarted after verify"
        );
    }

    #[test]
    fn test_wl_reg_on_pin_constant() {
        assert_eq!(WL_REG_ON_PIN, 41);
    }

    #[test]
    fn test_gpio_power_cycle_stub() {
        let result = gpio_power_cycle_wifi(500);
        assert!(result.is_ok());
    }

    // ======= Diagnostics ring buffer tests =======

    #[test]
    fn test_diagnostics_ring_buffer() {
        let mut rm = RecoveryManager::new(RecoveryConfig::default());
        rm.max_diagnostics = 3;
        rm.log(DiagLevel::Info, "msg1");
        rm.log(DiagLevel::Info, "msg2");
        rm.log(DiagLevel::Info, "msg3");
        rm.log(DiagLevel::Warn, "msg4");
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
        assert_eq!(rm.diagnostics[0].message, "msg15");
        assert_eq!(rm.diagnostics[4].message, "msg19");
    }

    #[test]
    fn test_diagnostics_buffer_standalone() {
        let mut buf = DiagnosticsBuffer::new(3);
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);

        buf.push(DiagLevel::Info, "first");
        buf.push(DiagLevel::Warn, "second");
        buf.push(DiagLevel::Error, "third");
        assert_eq!(buf.len(), 3);
        assert!(!buf.is_empty());

        buf.push(DiagLevel::Info, "fourth");
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.entries[0].message, "second");
        assert_eq!(buf.entries[2].message, "fourth");
    }

    #[test]
    fn test_diagnostics_buffer_by_level() {
        let mut buf = DiagnosticsBuffer::new(100);
        buf.push(DiagLevel::Info, "i1");
        buf.push(DiagLevel::Info, "i2");
        buf.push(DiagLevel::Warn, "w1");
        buf.push(DiagLevel::Error, "e1");

        assert_eq!(buf.by_level(DiagLevel::Info).len(), 2);
        assert_eq!(buf.by_level(DiagLevel::Warn).len(), 1);
        assert_eq!(buf.by_level(DiagLevel::Error).len(), 1);
    }

    #[test]
    fn test_diagnostics_buffer_format() {
        let mut buf = DiagnosticsBuffer::new(10);
        buf.push(DiagLevel::Info, "boot ok");
        buf.push(DiagLevel::Error, "wifi down");

        let output = buf.format_all();
        assert!(output.contains("[INFO] boot ok"));
        assert!(output.contains("[ERROR] wifi down"));
    }

    #[test]
    fn test_diagnostics_buffer_write_to_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("oxigotchi_test_diag.txt");
        let path_str = path.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&path);

        let mut buf = DiagnosticsBuffer::new(10);
        buf.push(DiagLevel::Info, "test entry");

        let result = buf.write_to_file(&path_str);
        assert!(result.is_ok(), "write_to_file failed: {result:?}");

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[INFO] test entry"));

        // Second write should append
        buf.push(DiagLevel::Warn, "second entry");
        buf.write_to_file(&path_str).unwrap();
        let content2 = std::fs::read_to_string(&path).unwrap();
        assert!(content2.contains("[WARN] second entry"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_diag_level_tags() {
        assert_eq!(DiagLevel::Info.tag(), "INFO");
        assert_eq!(DiagLevel::Warn.tag(), "WARN");
        assert_eq!(DiagLevel::Error.tag(), "ERROR");
    }

    // ======= Health check tests =======

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
    fn test_check_wifi_health_stub() {
        #[cfg(not(unix))]
        {
            assert_eq!(check_wifi_health(), HealthCheck::Ok);
        }
    }

    #[test]
    fn test_interface_exists_stub() {
        #[cfg(not(unix))]
        {
            assert!(interface_exists("wlan0"));
            assert!(interface_exists("wlan0mon"));
            assert!(interface_exists("nonexistent"));
        }
    }

    // ======= Watchdog tests =======

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
    fn test_watchdog_needs_ping_after_timeout() {
        let mut wd = Watchdog::new(true, 60);
        wd.last_ping = Some(Instant::now() - Duration::from_secs(31));
        assert!(wd.needs_ping());
    }

    #[test]
    fn test_watchdog_open_disabled() {
        let mut wd = Watchdog::new(false, 60);
        assert!(wd.open().is_ok());
    }

    #[test]
    fn test_watchdog_ping_updates_timestamp() {
        let mut wd = Watchdog::new(true, 60);
        assert!(wd.last_ping.is_none());
        wd.ping();
        assert!(wd.last_ping.is_some());
    }

    #[test]
    fn test_watchdog_close_idempotent() {
        let mut wd = Watchdog::new(true, 60);
        wd.close();
        wd.close();
    }

    // ======= Service management tests =======

    #[test]
    fn test_build_systemctl_restart() {
        let args = build_systemctl_args("restart", "ssh");
        assert_eq!(args, vec!["systemctl", "restart", "ssh"]);
    }

    #[test]
    fn test_build_systemctl_stop() {
        let args = build_systemctl_args("stop", "pwnagotchi");
        assert_eq!(args, vec!["systemctl", "stop", "pwnagotchi"]);
    }

    #[test]
    fn test_build_systemctl_start() {
        let args = build_systemctl_args("start", "bettercap");
        assert_eq!(args, vec!["systemctl", "start", "bettercap"]);
    }

    #[test]
    fn test_build_systemctl_is_active() {
        let args = build_systemctl_args("is-active", "wlan-keepalive");
        assert_eq!(args, vec!["systemctl", "is-active", "wlan-keepalive"]);
    }

    #[test]
    fn test_is_service_active_stub() {
        #[cfg(not(unix))]
        {
            assert!(!is_service_active("anything"));
        }
    }

    // ======= Boot diagnostics tests =======

    #[test]
    fn test_collect_boot_diagnostics_has_markers() {
        let report = collect_boot_diagnostics();
        assert!(report.contains("=== Boot"));
        assert!(report.contains("=== End ==="));
    }

    #[test]
    fn test_write_boot_diagnostics_to_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("oxigotchi_test_bootlog.txt");
        let path_str = path.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&path);

        let result = write_boot_diagnostics(&path_str);
        assert!(result.is_ok(), "write_boot_diagnostics failed: {result:?}");

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("=== Boot"));

        let _ = std::fs::remove_file(&path);
    }

    // ======= Config defaults =======

    #[test]
    fn test_config_defaults() {
        let cfg = RecoveryConfig::default();
        assert_eq!(cfg.check_interval_secs, 10);
        assert_eq!(cfg.max_soft_retries, 3);
        assert_eq!(cfg.max_hard_retries, 2);
        assert_eq!(cfg.gpio_cycle_delay_ms, 3000);
        assert!(cfg.watchdog_enabled);
        assert_eq!(cfg.watchdog_timeout_secs, 60);
        assert_eq!(cfg.recovery_cooldown_secs, 60);
        assert_eq!(cfg.max_total_retries_before_reboot, 5);
    }

    #[test]
    fn test_mmc_constants() {
        assert_eq!(MMC_DEVICE, "3f300000.mmcnr");
        assert_eq!(MMC_DRIVER_PATH, "/sys/bus/platform/drivers/mmc-bcm2835");
    }

    // ======= Recovery tracking =======

    #[test]
    fn test_record_recovery_increments_count() {
        let mut rm = RecoveryManager::default();
        assert_eq!(rm.total_recoveries, 0);
        rm.record_recovery();
        assert_eq!(rm.total_recoveries, 1);
        rm.record_recovery();
        assert_eq!(rm.total_recoveries, 2);
    }

    #[test]
    fn test_record_recovery_sets_timestamp() {
        let mut rm = RecoveryManager::default();
        assert!(rm.last_recovery.is_none());
        rm.record_recovery();
        assert!(rm.last_recovery.is_some());
    }

    #[test]
    fn test_should_reboot_below_threshold() {
        let rm = RecoveryManager::new(RecoveryConfig {
            max_total_retries_before_reboot: 10,
            ..Default::default()
        });
        assert!(!rm.should_reboot());
    }

    #[test]
    fn test_should_reboot_at_threshold() {
        let mut rm = RecoveryManager::new(RecoveryConfig {
            max_total_retries_before_reboot: 5,
            ..Default::default()
        });
        rm.soft_retry_count = 3;
        rm.hard_retry_count = 2;
        assert!(rm.should_reboot());
    }

    // ======= RecoveryAction variants =======

    #[test]
    fn test_recovery_action_all_variants_distinct() {
        let actions = [
            RecoveryAction::None,
            RecoveryAction::SoftRecover,
            RecoveryAction::HardRecover,
            RecoveryAction::Reboot,
            RecoveryAction::GiveUp,
        ];
        for (i, a) in actions.iter().enumerate() {
            for (j, b) in actions.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }
}
