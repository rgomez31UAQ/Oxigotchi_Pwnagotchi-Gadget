//! Bluetooth PAN tethering module.
//!
//! Manages Bluetooth Personal Area Network (PAN) connections for
//! internet tethering via a paired phone.
//!
//! Uses `nmcli` and `bluetoothctl` CLI tools (no D-Bus crate needed).
//! All Command calls are `#[cfg(unix)]` gated.

use std::time::Instant;

/// Default nmcli connection profile name.
pub const DEFAULT_CONNECTION_NAME: &str = "Xiaomi 13T Network";
/// The bnep0 interface sysfs path for status checking.
pub const BNEP0_SYSFS_PATH: &str = "/sys/class/net/bnep0";

/// Bluetooth connection states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtState {
    /// Bluetooth is off or adapter not found.
    Off,
    /// Adapter is on but not connected.
    Disconnected,
    /// Attempting to connect to a paired device.
    Connecting,
    /// Connected via PAN, internet available.
    Connected,
    /// Connection failed, will retry.
    Error,
}

/// Configuration for Bluetooth tethering.
#[derive(Debug, Clone)]
pub struct BtConfig {
    /// MAC address of the paired phone.
    pub phone_mac: String,
    /// nmcli connection profile name.
    pub connection_name: String,
    /// Whether to auto-connect on boot.
    pub auto_connect: bool,
    /// Retry interval in seconds on connection failure.
    pub retry_interval_secs: u64,
    /// Maximum retry attempts before giving up (0 = unlimited).
    pub max_retries: u32,
}

