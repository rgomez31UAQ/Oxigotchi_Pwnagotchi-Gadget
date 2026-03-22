//! EpochState: daemon state snapshot passed to Lua plugins each epoch.

use mlua::prelude::*;

/// Flat snapshot of daemon state for Lua plugins.
#[derive(Debug, Clone, Default)]
pub struct EpochState {
    // timing
    pub uptime_secs: u64,
    pub epoch: u64,
    pub mode: String,

    // wifi / AO
    pub channel: u8,
    pub aps_seen: u32,
    pub handshakes: u32,
    pub captures_total: usize,
    pub blind_epochs: u32,
    pub ao_state: String,
    pub ao_pid: u32,
    pub ao_crash_count: u32,
    pub ao_uptime_str: String,

    // battery
    pub battery_level: u8,
    pub battery_charging: bool,
    pub battery_voltage_mv: u16,
    pub battery_low: bool,
    pub battery_critical: bool,
    pub battery_available: bool,

    // bluetooth
    pub bt_connected: bool,
    pub bt_short: String,
    pub bt_ip: String,
    pub bt_internet: bool,

    // network
    pub internet_online: bool,
    pub display_ip: String,

    // personality
    pub mood: f32,
    pub face: String,
    pub level: u32,
    pub xp: u64,
    pub status_message: String,

    // system
    pub cpu_temp: f32,
    pub mem_used_mb: u32,
    pub mem_total_mb: u32,
    pub cpu_percent: f32,
    pub cpu_freq_ghz: String,
}

impl EpochState {
    /// Convert to a Lua table.
    pub fn to_lua_table(&self, lua: &Lua) -> LuaResult<LuaTable> {
        let t = lua.create_table()?;

        t.set("uptime_secs", self.uptime_secs)?;
        t.set("epoch", self.epoch)?;
        t.set("mode", self.mode.as_str())?;

        t.set("channel", self.channel)?;
        t.set("aps_seen", self.aps_seen)?;
        t.set("handshakes", self.handshakes)?;
        t.set("captures_total", self.captures_total as u64)?;
        t.set("blind_epochs", self.blind_epochs)?;
        t.set("ao_state", self.ao_state.as_str())?;
        t.set("ao_pid", self.ao_pid)?;
        t.set("ao_crash_count", self.ao_crash_count)?;
        t.set("ao_uptime_str", self.ao_uptime_str.as_str())?;

        t.set("battery_level", self.battery_level)?;
        t.set("battery_charging", self.battery_charging)?;
        t.set("battery_voltage_mv", self.battery_voltage_mv)?;
        t.set("battery_low", self.battery_low)?;
        t.set("battery_critical", self.battery_critical)?;
        t.set("battery_available", self.battery_available)?;

        t.set("bt_connected", self.bt_connected)?;
        t.set("bt_short", self.bt_short.as_str())?;
        t.set("bt_ip", self.bt_ip.as_str())?;
        t.set("bt_internet", self.bt_internet)?;

        t.set("internet_online", self.internet_online)?;
        t.set("display_ip", self.display_ip.as_str())?;

        t.set("mood", self.mood)?;
        t.set("face", self.face.as_str())?;
        t.set("level", self.level)?;
        t.set("xp", self.xp)?;
        t.set("status_message", self.status_message.as_str())?;

        t.set("cpu_temp", self.cpu_temp)?;
        t.set("mem_used_mb", self.mem_used_mb)?;
        t.set("mem_total_mb", self.mem_total_mb)?;
        t.set("cpu_percent", self.cpu_percent)?;
        t.set("cpu_freq_ghz", self.cpu_freq_ghz.as_str())?;

        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_state_default() {
        let s = EpochState::default();
        assert_eq!(s.uptime_secs, 0);
        assert_eq!(s.epoch, 0);
        assert_eq!(s.ao_state, "");
    }

    #[test]
    fn test_epoch_state_to_lua_table() {
        let lua = Lua::new();
        let s = EpochState {
            uptime_secs: 7200,
            epoch: 42,
            aps_seen: 15,
            handshakes: 3,
            battery_level: 85,
            bt_connected: true,
            mood: 0.75,
            level: 3,
            xp: 450,
            cpu_temp: 45.2,
            ..Default::default()
        };
        let table = s.to_lua_table(&lua).unwrap();
        assert_eq!(table.get::<u64>("uptime_secs").unwrap(), 7200);
        assert_eq!(table.get::<u64>("epoch").unwrap(), 42);
        assert_eq!(table.get::<u32>("aps_seen").unwrap(), 15);
        assert_eq!(table.get::<u8>("battery_level").unwrap(), 85);
        assert!(table.get::<bool>("bt_connected").unwrap());
        assert_eq!(table.get::<u32>("level").unwrap(), 3);
    }

    #[test]
    fn test_epoch_state_mode_field() {
        let lua = Lua::new();
        let s = EpochState {
            mode: "RAGE".into(),
            ..Default::default()
        };
        let table = s.to_lua_table(&lua).unwrap();
        assert_eq!(table.get::<String>("mode").unwrap(), "RAGE");
    }
}
