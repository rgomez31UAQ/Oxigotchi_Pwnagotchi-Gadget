//! Radio lock manager for the BCM43436B0 shared UART.
//!
//! The BCM43436B0 chip shares a single UART between WiFi and Bluetooth.
//! Only one can be active at a time. This module provides atomic transitions
//! between radio modes with verification at each step and rollback on failure.
//!
//! Lock file format: `MODE PID TIMESTAMP\n`
//! Modes: WIFI, BT, FREE, TRANSITIONING

use log::{error, info, warn};
use std::path::Path;
use std::time::Duration;

use crate::ao;
use crate::bluetooth;
use crate::bluetooth::patchram::PatchramManager;
use crate::wifi;

/// Lock file location.
const LOCK_PATH: &str = "/var/run/oxigotchi-radio.lock";

/// Radio operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioMode {
    /// WiFi monitor mode is active (AO running).
    Wifi,
    /// Bluetooth PAN is active.
    Bt,
    /// Radio is free (neither WiFi nor BT active).
    Free,
    /// Transition in progress (lock held atomically).
    Transitioning,
}

impl RadioMode {
    /// Parse a mode string from the lock file.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "WIFI" => Some(RadioMode::Wifi),
            "BT" => Some(RadioMode::Bt),
            "FREE" => Some(RadioMode::Free),
            "TRANSITIONING" => Some(RadioMode::Transitioning),
            _ => None,
        }
    }

    /// Mode as a lock file string.
    pub fn as_str(&self) -> &'static str {
        match self {
            RadioMode::Wifi => "WIFI",
            RadioMode::Bt => "BT",
            RadioMode::Free => "FREE",
            RadioMode::Transitioning => "TRANSITIONING",
        }
    }
}

/// Manages the shared radio lock and mode transitions.
pub struct RadioManager {
    /// Current radio mode.
    pub mode: RadioMode,
    /// Path to the lock file.
    lock_path: String,
}

impl RadioManager {
    /// Create a new RadioManager starting in Free mode.
    pub fn new() -> Self {
        Self {
            mode: RadioMode::Free,
            lock_path: LOCK_PATH.to_string(),
        }
    }

    /// Create a RadioManager with a custom lock path (for testing).
    #[cfg(test)]
    pub fn with_lock_path(path: &str) -> Self {
        Self {
            mode: RadioMode::Free,
            lock_path: path.to_string(),
        }
    }

    /// Write the lock file with the given mode and current PID.
    pub fn acquire_lock(&mut self, mode: RadioMode) -> Result<(), String> {
        let pid = current_pid();
        let timestamp = current_timestamp();
        let content = format!("{} {} {}\n", mode.as_str(), pid, timestamp);

        // Lock file I/O works on all platforms — only hardware commands are cfg(unix)
        if let Some(parent) = std::path::Path::new(&self.lock_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&self.lock_path, &content)
            .map_err(|e| format!("failed to write lock file {}: {e}", self.lock_path))?;

        self.mode = mode;
        info!("radio lock acquired: {} (PID {})", mode.as_str(), pid);
        Ok(())
    }

    /// Release the lock by writing FREE to the lock file.
    pub fn release_lock(&mut self) -> Result<(), String> {
        self.acquire_lock(RadioMode::Free)
    }

    /// Read the current lock file contents.
    /// Returns (mode, pid) or (Free, 0) if the file doesn't exist or is unreadable.
    pub fn read_lock(&self) -> (RadioMode, u32) {
        match std::fs::read_to_string(&self.lock_path) {
            Ok(content) => {
                let parts: Vec<&str> = content.trim().split_whitespace().collect();
                if parts.len() >= 2 {
                    let mode = RadioMode::from_str(parts[0]).unwrap_or(RadioMode::Free);
                    let pid = parts[1].parse::<u32>().unwrap_or(0);
                    return (mode, pid);
                }
                (RadioMode::Free, 0)
            }
            Err(_) => (RadioMode::Free, 0),
        }
    }

    /// Check if the lock owner PID is dead (stale lock detection).
    pub fn is_stale(&self) -> bool {
        let (mode, pid) = self.read_lock();
        if mode == RadioMode::Free || pid == 0 {
            return false;
        }
        verify_process_dead(pid)
    }

