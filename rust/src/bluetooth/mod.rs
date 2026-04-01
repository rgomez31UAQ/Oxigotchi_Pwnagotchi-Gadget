//! Bluetooth PAN tethering module.
//!
//! Manages Bluetooth Personal Area Network (PAN) connections for
//! internet tethering via a paired phone.
//!
//! Uses `nmcli` and `bluetoothctl` CLI tools (no D-Bus crate needed).
//! All Command calls are `#[cfg(unix)]` gated.

pub mod adapter;
pub mod attacks;
pub mod capture;
pub mod coex;
pub mod controller;
pub mod dbus;
pub mod discovery;
pub mod model;
pub mod patchram;
pub mod persistence;
pub mod supervisor;
pub mod ui;

use log::info;
use std::time::Instant;

/// Default nmcli connection profile name (empty = not configured).
pub const DEFAULT_CONNECTION_NAME: &str = "";
/// The bnep0 interface sysfs path for status checking.
pub const BNEP0_SYSFS_PATH: &str = "/sys/class/net/bnep0";

/// Bluetooth connection states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtState {
    /// Bluetooth is off or adapter not found.
    Off,
    /// Adapter is on but not connected.
    Disconnected,
    /// Pairing/trusting a device before connecting.
    Pairing,
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
    /// Whether Bluetooth tethering is enabled.
    pub enabled: bool,
    /// Display name of the phone (used for scan matching).
    pub phone_name: String,
    /// Whether to auto-connect on boot.
    pub auto_connect: bool,
    /// Whether to hide BT discoverability after connecting.
    pub hide_after_connect: bool,
}

impl Default for BtConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            phone_name: String::new(),
            auto_connect: true,
            hide_after_connect: true,
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

/// Build the bluetoothctl command args to power on the adapter.
pub fn build_power_on_args() -> Vec<String> {
    vec!["power".into(), "on".into()]
}

/// Build the bluetoothctl command args to power off the adapter.
pub fn build_power_off_args() -> Vec<String> {
    vec!["power".into(), "off".into()]
}

/// Build the bluetoothctl command args to enable the agent.
pub fn build_agent_on_args() -> Vec<String> {
    vec!["agent".into(), "on".into()]
}

/// Build the bluetoothctl command args to set the default agent.
pub fn build_default_agent_args() -> Vec<String> {
    vec!["default-agent".into()]
}

/// Build the bluetoothctl command args to pair with a device.
pub fn build_pair_args(mac: &str) -> Vec<String> {
    vec!["pair".into(), mac.into()]
}

/// Build the bluetoothctl command args to trust a device.
pub fn build_trust_args(mac: &str) -> Vec<String> {
    vec!["trust".into(), mac.into()]
}

/// Build the bluetoothctl command args to turn off discoverability.
pub fn build_discoverable_off_args() -> Vec<String> {
    vec!["discoverable".into(), "off".into()]
}

/// Build the bluetoothctl command args to turn on discoverability.
pub fn build_discoverable_on_args() -> Vec<String> {
    vec!["discoverable".into(), "on".into()]
}

/// Build the bluetoothctl command args to scan for devices (with timeout).
pub fn build_scan_on_args() -> Vec<String> {
    vec!["--timeout".into(), "10".into(), "scan".into(), "on".into()]
}

/// Build the nmcli command args to add a PAN bluetooth connection profile.
pub fn build_nmcli_add_pan_args(con_name: &str, mac: &str) -> Vec<String> {
    vec![
        "connection".into(),
        "add".into(),
        "type".into(),
        "bluetooth".into(),
        "con-name".into(),
        con_name.into(),
        "bt-type".into(),
        "panu".into(),
        "bluetooth.bdaddr".into(),
        mac.into(),
        "autoconnect".into(),
        "yes".into(),
    ]
}

/// Build the nmcli command args to show a connection profile.
pub fn build_nmcli_show_args(con_name: &str) -> Vec<String> {
    vec!["connection".into(), "show".into(), con_name.into()]
}

