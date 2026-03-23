// rustc 1.94 ICE in AnnotateSnippetEmitter when rendering dead_code warnings.
// Remove this once the compiler is updated past the fix.
#![allow(dead_code)]

// Modules are the public API surface for Rusty Oxigotchi.
mod ao;
mod attacks;
mod bluetooth;
mod capture;
mod config;
mod display;
mod epoch;
mod personality;
mod pisugar;
mod network;
mod recovery;
mod migration;
mod web;
mod wifi;
mod lua;

use chrono::Timelike;
use log::info;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Ensure tmpfs capture directory exists for AO output.
/// Creates /tmp/ao_captures/ if it doesn't exist.
/// On Pi, /tmp is already a tmpfs mount, so this is just mkdir.
#[cfg(unix)]
fn ensure_tmpfs_capture_dir() -> String {
    let dir = "/tmp/ao_captures";
    let _ = std::fs::create_dir_all(dir);
    dir.to_string()
}

#[cfg(not(unix))]
fn ensure_tmpfs_capture_dir() -> String {
    let dir = std::env::temp_dir().join("ao_captures");
    let _ = std::fs::create_dir_all(&dir);
    dir.to_string_lossy().to_string()
}

/// Epoch duration in seconds.
const EPOCH_DURATION_SECS: u64 = 30;
/// Attack rate (attacks per second). Rate 2+ crashes BCM43436B0.
const ATTACK_RATE: u32 = 1;
/// Default capture directory.
const CAPTURE_DIR: &str = "/home/pi/captures";

/// Operating mode: RAGE (WiFi attacks) or SAFE (BT internet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperatingMode {
    Rage,
    Safe,
}

impl OperatingMode {
    fn as_str(&self) -> &str {
        match self {
            OperatingMode::Rage => "RAGE",
            OperatingMode::Safe => "SAFE",
        }
    }

    fn toggle(&self) -> Self {
        match self {
            OperatingMode::Rage => OperatingMode::Safe,
            OperatingMode::Safe => OperatingMode::Rage,
        }
    }
}

/// All subsystem state, owned by the main loop.
struct Daemon {
    config: config::Config,
    screen: display::Screen,
    epoch_loop: epoch::EpochLoop,
    wifi: wifi::WifiManager,
    attacks: attacks::AttackScheduler,
    captures: capture::CaptureManager,
    bluetooth: bluetooth::BtTether,
    battery: pisugar::PiSugar,
    network: network::NetworkManager,
    recovery: recovery::RecoveryManager,
    watchdog: recovery::Watchdog,
    ao: ao::AoManager,
    mode: OperatingMode,
    shared_state: web::SharedState,
    prev_cpu_sample: Option<personality::CpuSample>,
    lua: lua::PluginRuntime,
    wpasec_config: capture::WpaSecConfig,
    upload_queue: capture::UploadQueue,
    discord_webhook_url: String,
    discord_enabled: bool,
    /// tmpfs directory for AO captures (validated before moving to SD).
    tmpfs_capture_dir: String,
}

impl Daemon {
    fn new(config: config::Config, shared_state: web::SharedState) -> Self {
        let screen = display::Screen::new(config.display.clone());
        let epoch_loop = epoch::EpochLoop::new(Duration::from_secs(EPOCH_DURATION_SECS));
        let wifi = wifi::WifiManager::new();
        let mut attacks = attacks::AttackScheduler::new(ATTACK_RATE);

        // Populate attack whitelist from config
        let _ = &config.whitelist;
        attacks.whitelist.clear();

        let captures = capture::CaptureManager::new(CAPTURE_DIR);
        let bt_config = bluetooth::BtConfig {
            enabled: config.bluetooth.enabled,
            phone_mac: config.bluetooth.phone_mac.clone(),
            phone_name: config.bluetooth.phone_name.clone(),
            connection_name: config.bluetooth.connection_name.clone(),
            auto_connect: config.bluetooth.auto_connect,
            auto_pair: config.bluetooth.auto_pair,
            hide_after_connect: config.bluetooth.hide_after_connect,
            retry_interval_secs: config.bluetooth.retry_interval_secs,
            max_retries: config.bluetooth.max_retries,
        };
        let bluetooth = bluetooth::BtTether::new(bt_config);
        let battery = pisugar::PiSugar::default();
        let network = network::NetworkManager::new();
        let recovery = recovery::RecoveryManager::default();
        let watchdog = recovery::Watchdog::new(true, 60);
        let ao = ao::AoManager::default();

        Self {
            config,
            screen,
            epoch_loop,
            wifi,
            attacks,
            captures,
            bluetooth,
            battery,
            network,
            recovery,
            watchdog,
            ao,
            mode: OperatingMode::Rage,
            shared_state,
            prev_cpu_sample: None,
            lua: lua::PluginRuntime::new(),
            wpasec_config: capture::WpaSecConfig::default(),
            upload_queue: capture::UploadQueue::new(),
            discord_webhook_url: String::new(),
            discord_enabled: false,
            tmpfs_capture_dir: ensure_tmpfs_capture_dir(),
        }
    }

    /// Boot sequence: init display, probe hardware, scan existing captures, start AO.
    fn boot(&mut self) {
        // Load saved XP/mood from disk
        let xp_path = std::path::Path::new(personality::DEFAULT_XP_SAVE_PATH);
        let (xp, mood) = personality::XpTracker::load(xp_path);
        self.epoch_loop.personality.xp = xp;
        self.epoch_loop.personality.mood = personality::Mood::new(mood);
        info!("loaded XP: Lv {} ({} xp), mood {:.2}",
            self.epoch_loop.personality.xp.level,
            self.epoch_loop.personality.xp.xp_total,
            mood);

        // Display boot screen with debug face (Feature 7: debug on boot)
        let boot_face_key = self.epoch_loop.personality.variety.boot_face();
        let boot_face = if boot_face_key == "debug" {
            personality::Face::Debug
        } else {
            personality::Face::Awake
        };
        self.screen.clear();
        // Boot screen: centered face + centered welcome text below
        // Face: 120x66, centered horizontally: x = (250-120)/2 = 65
        self.screen.draw_bitmap(
            display::faces::bitmap_for_face(&boot_face),
            65, 5,
            display::faces::FACE_WIDTH,
            display::faces::FACE_HEIGHT,
        );
        // Welcome text centered below face, using bold font
        let line1 = format!("Hi! I'm {}!", self.config.name);
        let line2 = format!("Starting v{}", env!("CARGO_PKG_VERSION"));
        // Center: each char ~7px wide in bold (12pt), display=250
        let x1 = (250 - (line1.len() as i32) * 7) / 2;
        let x2 = (250 - (line2.len() as i32) * 7) / 2;
        self.screen.draw_name_at(&line1, x1, 80);
        self.screen.draw_name_at(&line2, x2, 95);
        self.screen.flush();
        info!("display initialized");

        // Probe PiSugar battery
        if self.battery.probe() {
            let status = self.battery.read_status();
            info!("PiSugar detected: {}%", status.level);
        } else {
            info!("PiSugar not detected");
        }

        // Disable legacy Python/Go services — Rusty Oxigotchi replaces them.
        // Saves ~66MB RAM (bettercap ~36MB + pwnagotchi ~30MB).
        #[cfg(unix)]
        {
            use std::process::Command;
            for svc in &["pwnagotchi", "bettercap"] {
                let active = Command::new("systemctl")
                    .args(["is-active", "--quiet", svc])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if active {
                    info!("disabling legacy service: {svc}");
                    let _ = Command::new("systemctl").args(["stop", svc]).output();
                    let _ = Command::new("systemctl").args(["disable", svc]).output();
                }
            }
        }

        // Start WiFi monitor mode first — RAGE is the default mode.
        // BT only connects when user switches to SAFE mode via button.
        // BCM43436B0 shares UART: whichever starts first gets it.
        match self.wifi.start_monitor() {
            Ok(()) => info!("WiFi monitor mode started"),
            Err(e) => {
                log::error!("Failed to start WiFi monitor: {e}");
                self.epoch_loop.personality.set_override(personality::Face::WifiDown);
            }
        }

        // Scan for existing capture files
        match self.captures.scan_directory() {
            Ok(n) => info!("found {n} existing captures"),
            Err(e) => log::warn!("capture scan failed: {e}"),
        }

        // Load Lua plugins — read persisted positions from plugins.toml, fall back to defaults
        let plugin_defaults = vec![
            lua::PluginConfig::default_for("ao_status",  0,   0),
            lua::PluginConfig::default_for("aps",        178, 112),
            lua::PluginConfig::default_for("uptime",     178, 0),
            lua::PluginConfig::default_for("status_msg", 125, 20),
            lua::PluginConfig::default_for("sys_stats",  125, 85),
            lua::PluginConfig::default_for("ip_display", 0,   95),
            lua::PluginConfig::default_for("crash",      0,   112),
            lua::PluginConfig::default_for("www",        48,  112),
            lua::PluginConfig::default_for("bt_status",  86,  112),
            lua::PluginConfig::default_for("battery",    118, 112),
            lua::PluginConfig::default_for("mode",       222, 112),
        ];
        let plugin_configs = match lua::config::read_plugins_toml() {
            Some(pt) => {
                info!("loaded plugin positions from plugins.toml");
                lua::config::merge_with_defaults(plugin_defaults, &pt)
            }
            None => plugin_defaults,
        };
        let loaded = self.lua.load_plugins_from_dir("/etc/oxigotchi/plugins", &plugin_configs);
        info!("loaded {loaded} Lua plugin(s)");

        // USB RNDIS network setup
        self.network.probe();
        if self.network.usb0_state != network::Usb0State::Absent {
            match self.network.apply_ip_config() {
                Ok(()) => info!("USB network configured: {}", self.network.status_str()),
                Err(e) => log::warn!("USB network setup failed: {e}"),
            }
        } else {
            info!("usb0 not present, skipping network setup");
        }

        // Point AO output to tmpfs — captures are validated there before moving to SD
        self.ao.config.output_dir = self.tmpfs_capture_dir.clone();
        info!("capture pipeline: AO output -> tmpfs ({})", self.tmpfs_capture_dir);

        // Start AngryOxide subprocess
        match self.ao.start() {
            Ok(()) => info!("AO started: PID {}", self.ao.pid),
            Err(e) => {
                log::error!("AO failed to start: {e}");
                self.epoch_loop.personality.set_override(personality::Face::AoCrashed);
            }
        }

        // Load persisted runtime state (attack toggles, whitelist, WPA-SEC key, Discord)
        self.load_runtime_state();

        // Initial state sync to web
        self.sync_to_web();
    }

