//! PiSugar 3 battery monitoring via I2C.
//!
//! Reads battery level, charging status, and handles button presses.
//! On non-Pi platforms, provides a mock implementation.

use std::time::Instant;

// ---------------------------------------------------------------------------
// I2C register constants for PiSugar 3
// ---------------------------------------------------------------------------

/// PiSugar 3 battery I2C address.
pub const I2C_ADDR_BATTERY: u16 = 0x57;
/// PiSugar 3 RTC I2C address.
pub const I2C_ADDR_RTC: u16 = 0x68;
/// Register: battery level percentage (0-100).
pub const REG_BATTERY_LEVEL: u8 = 0x2A;
/// Register: battery voltage high byte (mV, big-endian).
pub const REG_VOLTAGE_HIGH: u8 = 0x22;
/// Register: battery voltage low byte (mV, big-endian).
pub const REG_VOLTAGE_LOW: u8 = 0x23;
/// Register: charging status (bit flags).
pub const REG_CHARGE_STATUS: u8 = 0x02;
/// Register: button event flags.
pub const REG_BUTTON_EVENT: u8 = 0x04;
/// Register: auto-shutdown battery level threshold.
pub const REG_AUTO_SHUTDOWN: u8 = 0x19;

// Charge status bit flags (register 0x02)
/// Bit 0: power cable connected.
pub const CHARGE_FLAG_POWER_CONNECTED: u8 = 0x01;
/// Bit 1: charging in progress.
pub const CHARGE_FLAG_CHARGING: u8 = 0x02;
/// Bit 2: charge complete / full.
pub const CHARGE_FLAG_FULL: u8 = 0x04;

// Button event bit flags (register 0x04)
/// Bit 0: single press detected.
pub const BUTTON_FLAG_SINGLE: u8 = 0x01;
/// Bit 1: double press detected.
pub const BUTTON_FLAG_DOUBLE: u8 = 0x02;
/// Bit 2: long press detected.
pub const BUTTON_FLAG_LONG: u8 = 0x04;

// ---------------------------------------------------------------------------
// Parsing helpers (pure functions, testable on any platform)
// ---------------------------------------------------------------------------

/// Parse battery level from register 0x2A.
/// Clamps to 0-100 range.
pub fn parse_battery_level(raw: u8) -> u8 {
    raw.min(100)
}

/// Parse voltage from two register bytes (0x22 high, 0x23 low) in big-endian mV.
pub fn parse_voltage_mv(high: u8, low: u8) -> u16 {
    u16::from_be_bytes([high, low])
}

/// Parse charge state from register 0x02 bit flags.
pub fn parse_charge_state(flags: u8) -> ChargeState {
    if flags & CHARGE_FLAG_FULL != 0 {
        ChargeState::Full
    } else if flags & CHARGE_FLAG_CHARGING != 0 {
        ChargeState::Charging
    } else if flags & CHARGE_FLAG_POWER_CONNECTED != 0 {
        // Power connected but not charging (e.g. maintenance/done)
        ChargeState::Full
    } else {
        ChargeState::Discharging
    }
}

/// Parse button event flags from register 0x04.
/// Returns the highest-priority event (long > double > single).
pub fn parse_button_event(flags: u8) -> Option<ButtonAction> {
    if flags & BUTTON_FLAG_LONG != 0 {
        Some(ButtonAction::LongPress)
    } else if flags & BUTTON_FLAG_DOUBLE != 0 {
        Some(ButtonAction::DoubleTap)
    } else if flags & BUTTON_FLAG_SINGLE != 0 {
        Some(ButtonAction::SingleTap)
    } else {
        None
    }
}

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
    /// I2C device address for PiSugar 3 battery.
    pub i2c_addr: u16,
    /// Battery level below which to trigger low warning.
    pub low_threshold: u8,
    /// Battery level below which to trigger critical/shutdown.
    pub critical_threshold: u8,
    /// Poll interval in seconds.
    pub poll_interval_secs: u64,
    /// Whether to auto-shutdown on critical battery.
    pub auto_shutdown: bool,
    /// Auto-shutdown level to write to register 0x19 (0 = disabled).
    pub auto_shutdown_level: u8,
}

