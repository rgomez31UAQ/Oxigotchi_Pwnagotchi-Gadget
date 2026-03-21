use serde::Deserialize;
use std::path::Path;

/// Top-level configuration, matching pwnagotchi's TOML format.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_main")]
    pub main: MainConfig,
    #[serde(default)]
    pub ui: UiConfig,

    // Convenience accessors populated after deserialization
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    pub whitelist: Vec<String>,
    #[serde(skip)]
    pub display: DisplayConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MainConfig {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default)]
    pub whitelist: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UiConfig {
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub fps: u32,
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub font: FontConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DisplayConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(rename = "type", default = "default_display_type")]
    pub display_type: String,
    #[serde(default)]
    pub rotation: u16,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            display_type: "waveshare_4".into(),
            rotation: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FontConfig {
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
    }
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
    fn populate_shortcuts(&mut self) {
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
        assert_eq!(cfg.display.rotation, 0);
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
}
