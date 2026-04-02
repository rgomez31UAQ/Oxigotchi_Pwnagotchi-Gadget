//! Migration tooling: import existing pwnagotchi configs and captures.
//!
//! Handles first-boot detection, config extraction from full pwnagotchi TOML
//! (with personality, bettercap, plugins sections), capture dedup, backup
//! creation, and systemd service generation.

use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Result of a migration operation.
#[derive(Debug)]
pub struct MigrationResult {
    pub config_migrated: bool,
    pub config_backed_up: bool,
    pub captures_found: usize,
    pub captures_imported: usize,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl MigrationResult {
    /// Returns `true` if no errors occurred during migration.
    pub fn success(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Paths used by the legacy pwnagotchi installation.
#[derive(Debug, Clone)]
pub struct LegacyPaths {
    /// Pwnagotchi config file.
    pub config: PathBuf,
    /// Directories containing handshake pcapng files (searched in order).
    pub handshake_dirs: Vec<PathBuf>,
    /// Directory for plugins.
    pub plugins: PathBuf,
}

impl Default for LegacyPaths {
    fn default() -> Self {
        Self {
            config: PathBuf::from("/etc/pwnagotchi/config.toml"),
            handshake_dirs: vec![
                PathBuf::from("/home/pi/handshakes"),
                PathBuf::from("/etc/pwnagotchi/handshakes"),
            ],
            plugins: PathBuf::from("/usr/local/share/pwnagotchi/custom-plugins"),
        }
    }
}

/// Oxigotchi installation paths.
#[derive(Debug, Clone)]
pub struct OxiPaths {
    /// Main config file.
    pub config: PathBuf,
    /// Capture/handshake directory.
    pub captures: PathBuf,
    /// Log directory.
    pub logs: PathBuf,
    /// Systemd service file.
    pub service: PathBuf,
    /// First-boot sentinel file.
    pub sentinel: PathBuf,
}

impl Default for OxiPaths {
    fn default() -> Self {
        Self {
            config: PathBuf::from("/etc/oxigotchi/config.toml"),
            captures: PathBuf::from("/home/pi/captures"),
            logs: PathBuf::from("/var/log/oxigotchi"),
            service: PathBuf::from("/etc/systemd/system/oxigotchi.service"),
            sentinel: PathBuf::from("/var/lib/.rusty-first-boot"),
        }
    }
}

// ── Pwnagotchi config parsing (superset of what Rusty uses) ──────────────

/// Full pwnagotchi config structure — we parse everything, extract what we need.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PwnagotchiConfig {
    #[serde(default)]
    pub main: PwnMain,
    #[serde(default)]
    pub ui: PwnUi,
    #[serde(default)]
    pub personality: PwnPersonality,
    #[serde(default)]
    pub bettercap: PwnBettercap,
    #[serde(default)]
    pub plugins: PwnPlugins,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PwnMain {
    #[serde(default = "default_pwn_name")]
    pub name: String,
    #[serde(default)]
    pub whitelist: Vec<String>,
    #[serde(default = "default_lang")]
    pub lang: String,
}

impl Default for PwnMain {
    fn default() -> Self {
        Self {
            name: default_pwn_name(),
            whitelist: Vec::new(),
            lang: default_lang(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PwnUi {
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub display: PwnDisplay,
    #[serde(default)]
    pub font: PwnFont,
    #[serde(default)]
    pub faces: PwnFaces,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PwnDisplay {
    #[serde(rename = "type", default = "default_display_type")]
    pub display_type: String,
    #[serde(default)]
    pub rotation: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for PwnDisplay {
    fn default() -> Self {
        Self {
            display_type: default_display_type(),
            rotation: 0,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PwnFont {
    #[serde(default = "default_font_name")]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PwnFaces {
    /// Whether to use PNG faces instead of kaomoji.
    #[serde(default)]
    pub png: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PwnPersonality {
    /// List of WiFi channels to hop.
    #[serde(default)]
    pub channels: Vec<u8>,
    /// Whether deauth attacks are enabled.
    #[serde(default = "default_true")]
    pub deauth: bool,
    /// Whether association attacks are enabled.
    #[serde(default = "default_true")]
    pub associate: bool,
}

impl Default for PwnPersonality {
    fn default() -> Self {
        Self {
            channels: Vec::new(),
            deauth: true,
            associate: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PwnBettercap {
    #[serde(default)]
    pub scheme: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub port: u16,
}

/// Plugin settings — we only care about angryoxide if present.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PwnPlugins {
    #[serde(default)]
    pub angryoxide: Option<PwnAngryOxide>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PwnAngryOxide {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ao_rate")]
    pub rate: u32,
    #[serde(default = "default_ao_dwell")]
    pub dwell: u32,
    #[serde(default = "default_ao_interface")]
    pub interface: String,
    #[serde(default = "default_ao_output_dir")]
    pub output_dir: String,
}

fn default_pwn_name() -> String {
    "pwnagotchi".into()
}
fn default_lang() -> String {
    "en".into()
}
fn default_display_type() -> String {
    "waveshare_4".into()
}
fn default_font_name() -> String {
    "DejaVuSansMono".into()
}
fn default_true() -> bool {
    true
}
fn default_ao_rate() -> u32 {
    1
}
fn default_ao_dwell() -> u32 {
    5
}
fn default_ao_interface() -> String {
    "wlan0mon".into()
}
fn default_ao_output_dir() -> String {
    "/home/pi/handshakes/".into()
}

// ── Config validation ────────────────────────────────────────────────────

/// Validation result for a Rusty config.
#[derive(Debug)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Valid rotation values.
const VALID_ROTATIONS: &[u16] = &[0, 90, 180, 270];

/// Known display types.
const KNOWN_DISPLAY_TYPES: &[&str] = &[
    "waveshare_4",
    "waveshare_3",
    "waveshare_27",
    "waveshare_213",
    "waveshare_154",
    "inky",
    "papirus",
    "dfrobot",
    "lcdhat",
    "displayhatmini",
];

/// Validate a Rusty config, returning errors for fatal issues and warnings
/// for non-fatal ones.
pub fn validate_config(cfg: &crate::config::Config) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Display type is required (empty string is bad)
    if cfg.ui.display.display_type.is_empty() {
        errors.push("ui.display.type is empty — must specify a display driver".into());
    } else if !KNOWN_DISPLAY_TYPES.contains(&cfg.ui.display.display_type.as_str()) {
        warnings.push(format!(
            "ui.display.type '{}' is not a known display type (known: {})",
            cfg.ui.display.display_type,
            KNOWN_DISPLAY_TYPES.join(", ")
        ));
    }

    // Rotation must be valid
    if !VALID_ROTATIONS.contains(&cfg.ui.display.rotation) {
        errors.push(format!(
            "ui.display.rotation {} is invalid — must be one of {:?}",
            cfg.ui.display.rotation, VALID_ROTATIONS
        ));
    }

    // Name should not be empty
    if cfg.main.name.is_empty() {
        warnings.push("main.name is empty — will use default 'oxigotchi'".into());
    }

    ValidationResult {
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

// ── Config extraction (pwnagotchi -> Rusty) ──────────────────────────────

/// Extract a Rusty config from a pwnagotchi config.
/// Only copies fields Rusty actually uses; discards bettercap, most personality, etc.
pub fn extract_rusty_config(pwn: &PwnagotchiConfig) -> crate::config::Config {
    let mut cfg = crate::config::Config {
        main: crate::config::MainConfig {
            name: pwn.main.name.clone(),
            whitelist: pwn.main.whitelist.clone(),
            lang: pwn.main.lang.clone(),
            default_mode: "SAFE".into(),
        },
        ui: crate::config::UiConfig {
            invert: pwn.ui.invert,
            fps: 0,
            display: crate::config::DisplayConfig {
                enabled: pwn.ui.display.enabled,
                display_type: pwn.ui.display.display_type.clone(),
                rotation: pwn.ui.display.rotation,
                invert: pwn.ui.invert,
            },
            font: crate::config::FontConfig {
                name: pwn.ui.font.name.clone(),
            },
        },
        bluetooth: crate::config::BluetoothConfig::default(),
        bt_feature: crate::bluetooth::model::config::BtFeatureConfig::default(),
        bt_attacks: crate::bluetooth::attacks::BtAttackConfig::default(),
        gpu: crate::gpu::config::GpuFeatureConfig::default(),
        qpu: crate::qpu::QpuFeatureConfig::default(),
        name: String::new(),
        whitelist: Vec::new(),
        display: crate::config::DisplayConfig::default(),
    };
    cfg.populate_shortcuts();
    cfg
}

/// Parse a pwnagotchi config TOML string.
pub fn parse_pwnagotchi_config(toml_str: &str) -> Result<PwnagotchiConfig, String> {
    toml::from_str(toml_str).map_err(|e| format!("Failed to parse pwnagotchi config: {e}"))
}

/// Write a Rusty config to a TOML string.
pub fn config_to_toml(cfg: &crate::config::Config) -> Result<String, String> {
    toml::to_string_pretty(cfg).map_err(|e| format!("Failed to serialize config: {e}"))
}

// ── Config migration (file-level) ────────────────────────────────────────

/// Migrate a pwnagotchi config to Rusty format.
/// Reads the source, parses as pwnagotchi TOML, extracts Rusty fields,
/// writes a clean Rusty config.toml.
pub fn migrate_config(from: &Path, to: &Path) -> Result<crate::config::Config, String> {
    if !from.exists() {
        return Err(format!("Source config not found: {}", from.display()));
    }

    let content = std::fs::read_to_string(from)
        .map_err(|e| format!("Failed to read {}: {}", from.display(), e))?;

    let pwn = parse_pwnagotchi_config(&content)?;
    let cfg = extract_rusty_config(&pwn);

    let output = config_to_toml(&cfg)?;

    // Create target directory if needed
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create dir {}: {}", parent.display(), e))?;
    }

    std::fs::write(to, &output).map_err(|e| format!("Failed to write {}: {}", to.display(), e))?;

    Ok(cfg)
}

// ── Backup ───────────────────────────────────────────────────────────────

/// Backup a config file by copying it to `<path>.pwnagotchi-backup`.
/// Returns the backup path on success.
pub fn backup_config(config_path: &Path) -> Result<PathBuf, String> {
    if !config_path.exists() {
        return Err(format!(
            "Cannot backup — file not found: {}",
            config_path.display()
        ));
    }

    let mut backup_path = config_path.as_os_str().to_owned();
    backup_path.push(".pwnagotchi-backup");
    let backup_path = PathBuf::from(backup_path);

    std::fs::copy(config_path, &backup_path).map_err(|e| {
        format!(
            "Failed to backup {} -> {}: {}",
            config_path.display(),
            backup_path.display(),
            e
        )
    })?;

    Ok(backup_path)
}

// ── Capture import with dedup ────────────────────────────────────────────

/// Count pcapng files in a directory (non-recursive).
pub fn count_captures(dir: &Path) -> Result<usize, String> {
    if !dir.exists() {
        return Ok(0);
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read dir {}: {}", dir.display(), e))?;
    let count = entries
        .filter_map(|e| e.ok())
        .filter(|e| is_capture_file(&e.path()))
        .count();
    Ok(count)
}

/// Check if a path is a capture file (pcapng or pcap extension).
fn is_capture_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext == "pcapng" || ext == "pcap")
}

/// Import captures from multiple source directories into a single destination,
/// deduplicating by filename. Files with the same name are skipped.
/// Returns (total_found, imported_count).
pub fn import_captures_dedup(
    source_dirs: &[PathBuf],
    dest: &Path,
) -> Result<(usize, usize), String> {
    std::fs::create_dir_all(dest)
        .map_err(|e| format!("Failed to create dir {}: {}", dest.display(), e))?;

    let mut seen_filenames: HashSet<String> = HashSet::new();
    let mut total_found = 0usize;
    let mut imported = 0usize;

    // Also track what already exists in dest
    if dest.exists() {
        if let Ok(entries) = std::fs::read_dir(dest) {
            for entry in entries.filter_map(|e| e.ok()) {
                if is_capture_file(&entry.path()) {
                    if let Some(name) = entry.path().file_name() {
                        seen_filenames.insert(name.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    for src_dir in source_dirs {
        if !src_dir.exists() {
            continue;
        }
        let entries = std::fs::read_dir(src_dir)
            .map_err(|e| format!("Failed to read dir {}: {}", src_dir.display(), e))?;

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !is_capture_file(&path) {
                continue;
            }
            total_found += 1;

            let filename = path.file_name().unwrap().to_string_lossy().to_string();

            if seen_filenames.contains(&filename) {
                continue; // dedup: already imported or exists at dest
            }

            let dest_file = dest.join(&filename);
            std::fs::copy(&path, &dest_file)
                .map_err(|e| format!("Failed to copy {}: {}", path.display(), e))?;
            seen_filenames.insert(filename);
            imported += 1;
        }
    }

    Ok((total_found, imported))
}

/// Copy capture files from source to destination directory (simple, no dedup).
pub fn import_captures(from: &Path, to: &Path) -> Result<usize, String> {
    if !from.exists() {
        return Ok(0);
    }

    std::fs::create_dir_all(to)
        .map_err(|e| format!("Failed to create dir {}: {}", to.display(), e))?;

    let entries = std::fs::read_dir(from)
        .map_err(|e| format!("Failed to read dir {}: {}", from.display(), e))?;

    let mut imported = 0;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if is_capture_file(&path) {
            let dest = to.join(path.file_name().unwrap());
            if !dest.exists() {
                std::fs::copy(&path, &dest)
                    .map_err(|e| format!("Failed to copy {}: {}", path.display(), e))?;
                imported += 1;
            }
        }
    }
    Ok(imported)
}

// ── First-boot detection ─────────────────────────────────────────────────

/// Check if this is a first boot (sentinel file missing).
pub fn is_first_boot(sentinel: &Path) -> bool {
    !sentinel.exists()
}

/// Create the first-boot sentinel file.
pub fn create_sentinel(sentinel: &Path) -> Result<(), String> {
    if let Some(parent) = sentinel.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create dir {}: {}", parent.display(), e))?;
    }
    std::fs::write(sentinel, "migrated\n")
        .map_err(|e| format!("Failed to create sentinel {}: {}", sentinel.display(), e))
}

// ── Systemd service generation ───────────────────────────────────────────

/// Log-level configuration for service generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn as_rust_log(&self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }
}

/// Options for systemd service generation.
pub struct ServiceOptions {
    pub binary_path: String,
    pub log_level: LogLevel,
    pub restart_sec: u32,
    pub config_path: String,
}

impl Default for ServiceOptions {
    fn default() -> Self {
        Self {
            binary_path: "/usr/local/bin/oxigotchi".into(),
            log_level: LogLevel::Info,
            restart_sec: 5,
            config_path: "/etc/oxigotchi/config.toml".into(),
        }
    }
}

/// Generate a systemd service file for oxigotchi.
pub fn generate_service_file(binary_path: &str) -> String {
    generate_service(&ServiceOptions {
        binary_path: binary_path.into(),
        ..Default::default()
    })
}

/// Generate a systemd service file with full options.
pub fn generate_service(opts: &ServiceOptions) -> String {
    format!(
        r#"[Unit]
Description=Oxigotchi WiFi capture daemon
After=network.target
Wants=network.target

[Service]
Type=simple
ExecStart={binary} --config {config}
Restart=always
RestartSec={restart_sec}
Environment=RUST_LOG={log_level}
StandardOutput=journal
StandardError=journal

# No security hardening — keep it easy for newbies
NoNewPrivileges=false
ProtectHome=false
ProtectSystem=false

[Install]
WantedBy=multi-user.target
"#,
        binary = opts.binary_path,
        config = opts.config_path,
        restart_sec = opts.restart_sec,
        log_level = opts.log_level.as_rust_log(),
    )
}

// ── Full migration orchestration ─────────────────────────────────────────

/// Run a full migration from pwnagotchi to oxigotchi.
/// - Checks first-boot sentinel
/// - Backs up original config
/// - Parses pwnagotchi config, extracts Rusty fields, writes clean config
/// - Imports captures from multiple source dirs with dedup
/// - Creates sentinel
pub fn run_migration(legacy: &LegacyPaths, oxi: &OxiPaths) -> MigrationResult {
    let mut result = MigrationResult {
        config_migrated: false,
        config_backed_up: false,
        captures_found: 0,
        captures_imported: 0,
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    // Check first-boot sentinel
    if !is_first_boot(&oxi.sentinel) {
        result
            .warnings
            .push("Not first boot — sentinel exists, skipping migration".into());
        return result;
    }

    // Backup original config
    if legacy.config.exists() {
        match backup_config(&legacy.config) {
            Ok(backup_path) => {
                result.config_backed_up = true;
                result.warnings.push(format!(
                    "Backed up original config to {}",
                    backup_path.display()
                ));
            }
            Err(e) => result.errors.push(format!("Config backup: {e}")),
        }
    }

    // Migrate config
    if legacy.config.exists() {
        match migrate_config(&legacy.config, &oxi.config) {
            Ok(cfg) => {
                result.config_migrated = true;
                // Validate the migrated config
                let validation = validate_config(&cfg);
                result.warnings.extend(validation.warnings);
                if !validation.valid {
                    for err in &validation.errors {
                        result.errors.push(format!("Config validation: {err}"));
                    }
                }
            }
            Err(e) => result.errors.push(format!("Config migration: {e}")),
        }
    } else {
        result
            .warnings
            .push("No pwnagotchi config found — using defaults".into());
    }

    // Import captures with dedup from all source dirs
    match import_captures_dedup(&legacy.handshake_dirs, &oxi.captures) {
        Ok((found, imported)) => {
            result.captures_found = found;
            result.captures_imported = imported;
        }
        Err(e) => result.errors.push(format!("Capture import: {e}")),
    }

    // Create sentinel
    if let Err(e) = create_sentinel(&oxi.sentinel) {
        result.errors.push(format!("Sentinel creation: {e}"));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── Pwnagotchi config parsing tests ──────────────────────────────────

    const FULL_PWN_CONFIG: &str = r#"
[main]
name = "mr-pwn"
whitelist = ["HomeNet", "HomeNet 5G"]
lang = "de"

[ui]
invert = true

[ui.display]
type = "waveshare_4"
rotation = 180
enabled = true

[ui.font]
name = "DejaVuSansMono"

[ui.faces]
png = false

[personality]
channels = [1, 6, 11]
deauth = true
associate = false

[bettercap]
scheme = "http"
hostname = "localhost"
port = 8081

[plugins.angryoxide]
enabled = true
rate = 2
dwell = 10
interface = "wlan1mon"
output_dir = "/data/captures/"
"#;

    const MINIMAL_PWN_CONFIG: &str = r#"
[main]
name = "tiny-bot"
"#;

    const AO_PLUGIN_CONFIG: &str = r#"
[main]
name = "ao-bot"

[plugins.angryoxide]
enabled = true
rate = 3
dwell = 8
interface = "wlan0mon"
output_dir = "/home/pi/ao-captures/"
"#;

    // ── Test: Parse full pwnagotchi config with all sections ─────────────

    #[test]
    fn test_parse_full_pwnagotchi_config() {
        let pwn = parse_pwnagotchi_config(FULL_PWN_CONFIG).unwrap();
        assert_eq!(pwn.main.name, "mr-pwn");
        assert_eq!(pwn.main.whitelist, vec!["HomeNet", "HomeNet 5G"]);
        assert_eq!(pwn.main.lang, "de");
        assert!(pwn.ui.invert);
        assert_eq!(pwn.ui.display.display_type, "waveshare_4");
        assert_eq!(pwn.ui.display.rotation, 180);
        assert!(pwn.ui.display.enabled);
        assert_eq!(pwn.ui.font.name, "DejaVuSansMono");
        assert!(!pwn.ui.faces.png);
        assert_eq!(pwn.personality.channels, vec![1, 6, 11]);
        assert!(pwn.personality.deauth);
        assert!(!pwn.personality.associate);
        assert_eq!(pwn.bettercap.scheme, "http");
        assert_eq!(pwn.bettercap.hostname, "localhost");
        assert_eq!(pwn.bettercap.port, 8081);
        let ao = pwn.plugins.angryoxide.as_ref().unwrap();
        assert!(ao.enabled);
        assert_eq!(ao.rate, 2);
        assert_eq!(ao.dwell, 10);
        assert_eq!(ao.interface, "wlan1mon");
        assert_eq!(ao.output_dir, "/data/captures/");
    }

    // ── Test: Parse minimal config (just [main]) ─────────────────────────

    #[test]
    fn test_parse_minimal_pwnagotchi_config() {
        let pwn = parse_pwnagotchi_config(MINIMAL_PWN_CONFIG).unwrap();
        assert_eq!(pwn.main.name, "tiny-bot");
        assert!(pwn.main.whitelist.is_empty());
        assert_eq!(pwn.main.lang, "en"); // default
        assert!(!pwn.ui.invert); // default
        assert_eq!(pwn.ui.display.display_type, "waveshare_4"); // default
        assert_eq!(pwn.ui.display.rotation, 0); // default
        assert!(pwn.ui.display.enabled); // default
        assert!(pwn.personality.channels.is_empty()); // default
        assert!(pwn.personality.deauth); // default true
        assert!(pwn.personality.associate); // default true
        assert!(pwn.plugins.angryoxide.is_none());
    }

    // ── Test: Parse config with angryoxide plugin settings ───────────────

    #[test]
    fn test_parse_angryoxide_plugin_config() {
        let pwn = parse_pwnagotchi_config(AO_PLUGIN_CONFIG).unwrap();
        assert_eq!(pwn.main.name, "ao-bot");
        let ao = pwn.plugins.angryoxide.as_ref().unwrap();
        assert!(ao.enabled);
        assert_eq!(ao.rate, 3);
        assert_eq!(ao.dwell, 8);
        assert_eq!(ao.interface, "wlan0mon");
        assert_eq!(ao.output_dir, "/home/pi/ao-captures/");
    }

    // ── Test: Extract Rusty config from full pwnagotchi config ───────────

    #[test]
    fn test_extract_rusty_config() {
        let pwn = parse_pwnagotchi_config(FULL_PWN_CONFIG).unwrap();
        let cfg = extract_rusty_config(&pwn);
        assert_eq!(cfg.name, "mr-pwn");
        assert_eq!(cfg.whitelist, vec!["HomeNet", "HomeNet 5G"]);
        assert_eq!(cfg.main.lang, "de");
        assert!(cfg.ui.invert);
        assert_eq!(cfg.display.display_type, "waveshare_4");
        assert_eq!(cfg.display.rotation, 180);
        assert!(cfg.display.enabled);
        assert_eq!(cfg.ui.font.name, "DejaVuSansMono");
    }

    // ── Test: Extract from minimal -> defaults for missing fields ────────

    #[test]
    fn test_extract_rusty_config_from_minimal() {
        let pwn = parse_pwnagotchi_config(MINIMAL_PWN_CONFIG).unwrap();
        let cfg = extract_rusty_config(&pwn);
        assert_eq!(cfg.name, "tiny-bot");
        assert!(cfg.whitelist.is_empty());
        assert_eq!(cfg.main.lang, "en");
        assert!(!cfg.ui.invert);
        assert_eq!(cfg.display.display_type, "waveshare_4");
        assert_eq!(cfg.display.rotation, 0);
    }

    // ── Test: Config round-trip (extract -> toml -> parse) ───────────────

    #[test]
    fn test_config_round_trip() {
        let pwn = parse_pwnagotchi_config(FULL_PWN_CONFIG).unwrap();
        let cfg = extract_rusty_config(&pwn);
        let toml_str = config_to_toml(&cfg).unwrap();
        // Parse it back as a Rusty config
        let reparsed = crate::config::Config::from_toml(&toml_str).unwrap();
        assert_eq!(reparsed.name, "mr-pwn");
        assert_eq!(reparsed.whitelist, vec!["HomeNet", "HomeNet 5G"]);
        assert_eq!(reparsed.display.display_type, "waveshare_4");
        assert_eq!(reparsed.display.rotation, 180);
    }

    // ── Test: Config validation — valid config ───────────────────────────

    #[test]
    fn test_validate_config_valid() {
        let cfg = crate::config::Config::defaults();
        let v = validate_config(&cfg);
        assert!(v.valid);
        assert!(v.errors.is_empty());
    }

    // ── Test: Config validation — missing display type ───────────────────

    #[test]
    fn test_validate_config_empty_display_type() {
        let mut cfg = crate::config::Config::defaults();
        cfg.ui.display.display_type = String::new();
        let v = validate_config(&cfg);
        assert!(!v.valid);
        assert!(v.errors.iter().any(|e| e.contains("display.type")));
    }

    // ── Test: Config validation — invalid rotation ───────────────────────

    #[test]
    fn test_validate_config_invalid_rotation() {
        let mut cfg = crate::config::Config::defaults();
        cfg.ui.display.rotation = 45;
        let v = validate_config(&cfg);
        assert!(!v.valid);
        assert!(v.errors.iter().any(|e| e.contains("rotation")));
    }

    // ── Test: Config validation — unknown display type warns ─────────────

    #[test]
    fn test_validate_config_unknown_display_type_warns() {
        let mut cfg = crate::config::Config::defaults();
        cfg.ui.display.display_type = "future_display_9000".into();
        let v = validate_config(&cfg);
        assert!(v.valid); // warning, not error
        assert!(
            v.warnings
                .iter()
                .any(|w| w.contains("not a known display type"))
        );
    }

    // ── Test: Config validation — empty name warns ───────────────────────

    #[test]
    fn test_validate_config_empty_name_warns() {
        let mut cfg = crate::config::Config::defaults();
        cfg.main.name = String::new();
        let v = validate_config(&cfg);
        assert!(v.valid); // warning, not error
        assert!(v.warnings.iter().any(|w| w.contains("name is empty")));
    }

    // ── Test: Backup creation ────────────────────────────────────────────

    #[test]
    fn test_backup_config() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "[main]\nname = \"test\"\n").unwrap();

        let backup_path = backup_config(&config_path).unwrap();
        assert!(backup_path.exists());
        assert_eq!(
            backup_path.file_name().unwrap().to_str().unwrap(),
            "config.toml.pwnagotchi-backup"
        );

        let backup_content = std::fs::read_to_string(&backup_path).unwrap();
        assert!(backup_content.contains("name = \"test\""));
    }

    #[test]
    fn test_backup_config_missing_source() {
        let result = backup_config(Path::new("/nonexistent/config.toml"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // ── Test: Capture dedup by filename ──────────────────────────────────

    #[test]
    fn test_capture_dedup_by_filename() {
        let tmp = TempDir::new().unwrap();
        let dir_a = tmp.path().join("dir_a");
        let dir_b = tmp.path().join("dir_b");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();

        // Same filename in both dirs (different content)
        std::fs::write(dir_a.join("ap1.pcapng"), "data-from-a").unwrap();
        std::fs::write(dir_b.join("ap1.pcapng"), "data-from-b-different").unwrap();
        // Unique file in dir_b
        std::fs::write(dir_b.join("ap2.pcap"), "unique-data").unwrap();
        // Non-capture file (should be ignored)
        std::fs::write(dir_a.join("notes.txt"), "not a capture").unwrap();

        let (found, imported) = import_captures_dedup(&[dir_a, dir_b], &dest).unwrap();

        assert_eq!(found, 3); // ap1 in dir_a, ap1 in dir_b, ap2 in dir_b
        assert_eq!(imported, 2); // ap1 (first seen from dir_a) + ap2
        assert!(dest.join("ap1.pcapng").exists());
        assert!(dest.join("ap2.pcap").exists());

        // ap1.pcapng should be from dir_a (first source wins)
        let content = std::fs::read_to_string(dest.join("ap1.pcapng")).unwrap();
        assert_eq!(content, "data-from-a");
    }

    // ── Test: Capture dedup skips existing files at dest ─────────────────

    #[test]
    fn test_capture_dedup_skips_existing_at_dest() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();

        // Pre-existing file at dest
        std::fs::write(dest.join("existing.pcapng"), "old-data").unwrap();
        // Same filename in source
        std::fs::write(src.join("existing.pcapng"), "new-data").unwrap();
        // New file
        std::fs::write(src.join("new.pcapng"), "fresh").unwrap();

        let (found, imported) = import_captures_dedup(&[src], &dest).unwrap();

        assert_eq!(found, 2); // existing + new
        assert_eq!(imported, 1); // only new (existing was deduped)

        // existing.pcapng should NOT be overwritten
        let content = std::fs::read_to_string(dest.join("existing.pcapng")).unwrap();
        assert_eq!(content, "old-data");
    }

    // ── Test: Capture dedup with empty/missing source dirs ───────────────

    #[test]
    fn test_capture_dedup_missing_dirs() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("dest");
        let (found, imported) = import_captures_dedup(
            &[
                PathBuf::from("/nonexistent/a"),
                PathBuf::from("/nonexistent/b"),
            ],
            &dest,
        )
        .unwrap();
        assert_eq!(found, 0);
        assert_eq!(imported, 0);
    }

    // ── Test: First-boot sentinel creation/detection ─────────────────────

    #[test]
    fn test_first_boot_detection() {
        let tmp = TempDir::new().unwrap();
        let sentinel = tmp.path().join("subdir").join(".rusty-first-boot");

        // First boot: sentinel does not exist
        assert!(is_first_boot(&sentinel));

        // Create sentinel
        create_sentinel(&sentinel).unwrap();

        // Not first boot anymore
        assert!(!is_first_boot(&sentinel));

        // Sentinel content
        let content = std::fs::read_to_string(&sentinel).unwrap();
        assert_eq!(content, "migrated\n");
    }

    #[test]
    fn test_is_first_boot_missing_sentinel() {
        assert!(is_first_boot(Path::new("/nonexistent/.rusty-first-boot")));
    }

    #[test]
    fn test_is_not_first_boot_existing_sentinel() {
        let tmp = TempDir::new().unwrap();
        let sentinel = tmp.path().join(".rusty-first-boot");
        std::fs::write(&sentinel, "migrated\n").unwrap();
        assert!(!is_first_boot(&sentinel));
    }

    // ── Test: Systemd service generation ─────────────────────────────────

    #[test]
    fn test_generate_service_file_basic() {
        let svc = generate_service_file("/usr/local/bin/oxigotchi");
        assert!(svc.contains("ExecStart=/usr/local/bin/oxigotchi"));
        assert!(svc.contains("[Unit]"));
        assert!(svc.contains("[Service]"));
        assert!(svc.contains("[Install]"));
        assert!(svc.contains("Restart=always"));
        assert!(svc.contains("RUST_LOG=info"));
    }

    #[test]
    fn test_generate_service_with_debug_log() {
        let svc = generate_service(&ServiceOptions {
            binary_path: "/usr/local/bin/oxigotchi".into(),
            log_level: LogLevel::Debug,
            restart_sec: 10,
            config_path: "/custom/path/config.toml".into(),
        });
        assert!(svc.contains("RUST_LOG=debug"));
        assert!(svc.contains("RestartSec=10"));
        assert!(svc.contains("--config /custom/path/config.toml"));
    }

    #[test]
    fn test_generate_service_with_trace_log() {
        let svc = generate_service(&ServiceOptions {
            log_level: LogLevel::Trace,
            ..Default::default()
        });
        assert!(svc.contains("RUST_LOG=trace"));
    }

    #[test]
    fn test_generate_service_no_security_hardening() {
        let svc = generate_service_file("/usr/local/bin/oxigotchi");
        assert!(svc.contains("NoNewPrivileges=false"));
        assert!(svc.contains("ProtectHome=false"));
        assert!(svc.contains("ProtectSystem=false"));
    }

    // ── Test: Full config migration (file-level) ─────────────────────────

    #[test]
    fn test_migrate_config_file() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("pwnagotchi-config.toml");
        let dest = tmp.path().join("oxi").join("config.toml");

        std::fs::write(&src, FULL_PWN_CONFIG).unwrap();

        let cfg = migrate_config(&src, &dest).unwrap();
        assert_eq!(cfg.name, "mr-pwn");
        assert!(dest.exists());

        // The output should be parseable as a Rusty config
        let output = std::fs::read_to_string(&dest).unwrap();
        let reparsed = crate::config::Config::from_toml(&output).unwrap();
        assert_eq!(reparsed.name, "mr-pwn");
        assert_eq!(reparsed.display.rotation, 180);
    }

    #[test]
    fn test_migrate_config_missing_source() {
        let result = migrate_config(
            Path::new("/nonexistent/config.toml"),
            Path::new("/tmp/test_oxi_config.toml"),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // ── Test: Full migration orchestration ───────────────────────────────

    #[test]
    fn test_run_migration_full() {
        let tmp = TempDir::new().unwrap();

        // Set up legacy paths
        let legacy_config = tmp.path().join("legacy").join("config.toml");
        let handshakes_a = tmp.path().join("legacy").join("handshakes_a");
        let handshakes_b = tmp.path().join("legacy").join("handshakes_b");
        std::fs::create_dir_all(legacy_config.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&handshakes_a).unwrap();
        std::fs::create_dir_all(&handshakes_b).unwrap();

        std::fs::write(&legacy_config, FULL_PWN_CONFIG).unwrap();
        std::fs::write(handshakes_a.join("net1.pcapng"), "data1").unwrap();
        std::fs::write(handshakes_b.join("net2.pcapng"), "data2").unwrap();
        // Duplicate filename across dirs
        std::fs::write(handshakes_b.join("net1.pcapng"), "data1-dupe").unwrap();

        let legacy = LegacyPaths {
            config: legacy_config.clone(),
            handshake_dirs: vec![handshakes_a, handshakes_b],
            plugins: tmp.path().join("plugins"),
        };

        let oxi = OxiPaths {
            config: tmp.path().join("oxi").join("config.toml"),
            captures: tmp.path().join("oxi").join("captures"),
            logs: tmp.path().join("oxi").join("logs"),
            service: tmp.path().join("oxi").join("service"),
            sentinel: tmp.path().join("oxi").join(".rusty-first-boot"),
        };

        let result = run_migration(&legacy, &oxi);
        assert!(result.success(), "errors: {:?}", result.errors);
        assert!(result.config_migrated);
        assert!(result.config_backed_up);
        assert_eq!(result.captures_found, 3); // net1 in a, net2 in b, net1 in b
        assert_eq!(result.captures_imported, 2); // net1 + net2 (deduped)

        // Sentinel should exist now
        assert!(!is_first_boot(&oxi.sentinel));

        // Backup should exist
        let mut backup = legacy_config.as_os_str().to_owned();
        backup.push(".pwnagotchi-backup");
        assert!(PathBuf::from(backup).exists());
    }

    #[test]
    fn test_run_migration_skips_when_not_first_boot() {
        let tmp = TempDir::new().unwrap();
        let sentinel = tmp.path().join(".rusty-first-boot");
        std::fs::write(&sentinel, "migrated\n").unwrap();

        let legacy = LegacyPaths {
            config: tmp.path().join("config.toml"),
            handshake_dirs: vec![],
            plugins: tmp.path().join("plugins"),
        };
        let oxi = OxiPaths {
            config: tmp.path().join("oxi-config.toml"),
            captures: tmp.path().join("captures"),
            logs: tmp.path().join("logs"),
            service: tmp.path().join("service"),
            sentinel,
        };

        let result = run_migration(&legacy, &oxi);
        // Should skip migration entirely
        assert!(!result.config_migrated);
        assert_eq!(result.captures_imported, 0);
        assert!(result.warnings.iter().any(|w| w.contains("Not first boot")));
    }

    #[test]
    fn test_run_migration_missing_legacy_config() {
        let tmp = TempDir::new().unwrap();
        let legacy = LegacyPaths {
            config: PathBuf::from("/nonexistent/config.toml"),
            handshake_dirs: vec![PathBuf::from("/nonexistent/handshakes")],
            plugins: PathBuf::from("/nonexistent/plugins"),
        };
        let oxi = OxiPaths {
            config: tmp.path().join("oxi").join("config.toml"),
            captures: tmp.path().join("oxi").join("captures"),
            logs: tmp.path().join("oxi").join("logs"),
            service: tmp.path().join("oxi").join("service"),
            sentinel: tmp.path().join("oxi").join(".rusty-first-boot"),
        };

        let result = run_migration(&legacy, &oxi);
        // Should succeed but with warnings about missing config
        assert!(!result.config_migrated);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("No pwnagotchi config found"))
        );
    }

    // ── Test: count_captures ─────────────────────────────────────────────

    #[test]
    fn test_count_captures() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.pcapng"), "data").unwrap();
        std::fs::write(tmp.path().join("b.pcap"), "data").unwrap();
        std::fs::write(tmp.path().join("c.txt"), "not a capture").unwrap();
        assert_eq!(count_captures(tmp.path()).unwrap(), 2);
    }

    #[test]
    fn test_count_captures_missing_dir() {
        let count = count_captures(Path::new("/nonexistent/dir"));
        assert_eq!(count.unwrap(), 0);
    }

    // ── Test: import_captures (simple) ───────────────────────────────────

    #[test]
    fn test_import_captures_simple() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();

        std::fs::write(src.join("net.pcapng"), "data").unwrap();
        std::fs::write(src.join("notes.txt"), "not imported").unwrap();

        let imported = import_captures(&src, &dest).unwrap();
        assert_eq!(imported, 1);
        assert!(dest.join("net.pcapng").exists());
        assert!(!dest.join("notes.txt").exists());
    }

    #[test]
    fn test_import_captures_missing_dir() {
        let result = import_captures(
            Path::new("/nonexistent/source"),
            Path::new("/tmp/test_oxi_dest"),
        );
        assert_eq!(result.unwrap(), 0);
    }

    // ── Test: Legacy/Oxi paths defaults ──────────────────────────────────

    #[test]
    fn test_legacy_paths_default() {
        let lp = LegacyPaths::default();
        assert_eq!(lp.config, PathBuf::from("/etc/pwnagotchi/config.toml"));
        assert_eq!(lp.handshake_dirs.len(), 2);
        assert_eq!(lp.handshake_dirs[0], PathBuf::from("/home/pi/handshakes"));
        assert_eq!(
            lp.handshake_dirs[1],
            PathBuf::from("/etc/pwnagotchi/handshakes")
        );
    }

    #[test]
    fn test_oxi_paths_default() {
        let op = OxiPaths::default();
        assert_eq!(op.config, PathBuf::from("/etc/oxigotchi/config.toml"));
        assert_eq!(op.captures, PathBuf::from("/home/pi/captures"));
        assert_eq!(op.sentinel, PathBuf::from("/var/lib/.rusty-first-boot"));
    }

    // ── Test: MigrationResult ────────────────────────────────────────────

    #[test]
    fn test_migration_result_success() {
        let r = MigrationResult {
            config_migrated: true,
            config_backed_up: true,
            captures_found: 5,
            captures_imported: 5,
            warnings: Vec::new(),
            errors: Vec::new(),
        };
        assert!(r.success());
    }

    #[test]
    fn test_migration_result_with_errors() {
        let r = MigrationResult {
            config_migrated: false,
            config_backed_up: false,
            captures_found: 0,
            captures_imported: 0,
            warnings: Vec::new(),
            errors: vec!["something failed".into()],
        };
        assert!(!r.success());
    }

    // ── Test: LogLevel ───────────────────────────────────────────────────

    #[test]
    fn test_log_level_as_rust_log() {
        assert_eq!(LogLevel::Error.as_rust_log(), "error");
        assert_eq!(LogLevel::Warn.as_rust_log(), "warn");
        assert_eq!(LogLevel::Info.as_rust_log(), "info");
        assert_eq!(LogLevel::Debug.as_rust_log(), "debug");
        assert_eq!(LogLevel::Trace.as_rust_log(), "trace");
    }

    // ── Test: Parse invalid TOML ─────────────────────────────────────────

    #[test]
    fn test_parse_invalid_toml() {
        let result = parse_pwnagotchi_config("not valid [[ toml");
        assert!(result.is_err());
    }

    // ── Test: Parse empty TOML gives defaults ────────────────────────────

    #[test]
    fn test_parse_empty_toml() {
        let pwn = parse_pwnagotchi_config("").unwrap();
        assert_eq!(pwn.main.name, "pwnagotchi"); // pwnagotchi default
        assert!(pwn.main.whitelist.is_empty());
        assert!(pwn.personality.deauth); // default true
        assert!(pwn.personality.associate); // default true
    }
}