/// Strip ANSI escape codes from a string.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c.is_ascii_alphabetic() {
                in_escape = false; // end of escape sequence
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Parse `bluetoothctl scan on` output for a device matching name or MAC.
/// Handles ANSI color codes in bluetoothctl output.
/// Output format (after stripping): "[NEW] Device AA:BB:CC:DD:EE:FF Device Name"
/// Returns the MAC address if found.
pub fn parse_scan_for_device(output: &str, name: &str, mac: &str) -> Option<String> {
    let mut first_named: Option<String> = None;
    for raw_line in output.lines() {
        let line = strip_ansi(raw_line);
        if let Some(rest) = line.strip_prefix("[NEW] Device ") {
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if !parts.is_empty() {
                let found_mac = parts[0];
                let found_name = if parts.len() >= 2 { parts[1] } else { "" };

                // Match by MAC (case-insensitive)
                if !mac.is_empty() && found_mac.eq_ignore_ascii_case(mac) {
                    return Some(found_mac.to_string());
                }
                // Match by name (case-insensitive substring)
                if !name.is_empty() && found_name.to_lowercase().contains(&name.to_lowercase()) {
                    return Some(found_mac.to_string());
                }
                // Track first device with a real name (not just a MAC echo)
                // for auto-detect mode (both name and mac filters empty)
                if first_named.is_none() && !found_name.is_empty() && !found_name.contains('-')
                // skip "AA-BB-CC-DD-EE-FF" style names
                {
                    first_named = Some(found_mac.to_string());
                }
            }
        }
    }
    // Auto-detect: if no name/mac filter specified, return first named device
    if name.is_empty() && mac.is_empty() {
        return first_named;
    }
    None
}

/// Parse ALL discovered devices from bluetoothctl scan output.
/// Returns a list of (MAC, name) pairs.
pub fn parse_scan_all_devices(output: &str) -> Vec<(String, String)> {
    let mut devices = Vec::new();
    for raw_line in output.lines() {
        let line = strip_ansi(raw_line);
        if let Some(rest) = line.strip_prefix("[NEW] Device ") {
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if !parts.is_empty() {
                let mac = parts[0].to_string();
                let name = if parts.len() >= 2 {
                    parts[1].to_string()
                } else {
                    String::new()
                };
                // Skip entries with no name or where name looks like a MAC address
                if !name.is_empty() && name != mac && !is_mac_like(&name) {
                    devices.push((mac, name));
                }
            }
        }
    }
    devices
}

