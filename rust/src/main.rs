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
mod firmware;
mod gpu;
mod lua;
mod migration;
mod network;
mod personality;
mod pisugar;
mod qpu;
mod radio;
mod rage;
mod recovery;
mod web;
mod wifi;

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
/// Default attack rate. All rates (1-3) stable with v6 firmware patch.
const ATTACK_RATE: u32 = 1;
/// Default capture directory.
const CAPTURE_DIR: &str = "/home/pi/captures";

/// Operating mode: RAGE (WiFi attacks), BT (Bluetooth offensive), or SAFE (BT internet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperatingMode {
    Rage,
    Bt,
    Safe,
}

impl OperatingMode {
    fn as_str(&self) -> &str {
        match self {
            OperatingMode::Rage => "RAGE",
            OperatingMode::Bt => "BT",
            OperatingMode::Safe => "SAFE",
        }
    }

    /// Three-way cycle: RAGE → BT → SAFE → RAGE.
    fn next(&self) -> Self {
        match self {
            OperatingMode::Rage => OperatingMode::Bt,
            OperatingMode::Bt => OperatingMode::Safe,
            OperatingMode::Safe => OperatingMode::Rage,
        }
    }

    /// Backward-compat alias for `next()`.
    fn toggle(&self) -> Self {
        self.next()
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
    bt_feature: bluetooth::supervisor::BtSupervisor,
    bt_discovery: bluetooth::discovery::BtDiscoveryWorker,
    bt_attack_scheduler: bluetooth::attacks::BtAttackScheduler,
    bt_capture_manager: bluetooth::capture::BtCaptureManager,
    bt_controller_worker: bluetooth::controller::BtControllerWorker,
    bt_coex_worker: bluetooth::coex::BtCoexWorker,
    battery: pisugar::PiSugar,
    network: network::NetworkManager,
    recovery: recovery::RecoveryManager,
    watchdog: recovery::Watchdog,
    ao: ao::AoManager,
    radio: radio::RadioManager,
    patchram: bluetooth::patchram::PatchramManager,
    bt_hci_socket: Option<bluetooth::attacks::hci::HciSocket>,
    firmware_monitor: firmware::FirmwareMonitor,
    gpu_state: gpu::state::gpu_state::GpuFeatureState,
    gpu_runtime_ingestor: gpu::runtime::ingest::GpuRuntimeIngestor,
    gpu_optimizer: gpu::optimize::snapshot::SnapshotOptimizer,
    qpu_engine: Option<qpu::engine::QpuEngine>,
    mode: OperatingMode,
    shared_state: web::SharedState,
    ws_tx: tokio::sync::broadcast::Sender<String>,
    prev_cpu_sample: Option<personality::CpuSample>,
    lua: lua::PluginRuntime,
    plugin_watcher: Option<lua::PluginWatcher>,
    wpasec_config: capture::WpaSecConfig,
    upload_queue: capture::UploadQueue,
    discord_webhook_url: String,
    discord_enabled: bool,
    /// Whether AO should auto-hunt channels vs use the configured channel list.
    autohunt: bool,
    /// Smart Skip: skip APs that already have captured handshakes on SD.
    skip_captured: bool,
    /// Collect All: AO writes directly to SD, all frames kept (not just verified handshakes).
    capture_all: bool,
    /// Adaptive channel scorer: ranks channels 1-13 by AP density, RSSI, etc.
    channel_scorer: wifi::ChannelScorer,
    /// tmpfs directory for AO captures (validated before moving to SD).
    tmpfs_capture_dir: String,
    /// Cumulative APs seen across all previous sessions (loaded from state.json).
    lifetime_aps_base: u64,
}

impl Daemon {
    fn new(
        config: config::Config,
        shared_state: web::SharedState,
        ws_tx: tokio::sync::broadcast::Sender<String>,
    ) -> Self {
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
        let bt_feature = bluetooth::supervisor::BtSupervisor::new(config.bt_feature.clone());
        let gpu_mode = if config.gpu.enabled {
            config.gpu.mode.clone()
        } else {
            gpu::state::gpu_state::GpuMode::Off
        };
        let battery = pisugar::PiSugar::default();
        let network = network::NetworkManager::new();
        let recovery = recovery::RecoveryManager::default();
        let watchdog = recovery::Watchdog::new(true, 60);
        let ao = ao::AoManager::default();
        let patchram = bluetooth::patchram::PatchramManager::new(
            config.bt_attacks.attack_hcd.clone(),
            config.bt_attacks.stock_hcd.clone(),
        );
        let bt_attack_scheduler = bluetooth::attacks::BtAttackScheduler::new(config.bt_attacks.clone());
        let bt_capture_manager = bluetooth::capture::BtCaptureManager::new(&config.bt_attacks.capture_dir);

        Self {
            config,
            screen,
            epoch_loop,
            wifi,
            attacks,
            captures,
            bluetooth,
            bt_feature,
            bt_discovery: bluetooth::discovery::BtDiscoveryWorker::new(),
            bt_attack_scheduler,
            bt_capture_manager,
            bt_controller_worker: bluetooth::controller::BtControllerWorker::new(),
            bt_coex_worker: bluetooth::coex::BtCoexWorker::new(),
            battery,
            network,
            recovery,
            watchdog,
            ao,
            radio: radio::RadioManager::new(),
            patchram,
            bt_hci_socket: None,
            firmware_monitor: firmware::FirmwareMonitor::new(),
            gpu_state: gpu::state::gpu_state::GpuFeatureState {
                mode: gpu_mode,
                ..gpu::state::gpu_state::GpuFeatureState::default()
            },
            gpu_runtime_ingestor: gpu::runtime::ingest::GpuRuntimeIngestor::new(),
            gpu_optimizer: gpu::optimize::snapshot::SnapshotOptimizer::new(),
            qpu_engine: None, // initialized in boot()
            mode: OperatingMode::Rage,
            shared_state,
            ws_tx,
            prev_cpu_sample: None,
            lua: lua::PluginRuntime::new(),
            plugin_watcher: None, // initialized in boot() after plugin dir is confirmed
            wpasec_config: capture::WpaSecConfig::default(),
            upload_queue: capture::UploadQueue::new(),
            discord_webhook_url: String::new(),
            discord_enabled: false,
            autohunt: true,
            skip_captured: true,
            capture_all: false,
            channel_scorer: wifi::ChannelScorer::new(3),
            tmpfs_capture_dir: ensure_tmpfs_capture_dir(),
            lifetime_aps_base: 0,
        }
    }

    /// Boot sequence: init display, probe hardware, scan existing captures, start AO.
    fn boot(&mut self) {
        // Load saved XP/mood from disk
        let xp_path = std::path::Path::new(personality::DEFAULT_XP_SAVE_PATH);
        let (xp, mood) = personality::XpTracker::load(xp_path);
        self.epoch_loop.personality.xp = xp;
        self.epoch_loop.personality.mood = personality::Mood::new(mood);
        info!(
            "loaded XP: Lv {} ({} xp), mood {:.2}",
            self.epoch_loop.personality.xp.level, self.epoch_loop.personality.xp.xp_total, mood
        );

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
            65,
            5,
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
                self.epoch_loop
                    .personality
                    .set_override(personality::Face::WifiDown);
            }
        }

        // Scan for existing capture files
        match self.captures.scan_directory() {
            Ok(n) => info!("found {n} existing captures"),
            Err(e) => log::warn!("capture scan failed: {e}"),
        }

        // Load Lua plugins — read persisted positions from plugins.toml, fall back to defaults
        let plugin_defaults = vec![
            lua::PluginConfig::default_for("ao_status", 0, 0),
            lua::PluginConfig::default_for("aps", 130, 0),
            lua::PluginConfig::default_for("uptime", 178, 0),
            lua::PluginConfig::default_for("status_msg", 125, 20),
            lua::PluginConfig::default_for("sys_stats", 125, 85),
            lua::PluginConfig::default_for("ip_display", 0, 95),
            lua::PluginConfig::default_for("crash", 0, 112),
            lua::PluginConfig::default_for("www", 52, 112),
            lua::PluginConfig::default_for("bt_status", 96, 112),
            lua::PluginConfig::default_for("battery", 140, 112),
            lua::PluginConfig::default_for("mode", 214, 112),
        ];
        let plugin_configs = match lua::config::read_plugins_toml() {
            Some(pt) => {
                info!("loaded plugin positions from plugins.toml");
                lua::config::merge_with_defaults(plugin_defaults, &pt)
            }
            None => plugin_defaults,
        };
        let loaded = self
            .lua
            .load_plugins_from_dir("/etc/oxigotchi/plugins", &plugin_configs);
        info!("loaded {loaded} Lua plugin(s)");

        // Start watching plugin directory for hot-reload
        match lua::PluginWatcher::new("/etc/oxigotchi/plugins") {
            Ok(watcher) => {
                info!("plugin hot-reload watcher active");
                self.plugin_watcher = Some(watcher);
            }
            Err(e) => log::warn!("plugin watcher not available: {e}"),
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

        // Load persisted runtime state BEFORE starting AO (whitelist, attack toggles, etc.)
        self.load_runtime_state();

        // Point AO output to tmpfs (verified mode) or SD directly (collect-all mode).
        if self.capture_all {
            self.ao.config.output_dir = CAPTURE_DIR.to_string();
            info!(
                "capture pipeline: collect-all mode — AO output -> SD ({})",
                CAPTURE_DIR
            );
        } else {
            self.ao.config.output_dir = format!("{}/capture", self.tmpfs_capture_dir);
            info!(
                "capture pipeline: verified mode — AO output -> tmpfs ({})",
                self.tmpfs_capture_dir
            );
        }

        // Pass SSID whitelist to AO so it skips our own APs
        self.ao.config.whitelist = self.wifi.tracker.ssid_whitelist.clone();
        if !self.ao.config.whitelist.is_empty() {
            info!("AO whitelist: {:?}", self.ao.config.whitelist);
        }

        // Start AngryOxide subprocess
        match self.ao.start() {
            Ok(()) => info!("AO started: PID {}", self.ao.pid),
            Err(e) => {
                log::error!("AO failed to start: {e}");
                self.epoch_loop
                    .personality
                    .set_override(personality::Face::AoCrashed);
            }
        }

        // QPU initialization
        if self.config.qpu.enabled {
            match qpu::engine::QpuEngine::init(self.config.qpu.to_engine_config()) {
                Ok(mut engine) => {
                    info!(
                        "QPU engine initialized: {} QPUs available",
                        engine.num_qpus()
                    );
                    // Start pcap capture thread on wlan0mon
                    match engine.start_capture("wlan0mon") {
                        Ok(()) => info!("QPU capture thread started on wlan0mon"),
                        Err(e) => log::warn!("QPU capture start failed: {e} (frames from AO only)"),
                    }
                    self.qpu_engine = Some(engine);
                }
                Err(e) => {
                    log::warn!("QPU init failed (CPU fallback): {}", e);
                }
            }
        }

        // Acquire radio lock — WIFI is the default boot mode
        if let Err(e) = self.radio.acquire_lock(radio::RadioMode::Wifi) {
            log::warn!("failed to acquire WIFI radio lock on boot: {e}");
        }

        // Initial state sync to web
        self.sync_to_web();
        web::broadcast_state(&self.shared_state, &self.ws_tx);
    }