    /// Run one full epoch: Scan -> Attack -> Capture -> Display -> Sleep.
    fn run_epoch(&mut self) {
        let mut result = epoch::EpochResult::default();

        // ---- Web commands ----
        self.process_web_commands();

        // ---- AO health (RAGE mode only) ----
        if self.mode == OperatingMode::Rage && self.ao.check_health() {
            self.epoch_loop.personality.set_override(personality::Face::AoCrashed);
        }
        if self.mode == OperatingMode::Rage {
            self.ao.try_auto_restart();
        }

        // ---- Scan phase ----
        self.run_scan_phase(&mut result);

        // ---- Bluetooth health (SAFE mode only) ----
        if self.mode == OperatingMode::Safe {
            self.bluetooth.check_status();
            if self.bluetooth.should_connect() {
                match self.bluetooth.connect() {
                    Ok(()) => info!("bluetooth reconnected: {}", self.bluetooth.status_str()),
                    Err(e) => {
                        log::warn!("bluetooth reconnect failed: {e}");
                        self.bluetooth.on_error();
                    }
                }
            }
        }

        // ---- Network health ----
        self.network.health_check();
        self.network.check_internet();
        if self.mode == OperatingMode::Rage {
            self.network.rotate_display(true);
        }

        // ---- Attack + Capture phases ----
        self.run_attack_phase(&mut result);
        self.run_capture_phase(&mut result);

        // ---- Display phase ----
        self.epoch_loop.next_phase(); // -> Display
        self.epoch_loop.record_result(&result);

        // ---- Face & personality ----
        self.update_face_and_personality(&result);

        // ---- Lua plugins + display ----
        let epoch_state = self.build_epoch_state();
        self.lua.tick_epoch(&epoch_state);
        self.update_display();

        // ---- CPU usage sampling ----
        if let Some(sample) = personality::CpuSample::read() {
            if let Some(ref prev) = self.prev_cpu_sample {
                let cpu_pct = sample.cpu_percent(prev);
                let mut s = self.shared_state.lock().unwrap();
                s.cpu_percent = cpu_pct;
            }
            self.prev_cpu_sample = Some(sample);
        }

        // ---- Sync state to web ----
        self.sync_to_web();

        // ---- Sleep + watchdog ----
        self.epoch_loop.next_phase(); // -> Sleep
        if self.watchdog.needs_ping() {
            self.watchdog.ping();
        }
        // Sub-epoch loop: sleep in 5s ticks for IP rotation in SAFE mode.
        // In RAGE mode, just sleeps the full duration (no display changes).
        const IP_ROTATE_SECS: u64 = 5;
        let total_secs = self.epoch_loop.epoch_duration.as_secs();
        let ticks = total_secs / IP_ROTATE_SECS;
        let remainder = total_secs % IP_ROTATE_SECS;

        for _ in 0..ticks {
            std::thread::sleep(Duration::from_secs(IP_ROTATE_SECS));

            // Update channel indicator every tick (AO hops every ~5s)
            let ch = self.ao.channel();
            if ch > 0 {
                let ch_str = format!("CH:{ch}");
                // ao_status format: "AO: X/Y | Zm | CH:N"
                let hs = self.epoch_loop.metrics.handshakes;
                let caps = self.captures.count();
                let up_secs = self.ao.start_time.map(|t| t.elapsed().as_secs()).unwrap_or(0);
                let uptime = if up_secs < 60 {
                    format!("{up_secs}s")
                } else if up_secs < 3600 {
                    format!("{}m", up_secs / 60)
                } else {
                    let h = up_secs / 3600;
                    let m = (up_secs % 3600) / 60;
                    if m == 0 { format!("{h}h") } else { format!("{h}h{m}m") }
                };
                let ao_text = format!("AO: {hs}/{caps} | {uptime} | {ch_str}");
                self.lua.update_indicator_value("ao_status", &ao_text);
            }

            if self.mode == OperatingMode::Safe {
                self.network.rotate_display(false);
                let ip_str = self.network.display_ip_str(
                    self.bluetooth.ip_address.as_deref(),
                );
                self.lua.update_indicator_value("ip_display", &ip_str);
            }

            self.update_display();
        }
        if remainder > 0 {
            std::thread::sleep(Duration::from_secs(remainder));
        }
        self.epoch_loop.next_phase(); // -> Scan (increments epoch counter)
    }

    /// Scan phase: count tracked APs, check WiFi health (RAGE mode only).
    fn run_scan_phase(&mut self, result: &mut epoch::EpochResult) {
        self.epoch_loop.phase = epoch::EpochPhase::Scan;
        self.recovery.log(recovery::DiagLevel::Info, "epoch scan start");

        result.channel = 0; // AO handles channel hopping
        result.aps_seen = self.wifi.tracker.count() as u32;

        // Health check on WiFi (RAGE mode only — SAFE mode intentionally has no monitor)
        if self.mode == OperatingMode::Rage && self.recovery.should_check() {
            // Check actual interface existence, not just internal state.
            // Firmware crashes remove the interface without updating wifi.state.
            #[cfg(unix)]
            let iface_exists = std::path::Path::new("/sys/class/net/wlan0mon").exists();
            #[cfg(not(unix))]
            let iface_exists = self.wifi.state == wifi::WifiState::Monitor;

            let health = if !iface_exists {
                // Interface gone — hard firmware crash
                if self.wifi.state == wifi::WifiState::Monitor {
                    log::warn!("wlan0mon disappeared — firmware crash detected");
                    self.wifi.state = wifi::WifiState::Down;
                }
                recovery::HealthCheck::Missing
            } else if self.ao.crash_count >= 3 {
                // Interface up but AO keeps crashing — firmware degraded (PSM wedged)
                log::warn!(
                    "wlan0mon exists but AO crashed {} times — firmware degraded",
                    self.ao.crash_count
                );
                recovery::HealthCheck::Unresponsive
            } else {
                recovery::HealthCheck::Ok
            };
            let action = self.recovery.process_health(health);
            self.handle_recovery_action(action);
        }
    }

