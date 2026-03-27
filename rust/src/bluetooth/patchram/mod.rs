//! PatchramManager — loads/unloads attack and stock HCD firmware via kernel module.
//!
//! The BCM43430B0 BT chip firmware is loaded automatically by the kernel's
//! `hci_uart` + `btbcm` modules. To swap firmware: copy HCD to the firmware
//! search paths, then `rmmod hci_uart; modprobe hci_uart`.
//! **Never use hciattach** — the kernel serdev driver handles UART binding.

pub mod hcd;

use log::info;
#[cfg(target_os = "linux")]
use log::warn;

/// Current state of the patchram firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchramState {
    /// No HCD loaded, hci_uart not managing BT.
    Unloaded,
    /// Stock (vendor) HCD loaded.
    Stock,
    /// Attack (patched) HCD loaded.
    Attack,
    /// Last load/unload failed.
    Error,
}

impl PatchramState {
    /// Return a lowercase string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unloaded => "unloaded",
            Self::Stock => "stock",
            Self::Attack => "attack",
            Self::Error => "error",
        }
    }
}

/// Manages loading and unloading HCD firmware via kernel module reload.
pub struct PatchramManager {
    /// Current patchram state.
    pub state: PatchramState,
    /// Path to the attack (patched) HCD file.
    pub attack_hcd: String,
    /// Path to the stock (vendor) HCD file.
    pub stock_hcd: String,
    /// Last error message, if any.
    pub last_error: Option<String>,
}

impl PatchramManager {
    /// Create a new PatchramManager with paths to attack and stock HCD files.
    pub fn new(attack_hcd: String, stock_hcd: String) -> Self {
        Self {
            state: PatchramState::Unloaded,
            attack_hcd,
            stock_hcd,
            last_error: None,
        }
    }

    /// Load the attack HCD firmware.
    #[cfg(target_os = "linux")]
    pub fn load_attack(&mut self) -> Result<(), String> {
        info!("patchram: loading attack HCD: {}", self.attack_hcd);
        hcd::validate_hcd(&self.attack_hcd).map_err(|e| {
            self.state = PatchramState::Error;
            self.last_error = Some(e.clone());
            e
        })?;
        self.load_hcd(&self.attack_hcd.clone(), PatchramState::Attack)
    }

    /// Load the stock HCD firmware.
    #[cfg(target_os = "linux")]
    pub fn load_stock(&mut self) -> Result<(), String> {
        info!("patchram: loading stock HCD");
        // For stock, restore from backup if available
        let stock_src = if std::path::Path::new(hcd::FIRMWARE_BRCM_BACKUP).exists() {
            hcd::FIRMWARE_BRCM_BACKUP.to_string()
        } else {
            self.stock_hcd.clone()
        };
        hcd::validate_hcd(&stock_src).map_err(|e| {
            self.state = PatchramState::Error;
            self.last_error = Some(e.clone());
            e
        })?;
        self.load_hcd(&stock_src, PatchramState::Stock)
    }

    /// Unload: rmmod hci_uart, restore stock firmware.
    #[cfg(target_os = "linux")]
    pub fn unload(&mut self) -> Result<(), String> {
        info!("patchram: unloading");
        // Stop bluetooth service first (with timeout to avoid 90s hang)
        stop_bluetooth_service();
        // Unload kernel module (releases UART, 5s timeout to prevent deadlock)
        if let Err(e) = run_cmd("timeout", &["5", "rmmod", "hci_uart"]) {
            warn!("patchram: rmmod hci_uart failed or timed out during unload: {e}");
        }
        // Restore stock firmware from backups
        restore_stock_firmware();
        self.state = PatchramState::Unloaded;
        self.last_error = None;
        info!("patchram: unloaded");
        Ok(())
    }

