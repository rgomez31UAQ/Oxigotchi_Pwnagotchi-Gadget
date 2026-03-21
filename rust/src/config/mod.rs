//! Configuration loading and TOML parsing.
//!
//! Reads the pwnagotchi-compatible `config.toml` and exposes typed fields
//! for the daemon to consume.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Top-level configuration, matching pwnagotchi's TOML format.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Main section (name, whitelist, etc.).
    #[serde(default = "default_main")]
    pub main: MainConfig,
    /// UI section (display, font, etc.).
    #[serde(default)]
    pub ui: UiConfig,

    // Convenience accessors populated after deserialization
    /// Shortcut for `main.name`.
    #[serde(skip)]
    pub name: String,
    /// Shortcut for `main.whitelist`.
    #[serde(skip)]
    pub whitelist: Vec<String>,
    /// Shortcut for `ui.display`.
    #[serde(skip)]
    pub display: DisplayConfig,
}

/// The `[main]` TOML section.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MainConfig {
    /// Device name displayed on screen and in the web dashboard.
    #[serde(default = "default_name")]
    pub name: String,
    /// SSIDs or BSSIDs to never attack.
    #[serde(default)]
    pub whitelist: Vec<String>,
    /// Language code (e.g. "en").
    #[serde(default = "default_lang")]
    pub lang: String,
}

/// The `[ui]` TOML section.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct UiConfig {
    /// Whether to invert the display colors.
    #[serde(default)]
    pub invert: bool,
    /// Target frames per second (0 = default).
    #[serde(default, deserialize_with = "deserialize_fps")]
    pub fps: u32,
    /// Display hardware settings.
    #[serde(default)]
    pub display: DisplayConfig,
    /// Font settings.
    #[serde(default)]
    pub font: FontConfig,
}

/// The `[ui.display]` TOML section.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DisplayConfig {
    /// Whether the display is enabled at all.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Display driver type (e.g. "waveshare_4", "inky").
    #[serde(rename = "type", default = "default_display_type")]
    pub display_type: String,
    /// Screen rotation in degrees (0, 90, 180, 270).
    #[serde(default)]
    pub rotation: u16,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            display_type: "waveshare_4".into(),
            rotation: 180,
        }
    }
}

/// The `[ui.font]` TOML section.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FontConfig {
    /// Font family name.
    #[serde(default = "default_font_name")]
    pub name: String,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            name: default_font_name(),
        }
    }
}

fn default_main() -> MainConfig {
    MainConfig {
        name: default_name(),
        whitelist: Vec::new(),
        lang: default_lang(),
    }
}

fn default_lang() -> String {
    "en".into()
}

fn deserialize_fps<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    struct FpsVisitor;
    impl<'de> de::Visitor<'de> for FpsVisitor {
        type Value = u32;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an integer or float for fps")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u32, E> {
            Ok(v as u32)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u32, E> {
            Ok(v.max(0) as u32)
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<u32, E> {
            Ok(v as u32)
        }
    }
    deserializer.deserialize_any(FpsVisitor)
}

fn default_name() -> String {
    "oxigotchi".into()
}

fn default_true() -> bool {
    true
}

fn default_display_type() -> String {
    "waveshare_4".into()
}

fn default_font_name() -> String {
    "DejaVuSansMono".into()
}

impl Config {
    /// Load configuration from a TOML file, falling back to defaults if the file is
    /// missing or unparseable.
    pub fn load_or_default(path: &str) -> Self {
        if Path::new(path).exists() {
            match std::fs::read_to_string(path) {
                Ok(contents) => match Self::from_toml(&contents) {
                    Ok(cfg) => return cfg,
                    Err(e) => {
                        log::warn!("Failed to parse config {}: {} — using defaults", path, e);
                    }
                },
                Err(e) => {
                    log::warn!("Failed to read config {}: {} — using defaults", path, e);
                }
            }
        } else {
            log::info!("Config file {} not found — using defaults", path);
        }
        Self::defaults()
    }

