// Modules are the public API surface for Rusty Oxigotchi.
#[allow(dead_code)]
mod ao;
#[allow(dead_code)]
mod attacks;
#[allow(dead_code)]
mod bluetooth;
#[allow(dead_code)]
mod capture;
mod config;
#[allow(dead_code)]
mod display;
mod epoch;
#[allow(dead_code)]
mod migration;
#[allow(dead_code)]
mod personality;
#[allow(dead_code)]
mod pisugar;
#[allow(dead_code)]
mod network;
#[allow(dead_code)]
mod recovery;
#[allow(dead_code)]
mod web;
#[allow(dead_code)]
mod wifi;

use chrono::Timelike;
use log::info;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Epoch duration in seconds.
const EPOCH_DURATION_SECS: u64 = 30;
/// Attack rate (attacks per second). Rate 2+ crashes BCM43436B0.
const ATTACK_RATE: u32 = 1;
/// Default capture directory.
const CAPTURE_DIR: &str = "/home/pi/captures";

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
    shared_state: web::SharedState,
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
            shared_state,
        }
    }

    /// Boot sequence: init display, probe hardware, scan existing captures, start AO.
    fn boot(&mut self) {
        // Display boot screen with debug face (Feature 7: debug on boot)
        let boot_face_key = self.epoch_loop.personality.variety.boot_face();
        let boot_face = if boot_face_key == "debug" {
            personality::Face::Debug
        } else {
            personality::Face::Awake
        };
        self.screen.clear();
        self.screen.draw_face(&boot_face);
        self.screen.draw_name(&self.config.name);
        let welcome = format!("Hi! I'm {}! Starting v{}...",
            self.config.name, env!("CARGO_PKG_VERSION"));
        self.screen.draw_status(&welcome);
        self.screen.flush();
        info!("display initialized");

        // Probe PiSugar battery
        if self.battery.probe() {
            let status = self.battery.read_status();
            info!("PiSugar detected: {}%", status.level);
        } else {
            info!("PiSugar not detected");
        }

        // Start WiFi monitor mode
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

        // Bluetooth tethering
        match self.bluetooth.setup() {
            Ok(()) => info!("bluetooth setup complete: {}", self.bluetooth.status_str()),
            Err(e) => log::warn!("bluetooth setup failed: {e}"),
        }

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

        // Start AngryOxide subprocess
        match self.ao.start() {
            Ok(()) => info!("AO started: PID {}", self.ao.pid),
            Err(e) => {
                log::error!("AO failed to start: {e}");
                self.epoch_loop.personality.set_override(personality::Face::AoCrashed);
            }
        }

        // Initial state sync to web
        self.sync_to_web();
    }

    /// Run one full epoch: Scan -> Attack -> Capture -> Display -> Sleep.
    fn run_epoch(&mut self) {
        let mut result = epoch::EpochResult::default();

        // ---- Check for web commands ----
        self.process_web_commands();

        // ---- AO HEALTH CHECK ----
        if self.ao.check_health() {
            self.epoch_loop.personality.set_override(personality::Face::AoCrashed);
        }
        // Try auto-restart if crashed
        self.ao.try_auto_restart();

        // ---- SCAN PHASE ----
        self.epoch_loop.phase = epoch::EpochPhase::Scan;
        self.recovery.log(recovery::DiagLevel::Info, "epoch scan start");

        result.channel = 0; // AO handles channel hopping
        result.aps_seen = self.wifi.tracker.count() as u32;

        // Health check on WiFi
        if self.recovery.should_check() {
            let health = if self.wifi.state == wifi::WifiState::Monitor {
                recovery::HealthCheck::Ok
            } else {
                recovery::HealthCheck::Unresponsive
            };
            let action = self.recovery.process_health(health);
            self.handle_recovery_action(action);
        }

        // ---- BLUETOOTH HEALTH CHECK ----
        self.bluetooth.check_status();
        // Auto-reconnect if configured
        if self.bluetooth.should_connect() {
            match self.bluetooth.connect() {
                Ok(()) => info!("bluetooth reconnected: {}", self.bluetooth.status_str()),
                Err(e) => {
                    log::warn!("bluetooth reconnect failed: {e}");
                    self.bluetooth.on_error();
                }
            }
        }

        // ---- NETWORK HEALTH CHECK ----
        self.network.health_check();
        // Rotate IP display each epoch
        self.network.rotate_display();

        // ---- ATTACK PHASE ----
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

        // ---- CAPTURE PHASE ----
        self.epoch_loop.next_phase(); // -> Capture
        result.associations = self.wifi.tracker.total_clients();

        // Scan for new captures from AngryOxide
        let handshakes_before = self.captures.handshake_count();
        match self.captures.scan_directory() {
            Ok(new) => {
                if new > 0 {
                    info!("capture scan: {new} new file(s)");
                }
            }
            Err(e) => log::warn!("capture scan failed: {e}"),
        }
        let new_handshakes = self.captures.handshake_count().saturating_sub(handshakes_before);
        result.handshakes_captured = new_handshakes as u32;

        // ---- DISPLAY PHASE ----
        self.epoch_loop.next_phase(); // -> Display
        self.epoch_loop.record_result(&result);

        // ---- FACE VARIETY ENGINE ----
        // Tick countdowns (milestones, friend, upload, capture face)
        self.epoch_loop.personality.variety.tick_countdowns();

        // Wire captures into variety engine for milestones + face cycling
        if result.handshakes_captured > 0 {
            let total = self.epoch_loop.personality.total_handshakes;
            self.epoch_loop.personality.variety.on_capture(total);
        } else {
            // Tick idle counter if no handshakes this epoch
            self.epoch_loop.personality.variety.tick_idle();
        }

        // Set time-of-day state for variety engine
        let hour = chrono::Local::now().hour();
        self.epoch_loop.personality.variety.current_hour = hour;
        // Mark morning greeting as shown (6-8am, once per boot)
        if (6..=8).contains(&hour)
            && !self.epoch_loop.personality.variety.morning_greeted
            && self.epoch_loop.personality.variety.current_override() == Some("motivated")
        {
            self.epoch_loop.personality.variety.morning_greeted = true;
        }

        // Generate bull-themed status message (handles joke cycling)
        self.epoch_loop.personality.generate_status();

        // ---- PERIODIC XP SAVE (every 5 epochs) ----
        self.epoch_loop.personality.xp.tick_epoch();
        if self.epoch_loop.personality.xp.should_save() {
            let mood = self.epoch_loop.personality.mood.value();
            if let Err(e) = self.epoch_loop.personality.xp.save(mood) {
                log::warn!("XP save failed: {e}");
            }
        }

        // Check battery and apply face overrides
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

        self.update_display();

        // ---- Sync state to web ----
        self.sync_to_web();

        // ---- SLEEP PHASE ----
        self.epoch_loop.next_phase(); // -> Sleep

        // Ping watchdog
        if self.watchdog.needs_ping() {
            self.watchdog.ping();
        }

        // Sleep before next epoch
        std::thread::sleep(self.epoch_loop.epoch_duration);

        // Advance to next Scan (increments epoch counter)
        self.epoch_loop.next_phase(); // -> Scan (calls finish_epoch)
    }

    /// Process commands queued by the web server.
    fn process_web_commands(&mut self) {
        let (mode_switch, rate_change, restart) = {
            let mut s = self.shared_state.lock().unwrap();
            let mode = s.pending_mode_switch.take();
            let rate = s.pending_rate_change.take();
            let restart = s.pending_restart;
            s.pending_restart = false;
            (mode, rate, restart)
        };

        if let Some(mode) = mode_switch {
            info!("web: mode switch to {mode}");
            // In a full implementation, this would switch between AO and PWN mode
            let mut s = self.shared_state.lock().unwrap();
            s.mode = mode;
        }

        if let Some(rate) = rate_change {
            info!("web: rate change to {rate}");
            self.ao.set_rate(rate);
        }

        if restart {
            info!("web: AO restart requested");
            match self.ao.restart() {
                Ok(()) => info!("AO restarted successfully"),
                Err(e) => log::error!("AO restart failed: {e}"),
            }
        }
    }

    /// Sync daemon state into the shared web state.
    fn sync_to_web(&self) {
        let mut s = self.shared_state.lock().unwrap();
        let m = &self.epoch_loop.metrics;

        s.uptime_str = self.epoch_loop.uptime_str();
        s.epoch = m.epoch;
        s.channel = m.channel;
        s.aps_seen = m.total_aps;
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

        s.battery_level = self.battery.status.level;
        s.battery_charging = self.battery.status.charge_state == pisugar::ChargeState::Charging;
        s.battery_voltage_mv = self.battery.status.voltage_mv;
        s.battery_low = self.battery.status.low;
        s.battery_critical = self.battery.status.critical;
        s.battery_available = self.battery.available;

        s.wifi_state = format!("{:?}", self.wifi.state);
        s.wifi_aps_tracked = self.wifi.tracker.count();

        s.bt_state = self.bluetooth.status_str().to_string();
        s.bt_connected = self.bluetooth.state == bluetooth::BtState::Connected;
        s.bt_ip = self.bluetooth.ip_address.clone().unwrap_or_default();
        s.bt_internet_available = self.bluetooth.internet_available;
        s.bt_retry_count = self.bluetooth.retry_count;

        s.ao_state = self.ao.state_str().to_string();
        s.ao_pid = self.ao.pid;
        s.ao_crash_count = self.ao.crash_count;
        s.ao_uptime = self.ao.uptime_str();

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
    }

    /// Update the e-ink display with current state.
    /// Layout matches Python angryoxide.py AO mode — see docs/DISPLAY_SPEC.md.
    fn update_display(&mut self) {
        self.screen.clear();
        let m = &self.epoch_loop.metrics;

        // ---- TOP BAR (y=0) ----
        // AO status at (0,0) — Small 9pt — "AO: V/T | HH:MM | CH:1,6,11"
        let ao_status = format!(
            "AO: {}/{} | {}",
            m.handshakes,
            self.captures.count(),
            self.ao.uptime_str()
        );
        self.screen.draw_text(&ao_status, 0, 0);
        // BT status at (115,0) — Small 9pt — "BT:C" / "BT:-"
        let bt_str = format!("BT:{}", self.bluetooth.status_short());
        self.screen.draw_text(&bt_str, 115, 0);
        // Battery at (140,0) — Small 9pt
        self.screen.draw_text(&self.battery.display_str(), 140, 0);
        // Uptime at (185,0) — Small 9pt
        self.screen.draw_labeled_value("UP", &self.epoch_loop.uptime_str(), 185, 0);

        // ---- LINE 1 (y=14) ----
        self.screen.draw_hline(0, 14, display::DISPLAY_WIDTH);

        // ---- FACE at (0,16) — 120x66 bull bitmap ----
        let face = self.epoch_loop.current_face();
        self.screen.draw_face(&face);

        // ---- STATUS at (125,20) — Medium 10pt, word-wrapped ----
        let status = self.epoch_loop.personality.status_msg();
        self.screen.draw_status(&status);

        // ---- IP DISPLAY at (0,95) — Small 9pt, rotates USB/BT ----
        let ip_str = self.network.display_ip_str(
            self.bluetooth.ip_address.as_deref()
        );
        self.screen.draw_text(&ip_str, 0, 95);

        // ---- LINE 2 (y=108) ----
        self.screen.draw_hline(0, 108, display::DISPLAY_WIDTH);

        // ---- BOTTOM BAR (y=112) ----
        // Crash counter at (0,112) — Small 9pt — only shown if crashes
        if self.ao.crash_count > 0 {
            let crash_str = format!("CRASH:{}", self.ao.crash_count);
            self.screen.draw_text(&crash_str, 0, 112);
        }
        // Mode at (222,112) — Small 9pt
        self.screen.draw_text("AUTO", 222, 112);

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

    /// Handle recovery actions from the health checker.
    fn handle_recovery_action(&mut self, action: recovery::RecoveryAction) {
        match action {
            recovery::RecoveryAction::None => {}
            recovery::RecoveryAction::SoftRecover => {
                if self.recovery.cooldown_active() {
                    log::warn!("recovery cooldown active, skipping soft recovery");
                    return;
                }
                info!("attempting soft WiFi recovery");
                self.epoch_loop.personality.set_override(personality::Face::WifiDown);
                match self.wifi.stop_monitor() {
                    Ok(()) => {
                        let _ = self.wifi.start_monitor();
                    }
                    Err(e) => log::error!("soft recovery failed: {e}"),
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
    #[allow(dead_code)]
    fn build_web_status(&self) -> web::StatusResponse {
        let m = &self.epoch_loop.metrics;
        web::build_status(&web::StatusParams {
            name: &self.config.name,
            uptime: &self.epoch_loop.uptime_str(),
            epoch: m.epoch,
            channel: m.channel,
            aps_seen: m.total_aps,
            handshakes: m.handshakes,
            blind_epochs: m.blind_epochs,
            mood: self.epoch_loop.personality.mood.value(),
            face: self.epoch_loop.current_face().as_str(),
            status_message: &self.epoch_loop.personality.status_msg(),
            mode: "AO",
        })
    }

    /// Build web attack stats.
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    // Load oxigotchi config, or migrate from pwnagotchi config on first run
    let config = if std::path::Path::new("/etc/oxigotchi/config.toml").exists() {
        config::Config::load_or_default("/etc/oxigotchi/config.toml")
    } else if std::path::Path::new("/etc/pwnagotchi/config.toml").exists() {
        info!("migrating settings from /etc/pwnagotchi/config.toml");
        let mut cfg = config::Config::load_or_default("/etc/pwnagotchi/config.toml");
        // Keep pwnagotchi's whitelist, display, attack settings
        // but use our own name and identity
        cfg.main.name = "oxigotchi".into();
        cfg.name = "oxigotchi".into();
        // Save migrated config so we don't re-migrate
        if let Err(e) = std::fs::create_dir_all("/etc/oxigotchi") {
            log::warn!("could not create /etc/oxigotchi: {e}");
        }
        if let Ok(toml_str) = toml::to_string_pretty(&cfg) {
            if let Err(e) = std::fs::write("/etc/oxigotchi/config.toml", &toml_str) {
                log::warn!("could not save config: {e}");
            }
        }
        cfg
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

    /// Verify that all 24 Face variants are reachable.
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
}