    /// Internal: perform the full load sequence for a given HCD path.
    ///
    /// Sequence:
    /// 1. Stop bluetooth service (5s timeout, force-kill if hung)
    /// 2. rmmod hci_uart (releases UART)
    /// 3. Backup stock firmware (first time only)
    /// 4. Copy HCD to BOTH firmware paths (btbcm checks both)
    /// 5. modprobe hci_uart (kernel serdev triggers btbcm firmware load)
    /// 6. Wait 4s for hci0 registration + firmware load
    /// 7. hciconfig hci0 up
    #[cfg(target_os = "linux")]
    fn load_hcd(&mut self, hcd_path: &str, target_state: PatchramState) -> Result<(), String> {
        // Step 1: Stop bluetooth service (with timeout to avoid 90s hang)
        stop_bluetooth_service();

        // Step 2: Unload kernel module (5s timeout to prevent deadlock if UART held)
        info!("patchram: rmmod hci_uart (5s timeout)");
        if let Err(e) = run_cmd("timeout", &["5", "rmmod", "hci_uart"]) {
            warn!("patchram: rmmod hci_uart failed or timed out (ok if not loaded): {e}");
        }
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Step 3: Backup stock firmware (first time only, no-clobber)
        backup_stock_firmware();

        // Step 4: Copy HCD to both firmware search paths
        info!("patchram: deploying {} to firmware paths", hcd_path);
        for dest in &[hcd::FIRMWARE_BRCM, hcd::FIRMWARE_SYNAPTICS] {
            if let Some(parent) = std::path::Path::new(dest).parent() {
                if parent.exists() {
                    std::fs::copy(hcd_path, dest).map_err(|e| {
                        let msg = format!("failed to copy HCD to {dest}: {e}");
                        self.state = PatchramState::Error;
                        self.last_error = Some(msg.clone());
                        msg
                    })?;
                    info!("patchram: copied -> {dest}");
                }
            }
        }

        // Step 5: Reload kernel module (triggers btbcm firmware load)
        info!("patchram: modprobe hci_uart");
        run_cmd("modprobe", &["hci_uart"]).map_err(|e| {
            let msg = format!("modprobe hci_uart failed: {e}");
            self.state = PatchramState::Error;
            self.last_error = Some(msg.clone());
            msg
        })?;

        // Step 6: Wait for hci0 registration + firmware load
        info!("patchram: waiting 4s for firmware load...");
        std::thread::sleep(std::time::Duration::from_secs(4));

        // Step 7: Bring hci0 up
        run_cmd("hciconfig", &["hci0", "up"]).map_err(|e| {
            let msg = format!("hciconfig hci0 up failed: {e}");
            self.state = PatchramState::Error;
            self.last_error = Some(msg.clone());
            msg
        })?;

        self.state = target_state;
        self.last_error = None;
        info!("patchram: loaded {} (state={})", hcd_path, target_state.as_str());
        Ok(())
    }

    // --- Non-Linux stubs ---