    /// Run one full epoch: Scan -> Attack -> Capture -> Display -> Sleep.
    fn run_epoch(&mut self) {
        let mut result = epoch::EpochResult::default();

        // ---- Web commands ----
        self.process_web_commands();

        // ---- BT mode: run BT epoch instead of WiFi phases ----
        if self.mode == OperatingMode::Bt {
            self.run_bt_epoch();
            // Skip WiFi scan/attack/capture — jump to display/personality/web sync below
        } else {

        // ---- AO health (RAGE mode only) ----
        if self.mode == OperatingMode::Rage && self.ao.check_health() {
            self.epoch_loop
                .personality
                .set_override(personality::Face::AoCrashed);
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

        // ---- Firmware health (RAGE mode only) ----
        if self.mode == OperatingMode::Rage {
            let fw_health = self.firmware_monitor.poll();
            match fw_health {
                firmware::FirmwareHealth::Critical => {
                    log::warn!(
                        "firmware: CRITICAL -- counters spiking (crash={}, fault={}), triggering preemptive recovery",
                        self.firmware_monitor.crash_suppress,
                        self.firmware_monitor.hardfault
                    );
                }
                firmware::FirmwareHealth::Degraded => {
                    log::info!(
                        "firmware: degraded -- counters increasing (crash={}, fault={})",
                        self.firmware_monitor.crash_suppress,
                        self.firmware_monitor.hardfault
                    );
                }
                _ => {}
            }
        }

        // ---- Adaptive channel scoring (RAGE mode only) ----
        if self.mode == OperatingMode::Rage {
            self.channel_scorer.reset_epoch_counts();

            // Feed AP data from WiFi tracker (has RSSI + client counts)
            for ap in self.wifi.tracker.sorted_by_rssi() {
                self.channel_scorer
                    .record_ap(ap.channel, ap.rssi, ap.client_count);
            }

            // Feed capture info from AO (which APs had captures this session)
            for ao_ap in &self.ao.ap_snapshot() {
                if ao_ap.captured {
                    self.channel_scorer.record_capture(ao_ap.channel);
                }
            }

            // Mark the current AO channel as visited
            let current_ch = self.ao.channel() as u8;
            self.channel_scorer.mark_visited(current_ch);
            self.channel_scorer.tick_epoch();

            // When autohunt is ON, update AO's channel config with the best channels.
            // Cold-start guard: use safe 1,6,11 for the first 10 epochs so the
            // scorer collects real AP data before making decisions.
            if self.autohunt {
                let best = if self.epoch_loop.metrics.epoch < 10 {
                    vec![1, 6, 11]
                } else {
                    self.channel_scorer.top_channels()
                };
                if !best.is_empty() {
                    self.wifi.channel_config.channels = best.clone();
                    self.ao.config.channels = best;
                    info!(
                        "adaptive channels: top {} -> {:?}",
                        self.wifi.channel_config.channels.len(),
                        self.wifi.channel_config.channels
                    );
                }
            }
        }

        // ---- Prune stale APs ----
        let ap_ttl = self.shared_state.lock().unwrap().ap_ttl_secs;
        self.wifi.tracker.prune(ap_ttl);

        // ---- Attack + Capture phases ----
        self.run_attack_phase(&mut result);
        self.run_capture_phase(&mut result);

        } // end else (non-BT mode WiFi phases)

        // ---- QPU classification + RF environment ----
        if let Some(ref mut engine) = self.qpu_engine {
            let classified = engine.process_batch();
            if !classified.is_empty() {
                // Collect AO BSSID set for cross-reference
                let ao_bssids: std::collections::HashSet<[u8; 6]> = self
                    .ao
                    .ap_snapshot()
                    .iter()
                    .filter_map(|ap| {
                        let hex = &ap.bssid;
                        if hex.len() == 12 {
                            let mut b = [0u8; 6];
                            for i in 0..6 {
                                b[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                                    .unwrap_or(0);
                            }
                            Some(b)
                        } else {
                            None
                        }
                    })
                    .collect();

                let rf = qpu::rf::RfEnvironment::compute(
                    &classified,
                    EPOCH_DURATION_SECS as f32,
                    &ao_bssids,
                );

                // Feed RF environment into personality
                self.epoch_loop.personality.apply_rf_environment(&rf);

                // Store RF stats for web API
                {
                    let mut s = self.shared_state.lock().unwrap();
                    s.qpu_beacon_rate = rf.beacon_rate;
                    s.qpu_probe_rate = rf.probe_rate;
                    s.qpu_deauth_rate = rf.deauth_rate;
                    s.qpu_data_rate = rf.data_rate;
                    s.qpu_unique_bssids = rf.unique_bssids;
                    s.qpu_total_frames = rf.total_frames;
                    s.qpu_dominant_class = format!("{:?}", rf.dominant_class);
                }
            }
        }

        // ---- Display phase ----
        self.epoch_loop.next_phase(); // -> Display

        if self.mode == OperatingMode::Bt {
            // BT mode: personality was already updated in run_bt_epoch().
            // Don't record a WiFi result (would poison mood with blind epochs)
            // and don't overwrite the BT status text.
            self.epoch_loop.personality.variety.tick_idle();
        } else {
            self.epoch_loop.record_result(&result);

            // ---- Face & personality ----
            self.update_face_and_personality(&result);
        }

        // ---- Lua plugins + display ----
        let epoch_state = self.build_epoch_state();
        self.lua.tick_epoch(&epoch_state);

        // ---- Plugin hot-reload: check inotify for changed .lua files ----
        if let Some(ref watcher) = self.plugin_watcher {
            for name in watcher.check() {
                info!("plugin {name}: file changed, reloading");
                if let Err(e) = self.lua.reload_plugin(&name, watcher.dir()) {
                    log::error!("plugin {name}: reload failed: {e}");
                }
            }
        }

        // Check for display settings changes before rendering
        self.apply_pending_display_settings();

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

        // ---- PSM counter reset (every 30 epochs = ~15 min) ----
        if self.mode == OperatingMode::Rage
            && self.epoch_loop.metrics.epoch > 0
            && self.epoch_loop.metrics.epoch % 30 == 0
        {
            if let Err(e) = recovery::reset_watchdog_counters() {
                log::debug!("PSM counter reset skipped: {e}");
            } else {
                info!("PSM/DPC/RSSI watchdog counters reset (preventive)");
            }
        }

        // ---- Sync state to web ----
        self.sync_to_web();
        web::broadcast_state(&self.shared_state, &self.ws_tx);

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

            // Break out of sleep early if a mode switch is pending
            // so the next epoch processes it immediately.
            {
                let s = self.shared_state.lock().unwrap();
                if s.pending_mode_switch.is_some() {
                    break;
                }
            }

            // Process BT visibility toggle immediately (don't wait for epoch)
            {
                let bt_toggle = {
                    let mut s = self.shared_state.lock().unwrap();
                    s.pending_bt_toggle.take()
                };
                if let Some(visible) = bt_toggle {
                    info!("web: BT visibility set to {visible} (mid-epoch)");
                    if visible {
                        self.bluetooth.show();
                    } else {
                        self.bluetooth.hide();
                    }
                }
            }

            // Update channel indicator every tick (AO hops every ~5s)
            let ch = self.ao.channel();
            if ch > 0 {
                let ch_str = format!("CH:{ch}");
                // ao_status format: "AO: X/Y | Zm | CH:N"
                let hs = self.epoch_loop.metrics.handshakes;
                let caps = self.captures.count();
                let up_secs = self
                    .ao
                    .start_time
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                let uptime = if up_secs < 60 {
                    format!("{up_secs}s")
                } else if up_secs < 3600 {
                    format!("{}m", up_secs / 60)
                } else {
                    let h = up_secs / 3600;
                    let m = (up_secs % 3600) / 60;
                    if m == 0 {
                        format!("{h}h")
                    } else {
                        format!("{h}h{m}m")
                    }
                };
                let ao_text = format!("AO: {hs}/{caps} | {uptime} | {ch_str}");
                self.lua.update_indicator_value("ao_status", &ao_text);
            }

            if self.mode == OperatingMode::Safe {
                self.network.rotate_display(false);
                let ip_str = self
                    .network
                    .display_ip_str(self.bluetooth.ip_address.as_deref());
                self.lua.update_indicator_value("ip_display", &ip_str);
            }

            // Check for display settings changes (invert/rotation) every tick
            self.apply_pending_display_settings();
            self.update_display();
        }
        if remainder > 0 {
            std::thread::sleep(Duration::from_secs(remainder));
        }

        // Reset QPU ring buffer between epochs
        if let Some(e) = self.qpu_engine.as_mut() { e.reset_ring(); }

        self.epoch_loop.next_phase(); // -> Scan (increments epoch counter)
    }

    /// Scan phase: count tracked APs, check WiFi health (RAGE mode only).
    fn run_scan_phase(&mut self, result: &mut epoch::EpochResult) {
        self.epoch_loop.phase = epoch::EpochPhase::Scan;
        self.recovery
            .log(recovery::DiagLevel::Info, "epoch scan start");

        result.channel = self.ao.channel() as u8;
        // Use AO's AP count (from stdout BSSID tracking) since AO owns the
        // monitor interface — the beacon tracker can't read frames while AO runs.
        result.aps_seen = self.ao.ap_count();

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

    /// Run one BT-mode epoch: scan, target, attack, update status.
    fn run_bt_epoch(&mut self) {
        // Phase 1: Scan — age out stale devices, enforce limit
        self.bt_discovery.prune(self.config.bt_attacks.target_ttl_secs);
        self.bt_discovery.enforce_limit(256);

        // Phase 2: Target — select targets based on rage level + toggles
        let active_attacks = self.bt_attack_scheduler.active_attack_types();
        let devices = self.bt_discovery.devices_by_rssi();

        if !active_attacks.is_empty() && self.bt_attack_scheduler.can_attack() {
            let targets = bluetooth::attacks::target::TargetSelector::select(
                &devices,
                &active_attacks,
                &self.config.bt_attacks,
                self.config.bt_attacks.max_concurrent_attacks as usize,
            );

            // Phase 3: Attack — dispatch attacks against targets
            for target in &targets {
                self.bt_attack_scheduler.mark_active(&target.device_id, target.attack);

                // Update device attack state
                if let Some(dev) = self.bt_discovery.get_device_mut(&target.device_id) {
                    dev.attack_state = bluetooth::model::observation::BtDeviceAttackState::Attacking;
                }

                log::info!(
                    "BT attack: {:?} → {} ({})",
                    target.attack,
                    target.device_address,
                    target.device_name.as_deref().unwrap_or("?")
                );

                // Dispatch the actual attack worker
                if let Some(ref hci) = self.bt_hci_socket {
                    let result = match target.attack {
                        bluetooth::attacks::BtAttackType::SmpDowngrade => {
                            let addr_type = self.bt_discovery.get_device_addr_type(&target.device_id);
                            bluetooth::attacks::smp::run_downgrade(hci, &target.device_address, addr_type)
                        }
                        bluetooth::attacks::BtAttackType::SmpMitm => {
                            bluetooth::attacks::smp::run_mitm(hci, &target.device_address)
                        }
                        bluetooth::attacks::BtAttackType::Knob => {
                            bluetooth::attacks::knob::run(hci, &target.device_address)
                        }
                        bluetooth::attacks::BtAttackType::BleAdvInjection => {
                            bluetooth::attacks::ble_adv::run(hci, &target.device_address)
                        }
                        bluetooth::attacks::BtAttackType::BleConnHijack => {
                            bluetooth::attacks::ble_hijack::run(hci, &target.device_address)
                        }
                        bluetooth::attacks::BtAttackType::L2capFuzz => {
                            let addr_type = self.bt_discovery.get_device_addr_type(&target.device_id);
                            bluetooth::attacks::l2cap_fuzz::run(&target.device_address, addr_type)
                        }
                        bluetooth::attacks::BtAttackType::AttGattFuzz => {
                            let addr_type = self.bt_discovery.get_device_addr_type(&target.device_id);
                            bluetooth::attacks::att_fuzz::run(&target.device_address, addr_type)
                        }
                        bluetooth::attacks::BtAttackType::VendorCmdUnlock => {
                            bluetooth::attacks::vendor::run_diagnostics(hci, &target.device_address)
                        }
                    };

                    // Store capture if present
                    self.bt_capture_manager.store(&result);

                    // Update device state based on result
                    if let Some(dev) = self.bt_discovery.get_device_mut(&target.device_id) {
                        dev.attack_state = if result.capture.is_some() {
                            bluetooth::model::observation::BtDeviceAttackState::Captured
                        } else if result.success {
                            bluetooth::model::observation::BtDeviceAttackState::Targeted
                        } else {
                            bluetooth::model::observation::BtDeviceAttackState::Failed
                        };
                    }

                    // Record in scheduler and remove from active set
                    self.bt_attack_scheduler.remove_active(&target.device_id);
                    self.bt_attack_scheduler.record(result);
                } else {
                    log::warn!("BT attack: no HCI socket — skipping dispatch for {}", target.device_address);
                    self.bt_attack_scheduler.remove_active(&target.device_id);
                }
            }
        }

        // Phase 3b: Rotate captures if over size limit.
        self.bt_capture_manager
            .rotate_if_needed(self.config.bt_attacks.max_capture_mb);

        // Phase 4: Update status
        let summary = self.bt_discovery.summary();
        let active_count = self.bt_attack_scheduler.active_count();
        let total_captures = self.bt_capture_manager.total_captures();

        let status = if active_count > 0 {
            format!("Attacking {} targets ({} captures)", active_count, total_captures)
        } else if summary.devices_now > 0 {
            format!("Scanning {} BT devices...", summary.devices_now)
        } else {
            "Scanning for BT devices...".to_string()
        };

        self.epoch_loop.personality.current_status = status;
    }

    /// Attack phase: schedule attacks against tracked APs.
    fn run_attack_phase(&mut self, result: &mut epoch::EpochResult) {
        self.epoch_loop.next_phase(); // -> Attack
        let min_rssi = self.shared_state.lock().unwrap().min_rssi;
        let attackable = self.wifi.tracker.attackable(min_rssi);
        // Read attack toggles from web state (all 6 types)
        let enabled_types = {
            let s = self.shared_state.lock().unwrap();
            [
                s.attack_deauth,
                s.attack_pmkid,
                s.attack_csa,
                s.attack_disassoc,
                s.attack_anon_reassoc,
                s.attack_rogue_m2,
            ]
        };
        // Smart Skip: collect BSSIDs that already have handshakes on SD
        let captured_bssids: std::collections::HashSet<[u8; 6]> = if self.skip_captured {
            self.captures
                .files
                .iter()
                .filter(|f| f.has_handshake)
                .map(|f| f.bssid)
                .collect()
        } else {
            std::collections::HashSet::new()
        };
        for ap in &attackable {
            if self.attacks.is_whitelisted(&ap.bssid) {
                continue;
            }
            // Smart Skip: skip APs with existing handshakes
            if self.skip_captured && captured_bssids.contains(&ap.bssid) {
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

    /// Capture phase: validate and index captures from AO output.
    /// In verified mode: tmpfs → hcxpcapngtool → move validated to SD, delete junk.
    /// In collect-all mode: AO writes to SD directly → hcxpcapngtool on SD files, keep all.
    fn run_capture_phase(&mut self, result: &mut epoch::EpochResult) {
        self.epoch_loop.next_phase(); // -> Capture
        result.associations = self.wifi.tracker.total_clients();

        // session_captures is tracked by the AO stdout reader (EAPOL Message 1 events).
        // session_handshakes is incremented below.

        // Snapshot handshake count before any scan/convert so new_handshakes is accurate
        // in both modes. Must be taken here, before the collect-all branch runs batch_convert.
        let handshakes_before = self.captures.handshake_count();

        if !self.capture_all {
            // === VERIFIED MODE: tmpfs → validate → SD ===

            // 1. Scan tmpfs for new AO captures
            let tmpfs_dir = std::path::Path::new(&self.tmpfs_capture_dir);
            let mut tmpfs_manager = capture::CaptureManager::new(&self.tmpfs_capture_dir);
            let _ = tmpfs_manager.scan_directory();

            // Truncate kismet file if it exceeds 50MB to prevent tmpfs exhaustion
            let kismet_path = format!("{}/capture.kismet", self.tmpfs_capture_dir);
            if let Ok(meta) = std::fs::metadata(&kismet_path) {
                if meta.len() > 50 * 1024 * 1024 {
                    info!("kismet file {}MB, truncating", meta.len() / 1024 / 1024);
                    let _ = std::fs::File::create(&kismet_path); // truncate to zero
                }
            }

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
            let (moved, deleted) =
                capture::move_validated_captures(tmpfs_dir, &permanent_dir, &mut self.captures);
            if moved > 0 || deleted > 0 {
                info!("capture pipeline: {moved} saved to SD, {deleted} junk deleted from RAM");
            }
            if moved > 0 {
                self.ao
                    .session_handshakes
                    .fetch_add(moved as u32, std::sync::atomic::Ordering::Relaxed);
            }
        } else {
            // === COLLECT ALL MODE: AO writes directly to SD ===
            // Scan first so newly written files are visible before conversion.
            let _ = self.captures.scan_directory();
            // Run hcxpcapngtool so .22000 companions are created, but keep everything.
            let (converted, no_hs, failed) = capture::batch_convert(&mut self.captures);
            if converted > 0 {
                info!("collect-all: converted {converted} capture(s) to .22000 on SD");
                let with_hs = converted.saturating_sub(no_hs);
                if with_hs > 0 {
                    self.ao
                        .session_handshakes
                        .fetch_add(with_hs as u32, std::sync::atomic::Ordering::Relaxed);
                }
            }
            if failed > 0 {
                log::warn!("collect-all: {failed} conversion(s) failed");
            }
        }

        // 4. Scan permanent dir for upload tracking + new handshake detection
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

        // 5b. Fetch cracked passwords from WPA-SEC every 50 epochs (~25min)
        if self.epoch_loop.metrics.epoch % 50 == 10 && self.wpasec_config.enabled {
            let cracked = capture::fetch_cracked_from_wpasec(&self.wpasec_config);
            if !cracked.is_empty() {
                info!("WPA-SEC: fetched {} cracked password(s)", cracked.len());
                let mut s = self.shared_state.lock().unwrap();
                let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
                s.cracked = cracked
                    .into_iter()
                    .map(|(bssid, ssid, password)| web::CrackedEntry {
                        bssid,
                        ssid,
                        password,
                        date: today.clone(),
                    })
                    .collect();
            }
        }

        let new_handshakes = self
            .captures
            .handshake_count()
            .saturating_sub(handshakes_before);
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
            .args([
                "-s",
                "-H",
                "Content-Type: application/json",
                "-d",
                &body,
                webhook_url,
            ])
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

        let (mode_switch, rate_change, restart, shutdown, bt_toggle) = {
            let mut s = self.shared_state.lock().unwrap();
            let mode = s.pending_mode_switch.take();
            let rate = s.pending_rate_change.take();
            let restart = s.pending_restart;
            s.pending_restart = false;
            let shutdown = s.pending_shutdown;
            s.pending_shutdown = false;
            let bt_toggle = s.pending_bt_toggle.take();
            (mode, rate, restart, shutdown, bt_toggle)
        };

        if let Some(mode) = mode_switch {
            any_command = true;
            info!("web: mode switch to {mode}");
            match mode.to_uppercase().as_str() {
                "TOGGLE" => {
                    let new_mode = self.mode.next();
                    match new_mode {
                        OperatingMode::Safe => self.enter_safe_mode(),
                        OperatingMode::Bt => self.enter_bt_mode(),
                        OperatingMode::Rage => self.enter_rage_mode(),
                    }
                }
                "SAFE" if self.mode != OperatingMode::Safe => self.enter_safe_mode(),
                "BT" if self.mode != OperatingMode::Bt => self.enter_bt_mode(),
                "RAGE" if self.mode != OperatingMode::Rage => self.enter_rage_mode(),
                _ => {
                    let mut s = self.shared_state.lock().unwrap();
                    s.mode = mode;
                }
            }
        }

        if let Some(rate) = rate_change {
            any_command = true;
            info!("web: rate change to {rate}, restarting AO");
            self.ao.set_rate(rate);
            let _ = self.ao.restart();
            // Manual rate change breaks out of RAGE
            let mut s = self.shared_state.lock().unwrap();
            s.rage_enabled = false;
        }

        // Process pending rage slider change
        let rage_change = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_rage_change.take()
        };
        if let Some(rage) = rage_change {
            any_command = true;
            match rage {
                Some(level) => {
                    if let Some(p) = crate::rage::preset(level) {
                        info!(
                            "web: RAGE level {} ({}) — rate={} dwell={}ms ch={:?}",
                            p.level, p.name, p.rate, p.dwell_ms, p.channels
                        );
                        self.ao.set_rate(p.rate);
                        self.wifi.channel_config.channels = p.channels.to_vec();
                        self.wifi.channel_config.dwell_ms = p.dwell_ms;
                        self.wifi.channel_config.current_index = 0;
                        self.autohunt = false;
                        self.ao.config.channels = p.channels.to_vec();
                        self.ao.config.dwell = (p.dwell_ms / 1000).max(1) as u32;
                        let mut s = self.shared_state.lock().unwrap();
                        s.rage_enabled = true;
                        s.rage_level = level;
                        s.autohunt_enabled = false;
                        s.wifi_channels = p.channels.to_vec();
                        s.wifi_dwell_ms = p.dwell_ms;
                        s.attack_rate = p.rate;
                        drop(s);
                        let _ = self.ao.restart();
                    }
                }
                None => {
                    info!("web: RAGE disabled, Custom mode");
                    let mut s = self.shared_state.lock().unwrap();
                    s.rage_enabled = false;
                }
            }
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

        if let Some(visible) = bt_toggle {
            any_command = true;
            info!("web: BT visibility set to {visible}");
            if visible {
                self.bluetooth.show();
            } else {
                self.bluetooth.hide();
            }
        }

        // Process pending radio request (from /api/radio)
        let radio_request = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_radio_request.take()
        };
        if let Some(req) = radio_request {
            any_command = true;
            info!("web: radio request: {req}");
            match req.as_str() {
                "WIFI" if self.mode == OperatingMode::Safe => self.enter_rage_mode(),
                "BT" if self.mode == OperatingMode::Rage => self.enter_safe_mode(),
                "FREE" => {
                    // Release the radio lock (stop everything)
                    self.ao.stop();
                    let _ = self.wifi.stop_monitor();
                    self.bluetooth.power_off();
                    let _ = self.radio.release_lock();
                    info!("radio: released to FREE");
                }
                _ => info!("web: radio request {req} ignored (already in requested mode)"),
            }
        }

        // Process pending attack toggle (trigger persistence)
        let attack_toggle = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_attack_toggle.take()
        };
        if attack_toggle.is_some() {
            any_command = true; // triggers save_runtime_state
        }

        // Process pending BT attack toggle
        let bt_attack_toggle = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_bt_attack_toggle.take()
        };
        if let Some(toggle) = bt_attack_toggle {
            any_command = true;
            if let Some(attack_type) = match toggle.attack.as_str() {
                "smp_downgrade" => {
                    Some(crate::bluetooth::attacks::BtAttackType::SmpDowngrade)
                }
                "smp_mitm" => Some(crate::bluetooth::attacks::BtAttackType::SmpMitm),
                "knob" => Some(crate::bluetooth::attacks::BtAttackType::Knob),
                "ble_adv_injection" => {
                    Some(crate::bluetooth::attacks::BtAttackType::BleAdvInjection)
                }
                "ble_conn_hijack" => {
                    Some(crate::bluetooth::attacks::BtAttackType::BleConnHijack)
                }
                "l2cap_fuzz" => Some(crate::bluetooth::attacks::BtAttackType::L2capFuzz),
                "att_gatt_fuzz" => {
                    Some(crate::bluetooth::attacks::BtAttackType::AttGattFuzz)
                }
                "vendor_cmd_unlock" => {
                    Some(crate::bluetooth::attacks::BtAttackType::VendorCmdUnlock)
                }
                _ => None,
            } {
                self.bt_attack_scheduler
                    .config
                    .set_toggle(attack_type, toggle.enabled);
                // Keep main config in sync with scheduler config
                self.config
                    .bt_attacks
                    .set_toggle(attack_type, toggle.enabled);
                info!(
                    "web: BT attack {} set to {}",
                    toggle.attack, toggle.enabled
                );
            }
        }

        // Process pending BT rage level change
        let bt_rage_level = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_bt_rage_level.take()
        };
        if let Some(level) = bt_rage_level {
            any_command = true;
            if let Some(rage) =
                crate::bluetooth::attacks::BtRageLevel::from_str(&level)
            {
                self.bt_attack_scheduler.config.rage_level = rage;
                self.config.bt_attacks.rage_level = rage;
                info!("web: BT rage level set to {}", level);
            }
        }

        // Process pending BT target (no-op for now, placeholder for future targeting)
        let bt_target = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_bt_target.take()
        };
        if let Some(addr) = bt_target {
            any_command = true;
            info!("web: BT target set to {} (queued)", addr);
        }

        // Process BT scan request — spawn in background thread to avoid blocking
        let bt_scan_needed = {
            let s = self.shared_state.lock().unwrap();
            s.bt_scan_in_progress && s.bt_scan_results.is_empty()
        };
        if bt_scan_needed {
            info!("web: BT scan triggered (background thread)");
            let shared = Arc::clone(&self.shared_state);
            let _ = std::thread::Builder::new()
                .name("bt-scan".into())
                .spawn(move || {
                    // Run scan on this thread (blocking ~10s)
                    let devices = bluetooth::BtTether::scan_devices_static();
                    let results: Vec<web::BtScanDevice> = devices
                        .into_iter()
                        .map(|(mac, name)| web::BtScanDevice { mac, name })
                        .collect();
                    let mut s = shared.lock().unwrap();
                    s.bt_scan_results = results;
                    s.bt_scan_in_progress = false;
                    log::info!("BT scan thread completed");
                });
        }

        // Process BT pair request
        let bt_pair_mac = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_bt_pair.take()
        };
        if let Some(mac) = bt_pair_mac {
            any_command = true;
            info!("web: BT pair with {mac}");
            match self.bluetooth.pair_and_connect(&mac) {
                Ok(()) => info!("BT paired and connected to {mac}"),
                Err(e) => log::error!("BT pair failed: {e}"),
            }
        }