    /// Parse configuration from a TOML string.
    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        let mut cfg: Config = toml::from_str(toml_str)?;
        cfg.populate_shortcuts();
        Ok(cfg)
    }

    /// Return a default configuration.
    pub fn defaults() -> Self {
        let mut cfg = Config {
            main: default_main(),
            ui: UiConfig::default(),
            name: String::new(),
            whitelist: Vec::new(),
            display: DisplayConfig::default(),
        };
        cfg.populate_shortcuts();
        cfg
    }

    /// Copy nested fields into top-level convenience fields.
    pub(crate) fn populate_shortcuts(&mut self) {
        self.name = self.main.name.clone();
        self.whitelist = self.main.whitelist.clone();
        self.display = self.ui.display.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TOML: &str = r#"
[main]
name = "oxigotchi"
whitelist = ["Alpha", "Alpha 5G"]

[ui]
invert = true
fps = 0

[ui.display]
enabled = true
type = "waveshare_4"
rotation = 180

[ui.font]
name = "DejaVuSansMono"
"#;

    #[test]
    fn test_parse_full_config() {
        let cfg = Config::from_toml(SAMPLE_TOML).unwrap();
        assert_eq!(cfg.name, "oxigotchi");
        assert_eq!(cfg.whitelist, vec!["Alpha", "Alpha 5G"]);
        assert!(cfg.ui.invert);
        assert_eq!(cfg.ui.fps, 0);
        assert!(cfg.display.enabled);
        assert_eq!(cfg.display.display_type, "waveshare_4");
        assert_eq!(cfg.display.rotation, 180);
        assert_eq!(cfg.ui.font.name, "DejaVuSansMono");
    }

    #[test]
    fn test_defaults() {
        let cfg = Config::defaults();
        assert_eq!(cfg.name, "oxigotchi");
        assert!(cfg.whitelist.is_empty());
        assert!(cfg.display.enabled);
        assert_eq!(cfg.display.display_type, "waveshare_4");
        assert_eq!(cfg.display.rotation, 180);
    }

    #[test]
    fn test_partial_config() {
        let toml = r#"
[main]
name = "mybot"
"#;
        let cfg = Config::from_toml(toml).unwrap();
        assert_eq!(cfg.name, "mybot");
        assert!(cfg.whitelist.is_empty());
        assert!(cfg.display.enabled); // default
    }

    #[test]
    fn test_empty_config() {
        let cfg = Config::from_toml("").unwrap();
        assert_eq!(cfg.name, "oxigotchi");
    }

    #[test]
    fn test_load_missing_file() {
        let cfg = Config::load_or_default("/nonexistent/config.toml");
        assert_eq!(cfg.name, "oxigotchi");
    }

    #[test]
    fn test_display_type_rename() {
        // Ensure `type` field works (it's a reserved word, uses #[serde(rename)])
        let toml = r#"
[ui.display]
type = "inky"
"#;
        let cfg = Config::from_toml(toml).unwrap();
        assert_eq!(cfg.display.display_type, "inky");
    }

    #[test]
    fn test_partial_config_ui_only() {
        let toml = r#"
[ui]
invert = true
"#;
        let cfg = Config::from_toml(toml).unwrap();
        assert_eq!(cfg.name, "oxigotchi"); // default name
        assert!(cfg.ui.invert);
        assert!(cfg.display.enabled); // default
    }

    #[test]
    fn test_partial_config_display_only() {
        let toml = r#"
[ui.display]
rotation = 90
"#;
        let cfg = Config::from_toml(toml).unwrap();
        assert_eq!(cfg.display.rotation, 90);
        assert!(cfg.display.enabled); // default
        assert_eq!(cfg.display.display_type, "waveshare_4"); // default
    }

    #[test]
    fn test_invalid_toml_returns_error() {
        let result = Config::from_toml("not valid [[ toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_fields_ignored() {
        let toml = r#"
[main]
name = "test"
unknown_field = 42

[ui]
some_future_option = true
"#;
        // serde should ignore unknown fields (default behavior)
        let cfg = Config::from_toml(toml).unwrap();
        assert_eq!(cfg.name, "test");
    }
}