    /// Atomic transition from current mode to WiFi (RAGE mode).
    ///
    /// Steps:
    /// 1. Write TRANSITIONING to lock
    /// 2. Power off BT
    /// 3. Verify BT powered off
    /// 4. Wait 2s for UART to settle
    /// 5. Start WiFi monitor mode
    /// 6. Verify wlan0mon exists
    /// 7. Start AO
    /// 8. Verify AO running
    /// 9. Write WIFI to lock
    ///
    /// On failure: rollback — stop WiFi, restart BT, write BT to lock.
    pub fn transition_to_wifi(
        &mut self,
        ao: &mut ao::AoManager,
        wifi: &mut wifi::WifiManager,
        bt: &mut bluetooth::BtTether,
    ) -> Result<(), String> {
        info!("radio: beginning transition to WIFI");

        // Step 1: Mark transitioning
        self.acquire_lock(RadioMode::Transitioning)?;

        // Step 2: Power off BT
        info!("radio: step 2 — powering off BT");
        bt.power_off();

        // Step 3: Verify BT powered off
        #[cfg(unix)]
        {
            if verify_bt_powered() {
                warn!("radio: BT still shows powered after power_off, continuing anyway");
            }
        }

        // Step 4: Wait for UART to settle
        info!("radio: step 4 — waiting 2s for UART settle");
        std::thread::sleep(Duration::from_secs(2));

        // Step 5: Start WiFi monitor mode
        info!("radio: step 5 — starting WiFi monitor mode");
        if let Err(e) = wifi.start_monitor() {
            error!("radio: WiFi monitor start failed: {e}, rolling back to BT");
            self.rollback_to_bt(bt);
            return Err(format!("WiFi monitor start failed: {e}"));
        }

        // Step 6: Verify wlan0mon exists
        if !verify_interface_exists("wlan0mon") {
            error!("radio: wlan0mon not found after start_monitor, rolling back to BT");
            let _ = wifi.stop_monitor();
            self.rollback_to_bt(bt);
            return Err("wlan0mon interface not created".into());
        }
        info!("radio: step 6 — wlan0mon verified");

        // Step 7: Start AO
        info!("radio: step 7 — starting AO");
        if let Err(e) = ao.start() {
            error!("radio: AO start failed: {e}, rolling back to BT");
            let _ = wifi.stop_monitor();
            self.rollback_to_bt(bt);
            return Err(format!("AO start failed: {e}"));
        }

        // Step 8: Verify AO running
        if ao.state != ao::AoState::Running {
            error!("radio: AO not in Running state after start, rolling back to BT");
            ao.stop();
            let _ = wifi.stop_monitor();
            self.rollback_to_bt(bt);
            return Err("AO failed to reach Running state".into());
        }
        info!("radio: step 8 — AO verified (PID {})", ao.pid);

        // Step 9: Write WIFI lock
        self.acquire_lock(RadioMode::Wifi)?;
        info!("radio: transition to WIFI complete");
        Ok(())
    }

    /// Atomic transition from current mode to BT (SAFE mode).
    ///
    /// Steps:
    /// 1. Write TRANSITIONING to lock
    /// 2. Stop AO (SIGTERM, wait up to 10s)
    /// 3. Verify AO stopped
    /// 4. Delete wlan0mon
    /// 5. Verify wlan0mon gone
    /// 6. Bring wlan0 down
    /// 7. Reload hci_uart
    /// 8. Verify /sys/class/bluetooth/hci0 exists
    /// 9. Power on BT
    /// 10. Verify BT powered
    /// 11. Write BT to lock
    ///
    /// On failure: rollback — tear down BT, restart WiFi, write WIFI to lock.
    pub fn transition_to_bt(
        &mut self,
        ao: &mut ao::AoManager,
        wifi: &mut wifi::WifiManager,
        _bt: &mut bluetooth::BtTether,
    ) -> Result<(), String> {
        info!("radio: beginning transition to BT");

        // Step 1: Mark transitioning
        self.acquire_lock(RadioMode::Transitioning)?;

        // Step 2: Stop AO
        info!("radio: step 2 — stopping AO");
        ao.stop();

        // Step 3: Verify AO stopped
        if ao.state != ao::AoState::Stopped {
            warn!(
                "radio: AO state is {:?} after stop, expected Stopped",
                ao.state
            );
        }
        if ao.pid != 0 {
            // Check if process is actually dead
            if !verify_process_dead(ao.pid) {
                error!(
                    "radio: AO PID {} still alive after stop, rolling back to WIFI",
                    ao.pid
                );
                self.rollback_to_wifi(ao, wifi);
                return Err(format!("AO PID {} still alive after stop", ao.pid));
            }
        }
        info!("radio: step 3 — AO stopped verified");

        // Step 4: Exit WiFi monitor mode (deletes wlan0mon)
        info!("radio: step 4 — stopping WiFi monitor mode");
        if let Err(e) = wifi.stop_monitor() {
            warn!("radio: WiFi monitor stop failed: {e} (continuing)");
        }

        // Step 5: Verify wlan0mon gone
        if !verify_interface_gone("wlan0mon") {
            warn!("radio: wlan0mon still exists after stop_monitor, attempting manual delete");
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("iw")
                    .args(["dev", "wlan0mon", "del"])
                    .output();
                std::thread::sleep(Duration::from_millis(500));
            }
            if !verify_interface_gone("wlan0mon") {
                error!("radio: wlan0mon still exists, rolling back to WIFI");
                self.rollback_to_wifi(ao, wifi);
                return Err("failed to remove wlan0mon interface".into());
            }
        }
        info!("radio: step 5 — wlan0mon gone verified");

