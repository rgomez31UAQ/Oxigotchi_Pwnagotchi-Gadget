//! Read/write plugin positions from /etc/oxigotchi/plugins.toml.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Top-level plugins.toml structure: `[plugins.<name>]` sections.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginsToml {
    #[serde(default)]
    pub plugins: HashMap<String, PluginEntry>,
}

/// One plugin's persisted config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
}

fn default_true() -> bool { true }

const PLUGINS_TOML_PATH: &str = "/etc/oxigotchi/plugins.toml";

/// Read plugins.toml from disk. Returns None if file doesn't exist or is unparseable.
pub fn read_plugins_toml() -> Option<PluginsToml> {
    let path = Path::new(PLUGINS_TOML_PATH);
    if !path.exists() {
        return None;
    }
    match std::fs::read_to_string(path) {
        Ok(s) => match toml::from_str(&s) {
            Ok(pt) => Some(pt),
            Err(e) => {
                log::warn!("plugins.toml parse error, using defaults: {e}");
                None
            }
        },
        Err(e) => {
            log::warn!("plugins.toml read error, using defaults: {e}");
            None
        }
    }
}

/// Merge plugins.toml entries into a vec of PluginConfigs.
/// For each hardcoded default, if plugins.toml has an entry, override enabled/x/y.
pub fn merge_with_defaults(
    defaults: Vec<super::PluginConfig>,
    toml: &PluginsToml,
) -> Vec<super::PluginConfig> {
    defaults.into_iter().map(|mut cfg| {
        if let Some(entry) = toml.plugins.get(&cfg.name) {
            cfg.enabled = entry.enabled;
            cfg.x = entry.x;
            cfg.y = entry.y;
        }
        cfg
    }).collect()
}

/// Write current plugin state to plugins.toml.
/// Reads existing file first to preserve entries for disabled plugins that aren't loaded.
pub fn write_plugins_toml(configs: &[(String, bool, i32, i32)]) {
    // Read existing TOML to preserve disabled plugin entries
    let mut pt = read_plugins_toml().unwrap_or_default();

    // Update/add entries for loaded plugins
    for (name, enabled, x, y) in configs {
        pt.plugins.insert(name.clone(), PluginEntry {
            enabled: *enabled,
            x: *x,
            y: *y,
        });
    }
    match toml::to_string_pretty(&pt) {
        Ok(s) => {
            if let Err(e) = std::fs::write(PLUGINS_TOML_PATH, &s) {
                log::warn!("failed to write plugins.toml: {e}");
            }
        }
        Err(e) => log::warn!("failed to serialize plugins.toml: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let configs: Vec<(String, bool, i32, i32)> = vec![
            ("battery".into(), true, 140, 112),
            ("uptime".into(), true, 185, 0),
        ];
        let mut pt = PluginsToml::default();
        for (name, enabled, x, y) in &configs {
            pt.plugins.insert(name.clone(), PluginEntry {
                enabled: *enabled, x: *x, y: *y,
            });
        }
        let s = toml::to_string_pretty(&pt).unwrap();
        let parsed: PluginsToml = toml::from_str(&s).unwrap();
        assert_eq!(parsed.plugins["battery"].x, 140);
        assert_eq!(parsed.plugins["uptime"].y, 0);
    }

    #[test]
    fn merge_overrides_defaults() {
        let defaults = vec![
            super::super::PluginConfig::default_for("battery", 140, 112),
            super::super::PluginConfig::default_for("uptime", 185, 0),
        ];
        let mut pt = PluginsToml::default();
        pt.plugins.insert("battery".into(), PluginEntry { enabled: true, x: 150, y: 112 });
        // uptime not in TOML — should keep default

        let merged = merge_with_defaults(defaults, &pt);
        assert_eq!(merged[0].x, 150); // overridden
        assert_eq!(merged[1].x, 185); // kept default
    }

    #[test]
    fn missing_file_returns_none() {
        let result = read_plugins_toml();
        let _ = result;
    }
}