impl Default for PiSugarConfig {
    fn default() -> Self {
        Self {
            i2c_bus: 1,
            i2c_addr: I2C_ADDR_BATTERY,
            low_threshold: 20,
            critical_threshold: 5,
            poll_interval_secs: 30,
            auto_shutdown: true,
            auto_shutdown_level: 5,
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

// ---------------------------------------------------------------------------
// I2C backend (real on aarch64, stub on other platforms)
// ---------------------------------------------------------------------------

/// Errors from I2C operations.
#[derive(Debug)]
pub enum I2cError {
    /// I2C bus could not be opened.
    BusOpen(String),
    /// I2C device not found at address.
    DeviceNotFound(u16),
    /// Read/write failed.
    IoError(String),
}

impl std::fmt::Display for I2cError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            I2cError::BusOpen(e) => write!(f, "I2C bus open failed: {e}"),
            I2cError::DeviceNotFound(addr) => write!(f, "I2C device not found at 0x{addr:02X}"),
            I2cError::IoError(e) => write!(f, "I2C I/O error: {e}"),
        }
    }
}

/// Read a single register from the PiSugar over I2C.
#[cfg(target_arch = "aarch64")]
fn i2c_read_register(bus: u8, addr: u16, register: u8) -> Result<u8, I2cError> {
    let mut i2c =
        rppal::i2c::I2c::with_bus(bus).map_err(|e| I2cError::BusOpen(e.to_string()))?;
    i2c.set_slave_address(addr)
        .map_err(|e| I2cError::IoError(e.to_string()))?;
    let mut buf = [0u8; 1];
    i2c.write_read(&[register], &mut buf)
        .map_err(|e| I2cError::IoError(e.to_string()))?;
    Ok(buf[0])
}

#[cfg(not(target_arch = "aarch64"))]
fn i2c_read_register(_bus: u8, _addr: u16, _register: u8) -> Result<u8, I2cError> {
    Err(I2cError::DeviceNotFound(0x57))
}

/// Write a single register to the PiSugar over I2C.
#[cfg(target_arch = "aarch64")]
fn i2c_write_register(bus: u8, addr: u16, register: u8, value: u8) -> Result<(), I2cError> {
    let mut i2c =
        rppal::i2c::I2c::with_bus(bus).map_err(|e| I2cError::BusOpen(e.to_string()))?;
    i2c.set_slave_address(addr)
        .map_err(|e| I2cError::IoError(e.to_string()))?;
    i2c.write(&[register, value])
        .map_err(|e| I2cError::IoError(e.to_string()))?;
    Ok(())
}

#[cfg(not(target_arch = "aarch64"))]
fn i2c_write_register(_bus: u8, _addr: u16, _register: u8, _value: u8) -> Result<(), I2cError> {
    Err(I2cError::DeviceNotFound(0x57))
}

/// Probe whether an I2C device is present at the given address.
#[cfg(target_arch = "aarch64")]
fn i2c_probe(bus: u8, addr: u16) -> bool {
    let Ok(mut i2c) = rppal::i2c::I2c::with_bus(bus) else {
        return false;
    };
    if i2c.set_slave_address(addr).is_err() {
        return false;
    }
    // Try reading register 0x00 -- if the device ACKs, it is present
    let mut buf = [0u8; 1];
    i2c.write_read(&[0x00], &mut buf).is_ok()
}

#[cfg(not(target_arch = "aarch64"))]
fn i2c_probe(_bus: u8, _addr: u16) -> bool {
    false
}