    #[cfg(not(target_os = "linux"))]
    pub fn load_attack(&mut self) -> Result<(), String> {
        info!("patchram: (stub) load_attack → Attack");
        self.state = PatchramState::Attack;
        self.last_error = None;
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn load_stock(&mut self) -> Result<(), String> {
        info!("patchram: (stub) load_stock → Stock");
        self.state = PatchramState::Stock;
        self.last_error = None;
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn unload(&mut self) -> Result<(), String> {
        info!("patchram: (stub) unload → Unloaded");
        self.state = PatchramState::Unloaded;
        self.last_error = None;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Shell command wrappers (Linux only)
// ---------------------------------------------------------------------------

/// Run a command with args, return stdout on success or stderr on failure.
#[cfg(target_os = "linux")]
fn run_cmd(cmd: &str, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run {cmd}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Stop the bluetooth service with a 5-second timeout.
///
/// `systemctl stop bluetooth` can hang for ~90 seconds when the service is
/// actively managing an HCI device. We use `timeout 5` to cap the wait, then
/// fall back to `systemctl kill bluetooth` (SIGKILL) if the graceful stop
/// didn't finish in time.
#[cfg(target_os = "linux")]
fn stop_bluetooth_service() {
    info!("patchram: stopping bluetooth service (5s timeout)");
    match run_cmd("timeout", &["5", "systemctl", "stop", "bluetooth"]) {
        Ok(_) => info!("patchram: bluetooth service stopped"),
        Err(e) => {
            warn!("patchram: systemctl stop bluetooth timed out or failed: {e}");
            info!("patchram: force-killing bluetooth service");
            if let Err(e2) = run_cmd("systemctl", &["kill", "bluetooth"]) {
                warn!("patchram: systemctl kill bluetooth failed: {e2}");
            }
        }
    }
}

/// Backup stock firmware files (no-clobber: skip if backup exists).
#[cfg(target_os = "linux")]
fn backup_stock_firmware() {
    for (src, bak) in &[
        (hcd::FIRMWARE_BRCM, hcd::FIRMWARE_BRCM_BACKUP),
        (hcd::FIRMWARE_SYNAPTICS, hcd::FIRMWARE_SYNAPTICS_BACKUP),
    ] {
        if std::path::Path::new(src).exists() && !std::path::Path::new(bak).exists() {
            info!("patchram: backing up {src} -> {bak}");
            if let Err(e) = std::fs::copy(src, bak) {
                warn!("patchram: backup failed {src}: {e}");
            }
        }
    }
}

/// Restore stock firmware from backups.
#[cfg(target_os = "linux")]
fn restore_stock_firmware() {
    for (bak, dst) in &[
        (hcd::FIRMWARE_BRCM_BACKUP, hcd::FIRMWARE_BRCM),
        (hcd::FIRMWARE_SYNAPTICS_BACKUP, hcd::FIRMWARE_SYNAPTICS),
    ] {
        if std::path::Path::new(bak).exists() {
            info!("patchram: restoring {bak} -> {dst}");
            if let Err(e) = std::fs::copy(bak, dst) {
                warn!("patchram: restore failed {dst}: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_as_str() {
        assert_eq!(PatchramState::Unloaded.as_str(), "unloaded");
        assert_eq!(PatchramState::Stock.as_str(), "stock");
        assert_eq!(PatchramState::Attack.as_str(), "attack");
        assert_eq!(PatchramState::Error.as_str(), "error");
    }

    #[test]
    fn test_new_defaults() {
        let mgr = PatchramManager::new("/fw/attack.hcd".into(), "/fw/stock.hcd".into());
        assert_eq!(mgr.state, PatchramState::Unloaded);
        assert_eq!(mgr.attack_hcd, "/fw/attack.hcd");
        assert_eq!(mgr.stock_hcd, "/fw/stock.hcd");
        assert!(mgr.last_error.is_none());
    }

    #[test]
    fn test_stub_load_attack() {
        let mut mgr = PatchramManager::new("a.hcd".into(), "s.hcd".into());
        let result = mgr.load_attack();
        #[cfg(not(target_os = "linux"))]
        {
            assert!(result.is_ok());
            assert_eq!(mgr.state, PatchramState::Attack);
        }
        #[cfg(target_os = "linux")]
        {
            let _ = result;
        }
    }

    #[test]
    fn test_stub_load_stock() {
        let mut mgr = PatchramManager::new("a.hcd".into(), "s.hcd".into());
        let result = mgr.load_stock();
        #[cfg(not(target_os = "linux"))]
        {
            assert!(result.is_ok());
            assert_eq!(mgr.state, PatchramState::Stock);
        }
        #[cfg(target_os = "linux")]
        {
            let _ = result;
        }
    }

    #[test]
    fn test_stub_unload() {
        let mut mgr = PatchramManager::new("a.hcd".into(), "s.hcd".into());
        let result = mgr.unload();
        #[cfg(not(target_os = "linux"))]
        {
            assert!(result.is_ok());
            assert_eq!(mgr.state, PatchramState::Unloaded);
        }
        #[cfg(target_os = "linux")]
        {
            let _ = result;
        }
    }
}
