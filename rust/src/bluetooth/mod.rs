//! Bluetooth PAN tethering module.
//!
//! Manages Bluetooth Personal Area Network (PAN) connections for
//! internet tethering via a paired phone.

use std::time::Instant;

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
            auto_connect: false,
            retry_interval_secs: 30,
            max_retries: 0,
        }
    }
}

/// Bluetooth tether manager.
pub struct BtTether {
    pub state: BtState,
    pub config: BtConfig,
    pub retry_count: u32,
    pub last_attempt: Option<Instant>,
    /// Whether internet is reachable through the BT connection.
    pub internet_available: bool,
}

impl BtTether {
    pub fn new(config: BtConfig) -> Self {
        Self {
            state: BtState::Off,
            config,
            retry_count: 0,
            last_attempt: None,
            internet_available: false,
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
                    Some(t) => {
                        t.elapsed().as_secs() >= self.config.retry_interval_secs
                    }
                    None => true,
                }
            }
        }
    }

    /// Stub: initiate BT PAN connection.
    pub fn connect(&mut self) -> Result<(), String> {
        if self.config.phone_mac.is_empty() {
            return Err("No phone MAC configured".into());
        }
        self.state = BtState::Connecting;
        self.last_attempt = Some(Instant::now());
        // Stub: real implementation would use BlueZ D-Bus
        // For now, simulate successful connection
        self.state = BtState::Connected;
        self.retry_count = 0;
        self.internet_available = true;
        Ok(())
    }

    /// Stub: disconnect BT PAN.
    pub fn disconnect(&mut self) {
        self.state = BtState::Disconnected;
        self.internet_available = false;
    }

    /// Handle a connection failure.
    pub fn on_error(&mut self) {
        self.state = BtState::Error;
        self.retry_count += 1;
        self.internet_available = false;
    }

    /// Status string for display.
    pub fn status_str(&self) -> &'static str {
        match self.state {
            BtState::Off => "BT OFF",
            BtState::Disconnected => "BT DISC",
            BtState::Connecting => "BT ...",
            BtState::Connected => "BT OK",
            BtState::Error => "BT ERR",
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

    #[test]
    fn test_default_state() {
        let bt = BtTether::default();
        assert_eq!(bt.state, BtState::Off);
        assert!(!bt.internet_available);
    }

    #[test]
    fn test_connect_requires_mac() {
        let mut bt = BtTether::default();
        assert!(bt.connect().is_err());
    }

    #[test]
    fn test_connect_success() {
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
        bt.disconnect();
        assert_eq!(bt.state, BtState::Disconnected);
        assert!(!bt.internet_available);
    }

    #[test]
    fn test_should_connect_auto_off() {
        let bt = BtTether::default();
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
        assert!(!bt.should_connect()); // Max retries reached
    }

    #[test]
    fn test_on_error() {
        let mut bt = BtTether::default();
        bt.state = BtState::Connecting;
        bt.on_error();
        assert_eq!(bt.state, BtState::Error);
        assert_eq!(bt.retry_count, 1);
    }

    #[test]
    fn test_status_strings() {
        let mut bt = BtTether::default();
        assert_eq!(bt.status_str(), "BT OFF");
        bt.state = BtState::Connected;
        assert_eq!(bt.status_str(), "BT OK");
        bt.state = BtState::Error;
        assert_eq!(bt.status_str(), "BT ERR");
    }
}
