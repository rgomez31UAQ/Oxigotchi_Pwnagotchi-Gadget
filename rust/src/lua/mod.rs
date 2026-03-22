//! Lua plugin runtime: plugin loading, indicator registry, epoch ticking.

pub mod state;

use mlua::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Font size for indicators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorFont {
    /// ProFont 9pt (6px wide).
    Small,
    /// ProFont 10pt (7px wide).
    Medium,
}

/// A text indicator registered by a Lua plugin.
#[derive(Debug, Clone)]
pub struct Indicator {
    /// Unique name (e.g. "uptime", "battery").
    pub name: String,
    /// Current text value to display.
    pub value: String,
    /// X position on display (0-249).
    pub x: i32,
    /// Y position on display (0-121).
    pub y: i32,
    /// Optional label prefix (e.g. "UP" renders as "UP: {value}").
    pub label: Option<String>,
    /// Font size.
    pub font: IndicatorFont,
    /// Word-wrap width in chars (0 = no wrap).
    pub wrap_width: u32,
}

/// Plugin metadata read from Lua file scope.
#[derive(Debug, Clone)]
pub struct PluginMeta {
    pub name: String,
    pub version: String,
    pub author: String,
    pub tag: String, // "default" or "community"
}

/// Plugin config from TOML.
#[derive(Debug, Clone)]
pub struct PluginConfig {
    pub name: String,
    pub enabled: bool,
    pub x: i32,
    pub y: i32,
    /// Extra config keys passed to Lua's on_load(config).
    pub extra: std::collections::HashMap<String, String>,
}

impl PluginConfig {
    pub fn default_for(name: &str, x: i32, y: i32) -> Self {
        Self {
            name: name.to_string(),
            enabled: true,
            x,
            y,
            extra: std::collections::HashMap::new(),
        }
    }
}

/// A loaded Lua plugin with its environment registry key.
struct LoadedPlugin {
    name: String,
    meta: PluginMeta,
    env_key: LuaRegistryKey,
}

/// The Lua plugin runtime. Owns the Lua VM and all loaded plugins.
pub struct PluginRuntime {
    lua: Lua,
    plugins: Vec<LoadedPlugin>,
    indicators: Arc<Mutex<HashMap<String, Indicator>>>,
}