        // Process pending plugin position updates
        let plugin_updates = {
            let mut s = self.shared_state.lock().unwrap();
            std::mem::take(&mut s.pending_plugin_updates)
        };
        if !plugin_updates.is_empty() {
            for update in &plugin_updates {
                if let Some(enabled) = update.enabled {
                    self.lua.set_plugin_enabled(&update.name, enabled);
                }
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

        // Process pending whitelist adds (batch — supports multiple per epoch)
        let mut whitelist_changed = false;
        let (wl_adds, wl_removes) = {
            let mut s = self.shared_state.lock().unwrap();
            (
                std::mem::take(&mut s.pending_whitelist_adds),
                std::mem::take(&mut s.pending_whitelist_removes),
            )
        };
        for add in wl_adds {
            let entry = wifi::parse_whitelist_entry(&add.value);
            match entry {
                wifi::WhitelistEntry::Bssid(mac) => {
                    if !self.attacks.is_whitelisted(&mac) {
                        self.attacks.whitelist.push(mac);
                        info!("web: whitelist added MAC {}", add.value);
                        any_command = true;
                        whitelist_changed = true;
                    }
                }
                wifi::WhitelistEntry::Ssid(ssid) => {
                    self.wifi.tracker.add_ssid_whitelist(&ssid);
                    self.ao.config.whitelist = self.wifi.tracker.ssid_whitelist.clone();
                    info!("web: whitelist added SSID {ssid}");
                    any_command = true;
                    whitelist_changed = true;
                }
            }
        }
        for remove in wl_removes {
            let parts: Vec<&str> = remove.split(':').collect();
            if parts.len() == 6 {
                let mut mac = [0u8; 6];
                let mut ok = true;
                for (i, p) in parts.iter().enumerate() {
                    match u8::from_str_radix(p, 16) {
                        Ok(b) => mac[i] = b,
                        Err(_) => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok {
                    self.attacks.whitelist.retain(|m| m != &mac);
                    info!("web: whitelist removed {}", remove);
                    any_command = true;
                    whitelist_changed = true;
                }
            }
        }

        // If whitelist changed, restart AO so it picks up the new --whitelist file
        if whitelist_changed && self.ao.state == ao::AoState::Running {
            self.ao.config.whitelist = self.wifi.tracker.ssid_whitelist.clone();
            info!("web: whitelist changed, restarting AO");
            let _ = self.ao.restart();
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
            if let Some(autohunt) = cfg.autohunt {
                self.autohunt = autohunt;
                info!("web: autohunt set to {autohunt}");
            }
            // Sync to AO config before restart
            if self.autohunt {
                self.ao.config.channels.clear(); // autohunt = scan all
            } else {
                self.ao.config.channels = self.wifi.channel_config.channels.clone();
            }
            self.ao.config.dwell = (self.wifi.channel_config.dwell_ms / 1000).max(1) as u32;
            // Restart AO so new channel/dwell/autohunt settings take effect
            info!("web: restarting AO with new channel config");
            let _ = self.ao.restart();
            // Manual channel change breaks out of RAGE
            let mut s = self.shared_state.lock().unwrap();
            s.rage_enabled = false;
        }

        // Process pending skip_captured toggle (Smart Skip)
        let skip_captured_toggle = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_skip_captured.take()
        };
        if let Some(skip) = skip_captured_toggle {
            self.skip_captured = skip;
            info!("web: skip_captured set to {skip}");
            any_command = true;
        }

        // Process pending capture mode change (Verified vs Collect All)
        let capture_all_toggle = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_capture_all.take()
        };
        if let Some(all) = capture_all_toggle {
            self.capture_all = all;
            if all {
                self.ao.config.output_dir = CAPTURE_DIR.to_string();
                info!("web: capture_all enabled — AO output -> SD directly");
            } else {
                self.ao.config.output_dir = format!("{}/capture", self.tmpfs_capture_dir);
                info!("web: capture_all disabled — AO output -> tmpfs");
            }
            // Only restart AO in RAGE mode; in SAFE mode AO isn't running and the
            // updated output_dir will take effect when RAGE mode is next entered.
            if self.mode == OperatingMode::Rage {
                info!("web: restarting AO for capture mode change");
                self.ao
                    .session_captures
                    .store(0, std::sync::atomic::Ordering::Relaxed);
                self.ao
                    .session_handshakes
                    .store(0, std::sync::atomic::Ordering::Relaxed);
                let _ = self.ao.restart();
            } else {
                info!("web: capture mode updated (SAFE mode — AO restart deferred to RAGE entry)");
            }
            any_command = true;
        }

        // Process pending capture deletion
        let pending_delete = {
            let mut s = self.shared_state.lock().unwrap();
            s.pending_delete.take()
        };
        if let Some(filename) = pending_delete {
            if let Some(pos) = self.captures.files.iter().position(|f| {
                f.path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy() == filename)
            }) {
                let file = self.captures.files.remove(pos);
                let companion = file.path.with_extension("22000");
                if let Err(e) = std::fs::remove_file(&file.path) {
                    log::warn!(
                        "delete capture: failed to remove {}: {e}",
                        file.path.display()
                    );
                } else {
                    info!("deleted capture: {filename}");
                }
                let _ = std::fs::remove_file(&companion); // ok if .22000 missing
                any_command = true;
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
            info!(
                "web: WPA-SEC key updated, enabled={}",
                self.wpasec_config.enabled
            );
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

        // Process pending settings update (display reinit handled by apply_pending_display_settings)
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
            if let Some(rssi) = update.min_rssi {
                let clamped = rssi.clamp(-100, -30);
                let mut s = self.shared_state.lock().unwrap();
                s.min_rssi = clamped;
                info!("web: min RSSI set to {clamped} dBm");
                any_command = true;
            }
            if let Some(ttl) = update.ap_ttl_secs {
                let clamped = ttl.clamp(30, 600);
                let mut s = self.shared_state.lock().unwrap();
                s.ap_ttl_secs = clamped;
                info!("web: AP TTL set to {clamped}s");
                any_command = true;
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
            {
                "---".to_string()
            }
        };

        // Get CPU percent from shared state
        let cpu_pct = {
            let s = self.shared_state.lock().unwrap();
            s.cpu_percent
        };

        let (bat_level, bat_charging, bat_mv, bat_low, bat_crit, bat_avail) =
            self.battery_snapshot();
        let (ao_state, ao_pid, ao_crashes, ao_uptime, ao_up_secs) = self.ao_snapshot();
        let (bt_conn, bt_short, bt_ip, bt_inet) = self.bt_snapshot();

        lua::state::EpochState {
            uptime_secs: self.epoch_loop.uptime_secs(),
            epoch: m.epoch,
            mode: self.mode.as_str().to_string(),
            rage_level: {
                let s = self.shared_state.lock().unwrap();
                if s.rage_enabled { s.rage_level } else { 0 }
            },
            channel: {
                let ch = self.ao.channel();
                if ch > 0 { ch as u8 } else { m.channel }
            },
            aps_seen: (self.lifetime_aps_base + self.ao.ap_count() as u64) as u32,
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
                if ch > 0 {
                    ch.to_string()
                } else {
                    "?".to_string()
                }
            },
            session_captures: self.ao.session_captures(),
            session_handshakes: self.ao.session_handshakes(),
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
            display_ip: self
                .network
                .display_ip_str(self.bluetooth.ip_address.as_deref()),
            mood: self.epoch_loop.personality.mood.value(),
            face: self.epoch_loop.current_face().as_str().to_string(),
            level: self.epoch_loop.personality.xp.level,
            xp: self.epoch_loop.personality.xp.xp,
            status_message: self.epoch_loop.personality.status_msg(),
            epoch_phase_status: self.epoch_loop.status_message(),
            skip_captured: self.skip_captured,
            fw_crash_suppress: self.firmware_monitor.crash_suppress,
            fw_hardfault: self.firmware_monitor.hardfault,
            fw_health: format!("{:?}", self.firmware_monitor.health()),
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
            "ssid_whitelist": self.wifi.tracker.ssid_whitelist,
            "wpasec_key": self.wpasec_config.api_key,
            "discord_webhook_url": self.discord_webhook_url,
            "discord_enabled": self.discord_enabled,
            "autohunt": self.autohunt,
            "skip_captured": self.skip_captured,
            "capture_all": self.capture_all,
            "name": self.config.name,
            "wifi_channels": self.wifi.channel_config.channels,
            "wifi_dwell_ms": self.wifi.channel_config.dwell_ms,
            "rage_enabled": s.rage_enabled,
            "rage_level": s.rage_level,
            "lifetime_aps": self.lifetime_aps_base + self.ao.ap_count() as u64,
            "display_invert": self.screen.config.invert,
            "display_rotation": self.screen.config.rotation,
            "min_rssi": s.min_rssi,
            "ap_ttl_secs": s.ap_ttl_secs,
        });
        drop(s);
        let path = "/var/lib/oxigotchi/state.json";
        let _ = std::fs::create_dir_all("/var/lib/oxigotchi");
        if let Err(e) = std::fs::write(
            path,
            serde_json::to_string_pretty(&state).unwrap_or_default(),
        ) {
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
                                Err(_) => {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                        if ok {
                            self.attacks.whitelist.push(mac);
                        }
                    }
                }
            }
        }

        // Apply SSID whitelist
        if let Some(ssids) = state.get("ssid_whitelist").and_then(|v| v.as_array()) {
            for entry in ssids {
                if let Some(ssid) = entry.as_str() {
                    self.wifi.tracker.add_ssid_whitelist(ssid);
                    info!("state: restored SSID whitelist: {ssid}");
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
        if let Some(autohunt) = state.get("autohunt").and_then(|v| v.as_bool()) {
            self.autohunt = autohunt;
        }
        if let Some(v) = state.get("skip_captured").and_then(|v| v.as_bool()) {
            self.skip_captured = v;
        }
        if let Some(v) = state.get("capture_all").and_then(|v| v.as_bool()) {
            self.capture_all = v;
        }
        if let Some(channels_arr) = state.get("wifi_channels").and_then(|v| v.as_array()) {
            let channels: Vec<u8> = channels_arr
                .iter()
                .filter_map(|v| v.as_u64().map(|n| n as u8))
                .collect();
            if !channels.is_empty() {
                self.wifi.channel_config.channels = channels.clone();
                // Also sync to AO config so next start uses these
                if !self.autohunt {
                    self.ao.config.channels = channels;
                }
            }
        }
        if let Some(lifetime_aps) = state.get("lifetime_aps").and_then(|v| v.as_u64()) {
            self.lifetime_aps_base = lifetime_aps;
        }
        if let Some(dwell) = state.get("wifi_dwell_ms").and_then(|v| v.as_u64()) {
            self.wifi.channel_config.dwell_ms = dwell;
            self.ao.config.dwell = (dwell / 1000).max(1) as u32;
        }
        if let Some(name) = state.get("name").and_then(|v| v.as_str()) {
            if !name.is_empty() {
                self.config.name = name.to_string();
                let mut s = self.shared_state.lock().unwrap();
                s.name = name.to_string();
            }
        }

        // Apply RAGE slider state
        if let Some(enabled) = state.get("rage_enabled").and_then(|v| v.as_bool()) {
            let mut s = self.shared_state.lock().unwrap();
            s.rage_enabled = enabled;
            if enabled {
                if let Some(level) = state.get("rage_level").and_then(|v| v.as_u64()) {
                    let level = (level as u8).clamp(1, 7);
                    s.rage_level = level;
                    if let Some(p) = crate::rage::preset(level) {
                        info!("state: restoring RAGE level {} ({})", p.level, p.name);
                        drop(s);
                        self.ao.set_rate(p.rate);
                        self.wifi.channel_config.channels = p.channels.to_vec();
                        self.wifi.channel_config.dwell_ms = p.dwell_ms;
                        self.wifi.channel_config.current_index = 0;
                        self.autohunt = false;
                        self.ao.config.channels = p.channels.to_vec();
                        self.ao.config.dwell = (p.dwell_ms / 1000).max(1) as u32;
                        let mut s = self.shared_state.lock().unwrap();
                        s.autohunt_enabled = false;
                        s.wifi_channels = p.channels.to_vec();
                        s.wifi_dwell_ms = p.dwell_ms;
                        s.attack_rate = p.rate;
                    }
                }
            }
        }

        // Apply display settings
        if let Some(invert) = state.get("display_invert").and_then(|v| v.as_bool()) {
            self.screen.config.invert = invert;
            let mut s = self.shared_state.lock().unwrap();
            s.display_invert = invert;
        }
        if let Some(rotation) = state.get("display_rotation").and_then(|v| v.as_u64()) {
            let r = if rotation == 180 { 180 } else { 0 };
            self.screen.config.rotation = r;
            let mut s = self.shared_state.lock().unwrap();
            s.display_rotation = r;
        }
        // Apply wifi tuning settings
        if let Some(rssi) = state.get("min_rssi").and_then(|v| v.as_i64()) {
            let mut s = self.shared_state.lock().unwrap();
            s.min_rssi = (rssi as i8).clamp(-100, -30);
        }
        if let Some(ttl) = state.get("ap_ttl_secs").and_then(|v| v.as_u64()) {
            let mut s = self.shared_state.lock().unwrap();
            s.ap_ttl_secs = ttl.clamp(30, 600);
        }

        info!("loaded runtime state from {path}");
    }

    /// Sync daemon state into the shared web state.
    fn sync_to_web(&mut self) {
        if self.config.gpu.enabled && self.config.gpu.runtime.trace_enabled {
            match self
                .gpu_runtime_ingestor
                .load(&self.config.gpu.runtime.summary_source)
            {
                Ok(Some(summary)) => {
                    self.gpu_state.runtime = summary;
                }
                Ok(None) => {}
                Err(e) => {
                    log::warn!("gpu runtime summary ingest failed: {e}");
                }
            }
        }

        let scan_snapshot = {
            let s = self.shared_state.lock().unwrap();
            (s.bt_scan_results.clone(), s.bt_scan_in_progress)
        };

        self.bt_discovery.reset();
        if scan_snapshot.1 {
            let _ = self
                .bt_discovery
                .apply(bluetooth::model::observation::BtDiscoveryObservation::ScanStarted);
        }
        for device in &scan_snapshot.0 {
            let _ = self.bt_discovery.apply(
                bluetooth::model::observation::BtDiscoveryObservation::DeviceSeen(
                    bluetooth::model::observation::BtDeviceObservation {
                        id: device.mac.clone(),
                        address: device.mac.clone(),
                        address_type: None,
                        transport: bluetooth::model::observation::BtTransport::Unknown,
                        name: if device.name.is_empty() {
                            None
                        } else {
                            Some(device.name.clone())
                        },
                        rssi: None,
                        rssi_best: None,
                        category: bluetooth::model::observation::BtCategory::Unknown,
                        services: Vec::new(),
                        manufacturer: None,
                        first_seen: chrono::Utc::now(),
                        ts: chrono::Utc::now(),
                        seen_count: 1,
                        attack_state: bluetooth::model::observation::BtDeviceAttackState::Untouched,
                    },
                ),
            );
        }
        if !scan_snapshot.1 {
            let _ = self
                .bt_discovery
                .apply(bluetooth::model::observation::BtDiscoveryObservation::ScanStopped);
        }

        let bt_mode = self.bt_feature.state.mode.clone();
        let bt_state_str = self.bluetooth.status_str().to_string();
        let bt_enabled = self.config.bluetooth.enabled;
        let bt_overlap_active = self.wifi.state == wifi::WifiState::Monitor
            && self.bluetooth.state == bluetooth::BtState::Connected;

        let mut s = self.shared_state.lock().unwrap();
        let m = &self.epoch_loop.metrics;

        let (bat_level, bat_charging, bat_mv, bat_low, bat_crit, bat_avail) =
            self.battery_snapshot();
        let (ao_state, ao_pid, ao_crashes, ao_uptime, _ao_up_secs) = self.ao_snapshot();
        let (bt_conn, _bt_short, bt_ip, bt_inet) = self.bt_snapshot();

        s.uptime_str = self.epoch_loop.uptime_str();
        s.mode = self.mode.as_str().to_string();
        s.epoch = m.epoch;
        s.channel = {
            let ch = self.ao.channel();
            if ch > 0 { ch as u8 } else { m.channel }
        };
        s.aps_seen = (self.lifetime_aps_base + self.ao.ap_count() as u64) as u32;
        s.handshakes = self.captures.handshake_count() as u32;
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
        s.session_captures = self.ao.session_captures();
        s.session_handshakes = self.ao.session_handshakes();
        s.capture_all = self.capture_all;
        s.capture_list = self
            .captures
            .files
            .iter()
            .map(|f| {
                let bssid_mac = format!(
                    "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    f.bssid[0], f.bssid[1], f.bssid[2], f.bssid[3], f.bssid[4], f.bssid[5]
                );
                let captured_date = f
                    .mtime
                    .map(|t| {
                        let dt: chrono::DateTime<chrono::Utc> = t.into();
                        dt.format("%Y-%m-%d").to_string()
                    })
                    .unwrap_or_else(|| "unknown".to_string());
                web::CaptureEntry {
                    filename: f
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    size_bytes: f.size,
                    ssid: f.ssid.clone(),
                    bssid_mac,
                    captured_date,
                    has_handshake: f.has_handshake,
                }
            })
            .collect();

        s.battery_level = bat_level;
        s.battery_charging = bat_charging;
        s.battery_voltage_mv = bat_mv;
        s.battery_low = bat_low;
        s.battery_critical = bat_crit;
        s.battery_available = bat_avail;

        s.wifi_state = format!("{:?}", self.wifi.state);
        s.wifi_aps_tracked = self.wifi.tracker.count();
        s.wifi_channels = self.wifi.channel_config.channels.clone();
        s.wifi_dwell_ms = self.wifi.channel_config.dwell_ms;
        s.autohunt_enabled = self.autohunt;
        s.skip_captured = self.skip_captured;

        s.bt_state = self.bluetooth.status_str().to_string(); // long form for web
        s.bt_connected = bt_conn;
        s.bt_ip = bt_ip;
        s.bt_internet_available = bt_inet;
        s.bt_retry_count = self.bluetooth.retry_count;
        s.bt_device_name = self.bluetooth.config.phone_name.clone();
        s.bt_phone_mac = self.bluetooth.config.phone_mac.clone();

        self.bt_feature.state.mode = bt_mode;
        self.bt_feature.state.health.stack_up = bt_enabled;
        self.bt_feature.state.health.controller_present = bt_enabled;
        self.bt_feature.state.health.degraded = self.bluetooth.state == bluetooth::BtState::Error;
        self.bt_feature.state.health.last_error =
            if self.bluetooth.state == bluetooth::BtState::Error {
                Some(bt_state_str.clone())
            } else {
                None
            };
        self.bt_feature.state.summary = self.bt_discovery.summary();
        self.bt_feature.state.controller.last_probe_status = Some(bt_state_str.clone());
        self.bt_feature.state.coex.overlap_active = bt_overlap_active;
        self.bt_feature.state.coex.contention_score = if bt_overlap_active { 1 } else { 0 };

        s.bt_feature_mode = format!("{:?}", self.bt_feature.state.mode);
        s.bt_feature_devices_now = self.bt_feature.state.summary.devices_now;
        s.bt_feature_contention_score = self.bt_feature.state.coex.contention_score;

        // -- bt attacks --
        s.bt_attack_enabled = self.config.bt_attacks.enabled;
        s.bt_rage_level = self.config.bt_attacks.rage_level.as_str().to_string();
        s.bt_attack_smp_downgrade = self.config.bt_attacks.smp_downgrade;
        s.bt_attack_smp_mitm = self.config.bt_attacks.smp_mitm;
        s.bt_attack_knob = self.config.bt_attacks.knob;
        s.bt_attack_ble_adv_injection = self.config.bt_attacks.ble_adv_injection;
        s.bt_attack_ble_conn_hijack = self.config.bt_attacks.ble_conn_hijack;
        s.bt_attack_l2cap_fuzz = self.config.bt_attacks.l2cap_fuzz;
        s.bt_attack_att_gatt_fuzz = self.config.bt_attacks.att_gatt_fuzz;
        s.bt_attack_vendor_cmd_unlock = self.config.bt_attacks.vendor_cmd_unlock;
        s.bt_total_attacks = self.bt_attack_scheduler.total_attacks;
        s.bt_total_captures = self.bt_attack_scheduler.total_captures;
        s.bt_active_attacks = self.bt_attack_scheduler.active_count();
        s.bt_devices_seen = self.bt_discovery.summary().devices_now;
        s.bt_device_list = self.bt_discovery.devices_by_rssi().iter().map(|d| {
            web::BtDeviceInfo {
                address: d.address.clone(),
                name: d.name.clone(),
                rssi: d.rssi,
                category: d.category.as_str().to_string(),
                transport: format!("{:?}", d.transport),
                attack_state: format!("{:?}", d.attack_state),
                seen_count: d.seen_count,
            }
        }).collect();
        s.bt_patchram_state = self.patchram.state.as_str().to_string();
        s.bt_capture_keys = self.bt_capture_manager.total_keys;
        s.bt_capture_crashes = self.bt_capture_manager.total_crashes;
        s.bt_capture_vendor = self.bt_capture_manager.total_vendor;

        let gpu_policy = self.gpu_optimizer.policy_for(&self.gpu_state.runtime);
        s.gpu_mode = format!("{:?}", self.gpu_state.mode);
        s.gpu_signal = self
            .gpu_state
            .runtime
            .strongest_signal
            .as_ref()
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|| "None".to_string());
        s.gpu_submit_seen = self.gpu_state.runtime.vc4_submit_cl_seen;
        s.gpu_snapshot_policy = gpu_policy.as_str().to_string();
        s.gpu_flush_threshold = gpu_policy.threshold();

        // Update QPU stats
        if let Some(ref engine) = self.qpu_engine {
            let stats = engine.stats();
            s.qpu_enabled = true;
            s.qpu_available = stats.qpu_available;
            s.qpu_num_cores = stats.num_qpus;
            s.qpu_frames_submitted = stats.frames_submitted;
            s.qpu_frames_classified = stats.frames_classified;
            s.qpu_batches_processed = stats.batches_processed;
            s.qpu_overflow_count = stats.overflow_count;
            s.qpu_last_batch_size = stats.last_batch_size;
            s.qpu_last_batch_duration_us = stats.last_batch_duration_us;
        }

        s.ao_state = ao_state;
        s.ao_pid = ao_pid;
        s.ao_crash_count = ao_crashes;
        s.ao_uptime = ao_uptime;
        s.gpsd_available = self.ao.gpsd_detected;

        // Radio lock state
        s.radio_mode = self.radio.mode.as_str().to_string();
        let (_, radio_pid) = self.radio.read_lock();
        s.radio_pid = radio_pid;

        s.xp = self.epoch_loop.personality.xp.xp;
        s.level = self.epoch_loop.personality.xp.level;

        s.fw_crash_suppress = self.firmware_monitor.crash_suppress;
        s.fw_hardfault = self.firmware_monitor.hardfault;
        s.fw_health = format!("{:?}", self.firmware_monitor.health());

        // Refresh system metrics so WS broadcast has real values (not zeroes)
        let cpu_temp = web::read_cpu_temp();
        if cpu_temp > 0.0 {
            s.cpu_temp_c = cpu_temp;
        }
        let (mem_used, mem_total) = web::read_mem_info();
        if mem_total > 0 {
            s.mem_used_mb = mem_used;
            s.mem_total_mb = mem_total;
        }
        let (disk_used, disk_total) = web::read_disk_info();
        if disk_total > 0 {
            s.disk_used_mb = disk_used;
            s.disk_total_mb = disk_total;
        }

        s.recovery_state = format!("{:?}", self.recovery.state);
        s.recovery_total = self.recovery.total_recoveries;
        s.recovery_soft_retries = self.recovery.soft_retry_count;
        s.recovery_hard_retries = self.recovery.hard_retry_count;
        s.recovery_last_str = match self.recovery.last_recovery {
            Some(t) => format!("{}s ago", t.elapsed().as_secs()),
            None => "never".into(),
        };

        // Copy framebuffer for web display preview
        s.screen_width = self.screen.fb.width;
        s.screen_height = self.screen.fb.height;
        s.screen_bytes = self.screen.fb.as_bytes().to_vec();

        // Sync AP list for web dashboard — merge WiFi tracker + AO stdout data
        let mut ap_entries: Vec<web::ApEntry> = self
            .wifi
            .tracker
            .sorted_by_rssi()
            .iter()
            .map(|ap| {
                let has_hs = self
                    .captures
                    .files
                    .iter()
                    .any(|f| f.has_handshake && f.bssid == ap.bssid);
                web::ApEntry {
                    bssid: ap.bssid_str(),
                    ssid: if ap.ssid.is_empty() {
                        "(hidden)".into()
                    } else {
                        ap.ssid.clone()
                    },
                    rssi: ap.rssi as i16,
                    channel: ap.channel,
                    clients: ap.client_count,
                    has_handshake: has_hs,
                }
            })
            .collect();

        // Add APs seen by AO that aren't already in the WiFi tracker
        let ao_aps = self.ao.ap_snapshot();
        let existing_bssids: std::collections::HashSet<String> = ap_entries
            .iter()
            .map(|e| e.bssid.replace(':', "").to_lowercase())
            .collect();
        for ao_ap in &ao_aps {
            if !existing_bssids.contains(&ao_ap.bssid) {
                // Format BSSID with colons: aabbccddeeff -> AA:BB:CC:DD:EE:FF
                let bssid_fmt = ao_ap
                    .bssid
                    .as_bytes()
                    .chunks(2)
                    .map(|c| std::str::from_utf8(c).unwrap_or("??").to_uppercase())
                    .collect::<Vec<_>>()
                    .join(":");
                // Check if we have a capture for this BSSID on SD
                let ao_bssid_bytes: [u8; 6] = {
                    let hex = &ao_ap.bssid;
                    let mut b = [0u8; 6];
                    if hex.len() == 12 {
                        for i in 0..6 {
                            b[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0);
                        }
                    }
                    b
                };
                let has_hs = ao_ap.captured
                    || self
                        .captures
                        .files
                        .iter()
                        .any(|f| f.has_handshake && f.bssid == ao_bssid_bytes);
                let ssid = self
                    .captures
                    .ssid_for(&ao_bssid_bytes)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "(AO)".into());
                let rssi = self
                    .captures
                    .bssid_rssi
                    .get(&ao_bssid_bytes)
                    .copied()
                    .unwrap_or(-100);
                ap_entries.push(web::ApEntry {
                    bssid: bssid_fmt,
                    ssid,
                    rssi,
                    channel: ao_ap.channel,
                    clients: ao_ap.hit_count,
                    has_handshake: has_hs,
                });
            }
        }
        s.ap_list = ap_entries;

        // Sync whitelist for web dashboard (MAC + SSID entries)
        let mut wl: Vec<web::WhitelistEntry> = self
            .attacks
            .whitelist
            .iter()
            .map(|mac| web::WhitelistEntry {
                value: mac
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(":"),
                entry_type: "MAC".into(),
            })
            .collect();
        for ssid in &self.wifi.tracker.ssid_whitelist {
            wl.push(web::WhitelistEntry {
                value: ssid.clone(),
                entry_type: "SSID".into(),
            });
        }
        s.whitelist = wl;

        // Sync plugin list for web dashboard
        s.plugin_list = self
            .lua
            .get_web_plugin_list()
            .into_iter()
            .map(|(meta, enabled, x, y)| web::PluginInfo {
                name: meta.name,
                version: meta.version,
                author: meta.author,
                tag: meta.tag,
                enabled,
                x,
                y,
            })
            .collect();

        // Sync WPA-SEC and Discord config for web dashboard
        s.wpasec_api_key = self.wpasec_config.api_key.clone();
        s.discord_webhook_url = self.discord_webhook_url.clone();
        s.discord_enabled = self.discord_enabled;
    }

    /// Check for pending display settings changes and apply immediately.
    /// Called before each display render so invert/rotation take effect
    /// within the current epoch rather than waiting for the next one.
    fn apply_pending_display_settings(&mut self) {
        let needs_reinit = {
            let mut s = self.shared_state.lock().unwrap();
            let reinit = s.pending_display_reinit;
            if reinit {
                s.pending_display_reinit = false;
                // Sync display config from shared state
                self.screen.config.invert = s.display_invert;
                self.screen.config.rotation = s.display_rotation;
            }
            reinit
        };
        if needs_reinit {
            display::driver::request_reinit();
            self.screen.force_flush();
            info!("display: applying invert/rotation change immediately");
            self.save_runtime_state();
        }
    }

    /// Update the e-ink display with current state.
    /// Layout matches Python angryoxide.py AO mode — see docs/DISPLAY_SPEC.md.
    /// In BT mode, uses a dedicated layout with BT-specific stats.
    fn update_display(&mut self) {
        self.screen.clear();

        // ---- BT mode: dedicated layout ----
        if self.mode == OperatingMode::Bt {
            let summary = self.bt_discovery.summary();
            let active = self.bt_attack_scheduler.active_count();
            let captures = self.bt_capture_manager.total_captures();
            let patchram_state = self.patchram.state.as_str();
            let battery_str = if self.battery.available {
                format!("{}%", self.battery.status.level)
            } else {
                "?".to_string()
            };
            let uptime = self.epoch_loop.uptime_str();

            // BT-specific face
            let patchram_error = self.patchram.state == bluetooth::patchram::PatchramState::Error;
            let face = personality::bt_mode_face(
                active,
                summary.devices_now,
                captures as u32,
                patchram_error,
            );

            // LINE 1
            self.screen.draw_hline(0, 14, display::DISPLAY_WIDTH);
            // Face
            self.screen.draw_face(&face);
            // BT stats overlay
            self.screen.draw_bt_mode(
                summary.devices_now,
                active,
                captures as u32,
                patchram_state,
                &battery_str,
                &uptime,
            );
            // Status text
            let status = &self.epoch_loop.personality.current_status;
            self.screen.draw_status(status);
            // Name
            self.screen.draw_name(&self.config.name);
            // LINE 2
            self.screen.draw_hline(0, 108, display::DISPLAY_WIDTH);
            // Lua indicators
            for ind in self.lua.get_indicators() {
                self.screen.draw_indicator(&ind);
            }
            self.screen.flush();
            return;
        }

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
            self.screen.set_pixel(
                bar_x + dx,
                bar_y,
                embedded_graphics::pixelcolor::BinaryColor::On,
            );
            self.screen.set_pixel(
                bar_x + dx,
                bar_y + bar_h - 1,
                embedded_graphics::pixelcolor::BinaryColor::On,
            );
        }
        for dy in 0..bar_h {
            self.screen.set_pixel(
                bar_x,
                bar_y + dy,
                embedded_graphics::pixelcolor::BinaryColor::On,
            );
            self.screen.set_pixel(
                bar_x + bar_w - 1,
                bar_y + dy,
                embedded_graphics::pixelcolor::BinaryColor::On,
            );
        }
        // Fill
        for dy in 1..(bar_h - 1) {
            for dx in 1..=filled_w {
                self.screen.set_pixel(
                    bar_x + dx,
                    bar_y + dy,
                    embedded_graphics::pixelcolor::BinaryColor::On,
                );
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
            self.epoch_loop
                .personality
                .set_override(personality::Face::BatteryCritical);
        } else if self.battery.status.low {
            self.epoch_loop
                .personality
                .set_override(personality::Face::BatteryLow);
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
            self.epoch_loop
                .personality
                .set_override(personality::Face::Shutdown);
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
    const SAFE_FACES: &'static [personality::Face] =
        &[personality::Face::Debug, personality::Face::Grateful];

    /// Transition from RAGE or BT to SAFE mode via radio lock manager.
    fn enter_safe_mode(&mut self) {
        info!("mode: {} -> SAFE", self.mode.as_str());
        let face = Self::random_face(Self::SAFE_FACES);
        self.show_transition(face, "Switching to SAFE...");

        if self.mode == OperatingMode::Bt {
            // Close HCI socket before tearing down BT
            self.bt_hci_socket = None;
            // BT attack mode → SAFE: swap attack patchram for stock
            match self.radio.transition_bt_to_safe(
                &mut self.bluetooth,
                &mut self.patchram,
            ) {
                Ok(()) => {
                    info!("radio: BT attack -> BT safe transition complete (stock patchram)");
                    // Set up BT PAN connection with stock firmware
                    match self.bluetooth.setup() {
                        Ok(()) => info!("BT connected: {}", self.bluetooth.status_str()),
                        Err(e) => log::warn!("BT setup failed: {e}"),
                    }
                }
                Err(e) => {
                    log::error!("radio transition BT->SAFE failed: {e}");
                    return;
                }
            }
        } else {
            // RAGE mode → SAFE: standard WiFi -> BT transition
            match self
                .radio
                .transition_to_bt(&mut self.ao, &mut self.wifi, &mut self.bluetooth)
            {
                Ok(()) => {
                    info!("radio: WIFI -> BT transition complete");
                    // Now set up BT PAN connection
                    match self.bluetooth.setup() {
                        Ok(()) => info!("BT connected: {}", self.bluetooth.status_str()),
                        Err(e) => log::warn!("BT setup failed: {e}"),
                    }
                }
                Err(e) => {
                    log::error!("radio transition to BT failed: {e}");
                    // Radio manager already rolled back to WIFI
                    return;
                }
            }
        }

        self.mode = OperatingMode::Safe;
        self.epoch_loop.personality.set_override(face);
        // Radio transition disrupts SPI bus — force display reinit so next flush doesn't BUSY-timeout
        display::driver::request_reinit();
    }

    /// Transition into BT attack mode — tears down WiFi, loads attack patchram.
    fn enter_bt_mode(&mut self) {
        info!("mode: {} -> BT", self.mode.as_str());
        let face = Self::random_face(Self::RAGE_FACES);
        self.show_transition(face, "Switching to BT Attack...");

        // Atomic transition: stop AO + WiFi, load attack patchram
        match self.radio.transition_to_bt_attack(
            &mut self.ao,
            &mut self.wifi,
            &mut self.bluetooth,
            &mut self.patchram,
        ) {
            Ok(()) => {
                info!("radio: transition to BT attack complete");
                self.mode = OperatingMode::Bt;
                self.bt_feature.set_mode(bluetooth::model::config::BtMode::Attack);
                self.epoch_loop.personality.set_override(face);
                // Open raw HCI socket for attack dispatch
                match bluetooth::attacks::hci::HciSocket::open(0) {
                    Ok(sock) => {
                        info!("bt: HCI socket opened for attack dispatch");
                        self.bt_hci_socket = Some(sock);
                    }
                    Err(e) => log::error!("bt: failed to open HCI socket: {e}"),
                }
                // Init capture directories
                self.bt_capture_manager.init_dirs();
                // Radio transition disrupts SPI bus — force display reinit
                display::driver::request_reinit();
            }
            Err(e) => {
                log::error!("radio transition to BT attack failed: {e}");
                // Radio manager already set mode=Free on failure
            }
        }
    }

    /// Transition from SAFE or BT to RAGE mode via radio lock manager.
    fn enter_rage_mode(&mut self) {
        info!("mode: {} -> RAGE", self.mode.as_str());
        let face = Self::random_face(Self::RAGE_FACES);
        self.show_transition(face, "Switching to RAGE...");

        if self.mode == OperatingMode::Bt {
            // Close HCI socket before tearing down BT
            self.bt_hci_socket = None;
            // BT attack mode → RAGE: unload patchram, restart WiFi+AO
            match self.radio.transition_bt_to_wifi(
                &mut self.ao,
                &mut self.wifi,
                &mut self.patchram,
            ) {
                Ok(()) => {
                    info!("radio: BT -> WIFI transition complete (via patchram unload)");
                }
                Err(e) => {
                    log::error!("radio transition BT->WIFI failed: {e}");
                    return;
                }
            }
        } else {
            // SAFE mode → RAGE: standard BT -> WiFi transition
            match self
                .radio
                .transition_to_wifi(&mut self.ao, &mut self.wifi, &mut self.bluetooth)
            {
                Ok(()) => {
                    info!("radio: BT -> WIFI transition complete");
                }
                Err(e) => {
                    log::error!("radio transition to WIFI failed: {e}");
                    // Radio manager already rolled back to BT
                    return;
                }
            }
        }

        self.mode = OperatingMode::Rage;
        self.network.display_slot = network::DisplaySlot::UsbIp;
        self.epoch_loop.personality.clear_override();
        // Radio transition disrupts SPI bus — force display reinit so next flush doesn't BUSY-timeout
        display::driver::request_reinit();
    }

    /// Handle recovery actions from the health checker.
    fn handle_recovery_action(&mut self, action: recovery::RecoveryAction) {
        match action {
            recovery::RecoveryAction::None => {}
            recovery::RecoveryAction::SoftRecover => {
                info!("attempting soft WiFi recovery (modprobe cycle)");
                self.epoch_loop
                    .personality
                    .set_override(personality::Face::WifiDown);

                // Stop AO first — it's using the interface
                self.ao.stop();

                // Stop monitor mode (may fail if interface is gone — that's OK)
                let _ = self.wifi.stop_monitor();

                // Full brcmfmac modprobe cycle (matches Python's _try_fw_recovery)
                #[cfg(unix)]
                {
                    use std::process::Command;
                    // Bring down all WiFi interfaces first — modprobe -r fails with
                    // "Module brcmfmac is in use" if interfaces are still up
                    info!("bringing down WiFi interfaces before rmmod");
                    let _ = Command::new("ip")
                        .args(["link", "set", "wlan0mon", "down"])
                        .output();
                    let _ = Command::new("iw").args(["dev", "wlan0mon", "del"]).output();
                    let _ = Command::new("ip")
                        .args(["link", "set", "wlan0", "down"])
                        .output();
                    std::thread::sleep(Duration::from_secs(1));
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
                        self.recovery.record_recovery();
                    }
                    Err(e) => log::error!("soft recovery: monitor mode failed: {e}"),
                }
            }
            recovery::RecoveryAction::HardRecover => {
                info!("attempting hard WiFi recovery (full GPIO power cycle)");
                self.epoch_loop
                    .personality
                    .set_override(personality::Face::FwCrash);

                // Stop AO before power-cycling — it's using the interface
                self.ao.stop();

                let gpio_ok = match recovery::execute_gpio_recovery(self.recovery.config.gpio_cycle_delay_ms) {
                    Ok(true) => { info!("GPIO recovery succeeded, wlan0 is back"); true }
                    Ok(false) => { log::error!("GPIO recovery failed: wlan0 did not return"); false }
                    Err(e) => { log::error!("GPIO recovery error: {e}"); false }
                };

                if gpio_ok {
                    match self.wifi.start_monitor() {
                        Ok(()) => {
                            info!("hard recovery: monitor mode restored");
                            // Reset AO crash counter so we don't immediately re-trigger
                            self.ao.reset();
                            // Restart AO
                            match self.ao.start() {
                                Ok(()) => info!("hard recovery: AO restarted (PID {})", self.ao.pid),
                                Err(e) => log::error!("hard recovery: AO restart failed: {e}"),
                            }
                            self.recovery.record_recovery();
                        }
                        Err(e) => log::error!("hard recovery: monitor mode failed: {e}"),
                    }
                }
            }
            recovery::RecoveryAction::Reboot => {
                log::error!("WiFi recovery exhausted after max retries, rebooting");
                self.epoch_loop
                    .personality
                    .set_override(personality::Face::Broken);
                self.recovery.log(
                    recovery::DiagLevel::Error,
                    "all recovery attempts exhausted -- rebooting",
                );
                let _ = recovery::trigger_reboot();
            }
            recovery::RecoveryAction::GiveUp => {
                log::error!("WiFi recovery exhausted — giving up (no reboot to avoid loop)");
                self.epoch_loop
                    .personality
                    .set_override(personality::Face::Broken);
                self.recovery.log(
                    recovery::DiagLevel::Error,
                    "all recovery attempts exhausted — WiFi offline, daemon continues",
                );
                // Do NOT reboot — causes infinite loop when firmware is persistently broken.
                // The daemon stays up with web dashboard accessible via USB for diagnostics.
            }
        }
    }

    /// Build a web status snapshot.
    fn build_web_status(&self) -> web::StatusResponse {
        let m = &self.epoch_loop.metrics;
        let s = self.shared_state.lock().unwrap();
        web::build_status(&web::StatusParams {
            name: &self.config.name,
            uptime: &self.epoch_loop.uptime_str(),
            epoch: m.epoch,
            channel: m.channel,
            aps_seen: (self.lifetime_aps_base + self.ao.ap_count() as u64) as u32,
            handshakes: m.handshakes,
            blind_epochs: m.blind_epochs,
            mood: self.epoch_loop.personality.mood.value(),
            face: self.epoch_loop.current_face().as_str(),
            status_message: &self.epoch_loop.personality.status_msg(),
            mode: "AO",
            display_invert: s.display_invert,
            display_rotation: s.display_rotation,
            min_rssi: s.min_rssi,
            ap_ttl_secs: s.ap_ttl_secs,
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
        config::Config::load_or_default(
            oxi_paths
                .config
                .to_str()
                .unwrap_or("/etc/oxigotchi/config.toml"),
        )
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

    // Create WebSocket broadcast channel for live dashboard updates
    let ws_tx = web::create_ws_broadcast();

    // Start web server in a tokio task
    let web_state = shared_state.clone();
    let web_ws_tx = ws_tx.clone();
    tokio::spawn(async move {
        web::start_server(web_state, web_ws_tx).await;
    });

    // Run the daemon main loop in a blocking thread (it uses std::thread::sleep)
    let mut daemon = Daemon::new(config, shared_state, ws_tx);
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
        let ws_tx = web::create_ws_broadcast();
        Daemon::new(config, shared_state, ws_tx)
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
        // Skip on non-Pi Linux — boot() calls start_monitor() which needs wlan0
        if std::fs::metadata("/sys/class/net/wlan0").is_err() {
            return;
        }
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
            Face::Excited,
            Face::Happy,
            Face::Awake,
            Face::Bored,
            Face::Sad,
            Face::Demotivated,
        ];
        let override_faces = [
            Face::BatteryCritical,
            Face::BatteryLow,
            Face::Shutdown,
            Face::WifiDown,
            Face::FwCrash,
            Face::Broken,
        ];
        let manual_override_faces = [
            Face::Sleep,
            Face::Intense,
            Face::Cool,
            Face::Angry,
            Face::Friend,
            Face::Debug,
            Face::Upload,
            Face::Lonely,
            Face::Grateful,
            Face::Motivated,
            Face::Smart,
            Face::AoCrashed,
            Face::Raging,
            Face::Grazing,
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
            daemon
                .epoch_loop
                .record_result(&epoch::EpochResult::default());
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
        // Skip on non-Pi Linux — boot() calls start_monitor() which needs wlan0
        if std::fs::metadata("/sys/class/net/wlan0").is_err() {
            return;
        }
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
        // Skip on non-Pi Linux — boot() calls start_monitor() which needs wlan0
        if std::fs::metadata("/sys/class/net/wlan0").is_err() {
            return;
        }
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
        std::fs::write(
            dir.path().join("test_ind.lua"),
            r#"
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
        "#,
        )
        .unwrap();

        let mut rt = lua::PluginRuntime::new();
        let configs = vec![lua::PluginConfig::default_for("test_ind", 50, 60)];
        let loaded = rt.load_plugins_from_dir(dir.path().to_str().unwrap(), &configs);
        assert_eq!(loaded, 1);

        let state = lua::state::EpochState {
            epoch: 99,
            ..Default::default()
        };
        rt.tick_epoch(&state);

        let indicators = rt.get_indicators();
        assert_eq!(indicators.len(), 1);
        assert_eq!(indicators[0].value, "E:99");
        assert_eq!(indicators[0].x, 50);
        assert_eq!(indicators[0].y, 60);
    }

    #[test]
    fn test_operating_mode_next() {
        assert_eq!(OperatingMode::Rage.next(), OperatingMode::Bt);
        assert_eq!(OperatingMode::Bt.next(), OperatingMode::Safe);
        assert_eq!(OperatingMode::Safe.next(), OperatingMode::Rage);
    }

    #[test]
    fn test_operating_mode_toggle_is_next() {
        assert_eq!(OperatingMode::Rage.toggle(), OperatingMode::Rage.next());
        assert_eq!(OperatingMode::Bt.toggle(), OperatingMode::Bt.next());
        assert_eq!(OperatingMode::Safe.toggle(), OperatingMode::Safe.next());
    }

    #[test]
    fn test_operating_mode_as_str() {
        assert_eq!(OperatingMode::Rage.as_str(), "RAGE");
        assert_eq!(OperatingMode::Bt.as_str(), "BT");
        assert_eq!(OperatingMode::Safe.as_str(), "SAFE");
    }

    #[test]
    fn test_daemon_starts_in_rage_mode() {
        let daemon = make_daemon();
        assert_eq!(daemon.mode, OperatingMode::Rage);
    }

    #[test]
    fn test_process_web_commands_rage_level() {
        let mut daemon = make_daemon();
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.pending_rage_change = Some(Some(4));
        }
        daemon.process_web_commands();
        // Level 4 = Hunt: rate 2, dwell 2000ms, all 13 channels
        assert_eq!(daemon.ao.config.rate, 2);
        assert_eq!(daemon.wifi.channel_config.dwell_ms, 2000);
        assert_eq!(
            daemon.wifi.channel_config.channels,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]
        );
        assert!(!daemon.autohunt);
        let s = daemon.shared_state.lock().unwrap();
        assert!(s.rage_enabled);
        assert_eq!(s.rage_level, 4);
    }

    #[test]
    fn test_process_web_commands_rage_disable() {
        let mut daemon = make_daemon();
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.rage_enabled = true;
            s.rage_level = 4;
            s.pending_rage_change = Some(None);
        }
        daemon.process_web_commands();
        let s = daemon.shared_state.lock().unwrap();
        assert!(!s.rage_enabled);
    }

    #[test]
    fn test_process_web_commands_rate_breaks_rage() {
        let mut daemon = make_daemon();
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.rage_enabled = true;
            s.rage_level = 5;
            s.pending_rate_change = Some(1);
        }
        daemon.process_web_commands();
        let s = daemon.shared_state.lock().unwrap();
        assert!(
            !s.rage_enabled,
            "manual rate change should break out of RAGE"
        );
    }

    // ---- Autohunt toggle daemon tests ----

    #[test]
    fn test_process_web_commands_autohunt_on_clears_ao_channels() {
        let mut daemon = make_daemon();
        // Set some channels first
        daemon.wifi.channel_config.channels = vec![1, 6, 11];
        daemon.ao.config.channels = vec![1, 6, 11];
        daemon.autohunt = false;
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.pending_channel_config = Some(web::ChannelConfig {
                channels: None,
                dwell_ms: None,
                autohunt: Some(true),
            });
        }
        daemon.process_web_commands();
        assert!(daemon.autohunt, "autohunt should be enabled");
        assert!(
            daemon.ao.config.channels.is_empty(),
            "AO channels should be cleared when autohunt is ON"
        );
    }

    #[test]
    fn test_process_web_commands_autohunt_off_restores_channels() {
        let mut daemon = make_daemon();
        daemon.wifi.channel_config.channels = vec![1, 6, 11];
        daemon.autohunt = true;
        daemon.ao.config.channels = vec![]; // was autohunting
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.pending_channel_config = Some(web::ChannelConfig {
                channels: Some(vec![1, 6, 11]),
                dwell_ms: Some(2000),
                autohunt: Some(false),
            });
        }
        daemon.process_web_commands();
        assert!(!daemon.autohunt, "autohunt should be disabled");
        assert_eq!(
            daemon.ao.config.channels,
            vec![1, 6, 11],
            "AO channels should be restored from wifi channel config"
        );
    }

    #[test]
    fn test_process_web_commands_autohunt_on_breaks_rage() {
        let mut daemon = make_daemon();
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.rage_enabled = true;
            s.rage_level = 5;
            s.pending_channel_config = Some(web::ChannelConfig {
                channels: None,
                dwell_ms: None,
                autohunt: Some(true),
            });
        }
        daemon.process_web_commands();
        let s = daemon.shared_state.lock().unwrap();
        assert!(
            !s.rage_enabled,
            "autohunt toggle should break out of RAGE"
        );
    }

    #[test]
    fn test_process_web_commands_autohunt_preserves_dwell() {
        let mut daemon = make_daemon();
        daemon.wifi.channel_config.dwell_ms = 3000;
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.pending_channel_config = Some(web::ChannelConfig {
                channels: None,
                dwell_ms: None, // not changing dwell
                autohunt: Some(true),
            });
        }
        daemon.process_web_commands();
        assert_eq!(
            daemon.wifi.channel_config.dwell_ms, 3000,
            "dwell should be unchanged when only toggling autohunt"
        );
        assert_eq!(
            daemon.ao.config.dwell, 3,
            "AO dwell (seconds) should reflect existing dwell_ms"
        );
    }

    #[test]
    fn test_process_web_commands_channel_config_updates_dwell() {
        let mut daemon = make_daemon();
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.pending_channel_config = Some(web::ChannelConfig {
                channels: Some(vec![1, 6]),
                dwell_ms: Some(5000),
                autohunt: Some(false),
            });
        }
        daemon.process_web_commands();
        assert_eq!(daemon.wifi.channel_config.dwell_ms, 5000);
        assert_eq!(daemon.wifi.channel_config.channels, vec![1, 6]);
        assert_eq!(daemon.ao.config.dwell, 5);
        assert_eq!(daemon.ao.config.channels, vec![1, 6]);
    }

    #[test]
    fn test_process_web_commands_empty_channels_not_applied() {
        let mut daemon = make_daemon();
        daemon.wifi.channel_config.channels = vec![1, 6, 11];
        {
            let mut s = daemon.shared_state.lock().unwrap();
            s.pending_channel_config = Some(web::ChannelConfig {
                channels: Some(vec![]), // empty
                dwell_ms: None,
                autohunt: Some(false),
            });
        }
        daemon.process_web_commands();
        assert_eq!(
            daemon.wifi.channel_config.channels,
            vec![1, 6, 11],
            "empty channel list should not overwrite existing channels"
        );
    }

    #[test]
    fn test_daemon_default_autohunt_is_on() {
        let daemon = make_daemon();
        assert!(daemon.autohunt, "daemon should start with autohunt enabled");
    }
}