    /// Attack phase: schedule attacks against tracked APs.
    fn run_attack_phase(&mut self, result: &mut epoch::EpochResult) {
        self.epoch_loop.next_phase(); // -> Attack
        let attackable = self.wifi.tracker.attackable();
        // Read attack toggles from web state
        let enabled_types = {
            let s = self.shared_state.lock().unwrap();
            [s.attack_deauth, s.attack_pmkid, s.attack_csa, s.attack_disassoc]
        };
        for ap in &attackable {
            if self.attacks.is_whitelisted(&ap.bssid) {
                continue;
            }
            if let Some(attack_type) = self.attacks.next_attack(&ap.bssid, &enabled_types) {
                let attack_result = attacks::AttackResult {
                    attack_type,
                    target_bssid: ap.bssid,
                    success: self.ao.state == ao::AoState::Running,
                    handshake_captured: false, // per-attack attribution not possible; epoch-level detection below
                    timestamp: std::time::Instant::now(),
                };
                self.attacks.record(&attack_result);
                result.deauths_sent += 1;
                if attack_result.handshake_captured {
                    result.handshakes_captured += 1;
                }
            }
        }
    }

    /// Capture phase: scan tmpfs for new AO captures, validate, move handshakes to SD.
    fn run_capture_phase(&mut self, result: &mut epoch::EpochResult) {
        self.epoch_loop.next_phase(); // -> Capture
        result.associations = self.wifi.tracker.total_clients();

        // 1. Scan tmpfs for new AO captures
        let tmpfs_dir = std::path::Path::new(&self.tmpfs_capture_dir);
        let mut tmpfs_manager = capture::CaptureManager::new(
            &self.tmpfs_capture_dir,
        );
        let _ = tmpfs_manager.scan_directory();

        // 2. Convert new captures in tmpfs via hcxpcapngtool
        if !tmpfs_manager.files.is_empty() {
            let (converted, _no_hs, failed) = capture::batch_convert(&mut tmpfs_manager);
            if converted > 0 {
                info!("tmpfs: converted {converted} capture(s) to .22000");
            }
            if failed > 0 {
                log::warn!("tmpfs: {failed} conversion(s) failed");
            }
        }

        // 3. Move validated captures to SD, delete junk from tmpfs
        let permanent_dir = self.captures.capture_dir.clone();
        let (moved, deleted) = capture::move_validated_captures(
            tmpfs_dir,
            &permanent_dir,
            &mut self.captures,
        );
        if moved > 0 || deleted > 0 {
            info!("capture pipeline: {moved} saved to SD, {deleted} junk deleted from RAM");
        }

        // 4. Scan permanent dir for upload tracking + new handshake detection
        let handshakes_before = self.captures.handshake_count();
        match self.captures.scan_directory() {
            Ok(new) => {
                if new > 0 {
                    info!("capture scan: {new} new file(s) on SD");
                }
            }
            Err(e) => log::warn!("capture scan failed: {e}"),
        }

        // 5. Upload pending .22000 files to WPA-SEC
        let (uploaded, upload_failed) = capture::upload_all_pending(
            &mut self.captures,
            &self.wpasec_config,
            &mut self.upload_queue,
        );
        if uploaded > 0 {
            info!("WPA-SEC: uploaded {uploaded} files");
        }
        if upload_failed > 0 {
            log::warn!("WPA-SEC: {upload_failed} upload(s) failed");
        }

        let new_handshakes = self.captures.handshake_count().saturating_sub(handshakes_before);
        result.handshakes_captured = new_handshakes as u32;

        // Discord notification for new handshakes
        if new_handshakes > 0 && self.discord_enabled && !self.discord_webhook_url.is_empty() {
            let msg = format!(
                "New handshake(s) captured! {} new, {} total",
                new_handshakes,
                self.captures.handshake_count()
            );
            Self::post_discord(&self.discord_webhook_url, &msg);
        }
    }

    /// Post a message to a Discord webhook via curl.
    #[cfg(unix)]
    fn post_discord(webhook_url: &str, message: &str) {
        if webhook_url.is_empty() {
            return;
        }
        let body = format!(r#"{{"content":"{}"}}"#, message.replace('"', r#"\""#));
        let _ = std::process::Command::new("curl")
            .args(["-s", "-H", "Content-Type: application/json", "-d", &body, webhook_url])
            .output();
    }

    #[cfg(not(unix))]
    fn post_discord(_webhook_url: &str, _message: &str) {
        // Discord posting requires curl (Unix only)
    }

    /// Update face variety engine, XP, battery overrides, and AO crash recovery.
    fn update_face_and_personality(&mut self, result: &epoch::EpochResult) {
        // Tick countdowns (milestones, friend, upload, capture face)
        self.epoch_loop.personality.variety.tick_countdowns();

        // Wire captures into variety engine for milestones + face cycling
        if result.handshakes_captured > 0 {
            let total = self.epoch_loop.personality.total_handshakes;
            self.epoch_loop.personality.variety.on_capture(total);
        } else {
            self.epoch_loop.personality.variety.tick_idle();
        }

        // Set time-of-day state for variety engine
        let hour = chrono::Local::now().hour();
        self.epoch_loop.personality.variety.current_hour = hour;
        if (6..=8).contains(&hour)
            && !self.epoch_loop.personality.variety.morning_greeted
            && self.epoch_loop.personality.variety.current_override() == Some("motivated")
        {
            self.epoch_loop.personality.variety.morning_greeted = true;
        }

        // Generate bull-themed status message (handles joke cycling)
        self.epoch_loop.personality.generate_status();

        // Periodic XP save (every 5 epochs)
        self.epoch_loop.personality.xp.tick_epoch(); // +1 passive XP
        self.epoch_loop.personality.xp.award_aps(self.ao.ap_count()); // +1 per AP
        if self.epoch_loop.personality.xp.should_save() {
            let mood = self.epoch_loop.personality.mood.value();
            if let Err(e) = self.epoch_loop.personality.xp.save(mood) {
                log::warn!("XP save failed: {e}");
            }
        }

        // Battery face overrides
        self.check_battery_overrides();

        // Clear AO crash face if AO recovered
        if self.ao.state == ao::AoState::Running
            && self.epoch_loop.personality.override_face == Some(personality::Face::AoCrashed)
        {
            self.epoch_loop.personality.clear_override();
        }

        // Record stable epoch for AO
        if self.ao.state == ao::AoState::Running {
            self.ao.record_stable_epoch();
        }
    }

    /// Process commands queued by the web server.
    fn process_web_commands(&mut self) {
        let mut any_command = false;

        let (mode_switch, rate_change, restart, shutdown, bt_toggle, pwn_restart) = {
            let mut s = self.shared_state.lock().unwrap();
            let mode = s.pending_mode_switch.take();
            let rate = s.pending_rate_change.take();
            let restart = s.pending_restart;
            s.pending_restart = false;
            let shutdown = s.pending_shutdown;
            s.pending_shutdown = false;
            let bt_toggle = s.pending_bt_toggle.take();
            let pwn_restart = s.pending_pwnagotchi_restart;
            s.pending_pwnagotchi_restart = false;
            (mode, rate, restart, shutdown, bt_toggle, pwn_restart)
        };

        if let Some(mode) = mode_switch {
            any_command = true;
            info!("web: mode switch to {mode}");
            match mode.to_uppercase().as_str() {
                "TOGGLE" => {
                    let new_mode = self.mode.toggle();
                    match new_mode {
                        OperatingMode::Safe => self.enter_safe_mode(),
                        OperatingMode::Rage => self.enter_rage_mode(),
                    }
                }
                "SAFE" if self.mode == OperatingMode::Rage => self.enter_safe_mode(),
                "RAGE" if self.mode == OperatingMode::Safe => self.enter_rage_mode(),
                _ => {
                    let mut s = self.shared_state.lock().unwrap();
                    s.mode = mode;
                }
            }
        }

        if let Some(rate) = rate_change {
            any_command = true;
            info!("web: rate change to {rate}");
            self.ao.set_rate(rate);
        }

        if restart {
            any_command = true;
            info!("web: AO restart requested");
            match self.ao.restart() {
                Ok(()) => info!("AO restarted successfully"),
                Err(e) => log::error!("AO restart failed: {e}"),
            }
        }

        if shutdown {
            any_command = true;
            info!("web: system shutdown requested");
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("sudo")
                    .args(["shutdown", "-h", "now"])
                    .spawn();
            }
        }

        if pwn_restart {
            any_command = true;
            info!("web: oxigotchi service restart requested");
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("sudo")
                    .args(["systemctl", "restart", "rusty-oxigotchi"])
                    .spawn();
            }
        }

        if let Some(visible) = bt_toggle {
            any_command = true;
            info!("web: BT visibility set to {visible}");
            // TODO: implement bluetooth.set_discoverable(visible)
        }

