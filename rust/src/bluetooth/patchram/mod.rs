//! PatchramManager — loads/unloads attack and stock HCD firmware on BT UART.
//!
//! The BCM43436B0 BT chip loads firmware via HCD files through `hciattach`
//! on `/dev/ttyS0`. This module manages switching between the stock HCD
//! (for normal BT tethering) and the attack HCD (for BT offensive mode).

pub mod hcd;

use log::info;
#[cfg(target_os = "linux")]
use log::warn;

/// Current state of the patchram firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchramState {
    /// No HCD loaded, hciattach not running.
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

/// Manages loading and unloading HCD firmware on the BT UART.
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
    ///
    /// Sequence: validate HCD → hci0 down → kill hciattach → hciattach with
    /// attack HCD → sleep 1s → hci0 up.
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
    ///
    /// Sequence: validate HCD → hci0 down → kill hciattach → hciattach with
    /// stock HCD → sleep 1s → hci0 up.
    #[cfg(target_os = "linux")]
    pub fn load_stock(&mut self) -> Result<(), String> {
        info!("patchram: loading stock HCD: {}", self.stock_hcd);
        hcd::validate_hcd(&self.stock_hcd).map_err(|e| {
            self.state = PatchramState::Error;
            self.last_error = Some(e.clone());
            e
        })?;
        self.load_hcd(&self.stock_hcd.clone(), PatchramState::Stock)
    }

    /// Unload: hci0 down, kill hciattach.
    #[cfg(target_os = "linux")]
    pub fn unload(&mut self) -> Result<(), String> {
        info!("patchram: unloading");
        run_hciconfig(&hcd::build_hci_down_args())?;
        run_killall("hciattach")?;
        self.state = PatchramState::Unloaded;
        self.last_error = None;
        info!("patchram: unloaded");
        Ok(())
    }

    /// Internal: perform the full load sequence for a given HCD path.
    #[cfg(target_os = "linux")]
    fn load_hcd(&mut self, hcd_path: &str, target_state: PatchramState) -> Result<(), String> {
        // Bring hci0 down (ignore errors — may not be up yet)
        if let Err(e) = run_hciconfig(&hcd::build_hci_down_args()) {
            warn!("patchram: hci0 down failed (ok if not up): {e}");
        }

        // Kill any existing hciattach (ignore errors — may not be running)
        if let Err(e) = run_killall("hciattach") {
            warn!("patchram: killall hciattach failed (ok if not running): {e}");
        }

        // Start hciattach with the HCD
        let args = hcd::build_hciattach_args(hcd_path);
        run_hciattach(&args).map_err(|e| {
            self.state = PatchramState::Error;
            self.last_error = Some(e.clone());
            e
        })?;

        // Give firmware time to load
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Bring hci0 up
        run_hciconfig(&hcd::build_hci_up_args()).map_err(|e| {
            self.state = PatchramState::Error;
            self.last_error = Some(e.clone());
            e
        })?;

        self.state = target_state;
        self.last_error = None;
        info!("patchram: loaded {} (state={})", hcd_path, target_state.as_str());
        Ok(())
    }

    // --- Non-Linux stubs ---

    /// Stub: load attack HCD on non-Linux (just updates state).
    #[cfg(not(target_os = "linux"))]
    pub fn load_attack(&mut self) -> Result<(), String> {
        info!("patchram: (stub) load_attack → Attack");
        self.state = PatchramState::Attack;
        self.last_error = None;
        Ok(())
    }

    /// Stub: load stock HCD on non-Linux (just updates state).
    #[cfg(not(target_os = "linux"))]
    pub fn load_stock(&mut self) -> Result<(), String> {
        info!("patchram: (stub) load_stock → Stock");
        self.state = PatchramState::Stock;
        self.last_error = None;
        Ok(())
    }

    /// Stub: unload on non-Linux (just updates state).
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

/// Run `hciconfig` with the given args.
#[cfg(target_os = "linux")]
fn run_hciconfig(args: &[String]) -> Result<String, String> {
    let output = std::process::Command::new("hciconfig")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run hciconfig: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Run `hciattach` with the given args.
#[cfg(target_os = "linux")]
fn run_hciattach(args: &[String]) -> Result<String, String> {
    let output = std::process::Command::new("hciattach")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run hciattach: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Run `killall <name>`.
#[cfg(target_os = "linux")]
fn run_killall(name: &str) -> Result<String, String> {
    let output = std::process::Command::new("killall")
        .arg(name)
        .output()
        .map_err(|e| format!("failed to run killall: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
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
        // On non-linux: Ok + state change. On linux: will fail (no hciconfig).
        // We only test the stub path in CI.
        #[cfg(not(target_os = "linux"))]
        {
            assert!(result.is_ok());
            assert_eq!(mgr.state, PatchramState::Attack);
        }
        #[cfg(target_os = "linux")]
        {
            let _ = result; // may fail on linux CI without bluetooth hardware
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