impl PluginRuntime {
    pub fn new() -> Self {
        Self {
            lua: Lua::new(),
            plugins: Vec::new(),
            indicators: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Load a Lua plugin from source code, execute it in a sandbox, call on_load(config).
    pub fn load_plugin_from_str(
        &mut self,
        name: &str,
        source: &str,
        config: &PluginConfig,
    ) -> Result<(), String> {
        // Build sandboxed _ENV table with safe stdlib subset + our API functions
        let env = self.build_env(name).map_err(|e| format!("env build: {e}"))?;

        // Load and execute chunk with sandboxed environment
        self.lua
            .load(source)
            .set_name(name)
            .set_environment(env.clone())
            .exec()
            .map_err(|e| format!("load {name}: {e}"))?;

        // Read plugin metadata from the `plugin` table in _ENV
        let meta = Self::read_meta(&env).map_err(|e| format!("meta {name}: {e}"))?;

        // Build config table for on_load
        let config_table = self.build_config_table(config)
            .map_err(|e| format!("config {name}: {e}"))?;

        // Call on_load(config) if it exists
        if let Ok(on_load) = env.get::<LuaFunction>("on_load") {
            on_load
                .call::<()>(config_table)
                .map_err(|e| format!("on_load {name}: {e}"))?;
        }

        // Store environment in registry for later tick_epoch calls
        let env_key = self.lua.create_registry_value(env)
            .map_err(|e| format!("registry {name}: {e}"))?;

        self.plugins.push(LoadedPlugin {
            name: name.to_string(),
            meta,
            env_key,
        });

        Ok(())
    }

    /// Call on_epoch(state) on every loaded plugin. Errors are caught and logged.
    pub fn tick_epoch(&self, epoch_state: &state::EpochState) {
        for plugin in &self.plugins {
            if let Err(e) = self.tick_one(plugin, epoch_state) {
                log::warn!("plugin {} on_epoch error: {}", plugin.name, e);
            }
        }
    }

    /// Return a snapshot of all registered indicators.
    pub fn get_indicators(&self) -> Vec<Indicator> {
        self.indicators.lock().unwrap().values().cloned().collect()
    }

    /// Number of loaded plugins.
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Return metadata for all loaded plugins.
    pub fn get_plugin_info(&self) -> Vec<PluginMeta> {
        self.plugins.iter().map(|p| p.meta.clone()).collect()
    }

    // ── private helpers ──────────────────────────────────────────────

    /// Build a sandboxed _ENV table with safe stdlib + our API functions.
    fn build_env(&self, _plugin_name: &str) -> LuaResult<LuaTable> {
        let env = self.lua.create_table()?;

        // Copy safe globals from the real global table
        let globals = self.lua.globals();
        let safe_keys = [
            "assert", "error", "ipairs", "next", "pairs", "pcall",
            "print", "select", "tonumber", "tostring", "type", "unpack",
            "xpcall", "rawequal", "rawget", "rawlen", "rawset",
            "string", "table", "math",
        ];
        for key in &safe_keys {
            if let Ok(val) = globals.get::<LuaValue>(*key) {
                env.set(*key, val)?;
            }
        }

        // register_indicator(name, opts)
        let indicators = Arc::clone(&self.indicators);
        let register_fn = self.lua.create_function(move |_lua, (ind_name, opts): (String, LuaTable)| {
            let x: i32 = opts.get("x").unwrap_or(0);
            let y: i32 = opts.get("y").unwrap_or(0);
            let font_str: String = opts.get("font").unwrap_or_else(|_| "small".to_string());
            let label: Option<String> = opts.get("label").ok();
            let wrap_width: u32 = opts.get("wrap_width").unwrap_or(0);

            let font = match font_str.as_str() {
                "medium" => IndicatorFont::Medium,
                _ => IndicatorFont::Small,
            };

            let indicator = Indicator {
                name: ind_name.clone(),
                value: String::new(),
                x,
                y,
                label,
                font,
                wrap_width,
            };

            indicators.lock().unwrap().insert(ind_name, indicator);
            Ok(())
        })?;
        env.set("register_indicator", register_fn)?;

        // set_indicator(name, value)
        let indicators = Arc::clone(&self.indicators);
        let set_fn = self.lua.create_function(move |_lua, (ind_name, value): (String, String)| {
            let mut map = indicators.lock().unwrap();
            if let Some(ind) = map.get_mut(&ind_name) {
                ind.value = value;
            }
            Ok(())
        })?;
        env.set("set_indicator", set_fn)?;

        // format_duration(secs) -> "HH:MM:SS"
        let fmt_fn = self.lua.create_function(|_lua, secs: u64| {
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            let s = secs % 60;
            Ok(format!("{:02}:{:02}:{:02}", h, m, s))
        })?;
        env.set("format_duration", fmt_fn)?;

        // log(msg)
        let log_fn = self.lua.create_function(|_lua, msg: String| {
            log::info!("[lua] {}", msg);
            Ok(())
        })?;
        env.set("log", log_fn)?;

        Ok(env)
    }

    /// Read PluginMeta from the `plugin` table in the given environment.
    fn read_meta(env: &LuaTable) -> LuaResult<PluginMeta> {
        let plugin_table: LuaTable = env.get("plugin")?;
        Ok(PluginMeta {
            name: plugin_table.get::<String>("name")?,
            version: plugin_table.get::<String>("version")?,
            author: plugin_table.get::<String>("author")?,
            tag: plugin_table.get("tag").unwrap_or_else(|_| "default".to_string()),
        })
    }

    /// Build a Lua table from PluginConfig to pass to on_load().
    fn build_config_table(&self, config: &PluginConfig) -> LuaResult<LuaTable> {
        let t = self.lua.create_table()?;
        t.set("name", config.name.as_str())?;
        t.set("enabled", config.enabled)?;
        t.set("x", config.x)?;
        t.set("y", config.y)?;
        for (k, v) in &config.extra {
            t.set(k.as_str(), v.as_str())?;
        }
        Ok(t)
    }

    /// Call on_epoch for a single plugin. Returns error if the call fails.
    fn tick_one(&self, plugin: &LoadedPlugin, epoch_state: &state::EpochState) -> Result<(), String> {
        let env: LuaTable = self.lua.registry_value(&plugin.env_key)
            .map_err(|e| format!("registry: {e}"))?;

        let on_epoch: LuaFunction = match env.get("on_epoch") {
            Ok(f) => f,
            Err(_) => return Ok(()), // no on_epoch defined, skip
        };

        let state_table = epoch_state.to_lua_table(&self.lua)
            .map_err(|e| format!("state table: {e}"))?;

        on_epoch.call::<()>(state_table)
            .map_err(|e| format!("{e}"))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indicator_default_fields() {
        let ind = Indicator {
            name: "test".into(),
            value: "hello".into(),
            x: 10,
            y: 20,
            label: None,
            font: IndicatorFont::Small,
            wrap_width: 0,
        };
        assert_eq!(ind.name, "test");
        assert_eq!(ind.value, "hello");
        assert_eq!(ind.x, 10);
        assert_eq!(ind.y, 20);
        assert!(ind.label.is_none());
        assert_eq!(ind.font, IndicatorFont::Small);
        assert_eq!(ind.wrap_width, 0);
    }

    #[test]
    fn test_indicator_with_label() {
        let ind = Indicator {
            name: "uptime".into(),
            value: "02:15".into(),
            x: 185,
            y: 0,
            label: Some("UP".into()),
            font: IndicatorFont::Small,
            wrap_width: 0,
        };
        assert_eq!(ind.label, Some("UP".into()));
    }

    #[test]
    fn test_indicator_with_wrap() {
        let ind = Indicator {
            name: "status".into(),
            value: "Sniffing the airwaves today".into(),
            x: 125,
            y: 20,
            label: None,
            font: IndicatorFont::Medium,
            wrap_width: 17,
        };
        assert_eq!(ind.wrap_width, 17);
        assert_eq!(ind.font, IndicatorFont::Medium);
    }

    #[test]
    fn test_plugin_config_default() {
        let cfg = PluginConfig::default_for("uptime", 185, 0);
        assert_eq!(cfg.name, "uptime");
        assert!(cfg.enabled);
        assert_eq!(cfg.x, 185);
        assert_eq!(cfg.y, 0);
        assert!(cfg.extra.is_empty());
    }

    #[test]
    fn test_plugin_meta() {
        let meta = PluginMeta {
            name: "uptime".into(),
            version: "1.0.0".into(),
            author: "oxigotchi".into(),
            tag: "default".into(),
        };
        assert_eq!(meta.tag, "default");
    }

    #[test]
    fn test_load_plugin_from_string() {
        let mut rt = PluginRuntime::new();
        let lua_code = r#"
            plugin = {}
            plugin.name = "test_plugin"
            plugin.version = "1.0.0"
            plugin.author = "tester"
            plugin.tag = "default"

            function on_load(config)
                register_indicator("test_ind", {
                    x = config.x or 10,
                    y = config.y or 20,
                    font = "small",
                })
            end

            function on_epoch(state)
                set_indicator("test_ind", "hello " .. state.epoch)
            end
        "#;
        let config = PluginConfig::default_for("test_plugin", 10, 20);
        rt.load_plugin_from_str("test_plugin", lua_code, &config).unwrap();
        assert_eq!(rt.plugin_count(), 1);

        let indicators = rt.get_indicators();
        assert_eq!(indicators.len(), 1);
        assert_eq!(indicators[0].name, "test_ind");
        assert_eq!(indicators[0].x, 10);
        assert_eq!(indicators[0].y, 20);
    }

    #[test]
    fn test_tick_epoch_updates_indicator() {
        let mut rt = PluginRuntime::new();
        let lua_code = r#"
            plugin = {}
            plugin.name = "ticker"
            plugin.version = "1.0.0"
            plugin.author = "tester"
            plugin.tag = "default"

            function on_load(config)
                register_indicator("val", { x = 0, y = 0, font = "small" })
            end

            function on_epoch(state)
                set_indicator("val", "epoch:" .. state.epoch)
            end
        "#;
        let config = PluginConfig::default_for("ticker", 0, 0);
        rt.load_plugin_from_str("ticker", lua_code, &config).unwrap();

        let state = state::EpochState { epoch: 42, ..Default::default() };
        rt.tick_epoch(&state);

        let indicators = rt.get_indicators();
        assert_eq!(indicators[0].value, "epoch:42");
    }

    #[test]
    fn test_plugin_error_does_not_crash() {
        let mut rt = PluginRuntime::new();
        let lua_code = r#"
            plugin = {}
            plugin.name = "bad"
            plugin.version = "1.0.0"
            plugin.author = "tester"
            plugin.tag = "default"

            function on_load(config) end

            function on_epoch(state)
                error("intentional error")
            end
        "#;
        let config = PluginConfig::default_for("bad", 0, 0);
        rt.load_plugin_from_str("bad", lua_code, &config).unwrap();

        let state = state::EpochState::default();
        // Should not panic — errors are caught and logged
        rt.tick_epoch(&state);
    }

    #[test]
    fn test_format_duration_available_in_lua() {
        let mut rt = PluginRuntime::new();
        let lua_code = r#"
            plugin = {}
            plugin.name = "fmt"
            plugin.version = "1.0.0"
            plugin.author = "tester"
            plugin.tag = "default"

            function on_load(config)
                register_indicator("dur", { x = 0, y = 0, font = "small" })
            end

            function on_epoch(state)
                set_indicator("dur", format_duration(state.uptime_secs))
            end
        "#;
        let config = PluginConfig::default_for("fmt", 0, 0);
        rt.load_plugin_from_str("fmt", lua_code, &config).unwrap();

        let state = state::EpochState { uptime_secs: 3661, ..Default::default() };
        rt.tick_epoch(&state);

        let indicators = rt.get_indicators();
        assert_eq!(indicators[0].value, "01:01:01");
    }

    #[test]
    fn test_wrap_width_from_lua() {
        let mut rt = PluginRuntime::new();
        let lua_code = r#"
            plugin = {}
            plugin.name = "wrap"
            plugin.version = "1.0.0"
            plugin.author = "tester"
            plugin.tag = "default"

            function on_load(config)
                register_indicator("msg", {
                    x = 125, y = 20,
                    font = "medium",
                    wrap_width = 17,
                })
            end

            function on_epoch(state) end
        "#;
        let config = PluginConfig::default_for("wrap", 125, 20);
        rt.load_plugin_from_str("wrap", lua_code, &config).unwrap();

        let indicators = rt.get_indicators();
        assert_eq!(indicators[0].wrap_width, 17);
        assert_eq!(indicators[0].font, IndicatorFont::Medium);
    }

    #[test]
    fn test_multiple_plugins() {
        let mut rt = PluginRuntime::new();
        for i in 0..3 {
            let code = format!(r#"
                plugin = {{}}
                plugin.name = "p{i}"
                plugin.version = "1.0.0"
                plugin.author = "t"
                plugin.tag = "default"
                function on_load(config)
                    register_indicator("ind{i}", {{ x = {x}, y = 0, font = "small" }})
                end
                function on_epoch(state)
                    set_indicator("ind{i}", "v{i}")
                end
            "#, i = i, x = i * 50);
            let config = PluginConfig::default_for(&format!("p{i}"), (i * 50) as i32, 0);
            rt.load_plugin_from_str(&format!("p{i}"), &code, &config).unwrap();
        }

        assert_eq!(rt.plugin_count(), 3);

        let state = state::EpochState::default();
        rt.tick_epoch(&state);

        let indicators = rt.get_indicators();
        assert_eq!(indicators.len(), 3);
    }
}