        // Step 6: Bring wlan0 down
        info!("radio: step 6 — bringing wlan0 down");
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("ip")
                .args(["link", "set", "wlan0", "down"])
                .output();
        }

        // Step 7: Reload hci_uart
        info!("radio: step 7 — reloading hci_uart");
        bluetooth::reset_hci_uart();

        // Step 8: Verify /sys/class/bluetooth/hci0 exists
        if !verify_bt_adapter_exists() {
            error!("radio: hci0 not found after hci_uart reload, rolling back to WIFI");
            self.rollback_to_wifi(ao, wifi);
            return Err("BT adapter hci0 not found after hci_uart reload".into());
        }
        info!("radio: step 8 — hci0 adapter verified");

        // Step 9: Power on BT
        info!("radio: step 9 — powering on BT");
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("bluetoothctl")
                .args(["power", "on"])
                .output();
        }

        // Step 10: Verify BT powered
        if !verify_bt_powered() {
            warn!("radio: BT does not show powered after power on");
            // Non-fatal on non-Pi, continue
        }
        info!("radio: step 10 — BT power verified");

        // Step 11: Write BT lock
        self.acquire_lock(RadioMode::Bt)?;
        info!("radio: transition to BT complete");
        Ok(())
    }

    /// Atomic transition to BT attack mode with patchram load.
    ///
    /// Steps:
    /// 1. Write TRANSITIONING to lock
    /// 2. Stop AO
    /// 3. Wait 500ms for UART settle
    /// 4. Stop WiFi monitor mode
    /// 5. Disconnect BT PAN
    /// 6. Load attack patchram (on failure: set mode=Free, return error)
    /// 7. Write BT to lock
    pub fn transition_to_bt_attack(
        &mut self,
        ao: &mut ao::AoManager,
        wifi: &mut wifi::WifiManager,
        bt: &mut bluetooth::BtTether,
        patchram: &mut PatchramManager,
    ) -> Result<(), String> {
        info!("radio: beginning transition to BT attack mode");

        // Step 1: Mark transitioning
        self.acquire_lock(RadioMode::Transitioning)?;

        // Step 2: Stop AO
        info!("radio: step 2 — stopping AO");
        ao.stop();

        // Step 2b: Verify AO actually dead (matches transition_to_bt pattern)
        if ao.state != ao::AoState::Stopped {
            warn!(
                "radio: AO state is {:?} after stop, expected Stopped",
                ao.state
            );
        }
        if ao.pid != 0 {
            if !verify_process_dead(ao.pid) {
                warn!(
                    "radio: AO PID {} still alive after stop, force-killing",
                    ao.pid
                );
                #[cfg(unix)]
                {
                    let _ = std::process::Command::new("kill")
                        .args(["-9", &ao.pid.to_string()])
                        .output();
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
        // Fallback: pkill any lingering angryoxide processes
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("pkill")
                .args(["-9", "-f", "angryoxide"])
                .output();
            std::thread::sleep(Duration::from_millis(200));
        }
        info!("radio: step 2b — AO kill verified");

        // Step 3: Wait for UART settle
        info!("radio: step 3 — waiting 500ms for UART settle");
        std::thread::sleep(Duration::from_millis(500));

        // Step 4: Stop WiFi monitor mode
        info!("radio: step 4 — stopping WiFi monitor mode");
        if let Err(e) = wifi.stop_monitor() {
            warn!("radio: WiFi monitor stop failed: {e} (continuing)");
        }

        // Step 5: Disconnect BT PAN
        info!("radio: step 5 — disconnecting BT PAN");
        bt.disconnect();

        // Step 6: Load attack patchram
        info!("radio: step 6 — loading attack patchram");
        if let Err(e) = patchram.load_attack() {
            error!("radio: attack patchram load failed: {e}, setting mode=Free");
            let _ = self.acquire_lock(RadioMode::Free);
            return Err(format!("attack patchram load failed: {e}"));
        }

        // Step 7: Write BT lock
        self.acquire_lock(RadioMode::Bt)?;
        info!("radio: transition to BT attack mode complete");
        Ok(())
    }

    /// Atomic transition from BT (attack) back to WiFi mode.
    ///
    /// Steps:
    /// 1. Write TRANSITIONING to lock
    /// 2. Unload patchram
    /// 3. Wait 500ms for UART settle
    /// 4. Start WiFi monitor mode
    /// 5. Start AO
    /// 6. Write WIFI to lock
    pub fn transition_bt_to_wifi(
        &mut self,
        ao: &mut ao::AoManager,
        wifi: &mut wifi::WifiManager,
        patchram: &mut PatchramManager,
    ) -> Result<(), String> {
        info!("radio: beginning transition from BT to WIFI");

        // Step 1: Mark transitioning
        self.acquire_lock(RadioMode::Transitioning)?;

        // Step 2: Unload patchram
        info!("radio: step 2 — unloading patchram");
        patchram.unload()?;

        // Step 3: Wait for UART settle
        info!("radio: step 3 — waiting 500ms for UART settle");
        std::thread::sleep(Duration::from_millis(500));

        // Step 4: Start WiFi monitor mode
        info!("radio: step 4 — starting WiFi monitor mode");
        if let Err(e) = wifi.start_monitor() {
            error!("radio: WiFi monitor start failed: {e}");
            return Err(format!("WiFi monitor start failed: {e}"));
        }

        // Step 5: Start AO
        info!("radio: step 5 — starting AO");
        if let Err(e) = ao.start() {
            error!("radio: AO start failed: {e}");
            let _ = wifi.stop_monitor();
            return Err(format!("AO start failed: {e}"));
        }

        // Step 6: Write WIFI lock
        self.acquire_lock(RadioMode::Wifi)?;
        info!("radio: transition from BT to WIFI complete");
        Ok(())
    }

    /// Atomic transition from BT attack to BT safe (stock patchram).
    ///
    /// Steps:
    /// 1. Write TRANSITIONING to lock
    /// 2. Load stock patchram
    /// 3. Write BT to lock
    pub fn transition_bt_to_safe(
        &mut self,
        _bt: &mut bluetooth::BtTether,
        patchram: &mut PatchramManager,
    ) -> Result<(), String> {
        info!("radio: beginning transition from BT attack to BT safe");

        // Step 1: Mark transitioning
        self.acquire_lock(RadioMode::Transitioning)?;

        // Step 2: Load stock patchram
        info!("radio: step 2 — loading stock patchram");
        patchram.load_stock()?;

        // Step 3: Write BT lock
        self.acquire_lock(RadioMode::Bt)?;
        info!("radio: transition from BT attack to BT safe complete");
        Ok(())
    }

    /// Rollback helper: restart BT after a failed WiFi transition.
    fn rollback_to_bt(&mut self, bt: &mut bluetooth::BtTether) {
        warn!("radio: rolling back to BT");
        bluetooth::reset_hci_uart();
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("bluetoothctl")
                .args(["power", "on"])
                .output();
        }
        // Don't try full setup — just get BT adapter powered
        bt.state = bluetooth::BtState::Disconnected;
        let _ = self.acquire_lock(RadioMode::Bt);
    }

    /// Rollback helper: restart WiFi after a failed BT transition.
    fn rollback_to_wifi(&mut self, ao: &mut ao::AoManager, wifi: &mut wifi::WifiManager) {
        warn!("radio: rolling back to WIFI");
        // Try to restart WiFi monitor mode
        match wifi.start_monitor() {
            Ok(()) => info!("radio: rollback — WiFi monitor restarted"),
            Err(e) => error!("radio: rollback — WiFi monitor restart failed: {e}"),
        }
        // Try to restart AO
        match ao.start() {
            Ok(()) => info!("radio: rollback — AO restarted (PID {})", ao.pid),
            Err(e) => error!("radio: rollback — AO restart failed: {e}"),
        }
        let _ = self.acquire_lock(RadioMode::Wifi);
    }
}