/// Check if a string looks like a MAC address (e.g., "AA-BB-CC-DD-EE-FF" or "AA:BB:CC:DD:EE:FF").
fn is_mac_like(s: &str) -> bool {
    // MAC addresses are 17 chars: XX:XX:XX:XX:XX:XX or XX-XX-XX-XX-XX-XX
    if s.len() != 17 {
        return false;
    }
    let sep = if s.contains(':') { ':' } else { '-' };
    s.split(sep).count() == 6
        && s.split(sep)
            .all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
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

/// Generate a deterministic nmcli connection profile name from a MAC address.
///
/// Strips non-hex characters and prefixes with "bt-pan-".
/// Example: "AA:BB:CC:DD:EE:FF" → "bt-pan-AABBCCDDEEFF"
pub fn generate_connection_name(mac: &str) -> String {
    let clean: String = mac.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    format!("bt-pan-{clean}")
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

/// Reload the hci_uart kernel module to reset the shared UART.
///
/// On BCM43436B0, WiFi monitor mode leaves the UART in a state where BT HCI
/// commands time out. Reloading hci_uart gives BT a clean UART connection.
#[cfg(unix)]
pub fn reset_hci_uart() {
    use log::{info, warn};
    info!("BT: reloading hci_uart to reset shared UART");
    let rmmod = std::process::Command::new("rmmod").arg("hci_uart").output();
    match rmmod {
        Ok(o) if o.status.success() => {}
        Ok(o) => warn!(
            "rmmod hci_uart: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => warn!("rmmod hci_uart failed: {e}"),
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
    let modprobe = std::process::Command::new("modprobe")
        .arg("hci_uart")
        .output();
    match modprobe {
        Ok(o) if o.status.success() => info!("BT: hci_uart reloaded"),
        Ok(o) => warn!(
            "modprobe hci_uart: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => warn!("modprobe hci_uart failed: {e}"),
    }
    // Wait for hci0 to re-register with the kernel
    std::thread::sleep(std::time::Duration::from_secs(4));
}

/// Stub for non-unix platforms.
#[cfg(not(unix))]
pub fn reset_hci_uart() {}

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
        if !self.config.auto_connect {
            return false;
        }
        match self.state {
            BtState::Off | BtState::Pairing | BtState::Connecting | BtState::Connected => false,
            BtState::Disconnected => true,
            BtState::Error => {
                match self.last_attempt {
                    Some(t) => t.elapsed().as_secs() >= 30,
                    None => true,
                }
            }
        }
    }

    /// Initiate BT PAN connection via nmcli.
    pub fn connect(&mut self) -> Result<(), String> {
        self.state = BtState::Connecting;
        self.last_attempt = Some(Instant::now());

        #[cfg(unix)]
        {
            // connection_name is resolved at pair_and_connect / setup time and stored in phone_name
            // For now use a placeholder; Task 6 will rewrite this flow with D-Bus.
            let args = build_connect_args("bt-pan");
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
            let nmcli_args = build_disconnect_nmcli_args("bt-pan");
            let _ = run_nmcli(&nmcli_args);
        }
        self.state = BtState::Disconnected;
        self.internet_available = false;
        self.ip_address = None;
    }

    /// Scan for nearby BT devices (blocking, ~10s). Returns list of (MAC, name).
    pub fn scan_devices(&self) -> Vec<(String, String)> {
        Self::scan_devices_static()
    }

    /// Static scan — can be called from a background thread without &self.
    pub fn scan_devices_static() -> Vec<(String, String)> {
        #[cfg(unix)]
        {
            log::info!("BT: scanning for devices (10s)...");
            let _ = run_bluetoothctl(&build_power_on_args());
            let _ = run_bluetoothctl(&build_agent_on_args());
            match run_bluetoothctl(&build_scan_on_args()) {
                Ok(output) => return parse_scan_all_devices(&output),
                Err(e) => {
                    log::error!("BT scan failed: {e}");
                    return Vec::new();
                }
            }
        }
        #[cfg(not(unix))]
        Vec::new()
    }

    /// Pair with a device by MAC, trust it, create nmcli profile, and connect.
    pub fn pair_and_connect(&mut self, mac: &str) -> Result<(), String> {
        #[cfg(unix)]
        {
            info!("BT: pairing with {mac}");
            self.state = BtState::Pairing;

            // Power on + agent
            let _ = run_bluetoothctl(&build_power_on_args());
            let _ = run_bluetoothctl(&build_agent_on_args());
            let _ = run_bluetoothctl(&build_default_agent_args());

            // Pair and trust
            let _ = run_bluetoothctl(&build_pair_args(mac));
            let _ = run_bluetoothctl(&build_trust_args(mac));

            // Ensure nmcli profile
            let con_name = generate_connection_name(mac);
            let profile_exists = run_nmcli(&build_nmcli_show_args(&con_name)).is_ok();
            if !profile_exists {
                info!("BT: creating nmcli profile '{con_name}'");
                let _ = run_nmcli(&build_nmcli_add_pan_args(&con_name, mac));
            }

            // Connect
            match self.connect() {
                Ok(()) => {
                    info!("BT: paired and connected to {mac}");
                    if self.config.hide_after_connect {
                        let _ = run_bluetoothctl(&build_discoverable_off_args());
                    }
                    Ok(())
                }
                Err(e) => {
                    self.state = BtState::Disconnected;
                    Err(format!("Paired but connect failed: {e}"))
                }
            }
        }
        #[cfg(not(unix))]
        {
            self.state = BtState::Connected;
            Ok(())
        }
    }

    /// Make BT adapter discoverable (visible to other devices).
    pub fn show(&mut self) {
        #[cfg(unix)]
        {
            // Disable discoverable timeout so it stays visible until explicitly hidden
            let _ = run_bluetoothctl(&["discoverable-timeout".into(), "0".into()]);
            let _ = run_bluetoothctl(&build_discoverable_on_args());
            // Also enable pairable so phones can initiate connections
            let _ = run_bluetoothctl(&["pairable".into(), "on".into()]);
        }
        info!("BT discoverable + pairable ON");
    }

    /// Hide BT adapter (turn off discoverability).
    pub fn hide(&mut self) {
        #[cfg(unix)]
        {
            let _ = run_bluetoothctl(&build_discoverable_off_args());
        }
        info!("BT discoverable OFF");
    }

    /// Power off the BT adapter to free the radio for WiFi monitor mode.
    pub fn power_off(&mut self) {
        self.disconnect();
        #[cfg(unix)]
        {
            let _ = run_bluetoothctl(&build_power_off_args());
        }
        self.state = BtState::Disconnected;
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
            BtState::Pairing => "BT PAIR",
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
            BtState::Pairing => "P",
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
            BtState::Pairing | BtState::Connecting => {} // ignore during pairing/connection
        }
    }

    /// Boot-time setup: power on adapter, scan for device, pair, create nmcli profile, connect.
    ///
    /// Does nothing and returns `Ok(())` if `config.enabled` is false.
    pub fn setup(&mut self) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }

        #[cfg(unix)]
        {
            use log::{info, warn};

            // 1. Power on
            info!("BT: powering on adapter");
            let _ = run_bluetoothctl(&build_power_on_args());
            let _ = run_bluetoothctl(&build_agent_on_args());
            let _ = run_bluetoothctl(&build_default_agent_args());

            // 2. Scan for device by name
            info!("BT: scanning for devices (10s)...");
            self.state = BtState::Pairing;
            let mac = match run_bluetoothctl(&build_scan_on_args()) {
                Ok(output) => {
                    match parse_scan_for_device(&output, &self.config.phone_name, "") {
                        Some(found) => {
                            info!("BT: found device {found}");
                            found
                        }
                        None => {
                            self.state = BtState::Disconnected;
                            return Err("No matching device found during scan".into());
                        }
                    }
                }
                Err(e) => {
                    self.state = BtState::Disconnected;
                    return Err(format!("BT scan failed: {e}"));
                }
            };

            // 3. Pair and trust
            info!("BT: pairing with {mac}");
            self.state = BtState::Pairing;
            let _ = run_bluetoothctl(&build_pair_args(&mac));
            let _ = run_bluetoothctl(&build_trust_args(&mac));

            // 4. Ensure nmcli profile exists
            let con_name = generate_connection_name(&mac);
            let profile_exists = run_nmcli(&build_nmcli_show_args(&con_name)).is_ok();
            if !profile_exists {
                info!("BT: creating nmcli profile '{con_name}'");
                match run_nmcli(&build_nmcli_add_pan_args(&con_name, &mac)) {
                    Ok(_) => info!("BT: profile created"),
                    Err(e) => warn!("BT: profile creation failed: {e}"),
                }
            }

            // 5. Connect
            info!("BT: connecting");
            match self.connect() {
                Ok(()) => info!("BT: connected, IP: {:?}", self.ip_address),
                Err(e) => {
                    warn!("BT: initial connect failed: {e}");
                    self.state = BtState::Disconnected;
                }
            }

            // 6. Hide visibility
            if self.config.hide_after_connect {
                info!("BT: hiding visibility");
                let _ = run_bluetoothctl(&build_discoverable_off_args());
            }
        }

        Ok(())
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
        let args = build_connect_args("My Phone PAN");
        assert_eq!(args, vec!["connection", "up", "My Phone PAN"]);
    }

    #[test]
    fn test_build_connect_args_custom_name() {
        let args = build_connect_args("My Phone PAN");
        assert_eq!(args, vec!["connection", "up", "My Phone PAN"]);
    }

    #[test]
    fn test_build_disconnect_nmcli_args() {
        let args = build_disconnect_nmcli_args("My Phone PAN");
        assert_eq!(args, vec!["connection", "down", "My Phone PAN"]);
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
        let output = "    inet 10.0.0.1/8 brd 10.255.255.255 scope global\n    inet 192.168.1.100/24 scope global\n";
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
    fn test_connect_non_unix_stub() {
        // On non-unix the stub always succeeds (no MAC needed)
        #[cfg(not(unix))]
        {
            let mut bt = BtTether::default();
            assert!(bt.connect().is_ok());
            assert_eq!(bt.state, BtState::Connected);
        }
        #[cfg(unix)]
        {
            // On unix without nmcli available, connect will error — that's fine
        }
    }

    #[test]
    fn test_connect_success_non_unix() {
        // Skip on non-Pi Linux — nmcli/bluetoothctl not available in WSL
        if cfg!(unix)
            && std::process::Command::new("nmcli")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_err()
        {
            return;
        }
        let config = BtConfig {
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
    fn test_should_connect_disconnected() {
        let config = BtConfig {
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
            auto_connect: true,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Connecting;
        assert!(!bt.should_connect());
    }

    #[test]
    fn test_error_retry_interval_elapsed() {
        let config = BtConfig {
            auto_connect: true,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Error;
        bt.retry_count = 2;
        bt.last_attempt = Some(Instant::now() - std::time::Duration::from_secs(31));
        assert!(bt.should_connect());
    }

    #[test]
    fn test_error_retry_interval_not_elapsed() {
        let config = BtConfig {
            auto_connect: true,
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
        // Skip on non-Pi Linux — nmcli/bluetoothctl not available in WSL
        if cfg!(unix)
            && std::process::Command::new("nmcli")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_err()
        {
            return;
        }
        let config = BtConfig {
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
        // Skip on non-Pi Linux — nmcli/bluetoothctl not available in WSL
        if cfg!(unix)
            && std::process::Command::new("nmcli")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_err()
        {
            return;
        }
        let config = BtConfig {
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        assert_eq!(bt.state, BtState::Off);
        bt.toggle();
        assert_eq!(bt.state, BtState::Connected);
    }

    #[test]
    fn test_toggle_from_error() {
        // Skip on non-Pi Linux — nmcli/bluetoothctl not available in WSL
        if cfg!(unix)
            && std::process::Command::new("nmcli")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_err()
        {
            return;
        }
        let config = BtConfig {
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
        assert!(!cfg.enabled);
        assert!(cfg.phone_name.is_empty());
        assert!(cfg.auto_connect);
        assert!(cfg.hide_after_connect);
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
        assert!(DEFAULT_CONNECTION_NAME.is_empty());
        assert_eq!(BNEP0_SYSFS_PATH, "/sys/class/net/bnep0");
    }

    // ===== New bluetoothctl builder tests =====

    #[test]
    fn test_build_power_on_args() {
        assert_eq!(build_power_on_args(), vec!["power", "on"]);
    }

    #[test]
    fn test_build_agent_on_args() {
        assert_eq!(build_agent_on_args(), vec!["agent", "on"]);
    }

    #[test]
    fn test_build_default_agent_args() {
        assert_eq!(build_default_agent_args(), vec!["default-agent"]);
    }

    #[test]
    fn test_build_pair_args() {
        let args = build_pair_args("AA:BB:CC:DD:EE:FF");
        assert_eq!(args, vec!["pair", "AA:BB:CC:DD:EE:FF"]);
    }

    #[test]
    fn test_build_trust_args() {
        let args = build_trust_args("AA:BB:CC:DD:EE:FF");
        assert_eq!(args, vec!["trust", "AA:BB:CC:DD:EE:FF"]);
    }

    #[test]
    fn test_build_discoverable_off_args() {
        assert_eq!(build_discoverable_off_args(), vec!["discoverable", "off"]);
    }

    #[test]
    fn test_build_discoverable_on_args() {
        assert_eq!(build_discoverable_on_args(), vec!["discoverable", "on"]);
    }

    #[test]
    fn test_parse_scan_all_devices_basic() {
        let output =
            "[NEW] Device AA:BB:CC:DD:EE:FF My Phone\n[NEW] Device 11:22:33:44:55:66 Galaxy S24";
        let devices = parse_scan_all_devices(output);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0], ("AA:BB:CC:DD:EE:FF".into(), "My Phone".into()));
        assert_eq!(
            devices[1],
            ("11:22:33:44:55:66".into(), "Galaxy S24".into())
        );
    }

    #[test]
    fn test_parse_scan_all_devices_skips_mac_names() {
        let output = "[NEW] Device AA:BB:CC:DD:EE:FF AA-BB-CC-DD-EE-FF\n[NEW] Device 11:22:33:44:55:66 Real Phone";
        let devices = parse_scan_all_devices(output);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].1, "Real Phone");
    }

    #[test]
    fn test_parse_scan_all_devices_allows_hyphenated_names() {
        let output = "[NEW] Device AA:BB:CC:DD:EE:FF Galaxy S24-Ultra\n[NEW] Device 11:22:33:44:55:66 Mi-Band";
        let devices = parse_scan_all_devices(output);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].1, "Galaxy S24-Ultra");
        assert_eq!(devices[1].1, "Mi-Band");
    }

    #[test]
    fn test_parse_scan_all_devices_empty() {
        assert!(parse_scan_all_devices("").is_empty());
        assert!(parse_scan_all_devices("some random log output").is_empty());
    }

    #[test]
    fn test_parse_scan_all_devices_ansi() {
        let output = "\x1b[1;34m[NEW] Device AA:BB:CC:DD:EE:FF My Phone\x1b[0m";
        let devices = parse_scan_all_devices(output);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].1, "My Phone");
    }

    #[test]
    fn test_is_mac_like() {
        assert!(is_mac_like("AA-BB-CC-DD-EE-FF"));
        assert!(is_mac_like("aa:bb:cc:dd:ee:ff"));
        assert!(!is_mac_like("Galaxy S24-Ultra"));
        assert!(!is_mac_like("Mi-Band"));
        assert!(!is_mac_like("short"));
        assert!(!is_mac_like(""));
    }

    #[test]
    fn test_build_scan_on_args() {
        assert_eq!(build_scan_on_args(), vec!["--timeout", "10", "scan", "on"]);
    }

    // ===== New nmcli builder tests =====

    #[test]
    fn test_build_nmcli_add_pan_args() {
        let args = build_nmcli_add_pan_args("MyPhone", "AA:BB:CC:DD:EE:FF");
        assert_eq!(
            args,
            vec![
                "connection",
                "add",
                "type",
                "bluetooth",
                "con-name",
                "MyPhone",
                "bt-type",
                "panu",
                "bluetooth.bdaddr",
                "AA:BB:CC:DD:EE:FF",
                "autoconnect",
                "yes",
            ]
        );
    }

    #[test]
    fn test_build_nmcli_show_args() {
        let args = build_nmcli_show_args("MyPhone");
        assert_eq!(args, vec!["connection", "show", "MyPhone"]);
    }

    // ===== Scan output parser tests =====

    #[test]
    fn test_parse_scan_output_finds_device() {
        let output = "[NEW] Device AA:BB:CC:DD:EE:FF My Phone\n[NEW] Device 11:22:33:44:55:66 Other Device\n";
        let result = parse_scan_for_device(output, "My Phone", "");
        assert_eq!(result, Some("AA:BB:CC:DD:EE:FF".to_string()));
    }

    #[test]
    fn test_parse_scan_output_finds_by_mac() {
        let output = "[NEW] Device AA:BB:CC:DD:EE:FF My Phone\n[NEW] Device 11:22:33:44:55:66 Other Device\n";
        let result = parse_scan_for_device(output, "", "AA:BB:CC:DD:EE:FF");
        assert_eq!(result, Some("AA:BB:CC:DD:EE:FF".to_string()));
    }

    #[test]
    fn test_parse_scan_output_no_match() {
        let output = "[NEW] Device AA:BB:CC:DD:EE:FF My Phone\n";
        let result = parse_scan_for_device(output, "Nonexistent", "");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_scan_output_with_ansi_colors() {
        // Real bluetoothctl output has ANSI color codes
        let output = "\x1b[0;92m[NEW]\x1b[0m Device 9C:9E:D5:E3:F2:19 Xiaomi 13T\n\
                      \x1b[0;92m[NEW]\x1b[0m Device 70:B1:3D:99:CE:4D [TV] Samsung 7 Series (50)\n";
        let result = parse_scan_for_device(output, "Xiaomi", "");
        assert_eq!(result, Some("9C:9E:D5:E3:F2:19".to_string()));
    }

    #[test]
    fn test_strip_ansi() {
        assert_eq!(strip_ansi("\x1b[0;92m[NEW]\x1b[0m Device"), "[NEW] Device");
        assert_eq!(strip_ansi("no escapes"), "no escapes");
        assert_eq!(strip_ansi(""), "");
    }

    // ===== Task 3: Pairing state tests =====

    #[test]
    fn test_pairing_state() {
        let mut bt = BtTether::default();
        bt.state = BtState::Pairing;
        assert_eq!(bt.status_str(), "BT PAIR");
        assert_eq!(bt.status_short(), "P");
    }

    #[test]
    fn test_should_connect_pairing() {
        let config = BtConfig {
            auto_connect: true,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Pairing;
        assert!(!bt.should_connect());
    }

    #[test]
    fn test_toggle_pairing_noop() {
        let config = BtConfig {
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        bt.state = BtState::Pairing;
        bt.toggle();
        assert_eq!(bt.state, BtState::Pairing);
    }

    #[test]
    fn test_setup_not_enabled() {
        let config = BtConfig {
            enabled: false,
            ..Default::default()
        };
        let mut bt = BtTether::new(config);
        let result = bt.setup();
        assert!(result.is_ok());
        assert_eq!(bt.state, BtState::Off);
    }

    #[test]
    fn test_connection_name_generation() {
        let name = generate_connection_name("AA:BB:CC:DD:EE:FF");
        assert_eq!(name, "bt-pan-AABBCCDDEEFF");
    }
}