// ---------------------------------------------------------------------------
// PiSugar manager
// ---------------------------------------------------------------------------

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

    /// Probe for PiSugar on I2C bus.
    pub fn probe(&mut self) -> bool {
        self.available = i2c_probe(self.config.i2c_bus, self.config.i2c_addr);
        if self.available && self.config.auto_shutdown && self.config.auto_shutdown_level > 0 {
            let _ = self.set_auto_shutdown_level(self.config.auto_shutdown_level);
        }
        self.available
    }

    /// Read battery status from I2C registers.
    /// On non-Pi or if not available, just applies thresholds to current status.
    pub fn read_status(&mut self) -> &BatteryStatus {
        if self.available {
            // Read battery level (register 0x2A)
            if let Ok(raw_level) = i2c_read_register(
                self.config.i2c_bus,
                self.config.i2c_addr,
                REG_BATTERY_LEVEL,
            ) {
                self.status.level = parse_battery_level(raw_level);
            }

            // Read voltage (registers 0x22-0x23, big-endian mV)
            if let Ok(v_high) = i2c_read_register(
                self.config.i2c_bus,
                self.config.i2c_addr,
                REG_VOLTAGE_HIGH,
            ) {
                if let Ok(v_low) = i2c_read_register(
                    self.config.i2c_bus,
                    self.config.i2c_addr,
                    REG_VOLTAGE_LOW,
                ) {
                    self.status.voltage_mv = parse_voltage_mv(v_high, v_low);
                }
            }

            // Read charging status (register 0x02)
            if let Ok(flags) = i2c_read_register(
                self.config.i2c_bus,
                self.config.i2c_addr,
                REG_CHARGE_STATUS,
            ) {
                self.status.charge_state = parse_charge_state(flags);
            }
        }

        // Apply thresholds
        self.status.low = self.status.level <= self.config.low_threshold;
        self.status.critical = self.status.level <= self.config.critical_threshold;
        &self.status
    }

    /// Read button events from I2C register 0x04.
    pub fn read_button_event(&mut self) -> Option<ButtonAction> {
        if !self.available {
            return None;
        }
        let flags = i2c_read_register(
            self.config.i2c_bus,
            self.config.i2c_addr,
            REG_BUTTON_EVENT,
        )
        .ok()?;
        parse_button_event(flags)
    }

    /// Poll button: read hardware events and feed through debouncer.
    pub fn poll_button(&mut self) -> Option<ButtonAction> {
        if let Some(hw_action) = self.read_button_event() {
            return Some(hw_action);
        }
        self.debouncer.check_timeout()
    }

    /// Set the auto-shutdown battery level on the PiSugar hardware.
    pub fn set_auto_shutdown_level(&self, level: u8) -> Result<(), I2cError> {
        i2c_write_register(
            self.config.i2c_bus,
            self.config.i2c_addr,
            REG_AUTO_SHUTDOWN,
            level,
        )
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
// Button action mapping
// ---------------------------------------------------------------------------

/// Mapped button actions for the PiSugar button.
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

    // ===== Register constant tests =====

    #[test]
    fn test_i2c_address_constants() {
        assert_eq!(I2C_ADDR_BATTERY, 0x57);
        assert_eq!(I2C_ADDR_RTC, 0x68);
    }

    #[test]
    fn test_register_addresses() {
        assert_eq!(REG_BATTERY_LEVEL, 0x2A);
        assert_eq!(REG_VOLTAGE_HIGH, 0x22);
        assert_eq!(REG_VOLTAGE_LOW, 0x23);
        assert_eq!(REG_CHARGE_STATUS, 0x02);
        assert_eq!(REG_BUTTON_EVENT, 0x04);
        assert_eq!(REG_AUTO_SHUTDOWN, 0x19);
    }

    #[test]
    fn test_charge_flag_constants() {
        assert_eq!(CHARGE_FLAG_POWER_CONNECTED, 0x01);
        assert_eq!(CHARGE_FLAG_CHARGING, 0x02);
        assert_eq!(CHARGE_FLAG_FULL, 0x04);
        assert_eq!(
            CHARGE_FLAG_POWER_CONNECTED & CHARGE_FLAG_CHARGING & CHARGE_FLAG_FULL,
            0
        );
    }

    #[test]
    fn test_button_flag_constants() {
        assert_eq!(BUTTON_FLAG_SINGLE, 0x01);
        assert_eq!(BUTTON_FLAG_DOUBLE, 0x02);
        assert_eq!(BUTTON_FLAG_LONG, 0x04);
        assert_eq!(
            BUTTON_FLAG_SINGLE & BUTTON_FLAG_DOUBLE & BUTTON_FLAG_LONG,
            0
        );
    }

    // ===== Battery level parsing tests =====

    #[test]
    fn test_parse_battery_level_normal() {
        assert_eq!(parse_battery_level(75), 75);
        assert_eq!(parse_battery_level(0), 0);
        assert_eq!(parse_battery_level(100), 100);
    }

    #[test]
    fn test_parse_battery_level_clamps_overflow() {
        assert_eq!(parse_battery_level(120), 100);
        assert_eq!(parse_battery_level(255), 100);
    }

    #[test]
    fn test_parse_battery_level_boundaries() {
        assert_eq!(parse_battery_level(1), 1);
        assert_eq!(parse_battery_level(99), 99);
        assert_eq!(parse_battery_level(101), 100);
    }

    // ===== Voltage conversion tests =====

    #[test]
    fn test_parse_voltage_mv_typical_values() {
        assert_eq!(parse_voltage_mv(0x10, 0x68), 4200);
        assert_eq!(parse_voltage_mv(0x0C, 0xE4), 3300);
    }

    #[test]
    fn test_parse_voltage_mv_zero() {
        assert_eq!(parse_voltage_mv(0x00, 0x00), 0);
    }

    #[test]
    fn test_parse_voltage_mv_max() {
        assert_eq!(parse_voltage_mv(0xFF, 0xFF), 65535);
    }

    #[test]
    fn test_parse_voltage_mv_big_endian_order() {
        assert_eq!(parse_voltage_mv(0x01, 0x00), 256);
        assert_eq!(parse_voltage_mv(0x00, 0x01), 1);
    }

    // ===== Charge state decoding tests =====

    #[test]
    fn test_parse_charge_state_discharging() {
        assert_eq!(parse_charge_state(0x00), ChargeState::Discharging);
    }

    #[test]
    fn test_parse_charge_state_charging() {
        assert_eq!(
            parse_charge_state(CHARGE_FLAG_CHARGING | CHARGE_FLAG_POWER_CONNECTED),
            ChargeState::Charging
        );
        assert_eq!(
            parse_charge_state(CHARGE_FLAG_CHARGING),
            ChargeState::Charging
        );
    }

    #[test]
    fn test_parse_charge_state_full() {
        assert_eq!(
            parse_charge_state(CHARGE_FLAG_FULL | CHARGE_FLAG_POWER_CONNECTED),
            ChargeState::Full
        );
        assert_eq!(parse_charge_state(CHARGE_FLAG_FULL), ChargeState::Full);
    }

    #[test]
    fn test_parse_charge_state_power_only() {
        assert_eq!(
            parse_charge_state(CHARGE_FLAG_POWER_CONNECTED),
            ChargeState::Full
        );
    }

    #[test]
    fn test_parse_charge_state_full_overrides_charging() {
        assert_eq!(
            parse_charge_state(CHARGE_FLAG_FULL | CHARGE_FLAG_CHARGING),
            ChargeState::Full
        );
    }

    // ===== Button event parsing tests =====

    #[test]
    fn test_parse_button_event_none() {
        assert_eq!(parse_button_event(0x00), None);
    }

    #[test]
    fn test_parse_button_event_single() {
        assert_eq!(
            parse_button_event(BUTTON_FLAG_SINGLE),
            Some(ButtonAction::SingleTap)
        );
    }

    #[test]
    fn test_parse_button_event_double() {
        assert_eq!(
            parse_button_event(BUTTON_FLAG_DOUBLE),
            Some(ButtonAction::DoubleTap)
        );
    }

    #[test]
    fn test_parse_button_event_long() {
        assert_eq!(
            parse_button_event(BUTTON_FLAG_LONG),
            Some(ButtonAction::LongPress)
        );
    }

    #[test]
    fn test_parse_button_event_long_overrides_double() {
        assert_eq!(
            parse_button_event(BUTTON_FLAG_LONG | BUTTON_FLAG_DOUBLE),
            Some(ButtonAction::LongPress)
        );
    }

    #[test]
    fn test_parse_button_event_double_overrides_single() {
        assert_eq!(
            parse_button_event(BUTTON_FLAG_DOUBLE | BUTTON_FLAG_SINGLE),
            Some(ButtonAction::DoubleTap)
        );
    }

    #[test]
    fn test_parse_button_event_all_flags() {
        assert_eq!(
            parse_button_event(BUTTON_FLAG_LONG | BUTTON_FLAG_DOUBLE | BUTTON_FLAG_SINGLE),
            Some(ButtonAction::LongPress)
        );
    }

    #[test]
    fn test_parse_button_event_unknown_bits_ignored() {
        assert_eq!(parse_button_event(0xF0), None);
        assert_eq!(parse_button_event(0xF1), Some(ButtonAction::SingleTap));
    }

    // ===== Debouncer tests =====

    #[test]
    fn test_button_debouncer_long_press() {
        let mut db = ButtonDebouncer::new();
        db.on_press();
        db.last_press = Some(Instant::now() - std::time::Duration::from_millis(1500));
        let action = db.on_press();
        assert_eq!(action, Some(ButtonAction::LongPress));
    }

    #[test]
    fn test_button_first_press_returns_none() {
        let mut db = ButtonDebouncer::new();
        let action = db.on_press();
        assert_eq!(action, None);
    }

    #[test]
    fn test_button_immediate_second_press() {
        let mut db = ButtonDebouncer::new();
        db.on_press();
        let action = db.on_press();
        assert_eq!(action, Some(ButtonAction::DoubleTap));
    }

    #[test]
    fn test_button_check_timeout_no_press() {
        let mut db = ButtonDebouncer::new();
        let action = db.check_timeout();
        assert_eq!(action, None);
    }

    #[test]
    fn test_button_check_timeout_resolves_single() {
        let mut db = ButtonDebouncer::new();
        db.on_press();
        db.last_press = Some(Instant::now() - std::time::Duration::from_millis(500));
        let action = db.check_timeout();
        assert_eq!(action, Some(ButtonAction::SingleTap));
    }

    #[test]
    fn test_button_debouncer_resets_after_gesture() {
        let mut db = ButtonDebouncer::new();
        db.on_press();
        let _ = db.on_press();
        let action = db.on_press();
        assert_eq!(action, None);
    }

    // ===== Existing battery/pisugar tests =====

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

    // ===== Button action mapping tests =====

    #[test]
    fn test_button_single_tap_maps_to_bluetooth() {
        assert_eq!(map_button_action(ButtonAction::SingleTap), MappedAction::Bluetooth);
    }

    #[test]
    fn test_button_double_tap_maps_to_auto_manual() {
        assert_eq!(map_button_action(ButtonAction::DoubleTap), MappedAction::AutoManual);
    }

    #[test]
    fn test_button_long_press_maps_to_ao_pwn() {
        assert_eq!(map_button_action(ButtonAction::LongPress), MappedAction::AoPwn);
    }

    // ===== Config defaults test =====

    #[test]
    fn test_config_defaults() {
        let cfg = PiSugarConfig::default();
        assert_eq!(cfg.i2c_bus, 1);
        assert_eq!(cfg.i2c_addr, 0x57);
        assert_eq!(cfg.low_threshold, 20);
        assert_eq!(cfg.critical_threshold, 5);
        assert_eq!(cfg.auto_shutdown_level, 5);
        assert!(cfg.auto_shutdown);
    }

    // ===== I2C error display =====

    #[test]
    fn test_i2c_error_display() {
        let e = I2cError::BusOpen("permission denied".into());
        assert!(e.to_string().contains("permission denied"));

        let e = I2cError::DeviceNotFound(0x57);
        assert!(e.to_string().contains("0x57"));

        let e = I2cError::IoError("timeout".into());
        assert!(e.to_string().contains("timeout"));
    }

    #[test]
    fn test_probe_not_available_on_non_pi() {
        let mut ps = PiSugar::default();
        assert!(!ps.probe());
        assert!(!ps.available);
    }

    #[test]
    fn test_read_button_event_unavailable() {
        let mut ps = PiSugar::default();
        assert_eq!(ps.read_button_event(), None);
    }

    #[test]
    fn test_poll_button_unavailable() {
        let mut ps = PiSugar::default();
        assert_eq!(ps.poll_button(), None);
    }

    #[test]
    fn test_set_auto_shutdown_level_non_pi() {
        let ps = PiSugar::default();
        assert!(ps.set_auto_shutdown_level(10).is_err());
    }
}
