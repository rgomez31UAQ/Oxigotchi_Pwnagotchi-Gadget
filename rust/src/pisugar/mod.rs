//! PiSugar 3 battery monitoring via I2C.
//!
//! Reads battery level, charging status, and handles button presses.
//! On non-Pi platforms, provides a mock implementation.

use std::time::Instant;

/// Battery charging state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeState {
    Discharging,
    Charging,
    Full,
    Unknown,
}

/// PiSugar button actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    SingleTap,
    DoubleTap,
    LongPress,
}

/// Battery status snapshot.
#[derive(Debug, Clone)]
pub struct BatteryStatus {
    /// Battery level as percentage (0-100).
    pub level: u8,
    /// Charging state.
    pub charge_state: ChargeState,
    /// Battery voltage in millivolts.
    pub voltage_mv: u16,
    /// Whether the battery is below critical threshold.
    pub critical: bool,
    /// Whether the battery is below low threshold.
    pub low: bool,
}

impl Default for BatteryStatus {
    fn default() -> Self {
        Self {
            level: 100,
            charge_state: ChargeState::Unknown,
            voltage_mv: 4200,
            critical: false,
            low: false,
        }
    }
}

/// PiSugar I2C configuration.
#[derive(Debug, Clone)]
pub struct PiSugarConfig {
    /// I2C bus number (usually 1 on Pi).
    pub i2c_bus: u8,
    /// I2C device address for PiSugar 3.
    pub i2c_addr: u16,
    /// Battery level below which to trigger low warning.
    pub low_threshold: u8,
    /// Battery level below which to trigger critical/shutdown.
    pub critical_threshold: u8,
    /// Poll interval in seconds.
    pub poll_interval_secs: u64,
    /// Whether to auto-shutdown on critical battery.
    pub auto_shutdown: bool,
}

impl Default for PiSugarConfig {
    fn default() -> Self {
        Self {
            i2c_bus: 1,
            i2c_addr: 0x57,
            low_threshold: 20,
            critical_threshold: 5,
            poll_interval_secs: 30,
            auto_shutdown: true,
        }
    }
}

/// Button debouncer to distinguish tap types.
#[derive(Debug)]
pub struct ButtonDebouncer {
    /// Last button press time.
    last_press: Option<Instant>,
    /// Number of presses in the current gesture.
    press_count: u32,
    /// Debounce window in milliseconds.
    debounce_ms: u64,
    /// Long press threshold in milliseconds.
    long_press_ms: u64,
}

impl ButtonDebouncer {
    /// Create a new button debouncer with default timing thresholds.
    pub fn new() -> Self {
        Self {
            last_press: None,
            press_count: 0,
            debounce_ms: 300,
            long_press_ms: 1000,
        }
    }

    /// Record a button press. Returns the detected action if the gesture is complete.
    pub fn on_press(&mut self) -> Option<ButtonAction> {
        let now = Instant::now();

        if let Some(last) = self.last_press {
            let elapsed_ms = now.duration_since(last).as_millis() as u64;

            if elapsed_ms >= self.long_press_ms {
                // Long press
                self.press_count = 0;
                self.last_press = Some(now);
                return Some(ButtonAction::LongPress);
            }

            if elapsed_ms <= self.debounce_ms {
                self.press_count += 1;
                self.last_press = Some(now);
                if self.press_count >= 2 {
                    self.press_count = 0;
                    return Some(ButtonAction::DoubleTap);
                }
                return None;
            }
        }

        // First press or new gesture
        self.press_count = 1;
        self.last_press = Some(now);
        None
    }

    /// Check if a gesture has timed out (single tap resolved).
    pub fn check_timeout(&mut self) -> Option<ButtonAction> {
        if let Some(last) = self.last_press
            && self.press_count == 1
        {
            let elapsed_ms = last.elapsed().as_millis() as u64;
            if elapsed_ms > self.debounce_ms && elapsed_ms < self.long_press_ms {
                self.press_count = 0;
                self.last_press = None;
                return Some(ButtonAction::SingleTap);
            }
        }
        None
    }
}

impl Default for ButtonDebouncer {
    fn default() -> Self {
        Self::new()
    }
}

/// PiSugar manager.
pub struct PiSugar {
    pub config: PiSugarConfig,
    pub status: BatteryStatus,
    pub debouncer: ButtonDebouncer,
    pub available: bool,
}

impl PiSugar {
    /// Create a new PiSugar manager with the given configuration.
    pub fn new(config: PiSugarConfig) -> Self {
        Self {
            config,
            status: BatteryStatus::default(),
            debouncer: ButtonDebouncer::new(),
            available: false,
        }
    }

    /// Stub: probe for PiSugar on I2C bus.
    pub fn probe(&mut self) -> bool {
        #[cfg(target_arch = "aarch64")]
        {
            // Would use rppal::i2c::I2c to probe the device
            self.available = false; // Stub
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            self.available = false;
        }
        self.available
    }

    /// Stub: read battery status from I2C.
    pub fn read_status(&mut self) -> &BatteryStatus {
        // On Pi, would read I2C registers
        // Apply thresholds
        self.status.low = self.status.level <= self.config.low_threshold;
        self.status.critical = self.status.level <= self.config.critical_threshold;
        &self.status
    }

    /// Update battery level (for testing or mock data).
    pub fn set_level(&mut self, level: u8) {
        self.status.level = level.min(100);
        self.status.low = level <= self.config.low_threshold;
        self.status.critical = level <= self.config.critical_threshold;
    }

    /// Check if shutdown should be triggered.
    pub fn should_shutdown(&self) -> bool {
        self.config.auto_shutdown && self.status.critical
    }

