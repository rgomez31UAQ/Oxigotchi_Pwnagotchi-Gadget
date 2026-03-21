//! Migration tooling: import existing pwnagotchi configs and captures.

use std::path::{Path, PathBuf};

/// Result of a migration operation.
#[derive(Debug)]
pub struct MigrationResult {
    pub config_migrated: bool,
    pub captures_found: usize,
    pub captures_imported: usize,
    pub errors: Vec<String>,
}

impl MigrationResult {
    pub fn success(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Paths used by the legacy pwnagotchi installation.
#[derive(Debug, Clone)]
pub struct LegacyPaths {
    /// Pwnagotchi config file.
    pub config: PathBuf,
    /// Directory containing handshake pcapng files.
    pub handshakes: PathBuf,
    /// Directory for plugins.
    pub plugins: PathBuf,
}

impl Default for LegacyPaths {
    fn default() -> Self {
        Self {
            config: PathBuf::from("/etc/pwnagotchi/config.toml"),
            handshakes: PathBuf::from("/root/handshakes"),
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
}

impl Default for OxiPaths {
    fn default() -> Self {
        Self {
            config: PathBuf::from("/etc/oxigotchi/config.toml"),
            captures: PathBuf::from("/home/pi/captures"),
            logs: PathBuf::from("/var/log/oxigotchi"),
            service: PathBuf::from("/etc/systemd/system/oxigotchi.service"),
        }
    }
}

/// Migrate a pwnagotchi config to oxigotchi format.
/// The TOML format is compatible — we just copy and validate.
pub fn migrate_config(from: &Path, to: &Path) -> Result<(), String> {
    if !from.exists() {
        return Err(format!("Source config not found: {}", from.display()));
    }

    let content = std::fs::read_to_string(from)
        .map_err(|e| format!("Failed to read {}: {}", from.display(), e))?;

    // Validate it's parseable
    crate::config::Config::from_toml(&content)
        .map_err(|e| format!("Invalid config format: {e}"))?;

    // Create target directory if needed
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create dir {}: {}", parent.display(), e))?;
    }

    std::fs::write(to, &content)
        .map_err(|e| format!("Failed to write {}: {}", to.display(), e))?;

    Ok(())
}

/// Count pcapng files in a directory (non-recursive).
pub fn count_captures(dir: &Path) -> Result<usize, String> {
    if !dir.exists() {
        return Ok(0);
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read dir {}: {}", dir.display(), e))?;
    let count = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "pcapng" || ext == "pcap")
        })
        .count();
    Ok(count)
}

/// Copy capture files from source to destination directory.
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
        if path
            .extension()
            .is_some_and(|ext| ext == "pcapng" || ext == "pcap")
        {
            let dest = to.join(path.file_name().unwrap());
            if !dest.exists() {
                std::fs::copy(&path, &dest).map_err(|e| {
                    format!("Failed to copy {}: {}", path.display(), e)
                })?;
                imported += 1;
            }
        }
    }
    Ok(imported)
}

/// Generate a systemd service file for oxigotchi.
pub fn generate_service_file(binary_path: &str) -> String {
    format!(
        r#"[Unit]
Description=Oxigotchi WiFi capture daemon
After=network.target
Wants=network.target

[Service]
Type=simple
ExecStart={binary_path}
Restart=always
RestartSec=5
Environment=RUST_LOG=info
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=false
ProtectHome=false
ProtectSystem=false

[Install]
WantedBy=multi-user.target
"#
    )
}

/// Run a full migration from pwnagotchi to oxigotchi.
pub fn run_migration(legacy: &LegacyPaths, oxi: &OxiPaths) -> MigrationResult {
    let mut result = MigrationResult {
        config_migrated: false,
        captures_found: 0,
        captures_imported: 0,
        errors: Vec::new(),
    };

    // Migrate config
    match migrate_config(&legacy.config, &oxi.config) {
        Ok(()) => result.config_migrated = true,
        Err(e) => result.errors.push(format!("Config migration: {e}")),
    }

    // Count and import captures
    match count_captures(&legacy.handshakes) {
        Ok(n) => result.captures_found = n,
        Err(e) => result.errors.push(format!("Capture count: {e}")),
    }

    match import_captures(&legacy.handshakes, &oxi.captures) {
        Ok(n) => result.captures_imported = n,
        Err(e) => result.errors.push(format!("Capture import: {e}")),
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_paths_default() {
        let lp = LegacyPaths::default();
        assert_eq!(lp.config, PathBuf::from("/etc/pwnagotchi/config.toml"));
        assert_eq!(lp.handshakes, PathBuf::from("/root/handshakes"));
    }

    #[test]
    fn test_oxi_paths_default() {
        let op = OxiPaths::default();
        assert_eq!(op.config, PathBuf::from("/etc/oxigotchi/config.toml"));
        assert_eq!(op.captures, PathBuf::from("/home/pi/captures"));
    }

    #[test]
    fn test_generate_service_file() {
        let svc = generate_service_file("/usr/local/bin/oxigotchi");
        assert!(svc.contains("ExecStart=/usr/local/bin/oxigotchi"));
        assert!(svc.contains("[Unit]"));
        assert!(svc.contains("[Service]"));
        assert!(svc.contains("[Install]"));
        assert!(svc.contains("Restart=always"));
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

    #[test]
    fn test_count_captures_missing_dir() {
        let count = count_captures(Path::new("/nonexistent/dir"));
        assert_eq!(count.unwrap(), 0);
    }

    #[test]
    fn test_migration_result_success() {
        let r = MigrationResult {
            config_migrated: true,
            captures_found: 5,
            captures_imported: 5,
            errors: Vec::new(),
        };
        assert!(r.success());
    }

    #[test]
    fn test_migration_result_with_errors() {
        let r = MigrationResult {
            config_migrated: false,
            captures_found: 0,
            captures_imported: 0,
            errors: vec!["something failed".into()],
        };
        assert!(!r.success());
    }

    #[test]
    fn test_import_captures_missing_dir() {
        let result = import_captures(
            Path::new("/nonexistent/source"),
            Path::new("/tmp/test_oxi_dest"),
        );
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_run_migration_missing_paths() {
        let legacy = LegacyPaths {
            config: PathBuf::from("/nonexistent/config.toml"),
            handshakes: PathBuf::from("/nonexistent/handshakes"),
            plugins: PathBuf::from("/nonexistent/plugins"),
        };
        let oxi = OxiPaths {
            config: PathBuf::from("/tmp/test_oxi_migration/config.toml"),
            captures: PathBuf::from("/tmp/test_oxi_migration/captures"),
            logs: PathBuf::from("/tmp/test_oxi_migration/logs"),
            service: PathBuf::from("/tmp/test_oxi_migration/service"),
        };
        let result = run_migration(&legacy, &oxi);
        assert!(!result.config_migrated);
        assert!(!result.success());
    }
}