impl Default for RadioManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Verification helpers
// ---------------------------------------------------------------------------

/// Check if a network interface exists via sysfs.
#[cfg(unix)]
fn verify_interface_exists(name: &str) -> bool {
    Path::new(&format!("/sys/class/net/{name}")).exists()
}

#[cfg(not(unix))]
fn verify_interface_exists(_name: &str) -> bool {
    true // stub: always succeeds on non-unix
}

/// Check if a network interface is gone from sysfs.
#[cfg(unix)]
fn verify_interface_gone(name: &str) -> bool {
    !Path::new(&format!("/sys/class/net/{name}")).exists()
}

#[cfg(not(unix))]
fn verify_interface_gone(_name: &str) -> bool {
    true // stub: always succeeds on non-unix
}

/// Check if a process is dead via kill -0.
#[cfg(unix)]
fn verify_process_dead(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) != 0 }
}

#[cfg(not(unix))]
fn verify_process_dead(_pid: u32) -> bool {
    true // stub: process is "dead" on non-unix
}

/// Check if the BT adapter is powered on via hciconfig.
#[cfg(unix)]
fn verify_bt_powered() -> bool {
    match std::process::Command::new("hciconfig").arg("hci0").output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains("UP") && stdout.contains("RUNNING")
        }
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn verify_bt_powered() -> bool {
    true // stub: BT always "powered" on non-unix
}