impl Default for BtConfig {
    fn default() -> Self {
        Self {
            phone_mac: String::new(),
            connection_name: DEFAULT_CONNECTION_NAME.to_string(),
            auto_connect: false,
            retry_interval_secs: 30,
            max_retries: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Command builders (pure functions, testable on any platform)
// ---------------------------------------------------------------------------

/// Build the nmcli command args to bring up a connection.
pub fn build_connect_args(connection_name: &str) -> Vec<String> {
    vec![
        "connection".to_string(),
        "up".to_string(),
        connection_name.to_string(),
    ]
}

/// Build the nmcli command args to bring down a connection.
pub fn build_disconnect_nmcli_args(connection_name: &str) -> Vec<String> {
    vec![
        "connection".to_string(),
        "down".to_string(),
        connection_name.to_string(),
    ]
}

/// Build the bluetoothctl command args to disconnect a device.
pub fn build_disconnect_bt_args(mac: &str) -> Vec<String> {
    vec!["disconnect".to_string(), mac.to_string()]
}

/// Build the ip command args to get the IPv4 address of bnep0.
pub fn build_ip_addr_args() -> Vec<String> {
    vec![
        "-4".to_string(),
        "addr".to_string(),
        "show".to_string(),
        "bnep0".to_string(),
    ]
}

/// Parse an IPv4 address from `ip -4 addr show bnep0` output.
///
/// Looks for a line like `inet 192.168.44.128/24 ...` and extracts the IP.
pub fn parse_ip_from_output(output: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("inet ") {
            if let Some(addr_cidr) = rest.split_whitespace().next() {
                if let Some(addr) = addr_cidr.split('/').next() {
                    return Some(addr.to_string());
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// System command execution (unix-only)
// ---------------------------------------------------------------------------

/// Run nmcli with the given arguments. Returns Ok(stdout) or Err(stderr).
#[cfg(unix)]
fn run_nmcli(args: &[String]) -> Result<String, String> {
    let output = std::process::Command::new("nmcli")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run nmcli: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Run bluetoothctl with the given arguments. Returns Ok(stdout) or Err(stderr).
#[cfg(unix)]
fn run_bluetoothctl(args: &[String]) -> Result<String, String> {
    let output = std::process::Command::new("bluetoothctl")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run bluetoothctl: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Run `ip` with the given arguments. Returns Ok(stdout) or Err(stderr).
#[cfg(unix)]
fn run_ip(args: &[String]) -> Result<String, String> {
    let output = std::process::Command::new("ip")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run ip: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Check if the bnep0 interface exists (unix-only).
#[cfg(unix)]
fn bnep0_exists() -> bool {
    std::path::Path::new(BNEP0_SYSFS_PATH).exists()
}

/// Stub: bnep0 never exists on non-unix.
#[cfg(not(unix))]
fn bnep0_exists() -> bool {
    false
}

// ---------------------------------------------------------------------------
// BtTether manager
// ---------------------------------------------------------------------------

/// Bluetooth tether manager.
pub struct BtTether {
    pub state: BtState,
    pub config: BtConfig,
    pub retry_count: u32,
    pub last_attempt: Option<Instant>,
    /// Whether internet is reachable through the BT connection.
    pub internet_available: bool,
    /// IP address obtained on bnep0, if any.
    pub ip_address: Option<String>,
}

impl BtTether {
    /// Create a new Bluetooth tether manager with the given configuration.
    pub fn new(config: BtConfig) -> Self {
        Self {
            state: BtState::Off,
            config,
            retry_count: 0,
            last_attempt: None,
            internet_available: false,
            ip_address: None,
        }
    }

    /// Check if we should attempt a connection.
    pub fn should_connect(&self) -> bool {
        if !self.config.auto_connect || self.config.phone_mac.is_empty() {
            return false;
        }
        match self.state {
            BtState::Off | BtState::Connecting | BtState::Connected => false,
            BtState::Disconnected => true,
            BtState::Error => {
                if self.config.max_retries > 0 && self.retry_count >= self.config.max_retries {
                    return false;
                }
                match self.last_attempt {
                    Some(t) => t.elapsed().as_secs() >= self.config.retry_interval_secs,
                    None => true,
                }
            }
        }
    }

    /// Initiate BT PAN connection via nmcli.
    pub fn connect(&mut self) -> Result<(), String> {
        if self.config.phone_mac.is_empty() {
            return Err("No phone MAC configured".into());
        }
        self.state = BtState::Connecting;
        self.last_attempt = Some(Instant::now());

        #[cfg(unix)]
        {
            let args = build_connect_args(&self.config.connection_name);
            match run_nmcli(&args) {
                Ok(_) => {
                    self.state = BtState::Connected;
                    self.retry_count = 0;
                    self.internet_available = true;
                    self.refresh_ip();
                    return Ok(());
                }
                Err(e) => {
                    self.on_error();
                    return Err(format!("nmcli connection up failed: {e}"));
                }
            }
        }

        #[cfg(not(unix))]
        {
            // Non-unix stub: simulate successful connection
            self.state = BtState::Connected;
            self.retry_count = 0;
            self.internet_available = true;
            Ok(())
        }
    }

    /// Disconnect BT PAN via nmcli + bluetoothctl.
    pub fn disconnect(&mut self) {
        #[cfg(unix)]
        {
            let nmcli_args = build_disconnect_nmcli_args(&self.config.connection_name);
            let _ = run_nmcli(&nmcli_args);
            if !self.config.phone_mac.is_empty() {
                let bt_args = build_disconnect_bt_args(&self.config.phone_mac);
                let _ = run_bluetoothctl(&bt_args);
            }
        }
        self.state = BtState::Disconnected;
        self.internet_available = false;
        self.ip_address = None;
    }

    /// Handle a connection failure.
    pub fn on_error(&mut self) {
        self.state = BtState::Error;
        self.retry_count += 1;
        self.internet_available = false;
        self.ip_address = None;
    }

    /// Check connection status by probing bnep0 interface.
    pub fn check_status(&mut self) -> BtState {
        if bnep0_exists() {
            if self.state != BtState::Connected {
                self.state = BtState::Connected;
                self.internet_available = true;
                self.refresh_ip();
            }
        } else if self.state == BtState::Connected {
            self.state = BtState::Disconnected;
            self.internet_available = false;
            self.ip_address = None;
        }
        self.state
    }

    /// Refresh the IP address from bnep0.
    pub fn refresh_ip(&mut self) {
        #[cfg(unix)]
        {
            let args = build_ip_addr_args();
            if let Ok(output) = run_ip(&args) {
                self.ip_address = parse_ip_from_output(&output);
            }
        }
    }

    /// Get the current IP address, if connected.
    pub fn get_ip(&self) -> Option<&str> {
        self.ip_address.as_deref()
    }

    /// Status string for display (full form).
    pub fn status_str(&self) -> &'static str {
        match self.state {
            BtState::Off => "BT OFF",
            BtState::Disconnected => "BT DISC",
            BtState::Connecting => "BT ...",
            BtState::Connected => "BT OK",
            BtState::Error => "BT ERR",
        }
    }

    /// Short status for the top bar (matches Python "BT C" / "BT -" format).
    pub fn status_short(&self) -> &'static str {
        match self.state {
            BtState::Off => "-",
            BtState::Disconnected => "-",
            BtState::Connecting => ".",
            BtState::Connected => "C",
            BtState::Error => "!",
        }
    }

    /// Toggle connection on/off (called from button handler).
    pub fn toggle(&mut self) {
        match self.state {
            BtState::Connected => self.disconnect(),
            BtState::Off | BtState::Disconnected | BtState::Error => {
                let _ = self.connect();
            }
            BtState::Connecting => {} // ignore during connection
        }
    }
}

impl Default for BtTether {
    fn default() -> Self {
        Self::new(BtConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Command builder tests =====

    #[test]
    fn test_build_connect_args() {
        let args = build_connect_args("Xiaomi 13T Network");
        assert_eq!(args, vec!["connection", "up", "Xiaomi 13T Network"]);
    }

    #[test]
    fn test_build_connect_args_custom_name() {
        let args = build_connect_args("My Phone PAN");
        assert_eq!(args, vec!["connection", "up", "My Phone PAN"]);
    }

    #[test]
    fn test_build_disconnect_nmcli_args() {
        let args = build_disconnect_nmcli_args("Xiaomi 13T Network");
        assert_eq!(args, vec!["connection", "down", "Xiaomi 13T Network"]);
    }

    #[test]
    fn test_build_disconnect_bt_args() {
        let args = build_disconnect_bt_args("AA:BB:CC:DD:EE:FF");
        assert_eq!(args, vec!["disconnect", "AA:BB:CC:DD:EE:FF"]);
    }

    #[test]
    fn test_build_ip_addr_args() {
        let args = build_ip_addr_args();
        assert_eq!(args, vec!["-4", "addr", "show", "bnep0"]);
    }

    // ===== IP parsing tests =====

    #[test]
    fn test_parse_ip_typical_output() {
        let output = r#"4: bnep0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500
    inet 192.168.44.128/24 brd 192.168.44.255 scope global dynamic bnep0
       valid_lft 3599sec preferred_lft 3599sec"#;
        assert_eq!(
            parse_ip_from_output(output),
            Some("192.168.44.128".to_string())
        );
    }

    #[test]
    fn test_parse_ip_no_inet_line() {
        let output = "4: bnep0: <BROADCAST,MULTICAST> mtu 1500\n    link/ether ...\n";
        assert_eq!(parse_ip_from_output(output), None);
    }

    #[test]
    fn test_parse_ip_empty_output() {
        assert_eq!(parse_ip_from_output(""), None);
    }

    #[test]
    fn test_parse_ip_multiple_interfaces() {
        let output =
            "    inet 10.0.0.1/8 brd 10.255.255.255 scope global\n    inet 192.168.1.100/24 scope global\n";
        assert_eq!(parse_ip_from_output(output), Some("10.0.0.1".to_string()));
    }

    #[test]
    fn test_parse_ip_with_cidr_stripped() {
        let output = "    inet 172.16.0.5/16 brd 172.16.255.255\n";
        let ip = parse_ip_from_output(output);
        assert_eq!(ip, Some("172.16.0.5".to_string()));
    }

    // ===== State machine tests =====

    #[test]
    fn test_default_state() {
        let bt = BtTether::default();
        assert_eq!(bt.state, BtState::Off);
        assert!(!bt.internet_available);
        assert!(bt.ip_address.is_none());
    }

    #[test]
    fn test_connect_requires_mac() {
        let mut bt = BtTether::default();
        assert!(bt.connect().is_err());
    }

    #[test]
    fn test_connect_success_non_unix() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            auto_connect: true,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Disconnected;
        assert!(bt.connect().is_ok());
        assert_eq!(bt.state, BtState::Connected);
        assert!(bt.internet_available);
    }

    #[test]
    fn test_disconnect() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Connected;
        bt.internet_available = true;
        bt.ip_address = Some("192.168.44.1".into());
        bt.disconnect();
        assert_eq!(bt.state, BtState::Disconnected);
        assert!(!bt.internet_available);
        assert!(bt.ip_address.is_none());
    }

    #[test]
    fn test_should_connect_auto_off() {
        let bt = BtTether::default();
        assert!(!bt.should_connect());
    }

    #[test]
    fn test_should_connect_no_mac() {
        let config = BtConfig {
            auto_connect: true,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Disconnected;
        assert!(!bt.should_connect());
    }

    #[test]
    fn test_should_connect_disconnected() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            auto_connect: true,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Disconnected;
        assert!(bt.should_connect());
    }

    #[test]
    fn test_should_connect_already_connected() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            auto_connect: true,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Connected;
        assert!(!bt.should_connect());
    }

    #[test]
    fn test_should_connect_while_connecting() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            auto_connect: true,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Connecting;
        assert!(!bt.should_connect());
    }

    #[test]
    fn test_error_retry_limit() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            auto_connect: true,
            max_retries: 3,
            retry_interval_secs: 0,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Error;
        bt.retry_count = 3;
        assert!(!bt.should_connect());
    }

    #[test]
    fn test_error_retry_not_yet_exhausted() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            auto_connect: true,
            max_retries: 3,
            retry_interval_secs: 0,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Error;
        bt.retry_count = 2;
        bt.last_attempt = Some(Instant::now() - std::time::Duration::from_secs(1));
        assert!(bt.should_connect());
    }

    #[test]
    fn test_error_unlimited_retries() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            auto_connect: true,
            max_retries: 0,
            retry_interval_secs: 0,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Error;
        bt.retry_count = 1000;
        bt.last_attempt = Some(Instant::now() - std::time::Duration::from_secs(1));
        assert!(bt.should_connect());
    }

    #[test]
    fn test_error_retry_interval_not_elapsed() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            auto_connect: true,
            max_retries: 0,
            retry_interval_secs: 30,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Error;
        bt.retry_count = 1;
        bt.last_attempt = Some(Instant::now());
        assert!(!bt.should_connect());
    }

    #[test]
    fn test_on_error() {
        let mut bt = BtTether::default();
        bt.state = BtState::Connecting;
        bt.internet_available = true;
        bt.ip_address = Some("1.2.3.4".into());
        bt.on_error();
        assert_eq!(bt.state, BtState::Error);
        assert_eq!(bt.retry_count, 1);
        assert!(!bt.internet_available);
        assert!(bt.ip_address.is_none());
    }

    #[test]
    fn test_on_error_increments() {
        let mut bt = BtTether::default();
        bt.on_error();
        bt.on_error();
        bt.on_error();
        assert_eq!(bt.retry_count, 3);
    }

    #[test]
    fn test_status_strings() {
        let mut bt = BtTether::default();
        assert_eq!(bt.status_str(), "BT OFF");
        bt.state = BtState::Connected;
        assert_eq!(bt.status_str(), "BT OK");
        bt.state = BtState::Error;
        assert_eq!(bt.status_str(), "BT ERR");
        bt.state = BtState::Disconnected;
        assert_eq!(bt.status_str(), "BT DISC");
        bt.state = BtState::Connecting;
        assert_eq!(bt.status_str(), "BT ...");
    }

    #[test]
    fn test_status_short() {
        let mut bt = BtTether::default();
        assert_eq!(bt.status_short(), "-");
        bt.state = BtState::Connected;
        assert_eq!(bt.status_short(), "C");
        bt.state = BtState::Connecting;
        assert_eq!(bt.status_short(), ".");
        bt.state = BtState::Error;
        assert_eq!(bt.status_short(), "!");
        bt.state = BtState::Disconnected;
        assert_eq!(bt.status_short(), "-");
    }

    // ===== Toggle state machine tests =====

    #[test]
    fn test_toggle_connect_disconnect() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Disconnected;
        bt.toggle();
        assert_eq!(bt.state, BtState::Connected);
        bt.toggle();
        assert_eq!(bt.state, BtState::Disconnected);
    }

    #[test]
    fn test_toggle_from_off() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        assert_eq!(bt.state, BtState::Off);
        bt.toggle();
        assert_eq!(bt.state, BtState::Connected);
    }

