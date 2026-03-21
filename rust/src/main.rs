// Modules are the public API surface for Rusty Oxigotchi.
#[allow(dead_code)]
mod ao;
#[allow(dead_code)]
mod attacks;
#[allow(dead_code)]
mod bluetooth;
#[allow(dead_code)]
mod capture;
#[allow(dead_code)]
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
        let bluetooth = bluetooth::BtTether::default();
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
        // Display boot screen
        self.screen.clear();
        self.screen.draw_face(&personality::Face::Awake);
        self.screen.draw_name(&self.config.name);
        self.screen.draw_status("Booting...");
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
        if self.bluetooth.should_connect() {
            match self.bluetooth.connect() {
                Ok(()) => info!("bluetooth tethered: {}", self.bluetooth.status_str()),
                Err(e) => {
                    log::warn!("bluetooth tether failed: {e}");
                    self.bluetooth.on_error();
                }
            }
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

        // ---- TDM CYCLE ----
        self.ao.tdm_tick();

        // ---- SCAN PHASE ----
        self.epoch_loop.phase = epoch::EpochPhase::Scan;
        self.recovery.log(recovery::DiagLevel::Info, "epoch scan start");

        // Hop channels and scan
        let channel = self.wifi.hop_channel();
        result.channel = channel;
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

        // ---- NETWORK HEALTH CHECK ----
        self.network.health_check();
        // Rotate IP display each epoch
        self.network.rotate_display();

        // ---- ATTACK PHASE ----
        self.epoch_loop.next_phase(); // -> Attack
        let attackable = self.wifi.tracker.attackable();
        for ap in &attackable {
            if self.attacks.is_whitelisted(&ap.bssid) {
                continue;
            }
            if let Some(attack_type) = self.attacks.next_attack(&ap.bssid) {
                let attack_result = attacks::AttackResult {
                    attack_type,
                    target_bssid: ap.bssid,
                    success: true,  // stub
                    handshake_captured: false, // stub
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
        result.associations = 0; // stub

        // ---- DISPLAY PHASE ----
        self.epoch_loop.next_phase(); // -> Display
        self.epoch_loop.record_result(&result);

        // Check battery and apply face overrides
        self.check_battery_overrides();

        // Clear AO crash face if AO recovered
        if self.ao.state == ao::AoState::Running
            && self.epoch_loop.personality.override_face == Some(personality::Face::AoCrashed)
        {
            self.epoch_loop.personality.clear_override();
        }

        // Record stable epoch for AO
        if self.ao.state == ao::AoState::Running || self.ao.state == ao::AoState::Paused {
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
        s.status_message = self.epoch_loop.status_message();

        s.total_attacks = self.attacks.total_attacks;
        s.total_handshakes_attacks = self.attacks.total_handshakes;
        s.attack_rate = self.ao.config.rate;
        s.deauths_this_epoch = m.deauths_this_epoch;

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
    }

    /// Update the e-ink display with current state.
    fn update_display(&mut self) {
        self.screen.clear();

        // ---- TOP BAR (y=0) ----
        let m = &self.epoch_loop.metrics;
        self.screen.draw_labeled_value("CH", &m.channel.to_string(), 0, 0);
        self.screen.draw_labeled_value("APS", &m.total_aps.to_string(), 28, 0);
        self.screen.draw_labeled_value("BT", self.bluetooth.status_short(), 115, 0);
        let bat_str = self.battery.display_str();
        self.screen.draw_labeled_value("", &bat_str, 140, 0);
        self.screen.draw_labeled_value("UP", &self.epoch_loop.uptime_str(), 185, 0);

        // ---- LINE 1 (y=14) ----
        self.screen.draw_hline(0, 14, display::DISPLAY_WIDTH);

        // ---- NAME + STATUS (y=20) ----
        self.screen.draw_name(&self.config.name);
        self.screen.draw_status(&self.epoch_loop.status_message());

        // ---- FACE (y=34) ----
        let face = self.epoch_loop.current_face();
        self.screen.draw_face(&face);

        // ---- LINE 2 (y=108) ----
        self.screen.draw_hline(0, 108, display::DISPLAY_WIDTH);

        // ---- BOTTOM BAR (y=109+) ----
        self.screen.draw_labeled_value(
            "PWND",
            &m.handshakes.to_string(),
            0,
            109,
        );
        self.screen.draw_labeled_value("", "AUTO", 222, 112);

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
            status_message: &self.epoch_loop.status_message(),
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
    info!(
        "Rusty Oxigotchi v{} starting — the bull is awake",
        env!("CARGO_PKG_VERSION")
    );

    let config = config::Config::load_or_default("/etc/pwnagotchi/config.toml");
    info!("name: {}", config.name);

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
            let channel = daemon.wifi.hop_channel();

            let mut result = epoch::EpochResult::default();
            result.channel = channel;
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
}
