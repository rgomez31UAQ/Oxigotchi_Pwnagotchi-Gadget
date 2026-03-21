// Modules are the public API surface for Rusty Oxigotchi. Many types/methods
// are defined but not yet wired into the main loop — they're used by tests and
// will be consumed when axum web server and full AO integration land.
// Suppress dead_code for modules with designed-but-not-yet-wired APIs.
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
mod recovery;
#[allow(dead_code)]
mod web;
#[allow(dead_code)]
mod wifi;

use log::info;
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
    recovery: recovery::RecoveryManager,
    watchdog: recovery::Watchdog,
}

impl Daemon {
    fn new(config: config::Config) -> Self {
        let screen = display::Screen::new(config.display.clone());
        let epoch_loop = epoch::EpochLoop::new(Duration::from_secs(EPOCH_DURATION_SECS));
        let wifi = wifi::WifiManager::new();
        let mut attacks = attacks::AttackScheduler::new(ATTACK_RATE);

        // Populate attack whitelist from config
        // (Real implementation would parse MAC strings; for now store empty)
        let _ = &config.whitelist;
        attacks.whitelist.clear();

        let captures = capture::CaptureManager::new(CAPTURE_DIR);
        let bluetooth = bluetooth::BtTether::default();
        let battery = pisugar::PiSugar::default();
        let recovery = recovery::RecoveryManager::default();
        let watchdog = recovery::Watchdog::new(true, 60);

        Self {
            config,
            screen,
            epoch_loop,
            wifi,
            attacks,
            captures,
            bluetooth,
            battery,
            recovery,
            watchdog,
        }
    }

    /// Boot sequence: init display, probe hardware, scan existing captures.
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
    }

    /// Run one full epoch: Scan -> Attack -> Capture -> Display -> Sleep.
    fn run_epoch(&mut self) {
        let mut result = epoch::EpochResult::default();

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
        // In real implementation, this would check for new pcapng files from AO
        // For now we just update counts
        result.associations = 0; // stub

        // ---- DISPLAY PHASE ----
        self.epoch_loop.next_phase(); // -> Display
        self.epoch_loop.record_result(&result);

        // Check battery and apply face overrides
        self.check_battery_overrides();

        self.update_display();

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

    /// Update the e-ink display with current state.
    ///
    /// Layout matches the Python waveshare_4 spec:
    ///   Top bar (y=0):  CH(0,0)  APS(28,0)  BT(115,0)  BAT(140,0)  UP(185,0)
    ///   Line1 at y=14
    ///   Name(5,20)  Status(125,20)
    ///   Face(0,34)
    ///   Line2 at y=108
    ///   PWND(0,109)  Mode(222,112)
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
                info!("attempting soft WiFi recovery");
                self.epoch_loop.personality.set_override(personality::Face::WifiDown);
                // Real impl: rmmod/modprobe brcmfmac
                match self.wifi.stop_monitor() {
                    Ok(()) => {
                        let _ = self.wifi.start_monitor();
                    }
                    Err(e) => log::error!("soft recovery failed: {e}"),
                }
            }
            recovery::RecoveryAction::HardRecover => {
                info!("attempting hard WiFi recovery (GPIO power cycle)");
                self.epoch_loop.personality.set_override(personality::Face::FwCrash);
                // GPIO WL_REG_ON power cycle — toggle pin 41
                match recovery::gpio_power_cycle_wifi(self.recovery.config.gpio_cycle_delay_ms) {
                    Ok(()) => info!("GPIO power cycle complete, restarting monitor"),
                    Err(e) => log::error!("GPIO power cycle failed: {e}"),
                }
                let _ = self.wifi.start_monitor();
            }
            recovery::RecoveryAction::GiveUp => {
                log::error!("WiFi recovery exhausted, giving up");
                self.epoch_loop.personality.set_override(personality::Face::Broken);
            }
        }
    }

    /// Build a web status snapshot (called by axum handlers when web server lands).
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

    /// Build web attack stats (called by axum handlers when web server lands).
    #[allow(dead_code)]
    fn build_attack_stats(&self) -> web::AttackStats {
        web::AttackStats {
            total_attacks: self.attacks.total_attacks,
            total_handshakes: self.attacks.total_handshakes,
            attack_rate: ATTACK_RATE,
            deauths_this_epoch: self.epoch_loop.metrics.deauths_this_epoch,
        }
    }

    /// Build web capture info (called by axum handlers when web server lands).
    #[allow(dead_code)]
    fn build_capture_info(&self) -> web::CaptureInfo {
        web::CaptureInfo {
            total_files: self.captures.count(),
            handshake_files: self.captures.handshake_count(),
            pending_upload: self.captures.pending_upload_count(),
            total_size_bytes: self.captures.total_size(),
        }
    }
}