    #[test]
    fn test_toggle_from_error() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Error;
        bt.retry_count = 5;
        bt.toggle();
        assert_eq!(bt.state, BtState::Connected);
        assert_eq!(bt.retry_count, 0);
    }

    #[test]
    fn test_toggle_ignored_while_connecting() {
        let config = BtConfig {
            phone_mac: "AA:BB:CC:DD:EE:FF".into(),
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Connecting;
        bt.toggle();
        assert_eq!(bt.state, BtState::Connecting);
    }

    // ===== Status detection tests =====

    #[test]
    fn test_check_status_no_bnep0() {
        let mut bt = BtTether::default();
        bt.state = BtState::Connected;
        bt.internet_available = true;
        let state = bt.check_status();
        assert_eq!(state, BtState::Disconnected);
        assert!(!bt.internet_available);
    }

    #[test]
    fn test_check_status_from_disconnected() {
        let mut bt = BtTether::default();
        bt.state = BtState::Disconnected;
        let state = bt.check_status();
        assert_eq!(state, BtState::Disconnected);
    }

    // ===== Config default tests =====

    #[test]
    fn test_config_defaults() {
        let cfg = BtConfig::default();
        assert!(cfg.phone_mac.is_empty());
        assert_eq!(cfg.connection_name, "Xiaomi 13T Network");
        assert!(!cfg.auto_connect);
        assert_eq!(cfg.retry_interval_secs, 30);
        assert_eq!(cfg.max_retries, 0);
    }

    // ===== IP getter tests =====

    #[test]
    fn test_get_ip_none() {
        let bt = BtTether::default();
        assert!(bt.get_ip().is_none());
    }

    #[test]
    fn test_get_ip_some() {
        let mut bt = BtTether::default();
        bt.ip_address = Some("192.168.44.128".into());
        assert_eq!(bt.get_ip(), Some("192.168.44.128"));
    }

    // ===== Constants tests =====

    #[test]
    fn test_constants() {
        assert_eq!(DEFAULT_CONNECTION_NAME, "Xiaomi 13T Network");
        assert_eq!(BNEP0_SYSFS_PATH, "/sys/class/net/bnep0");
    }
}