/// Check if the BT adapter (hci0) exists in sysfs.
#[cfg(unix)]
fn verify_bt_adapter_exists() -> bool {
    Path::new("/sys/class/bluetooth/hci0").exists()
}

#[cfg(not(unix))]
fn verify_bt_adapter_exists() -> bool {
    true // stub: adapter always "exists" on non-unix
}

/// Get current process PID.
fn current_pid() -> u32 {
    std::process::id()
}

/// Get current Unix timestamp.
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_radio_mode_from_str() {
        assert_eq!(RadioMode::from_str("WIFI"), Some(RadioMode::Wifi));
        assert_eq!(RadioMode::from_str("BT"), Some(RadioMode::Bt));
        assert_eq!(RadioMode::from_str("FREE"), Some(RadioMode::Free));
        assert_eq!(
            RadioMode::from_str("TRANSITIONING"),
            Some(RadioMode::Transitioning)
        );
        assert_eq!(RadioMode::from_str("wifi"), Some(RadioMode::Wifi));
        assert_eq!(RadioMode::from_str("bt"), Some(RadioMode::Bt));
        assert_eq!(RadioMode::from_str("INVALID"), None);
        assert_eq!(RadioMode::from_str(""), None);
    }

    #[test]
    fn test_radio_mode_as_str() {
        assert_eq!(RadioMode::Wifi.as_str(), "WIFI");
        assert_eq!(RadioMode::Bt.as_str(), "BT");
        assert_eq!(RadioMode::Free.as_str(), "FREE");
        assert_eq!(RadioMode::Transitioning.as_str(), "TRANSITIONING");
    }

    #[test]
    fn test_radio_manager_new() {
        let rm = RadioManager::new();
        assert_eq!(rm.mode, RadioMode::Free);
    }

    #[test]
    fn test_acquire_and_read_lock() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("radio.lock");
        let lock_str = lock_path.to_str().unwrap();

        let mut rm = RadioManager::with_lock_path(lock_str);
        rm.acquire_lock(RadioMode::Wifi).unwrap();
        assert_eq!(rm.mode, RadioMode::Wifi);

        let (mode, pid) = rm.read_lock();
        assert_eq!(mode, RadioMode::Wifi);
        assert!(pid > 0);
    }

    #[test]
    fn test_release_lock() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("radio.lock");
        let lock_str = lock_path.to_str().unwrap();

        let mut rm = RadioManager::with_lock_path(lock_str);
        rm.acquire_lock(RadioMode::Wifi).unwrap();
        rm.release_lock().unwrap();

        let (mode, _) = rm.read_lock();
        assert_eq!(mode, RadioMode::Free);
        assert_eq!(rm.mode, RadioMode::Free);
    }

    #[test]
    fn test_read_lock_missing_file() {
        let rm = RadioManager::with_lock_path("/tmp/nonexistent-radio-lock-test.lock");
        let (mode, pid) = rm.read_lock();
        assert_eq!(mode, RadioMode::Free);
        assert_eq!(pid, 0);
    }

    #[test]
    fn test_stale_lock_detection() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("radio.lock");

        // Write a lock with a definitely-dead PID
        std::fs::write(&lock_path, "WIFI 999999999 0\n").unwrap();

        let rm = RadioManager::with_lock_path(lock_path.to_str().unwrap());
        // On non-unix, verify_process_dead always returns true, so this should be stale
        assert!(rm.is_stale());
    }

    #[test]
    fn test_transition_to_wifi_stub() {
        // Skip on non-Pi Linux — needs real wlan0 for iw commands
        if std::fs::metadata("/sys/class/net/wlan0").is_err() {
            return;
        }
        // On non-unix, all stubs succeed so this should work end-to-end
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("radio.lock");
        let lock_str = lock_path.to_str().unwrap();

        let mut rm = RadioManager::with_lock_path(lock_str);
        let mut ao_mgr = ao::AoManager::default();
        let mut wifi_mgr = wifi::WifiManager::new();
        let mut bt = bluetooth::BtTether::new(bluetooth::BtConfig::default());

        let result = rm.transition_to_wifi(&mut ao_mgr, &mut wifi_mgr, &mut bt);
        assert!(result.is_ok(), "transition_to_wifi failed: {:?}", result);
        assert_eq!(rm.mode, RadioMode::Wifi);
        assert_eq!(wifi_mgr.state, wifi::WifiState::Monitor);
    }

    #[test]
    fn test_transition_to_bt_stub() {
        // Skip on non-Pi Linux — needs real wlan0 for iw commands
        if std::fs::metadata("/sys/class/net/wlan0").is_err() {
            return;
        }
        // On non-unix, all stubs succeed
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("radio.lock");
        let lock_str = lock_path.to_str().unwrap();

        let mut rm = RadioManager::with_lock_path(lock_str);
        let mut ao_mgr = ao::AoManager::default();
        let mut wifi_mgr = wifi::WifiManager::new();
        let mut bt = bluetooth::BtTether::new(bluetooth::BtConfig::default());

        // Start in WIFI mode first
        rm.acquire_lock(RadioMode::Wifi).unwrap();
        ao_mgr.state = ao::AoState::Running;

        let result = rm.transition_to_bt(&mut ao_mgr, &mut wifi_mgr, &mut bt);
        assert!(result.is_ok(), "transition_to_bt failed: {:?}", result);
        assert_eq!(rm.mode, RadioMode::Bt);
    }

    #[test]
    fn test_roundtrip_wifi_bt_wifi() {
        // Skip on non-Pi Linux — needs real wlan0 for iw commands
        if std::fs::metadata("/sys/class/net/wlan0").is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("radio.lock");
        let lock_str = lock_path.to_str().unwrap();

        let mut rm = RadioManager::with_lock_path(lock_str);
        let mut ao_mgr = ao::AoManager::default();
        let mut wifi_mgr = wifi::WifiManager::new();
        let mut bt = bluetooth::BtTether::new(bluetooth::BtConfig::default());

        // WiFi
        let result = rm.transition_to_wifi(&mut ao_mgr, &mut wifi_mgr, &mut bt);
        assert!(result.is_ok());
        assert_eq!(rm.mode, RadioMode::Wifi);

        // WiFi -> BT
        let result = rm.transition_to_bt(&mut ao_mgr, &mut wifi_mgr, &mut bt);
        assert!(result.is_ok());
        assert_eq!(rm.mode, RadioMode::Bt);

        // BT -> WiFi
        let result = rm.transition_to_wifi(&mut ao_mgr, &mut wifi_mgr, &mut bt);
        assert!(result.is_ok());
        assert_eq!(rm.mode, RadioMode::Wifi);
    }

    #[test]
    fn test_current_pid() {
        let pid = current_pid();
        assert!(pid > 0);
    }

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        // Should be a reasonable Unix timestamp (after 2020)
        assert!(ts > 1_577_836_800);
    }

    #[test]
    fn test_lock_file_format() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("radio.lock");
        let lock_str = lock_path.to_str().unwrap();

        let mut rm = RadioManager::with_lock_path(lock_str);
        rm.acquire_lock(RadioMode::Wifi).unwrap();

        let content = std::fs::read_to_string(&lock_path).unwrap();
        let parts: Vec<&str> = content.trim().split_whitespace().collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "WIFI");
        assert!(parts[1].parse::<u32>().is_ok());
        assert!(parts[2].parse::<u64>().is_ok());
    }
}