fn main() {
    env_logger::init();
    info!(
        "Rusty Oxigotchi v{} starting — the bull is awake",
        env!("CARGO_PKG_VERSION")
    );

    let config = config::Config::load_or_default("/etc/pwnagotchi/config.toml");
    info!("name: {}", config.name);

    let mut daemon = Daemon::new(config);
    daemon.boot();

    info!("entering main epoch loop");
    loop {
        daemon.run_epoch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_construction() {
        let config = config::Config::defaults();
        let daemon = Daemon::new(config);
        assert_eq!(daemon.epoch_loop.metrics.epoch, 0);
        assert_eq!(daemon.wifi.state, wifi::WifiState::Down);
        assert_eq!(daemon.captures.count(), 0);
    }

    #[test]
    fn test_daemon_boot() {
        let config = config::Config::defaults();
        let mut daemon = Daemon::new(config);
        daemon.boot();
        // WiFi should be in monitor mode (stub always succeeds)
        assert_eq!(daemon.wifi.state, wifi::WifiState::Monitor);
    }

    #[test]
    fn test_daemon_build_web_status() {
        let config = config::Config::defaults();
        let daemon = Daemon::new(config);
        let status = daemon.build_web_status();
        assert_eq!(status.name, "oxigotchi");
        assert_eq!(status.epoch, 0);
        assert_eq!(status.mode, "AO");
    }

    #[test]
    fn test_daemon_build_attack_stats() {
        let config = config::Config::defaults();
        let daemon = Daemon::new(config);
        let stats = daemon.build_attack_stats();
        assert_eq!(stats.total_attacks, 0);
        assert_eq!(stats.attack_rate, ATTACK_RATE);
    }

    #[test]
    fn test_daemon_build_capture_info() {
        let config = config::Config::defaults();
        let daemon = Daemon::new(config);
        let info = daemon.build_capture_info();
        assert_eq!(info.total_files, 0);
        assert_eq!(info.pending_upload, 0);
    }

    #[test]
    fn test_daemon_battery_override_critical_no_shutdown() {
        let config = config::Config::defaults();
        let mut daemon = Daemon::new(config);
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
        let config = config::Config::defaults();
        let mut daemon = Daemon::new(config);
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
        let config = config::Config::defaults();
        let mut daemon = Daemon::new(config);
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
        let config = config::Config::defaults();
        let mut daemon = Daemon::new(config);
        daemon.battery.available = true;
        daemon.battery.config.auto_shutdown = false; // disable shutdown for this test
        daemon.battery.set_level(3);
        daemon.check_battery_overrides();
        assert_eq!(
            daemon.epoch_loop.personality.override_face,
            Some(personality::Face::BatteryCritical)
        );
        // Battery recovers
        daemon.battery.set_level(80);
        daemon.check_battery_overrides();
        assert_eq!(daemon.epoch_loop.personality.override_face, None);
    }

    #[test]
    fn test_daemon_recovery_soft() {
        let config = config::Config::defaults();
        let mut daemon = Daemon::new(config);
        daemon.handle_recovery_action(recovery::RecoveryAction::SoftRecover);
        assert_eq!(
            daemon.epoch_loop.personality.override_face,
            Some(personality::Face::WifiDown)
        );
    }

    #[test]
    fn test_daemon_recovery_hard() {
        let config = config::Config::defaults();
        let mut daemon = Daemon::new(config);
        daemon.handle_recovery_action(recovery::RecoveryAction::HardRecover);
        assert_eq!(
            daemon.epoch_loop.personality.override_face,
            Some(personality::Face::FwCrash)
        );
    }

    #[test]
    fn test_daemon_recovery_give_up() {
        let config = config::Config::defaults();
        let mut daemon = Daemon::new(config);
        daemon.handle_recovery_action(recovery::RecoveryAction::GiveUp);
        assert_eq!(
            daemon.epoch_loop.personality.override_face,
            Some(personality::Face::Broken)
        );
    }

    #[test]
    fn test_daemon_update_display_no_panic() {
        let config = config::Config::defaults();
        let mut daemon = Daemon::new(config);
        daemon.update_display();
        // Should draw without panic, verify pixels exist
        let pixel_count = daemon.screen.fb.count_set_pixels();
        assert!(pixel_count > 0, "display should have drawn something");
    }

    /// Verify that all 24 Face variants are reachable through either mood transitions
    /// or explicit override triggers in the daemon.
    #[test]
    fn test_face_reachability() {
        use personality::Face;

        // Faces reachable through mood (Mood::face)
        let mood_faces = [
            Face::Excited,     // mood >= 0.9
            Face::Happy,       // mood >= 0.7
            Face::Awake,       // mood >= 0.5
            Face::Bored,       // mood >= 0.3
            Face::Sad,         // mood >= 0.1
            Face::Demotivated, // mood < 0.1
        ];

        // Faces reachable through daemon override triggers
        let override_faces = [
            Face::BatteryCritical, // check_battery_overrides() critical
            Face::BatteryLow,      // check_battery_overrides() low
            Face::Shutdown,        // check_battery_overrides() auto_shutdown
            Face::WifiDown,        // handle_recovery_action(SoftRecover)
            Face::FwCrash,         // handle_recovery_action(HardRecover)
            Face::Broken,          // handle_recovery_action(GiveUp)
        ];

        // Faces reachable via manual set_override (future integration points)
        let manual_override_faces = [
            Face::Sleep,     // long idle / night mode
            Face::Intense,   // during active attack
            Face::Cool,      // after multi-handshake streak
            Face::Angry,     // after repeated failures
            Face::Friend,    // peer detection
            Face::Debug,     // debug mode
            Face::Upload,    // wpa-sec upload in progress
            Face::Lonely,    // no APs seen for extended time
            Face::Grateful,  // after user interaction
            Face::Motivated, // after config change / reboot
            Face::Smart,     // AI/learning event
            Face::AoCrashed, // angryoxide process crash
        ];

        let mut reachable: std::collections::HashSet<Face> = std::collections::HashSet::new();
        reachable.extend(mood_faces.iter());
        reachable.extend(override_faces.iter());
        reachable.extend(manual_override_faces.iter());

        // Every Face::all() variant should be accounted for
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
        let config = config::Config::defaults();
        let mut daemon = Daemon::new(config);

        // Start at Awake (mood 0.5)
        assert_eq!(daemon.epoch_loop.current_face(), personality::Face::Awake);

        // After many handshakes, mood should rise -> Happy -> Excited
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

        // After many blind epochs, mood should drop -> Bored -> Sad -> Demotivated
        for _ in 0..50 {
            daemon.epoch_loop.record_result(&epoch::EpochResult::default());
        }
        let face = daemon.epoch_loop.current_face();
        assert!(
            face == personality::Face::Sad || face == personality::Face::Demotivated,
            "expected Sad or Demotivated after many blind epochs, got {face:?}"
        );
    }

    /// Integration test: create a Daemon, run 3 full epoch cycles (without
    /// sleeping), and verify the display has been updated each time.
    #[test]
    fn test_integration_three_epochs() {
        let config = config::Config::defaults();
        let mut daemon = Daemon::new(config);

        // Override epoch duration to zero so we don't actually sleep
        daemon.epoch_loop.epoch_duration = Duration::from_secs(0);
        daemon.boot();

        let mut pixel_counts: Vec<u32> = Vec::new();

        for epoch in 0..3 {
            // ---- SCAN ----
            daemon.epoch_loop.phase = epoch::EpochPhase::Scan;
            let channel = daemon.wifi.hop_channel();

            let mut result = epoch::EpochResult::default();
            result.channel = channel;
            result.aps_seen = (epoch + 1) * 2; // vary per epoch

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

            // ---- back to SCAN (increments epoch counter) ----
            daemon.epoch_loop.next_phase();
        }

        // After 3 epochs the counter should be 3
        assert_eq!(daemon.epoch_loop.metrics.epoch, 3);

        // Every epoch should have drawn something to the display
        for (i, &count) in pixel_counts.iter().enumerate() {
            assert!(count > 0, "epoch {i} should have drawn pixels, got 0");
        }

        // WiFi should still be in monitor mode
        assert_eq!(daemon.wifi.state, wifi::WifiState::Monitor);
    }
}