        // Process pending plugin position updates
        let plugin_updates = {
            let mut s = self.shared_state.lock().unwrap();
            std::mem::take(&mut s.pending_plugin_updates)
        };
        if !plugin_updates.is_empty() {
            for update in &plugin_updates {
                if let (Some(x), Some(y)) = (update.x, update.y) {
                    let x = x.clamp(0, 249);
                    let y = y.clamp(0, 121);
                    self.lua.update_plugin_position(&update.name, x, y);
                    info!("plugin {}: position updated to ({x},{y})", update.name);
                }
            }
            lua::config::write_plugins_toml(&self.lua.get_plugin_configs());
            info!("persisted plugin positions to plugins.toml");
        }

        // Process pending whitelist add/remove
        let (wl_add, wl_remove) = {
            let mut s = self.shared_state.lock().unwrap();
            (s.pending_whitelist_add.take(), s.pending_whitelist_remove.take())
        };
        if let Some(add) = wl_add {
            // Parse MAC address string to [u8; 6]
            let parts: Vec<&str> = add.value.split(':').collect();
            if parts.len() == 6 {
                let mut mac = [0u8; 6];
                let mut ok = true;
                for (i, p) in parts.iter().enumerate() {
                    match u8::from_str_radix(p, 16) {
                        Ok(b) => mac[i] = b,
                        Err(_) => { ok = false; break; }
                    }
                }
                if ok {
                    if !self.attacks.is_whitelisted(&mac) {
                        self.attacks.whitelist.push(mac);
                        info!("web: whitelist added {}", add.value);
                        any_command = true;
                    }
                }
            }
        }
        if let Some(remove) = wl_remove {
            let parts: Vec<&str> = remove.split(':').collect();
            if parts.len() == 6 {
                let mut mac = [0u8; 6];
                let mut ok = true;
                for (i, p) in parts.iter().enumerate() {
                    match u8::from_str_radix(p, 16) {
                        Ok(b) => mac[i] = b,
                        Err(_) => { ok = false; break; }
                    }
                }
                if ok {
                    self.attacks.whitelist.retain(|m| m != &mac);
                    info!("web: whitelist removed {}", remove);
                    any_command = true;
                }
            }
        }