    /// Display string for battery level.
    pub fn display_str(&self) -> String {
        if !self.available {
            return "BAT N/A".to_string();
        }
        let icon = match self.status.charge_state {
            ChargeState::Charging => "+",
            ChargeState::Full => "=",
            _ => "",
        };
        format!("BAT {}%{}", self.status.level, icon)
    }
}

impl Default for PiSugar {
    fn default() -> Self {
        Self::new(PiSugarConfig::default())
    }
}

// ---------------------------------------------------------------------------
// Button action mapping (Python: button-feedback.py → pisugar/buttons.rs)
// ---------------------------------------------------------------------------

/// Mapped button actions for the PiSugar button.
/// Single tap = toggle Bluetooth
/// Double tap = toggle AUTO/MANU mode
/// Long press = toggle AO/PWN mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappedAction {
    /// Toggle Bluetooth PAN tethering on/off.
    Bluetooth,
    /// Switch between AUTO and MANUAL mode.
    AutoManual,
    /// Switch between AO and PWN mode.
    AoPwn,
}

/// Map a raw button action to a semantic daemon action.
pub fn map_button_action(action: ButtonAction) -> MappedAction {
    match action {
        ButtonAction::SingleTap => MappedAction::Bluetooth,
        ButtonAction::DoubleTap => MappedAction::AutoManual,
        ButtonAction::LongPress => MappedAction::AoPwn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_battery() {
        let ps = PiSugar::default();
        assert_eq!(ps.status.level, 100);
        assert!(!ps.status.critical);
        assert!(!ps.status.low);
        assert!(!ps.available);
    }

    #[test]
    fn test_set_level_thresholds() {
        let mut ps = PiSugar::default();
        ps.set_level(15);
        assert!(ps.status.low);
        assert!(!ps.status.critical);

        ps.set_level(3);
        assert!(ps.status.low);
        assert!(ps.status.critical);
    }

    #[test]
    fn test_should_shutdown() {
        let mut ps = PiSugar::default();
        ps.set_level(3);
        assert!(ps.should_shutdown());

        ps.config.auto_shutdown = false;
        assert!(!ps.should_shutdown());
    }

    #[test]
    fn test_display_str_unavailable() {
        let ps = PiSugar::default();
        assert_eq!(ps.display_str(), "BAT N/A");
    }

    #[test]
    fn test_display_str_available() {
        let mut ps = PiSugar::default();
        ps.available = true;
        ps.status.level = 75;
        ps.status.charge_state = ChargeState::Discharging;
        assert_eq!(ps.display_str(), "BAT 75%");

        ps.status.charge_state = ChargeState::Charging;
        assert_eq!(ps.display_str(), "BAT 75%+");
    }

    #[test]
    fn test_set_level_clamps() {
        let mut ps = PiSugar::default();
        ps.set_level(150);
        assert_eq!(ps.status.level, 100);
    }

    #[test]
    fn test_button_debouncer_long_press() {
        let mut db = ButtonDebouncer::new();
        // First press
        db.on_press();
        // Simulate long press by backdating
        db.last_press = Some(Instant::now() - std::time::Duration::from_millis(1500));
        let action = db.on_press();
        assert_eq!(action, Some(ButtonAction::LongPress));
    }

    #[test]
    fn test_read_status_applies_thresholds() {
        let mut ps = PiSugar::default();
        ps.status.level = 10;
        ps.read_status();
        assert!(ps.status.low);
        assert!(!ps.status.critical);
    }

    #[test]
    fn test_charge_states() {
        assert_ne!(ChargeState::Charging, ChargeState::Discharging);
        assert_ne!(ChargeState::Full, ChargeState::Unknown);
    }

    // ---- Button action mapping tests ----

    #[test]
    fn test_button_single_tap_maps_to_bluetooth() {
        let mapped = map_button_action(ButtonAction::SingleTap);
        assert_eq!(mapped, MappedAction::Bluetooth);
    }

    #[test]
    fn test_button_double_tap_maps_to_auto_manual() {
        let mapped = map_button_action(ButtonAction::DoubleTap);
        assert_eq!(mapped, MappedAction::AutoManual);
    }

    #[test]
    fn test_button_long_press_maps_to_ao_pwn() {
        let mapped = map_button_action(ButtonAction::LongPress);
        assert_eq!(mapped, MappedAction::AoPwn);
    }

    // ---- Button press edge cases ----

    #[test]
    fn test_button_first_press_returns_none() {
        let mut db = ButtonDebouncer::new();
        // First press never resolves immediately (waiting for double-tap window)
        let action = db.on_press();
        assert_eq!(action, None);
    }

    #[test]
    fn test_button_immediate_second_press() {
        let mut db = ButtonDebouncer::new();
        db.on_press(); // first press
        // Immediate second press (0ms gap) is within debounce window
        let action = db.on_press();
        assert_eq!(action, Some(ButtonAction::DoubleTap));
    }

    #[test]
    fn test_button_check_timeout_no_press() {
        let mut db = ButtonDebouncer::new();
        // No press at all -- timeout should return None
        let action = db.check_timeout();
        assert_eq!(action, None);
    }

    #[test]
    fn test_battery_level_zero() {
        let mut ps = PiSugar::default();
        ps.set_level(0);
        assert!(ps.status.critical);
        assert!(ps.status.low);
        assert!(ps.should_shutdown());
    }

    #[test]
    fn test_battery_level_100() {
        let mut ps = PiSugar::default();
        ps.set_level(100);
        assert!(!ps.status.critical);
        assert!(!ps.status.low);
        assert!(!ps.should_shutdown());
    }

    #[test]
    fn test_battery_display_str_charging_full() {
        let mut ps = PiSugar::default();
        ps.available = true;
        ps.status.level = 100;
        ps.status.charge_state = ChargeState::Full;
        assert_eq!(ps.display_str(), "BAT 100%=");
    }
}