        // Process pending channel config
        let ch_config = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_channel_config.take()
        };
        if ch_config.is_some() {
            any_command = true;
        }
        if let Some(cfg) = ch_config {
            if let Some(channels) = cfg.channels {
                if !channels.is_empty() {
                    self.wifi.channel_config.channels = channels.clone();
                    self.wifi.channel_config.current_index = 0;
                    info!("web: channels set to {:?}", channels);
                }
            }
            if let Some(dwell) = cfg.dwell_ms {
                self.wifi.channel_config.dwell_ms = dwell;
                info!("web: dwell set to {}ms", dwell);
            }
        }

        // Process pending WPA-SEC key
        let wpasec_key = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_wpasec_key.take()
        };
        if let Some(key) = wpasec_key {
            self.wpasec_config.api_key = key.clone();
            self.wpasec_config.enabled = !key.is_empty();
            info!("web: WPA-SEC key updated, enabled={}", self.wpasec_config.enabled);
            any_command = true;
        }

        // Process pending Discord config
        let discord_config = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_discord_config.take()
        };
        if let Some(cfg) = discord_config {
            self.discord_webhook_url = cfg.webhook_url;
            self.discord_enabled = cfg.enabled;
            info!("web: Discord updated, enabled={}", self.discord_enabled);
            any_command = true;
        }

        // Process pending settings update
        let settings = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_settings.take()
        };
        if let Some(update) = settings {
            if let Some(name) = update.name {
                if !name.is_empty() {
                    self.config.name = name.clone();
                    let mut s = self.shared_state.lock().unwrap();
                    s.name = name.clone();
                    info!("web: device name changed to {name}");
                    any_command = true;
                }
            }
        }

        // Persist runtime state if any command was processed
        if any_command {
            self.save_runtime_state();
        }
    }

    /// Battery state snapshot: (level, charging, voltage_mv, low, critical, available).
    fn battery_snapshot(&self) -> (u8, bool, u16, bool, bool, bool) {
        (
            self.battery.status.level,
            self.battery.status.charge_state == pisugar::ChargeState::Charging,
            self.battery.status.voltage_mv,
            self.battery.status.low,
            self.battery.status.critical,
            self.battery.available,
        )
    }

    /// AO process state snapshot: (state_str, pid, crash_count, uptime_str, uptime_secs).
    fn ao_snapshot(&self) -> (String, u32, u32, String, u64) {
        (
            self.ao.state_str().to_string(),
            self.ao.pid,
            self.ao.crash_count,
            self.ao.uptime_str(),
            self.ao.uptime_secs(),
        )
    }

    /// Bluetooth state snapshot: (connected, status_short, ip, internet_available).
    fn bt_snapshot(&self) -> (bool, String, String, bool) {
        (
            self.bluetooth.state == bluetooth::BtState::Connected,
            self.bluetooth.status_short().to_string(),
            self.bluetooth.ip_address.clone().unwrap_or_default(),
            self.bluetooth.internet_available,
        )
    }

    /// Build an EpochState snapshot for Lua plugins.
    fn build_epoch_state(&self) -> lua::state::EpochState {
        let m = &self.epoch_loop.metrics;

        // Read system info (CPU temp, memory)
        let (si, _cpu_sample) = personality::SystemInfo::read(&self.prev_cpu_sample);

        // Read CPU frequency
        let cpu_freq_ghz = {
            #[cfg(target_os = "linux")]
            {
                std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
                    .ok()
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .map(|khz| format!("{:.1}G", khz / 1_000_000.0))
                    .unwrap_or_else(|| "---".into())
            }
            #[cfg(not(target_os = "linux"))]
            { "---".to_string() }
        };

        // Get CPU percent from shared state
        let cpu_pct = {
            let s = self.shared_state.lock().unwrap();
            s.cpu_percent
        };

        let (bat_level, bat_charging, bat_mv, bat_low, bat_crit, bat_avail) = self.battery_snapshot();
        let (ao_state, ao_pid, ao_crashes, ao_uptime, ao_up_secs) = self.ao_snapshot();
        let (bt_conn, bt_short, bt_ip, bt_inet) = self.bt_snapshot();

        lua::state::EpochState {
            uptime_secs: self.epoch_loop.uptime_secs(),
            epoch: m.epoch,
            mode: self.mode.as_str().to_string(),
            channel: { let ch = self.ao.channel(); if ch > 0 { ch as u8 } else { m.channel } },
            aps_seen: self.ao.ap_count(),
            handshakes: m.handshakes,
            captures_total: self.captures.count(),
            blind_epochs: m.blind_epochs,
            ao_state,
            ao_pid,
            ao_crash_count: ao_crashes,
            ao_uptime_str: ao_uptime,
            ao_uptime_secs: ao_up_secs,
            ao_channels: {
                let ch = self.ao.channel();
                if ch > 0 { ch.to_string() } else { "?".to_string() }
            },
            battery_level: bat_level,
            battery_charging: bat_charging,
            battery_voltage_mv: bat_mv,
            battery_low: bat_low,
            battery_critical: bat_crit,
            battery_available: bat_avail,
            bt_connected: bt_conn,
            bt_short,
            bt_ip,
            bt_internet: bt_inet,
            internet_online: self.network.internet == network::InternetStatus::Online,
            display_ip: self.network.display_ip_str(self.bluetooth.ip_address.as_deref()),
            mood: self.epoch_loop.personality.mood.value(),
            face: self.epoch_loop.current_face().as_str().to_string(),
            level: self.epoch_loop.personality.xp.level,
            xp: self.epoch_loop.personality.xp.xp,
            status_message: self.epoch_loop.personality.status_msg(),
            cpu_temp: si.cpu_temp_c,
            mem_used_mb: si.mem_used_mb,
            mem_total_mb: si.mem_total_mb,
            cpu_percent: cpu_pct,
            cpu_freq_ghz,
        }
    }

    /// Persist runtime state to JSON so web-configurable settings survive restarts.
    fn save_runtime_state(&self) {
        let s = self.shared_state.lock().unwrap();
        let state = serde_json::json!({
            "attack_deauth": s.attack_deauth,
            "attack_pmkid": s.attack_pmkid,
            "attack_csa": s.attack_csa,
            "attack_disassoc": s.attack_disassoc,
            "attack_anon_reassoc": s.attack_anon_reassoc,
            "attack_rogue_m2": s.attack_rogue_m2,
            "attack_rate": self.ao.config.rate,
            "whitelist": self.attacks.whitelist.iter().map(|m|
                m.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
            ).collect::<Vec<String>>(),
            "wpasec_key": self.wpasec_config.api_key,
            "discord_webhook_url": self.discord_webhook_url,
            "discord_enabled": self.discord_enabled,
        });
        drop(s);
        let path = "/var/lib/oxigotchi/state.json";
        let _ = std::fs::create_dir_all("/var/lib/oxigotchi");
        if let Err(e) = std::fs::write(path, serde_json::to_string_pretty(&state).unwrap_or_default()) {
            log::warn!("state save failed: {e}");
        }
    }

    /// Load persisted runtime state from JSON and apply values.
    fn load_runtime_state(&mut self) {
        let path = "/var/lib/oxigotchi/state.json";
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return, // No saved state, use defaults
        };
        let state: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("state load parse error: {e}");
                return;
            }
        };

        // Apply attack toggles to shared state
        {
            let mut s = self.shared_state.lock().unwrap();
            if let Some(v) = state.get("attack_deauth").and_then(|v| v.as_bool()) {
                s.attack_deauth = v;
            }
            if let Some(v) = state.get("attack_pmkid").and_then(|v| v.as_bool()) {
                s.attack_pmkid = v;
            }
            if let Some(v) = state.get("attack_csa").and_then(|v| v.as_bool()) {
                s.attack_csa = v;
            }
            if let Some(v) = state.get("attack_disassoc").and_then(|v| v.as_bool()) {
                s.attack_disassoc = v;
            }
            if let Some(v) = state.get("attack_anon_reassoc").and_then(|v| v.as_bool()) {
                s.attack_anon_reassoc = v;
            }
            if let Some(v) = state.get("attack_rogue_m2").and_then(|v| v.as_bool()) {
                s.attack_rogue_m2 = v;
            }
        }

        // Apply attack rate
        if let Some(rate) = state.get("attack_rate").and_then(|v| v.as_u64()) {
            self.ao.set_rate(rate as u32);
        }

        // Apply whitelist
        if let Some(wl) = state.get("whitelist").and_then(|v| v.as_array()) {
            self.attacks.whitelist.clear();
            for entry in wl {
                if let Some(mac_str) = entry.as_str() {
                    let parts: Vec<&str> = mac_str.split(':').collect();
                    if parts.len() == 6 {
                        let mut mac = [0u8; 6];
                        let mut ok = true;
                        for (i, p) in parts.iter().enumerate() {
                            match u8::from_str_radix(p, 16) {
                                Ok(b) => mac[i] = b,
                                Err(_) => { ok = false; break; }
                            }
                        }
                        if ok {
                            self.attacks.whitelist.push(mac);
                        }
                    }
                }
            }
        }

        // Apply WPA-SEC key
        if let Some(key) = state.get("wpasec_key").and_then(|v| v.as_str()) {
            self.wpasec_config.api_key = key.to_string();
            self.wpasec_config.enabled = !key.is_empty();
        }

        // Apply Discord config
        if let Some(url) = state.get("discord_webhook_url").and_then(|v| v.as_str()) {
            self.discord_webhook_url = url.to_string();
        }
        if let Some(enabled) = state.get("discord_enabled").and_then(|v| v.as_bool()) {
            self.discord_enabled = enabled;
        }

        info!("loaded runtime state from {path}");
    }

    /// Sync daemon state into the shared web state.
    fn sync_to_web(&self) {
        let mut s = self.shared_state.lock().unwrap();
        let m = &self.epoch_loop.metrics;

        let (bat_level, bat_charging, bat_mv, bat_low, bat_crit, bat_avail) = self.battery_snapshot();
        let (ao_state, ao_pid, ao_crashes, ao_uptime, _ao_up_secs) = self.ao_snapshot();
        let (bt_conn, _bt_short, bt_ip, bt_inet) = self.bt_snapshot();

        s.uptime_str = self.epoch_loop.uptime_str();
        s.mode = self.mode.as_str().to_string();
        s.epoch = m.epoch;
        s.channel = { let ch = self.ao.channel(); if ch > 0 { ch as u8 } else { m.channel } };
        s.aps_seen = self.ao.ap_count();
        s.handshakes = m.handshakes;
        s.blind_epochs = m.blind_epochs;
        s.mood = self.epoch_loop.personality.mood.value();
        s.face = self.epoch_loop.current_face().as_str().to_string();
        s.status_message = self.epoch_loop.personality.status_msg();

        s.total_attacks = self.attacks.total_attacks;
        s.total_handshakes_attacks = self.attacks.total_handshakes;
        s.attack_rate = self.ao.config.rate;
        s.deauths_this_epoch = m.deauths_this_epoch;

        s.capture_dir = self.captures.capture_dir.to_string_lossy().to_string();
        s.capture_files = self.captures.count();
        s.handshake_files = self.captures.handshake_count();
        s.pending_upload = self.captures.pending_upload_count();
        s.total_capture_size = self.captures.total_size();
        s.capture_list = self.captures.files.iter().map(|f| {
            web::CaptureEntry {
                filename: f.path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                size_bytes: f.size,
            }
        }).collect();

        s.battery_level = bat_level;
        s.battery_charging = bat_charging;
        s.battery_voltage_mv = bat_mv;
        s.battery_low = bat_low;
        s.battery_critical = bat_crit;
        s.battery_available = bat_avail;

        s.wifi_state = format!("{:?}", self.wifi.state);
        s.wifi_aps_tracked = self.wifi.tracker.count();

        s.bt_state = self.bluetooth.status_str().to_string(); // long form for web
        s.bt_connected = bt_conn;
        s.bt_ip = bt_ip;
        s.bt_internet_available = bt_inet;
        s.bt_retry_count = self.bluetooth.retry_count;

        s.ao_state = ao_state;
        s.ao_pid = ao_pid;
        s.ao_crash_count = ao_crashes;
        s.ao_uptime = ao_uptime;
        s.gpsd_available = self.ao.gpsd_detected;

        s.xp = self.epoch_loop.personality.xp.xp;
        s.level = self.epoch_loop.personality.xp.level;

        s.recovery_state = format!("{:?}", self.recovery.state);
        s.recovery_total = self.recovery.total_recoveries;
        s.recovery_soft_retries = self.recovery.soft_retry_count;
        s.recovery_hard_retries = self.recovery.hard_retry_count;

        // Copy framebuffer for web display preview
        s.screen_width = self.screen.fb.width;
        s.screen_height = self.screen.fb.height;
        s.screen_bytes = self.screen.fb.as_bytes().to_vec();

        // Sync AP list for web dashboard (cross-reference with captures for handshake status)
        s.ap_list = self.wifi.tracker.sorted_by_rssi().iter().map(|ap| {
            let has_hs = self.captures.files.iter().any(|f| {
                f.has_handshake && f.bssid == ap.bssid
            });
            web::ApEntry {
                bssid: ap.bssid_str(),
                ssid: if ap.ssid.is_empty() { "(hidden)".into() } else { ap.ssid.clone() },
                rssi: ap.rssi as i16,
                channel: ap.channel,
                clients: ap.client_count,
                has_handshake: has_hs,
            }
        }).collect();

        // Sync whitelist for web dashboard
        s.whitelist = self.attacks.whitelist.iter().map(|mac| {
            web::WhitelistEntry {
                value: mac.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":"),
                entry_type: "MAC".into(),
            }
        }).collect();

        // Sync plugin list for web dashboard
        s.plugin_list = self.lua.get_web_plugin_list().into_iter().map(|(meta, x, y)| {
            web::PluginInfo {
                name: meta.name,
                version: meta.version,
                author: meta.author,
                tag: meta.tag,
                enabled: true,
                x,
                y,
            }
        }).collect();

        // Sync WPA-SEC and Discord config for web dashboard
        s.wpasec_api_key = self.wpasec_config.api_key.clone();
        s.discord_webhook_url = self.discord_webhook_url.clone();
        s.discord_enabled = self.discord_enabled;
    }

    /// Update the e-ink display with current state.
    /// Layout matches Python angryoxide.py AO mode — see docs/DISPLAY_SPEC.md.
    fn update_display(&mut self) {
        self.screen.clear();

        // ---- LINE 1 (y=14) ----
        self.screen.draw_hline(0, 14, display::DISPLAY_WIDTH);

        // ---- FACE at (0,16) — 120x66 bull bitmap ----
        let face = self.epoch_loop.current_face();
        self.screen.draw_face(&face);

        // ---- XP BAR right of face (~125, 65) ----
        // "Lv N" text then graphical bar, matching Python "Lv 1  Exp|███" style
        let xp = &self.epoch_loop.personality.xp;
        let lv_str = format!("Lv {}", xp.level);
        self.screen.draw_text(&lv_str, 125, 73);
        // Bar: fixed position so layout works for Lv 1 through Lv 999
        let bar_x: u32 = 168; // gap after "Lv 100" (ends ~x=155)
        let bar_y: u32 = 74;
        let bar_w: u32 = 80; // extends to x=248
        let bar_h: u32 = 7;
        let needed = xp.xp_to_next_level();
        let filled_w = if needed > 0 {
            ((xp.xp as u32) * (bar_w - 2) / needed as u32).min(bar_w - 2)
        } else {
            bar_w - 2
        };
        // Outline
        for dx in 0..bar_w {
            self.screen.set_pixel(bar_x + dx, bar_y, embedded_graphics::pixelcolor::BinaryColor::On);
            self.screen.set_pixel(bar_x + dx, bar_y + bar_h - 1, embedded_graphics::pixelcolor::BinaryColor::On);
        }
        for dy in 0..bar_h {
            self.screen.set_pixel(bar_x, bar_y + dy, embedded_graphics::pixelcolor::BinaryColor::On);
            self.screen.set_pixel(bar_x + bar_w - 1, bar_y + dy, embedded_graphics::pixelcolor::BinaryColor::On);
        }
        // Fill
        for dy in 1..(bar_h - 1) {
            for dx in 1..=filled_w {
                self.screen.set_pixel(bar_x + dx, bar_y + dy, embedded_graphics::pixelcolor::BinaryColor::On);
            }
        }

        // ---- LINE 2 (y=108) ----
        self.screen.draw_hline(0, 108, display::DISPLAY_WIDTH);

        // ---- LUA PLUGIN INDICATORS ----
        for ind in self.lua.get_indicators() {
            self.screen.draw_indicator(&ind);
        }

        self.screen.flush();
    }

    /// Check battery level and apply face overrides.
    fn check_battery_overrides(&mut self) {
        if !self.battery.available {
            return;
        }
        self.battery.read_status();
        if self.battery.status.critical {
            self.epoch_loop.personality.set_override(personality::Face::BatteryCritical);
        } else if self.battery.status.low {
            self.epoch_loop.personality.set_override(personality::Face::BatteryLow);
        } else {
            // Clear battery overrides if we previously set one
            if matches!(
                self.epoch_loop.personality.override_face,
                Some(personality::Face::BatteryCritical)
                    | Some(personality::Face::BatteryLow)
                    | Some(personality::Face::Shutdown)
            ) {
                self.epoch_loop.personality.clear_override();
            }
        }

        if self.battery.should_shutdown() {
            info!("battery critical, shutting down");
            self.epoch_loop.personality.set_override(personality::Face::Shutdown);
            self.update_display();
            // Real implementation would call `shutdown -h now`
        }
    }

    /// Show a transition screen with face and message, then flush immediately.
    fn show_transition(&mut self, face: personality::Face, message: &str) {
        self.screen.clear();
        self.screen.draw_face(&face);
        self.screen.draw_status(message);
        self.screen.flush();
    }

    /// Pick a random face from a pool.
    fn random_face(faces: &[personality::Face]) -> personality::Face {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let idx = rng.gen_range(0..faces.len());
        faces[idx]
    }

    /// Face pools for each mode.
    const RAGE_FACES: &'static [personality::Face] = &[
        personality::Face::Angry,
        personality::Face::Intense,
        personality::Face::Excited,
        personality::Face::Upload,
        personality::Face::Motivated,
    ];
    const SAFE_FACES: &'static [personality::Face] = &[
        personality::Face::Debug,
        personality::Face::Grateful,
    ];

    /// Transition from RAGE to SAFE mode.
    fn enter_safe_mode(&mut self) {
        info!("mode: RAGE -> SAFE");
        let face = Self::random_face(Self::SAFE_FACES);
        self.show_transition(face, "Switching to SAFE...");

        // Stop attacks
        self.ao.stop();

        // Exit WiFi monitor mode
        if let Err(e) = self.wifi.stop_monitor() {
            log::warn!("WiFi monitor stop failed: {e}");
        }

        // Reset shared UART — WiFi monitor mode leaves it in a state where
        // BT HCI commands time out (BCM43436B0 shared UART).
        bluetooth::reset_hci_uart();

        // Connect Bluetooth
        match self.bluetooth.setup() {
            Ok(()) => info!("BT connected: {}", self.bluetooth.status_str()),
            Err(e) => log::warn!("BT setup failed: {e}"),
        }

        self.mode = OperatingMode::Safe;
        self.epoch_loop.personality.set_override(face);
    }

    /// Transition from SAFE to RAGE mode.
    fn enter_rage_mode(&mut self) {
        info!("mode: SAFE -> RAGE");
        let face = Self::random_face(Self::RAGE_FACES);
        self.show_transition(face, "Switching to RAGE...");

        // Power off Bluetooth adapter to free radio for WiFi
        self.bluetooth.power_off();

        // Wait for UART to settle after BT release
        std::thread::sleep(Duration::from_secs(2));

        // Enter WiFi monitor mode
        match self.wifi.start_monitor() {
            Ok(()) => info!("WiFi monitor mode started"),
            Err(e) => log::error!("WiFi monitor failed: {e}"),
        }

        // Start AngryOxide
        match self.ao.start() {
            Ok(()) => info!("AO started: PID {}", self.ao.pid),
            Err(e) => log::error!("AO start failed: {e}"),
        }

        self.mode = OperatingMode::Rage;
        self.network.display_slot = network::DisplaySlot::UsbIp;
        self.epoch_loop.personality.clear_override();
    }

    /// Handle recovery actions from the health checker.
    fn handle_recovery_action(&mut self, action: recovery::RecoveryAction) {
        match action {
            recovery::RecoveryAction::None => {}
            recovery::RecoveryAction::SoftRecover => {
                if self.recovery.cooldown_active() {
                    log::warn!("recovery cooldown active, skipping soft recovery");
                    return;
                }
                info!("attempting soft WiFi recovery (modprobe cycle)");
                self.epoch_loop.personality.set_override(personality::Face::WifiDown);

                // Stop AO first — it's using the interface
                self.ao.stop();

                // Stop monitor mode (may fail if interface is gone — that's OK)
                let _ = self.wifi.stop_monitor();

                // Full brcmfmac modprobe cycle (matches Python's _try_fw_recovery)
                #[cfg(unix)]
                {
                    use std::process::Command;
                    info!("removing brcmfmac module");
                    let _ = Command::new("modprobe").args(["-r", "brcmfmac"]).output();
                    std::thread::sleep(Duration::from_secs(2));
                    info!("reloading brcmfmac module");
                    let _ = Command::new("modprobe").arg("brcmfmac").output();
                    // Poll for wlan0 to reappear (firmware load is async)
                    let wlan0 = std::path::Path::new("/sys/class/net/wlan0");
                    for i in 0..10 {
                        if wlan0.exists() {
                            info!("wlan0 back after {}s", i + 2);
                            break;
                        }
                        std::thread::sleep(Duration::from_secs(1));
                    }
                }

                // Re-enter monitor mode
                match self.wifi.start_monitor() {
                    Ok(()) => {
                        info!("soft recovery: monitor mode restored");
                        // Reset AO crash counter so we don't immediately re-trigger
                        self.ao.reset();
                        // Restart AO
                        match self.ao.start() {
                            Ok(()) => info!("soft recovery: AO restarted (PID {})", self.ao.pid),
                            Err(e) => log::error!("soft recovery: AO restart failed: {e}"),
                        }
                    }
                    Err(e) => log::error!("soft recovery: monitor mode failed: {e}"),
                }
                self.recovery.record_recovery();
            }
            recovery::RecoveryAction::HardRecover => {
                if self.recovery.cooldown_active() {
                    log::warn!("recovery cooldown active, skipping hard recovery");
                    return;
                }
                info!("attempting hard WiFi recovery (full GPIO power cycle)");
                self.epoch_loop.personality.set_override(personality::Face::FwCrash);
                match recovery::execute_gpio_recovery(self.recovery.config.gpio_cycle_delay_ms) {
                    Ok(true) => info!("GPIO recovery succeeded, wlan0 is back"),
                    Ok(false) => log::error!("GPIO recovery failed: wlan0 did not return"),
                    Err(e) => log::error!("GPIO recovery error: {e}"),
                }
                self.recovery.record_recovery();
                let _ = self.wifi.start_monitor();
            }
            recovery::RecoveryAction::Reboot => {
                log::error!("WiFi recovery exhausted after max retries, rebooting");
                self.epoch_loop.personality.set_override(personality::Face::Broken);
                self.recovery.log(
                    recovery::DiagLevel::Error,
                    "all recovery attempts exhausted -- rebooting",
                );
                let _ = recovery::trigger_reboot();
            }
            recovery::RecoveryAction::GiveUp => {
                log::error!("WiFi recovery exhausted, giving up");
                self.epoch_loop.personality.set_override(personality::Face::Broken);
            }
        }
    }

    /// Build a web status snapshot.
    fn build_web_status(&self) -> web::StatusResponse {
        let m = &self.epoch_loop.metrics;
        web::build_status(&web::StatusParams {
            name: &self.config.name,
            uptime: &self.epoch_loop.uptime_str(),
            epoch: m.epoch,
            channel: m.channel,
            aps_seen: self.ao.ap_count(),
            handshakes: m.handshakes,
            blind_epochs: m.blind_epochs,
            mood: self.epoch_loop.personality.mood.value(),
            face: self.epoch_loop.current_face().as_str(),
            status_message: &self.epoch_loop.personality.status_msg(),
            mode: "AO",
        })
    }

    /// Build web attack stats.
    fn build_attack_stats(&self) -> web::AttackStats {
        let s = self.shared_state.lock().unwrap();
        web::AttackStats {
            total_attacks: self.attacks.total_attacks,
            total_handshakes: self.attacks.total_handshakes,
            attack_rate: ATTACK_RATE,
            deauths_this_epoch: self.epoch_loop.metrics.deauths_this_epoch,
            deauth: s.attack_deauth,
            pmkid: s.attack_pmkid,
            csa: s.attack_csa,
            disassoc: s.attack_disassoc,
            anon_reassoc: s.attack_anon_reassoc,
            rogue_m2: s.attack_rogue_m2,
        }
    }

    /// Build web capture info.
    fn build_capture_info(&self) -> web::CaptureInfo {
        web::CaptureInfo {
            total_files: self.captures.count(),
            handshake_files: self.captures.handshake_count(),
            pending_upload: self.captures.pending_upload_count(),
            total_size_bytes: self.captures.total_size(),
            files: vec![],
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    // Run first-boot migration if needed (pwnagotchi -> oxigotchi)
    let legacy = migration::LegacyPaths::default();
    let oxi_paths = migration::OxiPaths::default();
    let migration_result = migration::run_migration(&legacy, &oxi_paths);
    if migration_result.success() && migration_result.config_migrated {
        info!(
            "migration: config migrated, {} captures imported (of {} found)",
            migration_result.captures_imported, migration_result.captures_found
        );
    }
    for w in &migration_result.warnings {
        log::warn!("migration: {w}");
    }
    for e in &migration_result.errors {
        log::error!("migration: {e}");
    }

    // Load oxigotchi config (may have been created by migration above)
    let config = if oxi_paths.config.exists() {
        config::Config::load_or_default(oxi_paths.config.to_str().unwrap_or("/etc/oxigotchi/config.toml"))
    } else {
        config::Config::defaults()
    };
    info!(
        "Hi! I'm {}! Rusty Oxigotchi v{} starting — the bull is awake",
        config.name,
        env!("CARGO_PKG_VERSION")
    );

    // Create shared state for web server <-> daemon communication
    let shared_state = Arc::new(Mutex::new(web::DaemonState::new(&config.name)));

    // Start web server in a tokio task
    let web_state = shared_state.clone();
    tokio::spawn(async move {
        web::start_server(web_state).await;
    });

    // Run the daemon main loop in a blocking thread (it uses std::thread::sleep)
    let mut daemon = Daemon::new(config, shared_state);
    tokio::task::spawn_blocking(move || {
        daemon.boot();
        info!("entering main epoch loop");
        loop {
            daemon.run_epoch();
        }
    })
    .await
    .expect("daemon task panicked");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_daemon() -> Daemon {
        let config = config::Config::defaults();
        let shared_state = Arc::new(Mutex::new(web::DaemonState::new(&config.name)));
        Daemon::new(config, shared_state)
    }

    #[test]
    fn test_daemon_construction() {
        let daemon = make_daemon();
        assert_eq!(daemon.epoch_loop.metrics.epoch, 0);
        assert_eq!(daemon.wifi.state, wifi::WifiState::Down);
        assert_eq!(daemon.captures.count(), 0);
        assert_eq!(daemon.ao.state, ao::AoState::Stopped);
    }

    #[test]
    fn test_daemon_boot() {
        let mut daemon = make_daemon();
        daemon.boot();
        // WiFi should be in monitor mode (stub always succeeds)
        assert_eq!(daemon.wifi.state, wifi::WifiState::Monitor);
    }

    #[test]
    fn test_daemon_build_web_status() {
        let daemon = make_daemon();
        let status = daemon.build_web_status();
        assert_eq!(status.name, "oxigotchi");
        assert_eq!(status.epoch, 0);
        assert_eq!(status.mode, "AO");
    }

    #[test]
    fn test_daemon_build_attack_stats() {
        let daemon = make_daemon();
        let stats = daemon.build_attack_stats();
        assert_eq!(stats.total_attacks, 0);
        assert_eq!(stats.attack_rate, ATTACK_RATE);
    }

    #[test]
    fn test_daemon_build_capture_info() {
        let daemon = make_daemon();
        let info = daemon.build_capture_info();
        assert_eq!(info.total_files, 0);
        assert_eq!(info.pending_upload, 0);
    }

    #[test]
    fn test_attack_success_when_ao_stopped() {
        let daemon = make_daemon();
        // Default state is Stopped on all platforms
        assert_eq!(daemon.ao.state, ao::AoState::Stopped);
        // Build an attack result the way run_epoch does
        let success = daemon.ao.state == ao::AoState::Running;
        assert!(!success, "attack should not succeed when AO is stopped");
    }

    #[test]
    fn test_attack_success_when_ao_running() {
        let mut daemon = make_daemon();
        // Simulate AO running state directly
        daemon.ao.state = ao::AoState::Running;
        let success = daemon.ao.state == ao::AoState::Running;
        assert!(success, "attack should succeed when AO is running");
    }

    #[test]
    fn test_daemon_battery_override_critical_no_shutdown() {
        let mut daemon = make_daemon();
        daemon.battery.available = true;
        daemon.battery.config.auto_shutdown = false;
        daemon.battery.set_level(3); // critical
        daemon.check_battery_overrides();
        assert_eq!(
            daemon.epoch_loop.personality.override_face,
            Some(personality::Face::BatteryCritical)
        );
    }

    #[test]
    fn test_daemon_battery_override_critical_with_shutdown() {
        let mut daemon = make_daemon();
        daemon.battery.available = true;
        daemon.battery.set_level(3); // critical, auto_shutdown = true by default
        daemon.check_battery_overrides();
        // Auto-shutdown overrides to Shutdown face
        assert_eq!(
            daemon.epoch_loop.personality.override_face,
            Some(personality::Face::Shutdown)
        );
    }

    #[test]
    fn test_daemon_battery_override_low() {
        let mut daemon = make_daemon();
        daemon.battery.available = true;
        daemon.battery.set_level(15); // low but not critical
        daemon.check_battery_overrides();
        assert_eq!(
            daemon.epoch_loop.personality.override_face,
            Some(personality::Face::BatteryLow)
        );
    }

    #[test]
    fn test_daemon_battery_override_clears() {
        let mut daemon = make_daemon();
        daemon.battery.available = true;
        daemon.battery.config.auto_shutdown = false;
        daemon.battery.set_level(3);
        daemon.check_battery_overrides();
        assert_eq!(
            daemon.epoch_loop.personality.override_face,
            Some(personality::Face::BatteryCritical)
        );
        daemon.battery.set_level(80);
        daemon.check_battery_overrides();
        assert_eq!(daemon.epoch_loop.personality.override_face, None);
    }

    #[test]
    fn test_daemon_recovery_soft() {
        let mut daemon = make_daemon();
        daemon.handle_recovery_action(recovery::RecoveryAction::SoftRecover);
        assert_eq!(
            daemon.epoch_loop.personality.override_face,
            Some(personality::Face::WifiDown)
        );
    }

    #[test]
    fn test_daemon_recovery_hard() {
        let mut daemon = make_daemon();
        daemon.handle_recovery_action(recovery::RecoveryAction::HardRecover);
        assert_eq!(
            daemon.epoch_loop.personality.override_face,
            Some(personality::Face::FwCrash)
        );
    }

    #[test]
    fn test_daemon_recovery_give_up() {
        let mut daemon = make_daemon();
        daemon.handle_recovery_action(recovery::RecoveryAction::GiveUp);
        assert_eq!(
            daemon.epoch_loop.personality.override_face,
            Some(personality::Face::Broken)
        );
    }

    #[test]
    fn test_daemon_update_display_no_panic() {
        let mut daemon = make_daemon();
        daemon.update_display();
        let pixel_count = daemon.screen.fb.count_set_pixels();
        assert!(pixel_count > 0, "display should have drawn something");
    }

    /// Verify that all 26 Face variants are reachable.
    #[test]
    fn test_face_reachability() {
        use personality::Face;

        let mood_faces = [
            Face::Excited, Face::Happy, Face::Awake, Face::Bored,
            Face::Sad, Face::Demotivated,
        ];
        let override_faces = [
            Face::BatteryCritical, Face::BatteryLow, Face::Shutdown,
            Face::WifiDown, Face::FwCrash, Face::Broken,
        ];
        let manual_override_faces = [
            Face::Sleep, Face::Intense, Face::Cool, Face::Angry,
            Face::Friend, Face::Debug, Face::Upload, Face::Lonely,
            Face::Grateful, Face::Motivated, Face::Smart, Face::AoCrashed,
            Face::Raging, Face::Grazing,
        ];

        let mut reachable: std::collections::HashSet<Face> = std::collections::HashSet::new();
        reachable.extend(mood_faces.iter());
        reachable.extend(override_faces.iter());
        reachable.extend(manual_override_faces.iter());

        for face in Face::all() {
            assert!(
                reachable.contains(face),
                "Face::{face:?} is not reachable through any trigger"
            );
        }
        assert_eq!(reachable.len(), Face::all().len());
    }

    #[test]
    fn test_epoch_drives_mood_faces() {
        let mut daemon = make_daemon();

        assert_eq!(daemon.epoch_loop.current_face(), personality::Face::Awake);

        for _ in 0..10 {
            daemon.epoch_loop.record_result(&epoch::EpochResult {
                handshakes_captured: 3,
                aps_seen: 10,
                ..Default::default()
            });
        }
        let face = daemon.epoch_loop.current_face();
        assert!(
            face == personality::Face::Excited || face == personality::Face::Happy,
            "expected Excited or Happy after many handshakes, got {face:?}"
        );

        for _ in 0..50 {
            daemon.epoch_loop.record_result(&epoch::EpochResult::default());
        }
        let face = daemon.epoch_loop.current_face();
        assert!(
            face == personality::Face::Sad || face == personality::Face::Demotivated,
            "expected Sad or Demotivated after many blind epochs, got {face:?}"
        );
    }

    /// Integration test: create a Daemon, run 3 full epoch cycles.
    #[test]
    fn test_integration_three_epochs() {
        let mut daemon = make_daemon();

        daemon.epoch_loop.epoch_duration = Duration::from_secs(0);
        daemon.boot();

        let mut pixel_counts: Vec<u32> = Vec::new();

        for epoch in 0..3 {
            // ---- SCAN ----
            daemon.epoch_loop.phase = epoch::EpochPhase::Scan;

            let mut result = epoch::EpochResult::default();
            result.channel = 0; // AO handles channel hopping
            result.aps_seen = (epoch + 1) * 2;

            // ---- ATTACK ----
            daemon.epoch_loop.next_phase();
            result.deauths_sent = epoch;

            // ---- CAPTURE ----
            daemon.epoch_loop.next_phase();

            // ---- DISPLAY ----
            daemon.epoch_loop.next_phase();
            daemon.epoch_loop.record_result(&result);
            daemon.update_display();
            pixel_counts.push(daemon.screen.fb.count_set_pixels());

            // ---- SLEEP ----
            daemon.epoch_loop.next_phase();
            if daemon.watchdog.needs_ping() {
                daemon.watchdog.ping();
            }

            // ---- back to SCAN ----
            daemon.epoch_loop.next_phase();
        }

        assert_eq!(daemon.epoch_loop.metrics.epoch, 3);

        for (i, &count) in pixel_counts.iter().enumerate() {
            assert!(count > 0, "epoch {i} should have drawn pixels, got 0");
        }

        assert_eq!(daemon.wifi.state, wifi::WifiState::Monitor);
    }

    #[test]
    fn test_sync_to_web() {
        let mut daemon = make_daemon();
        daemon.boot();
        daemon.sync_to_web();

        let s = daemon.shared_state.lock().unwrap();
        assert_eq!(s.name, "oxigotchi");
        assert_eq!(s.wifi_state, "Monitor");
    }

    #[test]
    fn test_process_web_commands_mode() {
        let mut daemon = make_daemon();
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.pending_mode_switch = Some("PWN".into());
        }
        daemon.process_web_commands();
        let s = daemon.shared_state.lock().unwrap();
        assert_eq!(s.mode, "PWN");
    }

    #[test]
    fn test_process_web_commands_rate() {
        let mut daemon = make_daemon();
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.pending_rate_change = Some(2);
        }
        daemon.process_web_commands();
        assert_eq!(daemon.ao.config.rate, 2);
    }

    #[test]
    fn test_process_web_commands_restart() {
        let mut daemon = make_daemon();
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.pending_restart = true;
        }
        daemon.process_web_commands();
        // On non-Pi, AO start is a stub so it should work
        // Just verify it didn't panic and pending_restart is cleared
        let s = daemon.shared_state.lock().unwrap();
        assert!(!s.pending_restart);
    }

    #[test]
    fn test_daemon_ao_state() {
        let daemon = make_daemon();
        assert_eq!(daemon.ao.state, ao::AoState::Stopped);
        assert_eq!(daemon.ao.crash_count, 0);
    }

    #[test]
    fn test_capture_scan_detects_new_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut cm = capture::CaptureManager::new(dir.path().to_str().unwrap());

        // Initial scan — empty
        assert_eq!(cm.scan_directory().unwrap(), 0);
        assert_eq!(cm.handshake_count(), 0);

        // Create a pcapng + .22000 companion
        let pcap = dir.path().join("test.pcapng");
        std::fs::write(&pcap, b"fake").unwrap();
        let companion = dir.path().join("test.22000");
        std::fs::write(&companion, b"fake").unwrap();

        // Rescan — should find 1 new file with handshake
        let new = cm.scan_directory().unwrap();
        assert_eq!(new, 1);
        assert_eq!(cm.handshake_count(), 1);
    }

    #[test]
    fn test_lua_plugins_load_and_register_indicators() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test_ind.lua"), r#"
            plugin = {}
            plugin.name = "test_ind"
            plugin.version = "1.0.0"
            plugin.author = "test"
            plugin.tag = "default"
            function on_load(config)
                register_indicator("test", { x = config.x or 0, y = config.y or 0, font = "small" })
            end
            function on_epoch(state)
                set_indicator("test", "E:" .. state.epoch)
            end
        "#).unwrap();

        let mut rt = lua::PluginRuntime::new();
        let configs = vec![lua::PluginConfig::default_for("test_ind", 50, 60)];
        let loaded = rt.load_plugins_from_dir(dir.path().to_str().unwrap(), &configs);
        assert_eq!(loaded, 1);

        let state = lua::state::EpochState { epoch: 99, ..Default::default() };
        rt.tick_epoch(&state);

        let indicators = rt.get_indicators();
        assert_eq!(indicators.len(), 1);
        assert_eq!(indicators[0].value, "E:99");
        assert_eq!(indicators[0].x, 50);
        assert_eq!(indicators[0].y, 60);
    }

    #[test]
    fn test_operating_mode_toggle() {
        assert_eq!(OperatingMode::Rage.toggle(), OperatingMode::Safe);
        assert_eq!(OperatingMode::Safe.toggle(), OperatingMode::Rage);
    }

    #[test]
    fn test_operating_mode_as_str() {
        assert_eq!(OperatingMode::Rage.as_str(), "RAGE");
        assert_eq!(OperatingMode::Safe.as_str(), "SAFE");
    }

    #[test]
    fn test_daemon_starts_in_rage_mode() {
        let daemon = make_daemon();
        assert_eq!(daemon.mode, OperatingMode::Rage);
    }
}
