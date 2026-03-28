pub mod jokes;
pub mod messages;
pub mod variety;

use log::warn;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default save path for XP stats on Pi.
pub const DEFAULT_XP_SAVE_PATH: &str = "/home/pi/exp_stats.json";

/// All possible face expressions the oxigotchi can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Face {
    Awake,
    Sleep,
    Happy,
    Sad,
    Excited,
    Bored,
    Intense,
    Cool,
    Angry,
    Broken,
    Friend,
    Debug,
    Upload,
    Lonely,
    Grateful,
    Motivated,
    Demotivated,
    Smart,
    BatteryCritical,
    BatteryLow,
    WifiDown,
    FwCrash,
    AoCrashed,
    Shutdown,
    Raging,
    Grazing,
}

impl Face {
    /// Return the kaomoji string for this face.
    pub fn as_str(&self) -> &'static str {
        match self {
            Face::Awake => "(O_O)",
            Face::Sleep => "(-_-) zzZ",
            Face::Happy => "(^_^)",
            Face::Sad => "(;_;)",
            Face::Excited => "(>_<)!",
            Face::Bored => "(-_-)",
            Face::Intense => "(*_*)",
            Face::Cool => "(B_B)",
            Face::Angry => "(>_<)",
            Face::Broken => "(X_X)",
            Face::Friend => "(♥_♥)",
            Face::Debug => "(#_#)",
            Face::Upload => "(^_^)~",
            Face::Lonely => "('_')",
            Face::Grateful => "(^_^)b",
            Face::Motivated => "(9_9)",
            Face::Demotivated => "(._.)",
            Face::Smart => "(◉_◉)",
            Face::BatteryCritical => "(X_X)!",
            Face::BatteryLow => "(@_@)",
            Face::WifiDown => "(?_?)",
            Face::FwCrash => "(X_X)fw",
            Face::AoCrashed => "(X_X)ao",
            Face::Shutdown => "(~_~)",
            Face::Raging => "(>_<)>",
            Face::Grazing => "(~u~)",
        }
    }

    /// Return the lowercase key used for message/joke lookup.
    pub fn face_key(&self) -> &'static str {
        match self {
            Face::Awake => "awake",
            Face::Sleep => "sleep",
            Face::Happy => "happy",
            Face::Sad => "sad",
            Face::Excited => "excited",
            Face::Bored => "bored",
            Face::Intense => "intense",
            Face::Cool => "cool",
            Face::Angry => "angry",
            Face::Broken => "debug",
            Face::Friend => "friend",
            Face::Debug => "debug",
            Face::Upload => "upload",
            Face::Lonely => "lonely",
            Face::Grateful => "grateful",
            Face::Motivated => "motivated",
            Face::Demotivated => "demotivated",
            Face::Smart => "smart",
            Face::BatteryCritical => "angry",
            Face::BatteryLow => "sad",
            Face::WifiDown => "angry",
            Face::FwCrash => "angry",
            Face::AoCrashed => "angry",
            Face::Shutdown => "sleep",
            Face::Raging => "raging",
            Face::Grazing => "grazing",
        }
    }

    /// Convert a string key (from variety engine) back to a Face enum.
    /// Returns None for unknown keys.
    pub fn from_key(key: &str) -> Option<Face> {
        match key {
            "awake" => Some(Face::Awake),
            "sleep" => Some(Face::Sleep),
            "happy" => Some(Face::Happy),
            "sad" => Some(Face::Sad),
            "excited" => Some(Face::Excited),
            "bored" => Some(Face::Bored),
            "intense" => Some(Face::Intense),
            "cool" => Some(Face::Cool),
            "angry" => Some(Face::Angry),
            "broken" => Some(Face::Broken),
            "friend" => Some(Face::Friend),
            "debug" => Some(Face::Debug),
            "upload" => Some(Face::Upload),
            "lonely" => Some(Face::Lonely),
            "grateful" => Some(Face::Grateful),
            "motivated" => Some(Face::Motivated),
            "demotivated" => Some(Face::Demotivated),
            "smart" => Some(Face::Smart),
            "raging" => Some(Face::Raging),
            "grazing" => Some(Face::Grazing),
            _ => None,
        }
    }

    /// Return all face variants. Useful for iteration/testing.
    pub fn all() -> &'static [Face] {
        &[
            Face::Awake,
            Face::Sleep,
            Face::Happy,
            Face::Sad,
            Face::Excited,
            Face::Bored,
            Face::Intense,
            Face::Cool,
            Face::Angry,
            Face::Broken,
            Face::Friend,
            Face::Debug,
            Face::Upload,
            Face::Lonely,
            Face::Grateful,
            Face::Motivated,
            Face::Demotivated,
            Face::Smart,
            Face::BatteryCritical,
            Face::BatteryLow,
            Face::WifiDown,
            Face::FwCrash,
            Face::AoCrashed,
            Face::Shutdown,
            Face::Raging,
            Face::Grazing,
        ]
    }
}

/// Mood value ranging from 0.0 (miserable) to 1.0 (ecstatic).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mood {
    value: f32,
}

impl Mood {
    /// Create a new mood with the given value (clamped to 0.0..=1.0).
    pub fn new(value: f32) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
        }
    }

    /// Get the current mood value.
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Adjust mood by a delta (positive = happier, negative = sadder).
    pub fn adjust(&mut self, delta: f32) {
        self.value = (self.value + delta).clamp(0.0, 1.0);
    }

    /// Choose the best face for the current mood and context.
    pub fn face(&self) -> Face {
        match self.value {
            v if v >= 0.9 => Face::Excited,
            v if v >= 0.7 => Face::Happy,
            v if v >= 0.5 => Face::Awake,
            v if v >= 0.3 => Face::Bored,
            v if v >= 0.1 => Face::Sad,
            _ => Face::Demotivated,
        }
    }

    /// Return a status message appropriate for the current mood.
    pub fn status_message(&self) -> &'static str {
        match self.value {
            v if v >= 0.9 => "So many handshakes!",
            v if v >= 0.7 => "Having fun!",
            v if v >= 0.5 => "Scanning...",
            v if v >= 0.3 => "Not much going on...",
            v if v >= 0.1 => "Where is everyone?",
            _ => "The bull contemplates the void...",
        }
    }
}

impl Default for Mood {
    fn default() -> Self {
        Self::new(0.5)
    }
}

// ---------------------------------------------------------------------------
// Mood Engine: event-driven deltas
// ---------------------------------------------------------------------------

/// Mood adjustment constants.
pub mod mood_deltas {
    /// Mood increase on handshake capture.
    pub const HANDSHAKE: f32 = 0.1;
    /// Mood increase on new AP seen.
    pub const NEW_AP: f32 = 0.02;
    /// Mood increase on level up.
    pub const LEVEL_UP: f32 = 0.2;
    /// Mood decrease on blind epoch (no handshakes).
    pub const BLIND_EPOCH: f32 = -0.02;
    /// Mood decrease on crash (fw/ao).
    pub const CRASH: f32 = -0.1;
    /// Mood decrease per idle epoch (long stretch with nothing).
    pub const IDLE_DECAY: f32 = -0.01;
    /// Mood increase in busy RF environment (>100 frames/epoch).
    pub const RF_BUSY: f32 = 0.03;
    /// Mood decrease in quiet RF environment (0 frames).
    pub const RF_QUIET: f32 = -0.01;
    /// Mood increase during deauth storm (>10/sec).
    pub const RF_DEAUTH_STORM: f32 = 0.05;
    /// Mood increase during probe flood (>20/sec).
    pub const RF_PROBE_FLOOD: f32 = 0.02;
    /// Mood boost when bull tells a joke (slows decay during idle).
    pub const JOKE: f32 = 0.02;
}

// ---------------------------------------------------------------------------
// Status Message Engine (replaces Python voice.py)
// ---------------------------------------------------------------------------

/// Current system context for generating status messages.
#[derive(Debug, Clone, Default)]
pub struct SystemContext {
    /// Channels currently being scanned.
    pub scan_channels: Vec<u8>,
    /// SSID of the most recently captured handshake.
    pub last_handshake_ssid: Option<String>,
    /// Whether WiFi just recovered from a failure.
    pub wifi_recovered: bool,
    /// Current battery percentage (0-100), or None if unavailable.
    pub battery_percent: Option<u8>,
    /// Whether the battery is low (<= 20%).
    pub battery_low: bool,
    /// Number of consecutive blind epochs.
    pub blind_epochs: u32,
    /// Whether a level-up just occurred.
    pub level_up: bool,
    /// Current level (for level-up message).
    pub level: u32,
}

/// Generate a context-aware status message.
///
/// Priority order: battery low > wifi recovery > handshake > level up > scan > idle.
pub fn status_message(ctx: &SystemContext, mood: &Mood) -> String {
    // Battery low takes highest priority
    if ctx.battery_low {
        if let Some(pct) = ctx.battery_percent {
            return format!("Battery low: {}%", pct);
        }
    }

    // WiFi recovery
    if ctx.wifi_recovered {
        return "WiFi recovered!".to_string();
    }

    // Just captured a handshake
    if let Some(ref ssid) = ctx.last_handshake_ssid {
        return format!("Captured {}!", ssid);
    }

    // Level up
    if ctx.level_up {
        return format!("Level up! Lv.{}", ctx.level);
    }

    // Active scan
    if !ctx.scan_channels.is_empty() {
        let ch_str: Vec<String> = ctx.scan_channels.iter().map(|c| c.to_string()).collect();
        return format!("Scanning channels {}...", ch_str.join(","));
    }

    // Idle / bored messages based on blind epochs
    if ctx.blind_epochs >= 10 {
        return idle_message(ctx.blind_epochs, mood);
    }

    // Default: mood-based message
    mood.status_message().to_string()
}

/// Random-ish idle messages when bored (deterministic based on blind_epochs count).
fn idle_message(blind_epochs: u32, mood: &Mood) -> String {
    // Pick a message based on the epoch count, cycling through options
    let messages: &[&str] = if mood.value() < 0.2 {
        &[
            "Is anyone out there?",
            "So quiet...",
            "I miss the packets.",
            "Even the APs left...",
        ]
    } else {
        &[
            "Looking for networks...",
            "Patiently waiting...",
            "Searching the airwaves...",
            "Any WiFi around here?",
        ]
    };
    let idx = (blind_epochs as usize) % messages.len();
    messages[idx].to_string()
}

/// Select face based on BT mode activity.
pub fn bt_mode_face(
    active_attacks: u32,
    devices: u32,
    captures_this_session: u32,
    patchram_error: bool,
) -> Face {
    if patchram_error {
        return Face::Broken;
    }
    if captures_this_session > 0 {
        return Face::Excited;
    }
    if active_attacks > 0 {
        return Face::Raging;
    }
    if devices > 5 {
        return Face::Intense;
    }
    if devices > 0 {
        return Face::Cool;
    }
    Face::Lonely
}

// ---------------------------------------------------------------------------
// Personality state machine tracking mood and epoch statistics
// ---------------------------------------------------------------------------

/// Personality state machine tracking mood and epoch statistics.
#[derive(Debug)]
pub struct Personality {
    pub mood: Mood,
    /// Override face (e.g., for battery warnings, crashes).
    pub override_face: Option<Face>,
    /// Number of consecutive epochs with no handshakes.
    pub blind_epochs: u32,
    /// Total handshakes captured this session.
    pub total_handshakes: u32,
    /// Total APs seen this session.
    pub total_aps_seen: u32,
    /// XP/leveling tracker.
    pub xp: XpTracker,
    /// Current system context for status messages.
    pub context: SystemContext,
    /// Face variety engine (milestones, idle rotation, rare faces, etc.)
    pub variety: variety::FaceVariety,
    /// Joke phase state: 0 = question, 1 = punchline.
    joke_phase: u8,
    /// Epochs remaining in current joke phase.
    joke_epochs_left: u32,
    /// Index into face joke list (-1 equivalent = None).
    joke_index: Option<usize>,
    /// Which face's joke pool is active.
    joke_face: String,
    /// Status text cycling: epochs the current status has been shown.
    status_display_epochs: u32,
    /// The currently displayed status text.
    pub(crate) current_status: String,
}

impl Personality {
    /// Create a new personality with default mood (0.5) and no overrides.
    pub fn new() -> Self {
        Self {
            mood: Mood::default(),
            override_face: None,
            blind_epochs: 0,
            total_handshakes: 0,
            total_aps_seen: 0,
            xp: XpTracker::new(),
            context: SystemContext::default(),
            variety: variety::FaceVariety::new(),
            joke_phase: 0,
            joke_epochs_left: 0,
            joke_index: None,
            joke_face: String::new(),
            status_display_epochs: 0,
            current_status: String::new(),
        }
    }

    /// Get the face to display, considering overrides and variety engine.
    /// Priority: hardware override > variety engine > mood-based.
    pub fn current_face(&self) -> Face {
        // Hardware overrides (battery, crash, wifi) take highest priority
        if let Some(f) = self.override_face {
            return f;
        }
        // Face variety engine (milestones, idle rotation, capture cycling, etc.)
        if let Some(key) = self.variety.current_override() {
            if let Some(face) = Face::from_key(key) {
                return face;
            }
        }
        // Default: mood-based face
        self.mood.face()
    }

    /// Called when a handshake is captured. Returns true if leveled up.
    pub fn on_handshake(&mut self) -> bool {
        self.total_handshakes += 1;
        self.blind_epochs = 0;
        self.mood.adjust(mood_deltas::HANDSHAKE);

        let leveled = self.xp.award(XpTracker::XP_HANDSHAKE);
        if leveled {
            self.mood.adjust(mood_deltas::LEVEL_UP);
            self.context.level_up = true;
            self.context.level = self.xp.level;
        }
        leveled
    }

    /// Called when a deauth is sent. Returns true if leveled up.
    pub fn on_deauth(&mut self) -> bool {
        let leveled = self.xp.award(XpTracker::XP_DEAUTH);
        if leveled {
            self.mood.adjust(mood_deltas::LEVEL_UP);
            self.context.level_up = true;
            self.context.level = self.xp.level;
        }
        leveled
    }

    /// Called when an association is made. Returns true if leveled up.
    pub fn on_association(&mut self) -> bool {
        let leveled = self.xp.award(XpTracker::XP_ASSOCIATION);
        if leveled {
            self.mood.adjust(mood_deltas::LEVEL_UP);
            self.context.level_up = true;
            self.context.level = self.xp.level;
        }
        leveled
    }

    /// Called when APs are seen in an epoch. Returns true if leveled up.
    pub fn on_aps_seen(&mut self, count: u32) -> bool {
        self.total_aps_seen += count;
        let mut leveled = false;
        if count > 0 {
            self.mood.adjust(mood_deltas::NEW_AP);
            for _ in 0..count {
                if self.xp.award(XpTracker::XP_NEW_AP) {
                    leveled = true;
                    self.mood.adjust(mood_deltas::LEVEL_UP);
                    self.context.level_up = true;
                    self.context.level = self.xp.level;
                }
            }
        }
        leveled
    }

    /// Called at the end of a blind epoch (no handshakes).
    pub fn on_blind_epoch(&mut self) {
        self.blind_epochs += 1;
        self.context.blind_epochs = self.blind_epochs;

        // Graduated penalty: mild first, then heavier
        let penalty = match self.blind_epochs {
            1..=3 => mood_deltas::BLIND_EPOCH,
            4..=10 => mood_deltas::BLIND_EPOCH * 2.5, // -0.05
            _ => mood_deltas::BLIND_EPOCH * 4.0,      // -0.08
        };
        self.mood.adjust(penalty);
    }

    /// Called on each idle epoch when nothing is happening (no APs, no handshakes).
    pub fn on_idle_epoch(&mut self) {
        self.mood.adjust(mood_deltas::IDLE_DECAY);
    }

    /// Called when a crash occurs (firmware or AO).
    pub fn on_crash(&mut self) {
        self.mood.adjust(mood_deltas::CRASH);
    }

    /// Set an override face (e.g., for hardware warnings).
    pub fn set_override(&mut self, face: Face) {
        self.override_face = Some(face);
    }

    /// Clear any face override.
    pub fn clear_override(&mut self) {
        self.override_face = None;
    }

    /// Reset transient context flags (call at start of each epoch).
    pub fn reset_epoch_context(&mut self) {
        self.context.last_handshake_ssid = None;
        self.context.wifi_recovered = false;
        self.context.level_up = false;
        self.context.scan_channels.clear();
    }

    /// Generate and cache a bull-themed status message for the current state.
    ///
    /// Call this once per epoch (requires &mut self for joke state tracking).
    /// Uses slow cycling (3 epochs per message) and 30% chance for two-part jokes.
    pub fn generate_status(&mut self) {
        // If milestone is active, show milestone status
        if let Some(status) = self.variety.milestone_status {
            self.current_status = status.to_string();
            return;
        }

        let face = self.current_face();
        let face_name = face.face_key().to_string();

        // If face changed, reset joke state and immediately pick a message
        // from the new face's pool (don't clear — avoids fallback to static mood msg)
        if self.joke_face != face_name {
            self.joke_phase = 0;
            self.joke_epochs_left = 0;
            self.joke_index = None;
            self.joke_face = face_name.clone();
            self.status_display_epochs = 3; // force new message pick below
            // Pick an immediate message from the new face so status is never empty
            let msgs = messages::messages_for_face(&face_name);
            if !msgs.is_empty() {
                let mut rng = rand::thread_rng();
                self.current_status = msgs[rng.gen_range(0..msgs.len())].to_string();
            }
        }

        // If a joke is actively being displayed, continue it
        if self.joke_epochs_left > 0 {
            self.joke_epochs_left -= 1;
            if let Some(idx) = self.joke_index {
                let joke_list = jokes::jokes_for_face(&self.joke_face);
                if idx < joke_list.len() {
                    let part = if self.joke_phase == 0 {
                        joke_list[idx].0
                    } else {
                        joke_list[idx].1
                    };
                    self.current_status = part.to_string();
                    return;
                }
            }
        }

        // Question phase just ended — switch to punchline
        if self.joke_phase == 0 && self.joke_index.is_some() && self.joke_epochs_left == 0 {
            self.joke_phase = 1;
            self.joke_epochs_left = 2; // punchline displays for 2 more epochs
            self.status_display_epochs = 0; // reset so slow-cycling doesn't interfere
            if let Some(idx) = self.joke_index {
                let joke_list = jokes::jokes_for_face(&self.joke_face);
                if idx < joke_list.len() {
                    self.current_status = joke_list[idx].1.to_string();
                    return;
                }
            }
        }

        // Punchline done — clear joke state and force new message pick
        if self.joke_phase == 1 && self.joke_epochs_left == 0 {
            self.joke_index = None;
            self.joke_phase = 0;
            self.status_display_epochs = 3; // prevent slow cycling from holding stale punchline
        }

        // Slow cycling: keep current status for 3 epochs
        if self.status_display_epochs < 3 && !self.current_status.is_empty() {
            self.status_display_epochs += 1;
            return;
        }

        let mut rng = rand::thread_rng();

        // Higher joke rate when mood is low — bored bull cracks more jokes
        let joke_chance = if self.mood.value() < 0.3 { 0.45 } else { 0.30 };
        if rng.r#gen::<f32>() < joke_chance {
            let joke_list = jokes::jokes_for_face(&face_name);
            if !joke_list.is_empty() {
                let idx = rng.gen_range(0..joke_list.len());
                let question = joke_list[idx].0.to_string();
                self.joke_index = Some(idx);
                self.joke_phase = 0;
                self.joke_epochs_left = 2; // question held by countdown for 2 more epochs (3 total)
                self.joke_face = face_name;
                self.current_status = question;
                self.status_display_epochs = 1;
                self.mood.adjust(mood_deltas::JOKE);
                return;
            }
        }

        // Regular bull message
        let msgs = messages::messages_for_face(&face_name);
        if msgs.is_empty() {
            self.current_status = "AO scanning...".to_string();
            self.status_display_epochs = 1;
            return;
        }
        let idx = rng.gen_range(0..msgs.len());
        let mut msg = msgs[idx].to_string();
        // Avoid repeating
        if msgs.len() > 1 && msg == self.current_status {
            let filtered: Vec<_> = msgs.iter().filter(|m| **m != self.current_status).collect();
            if !filtered.is_empty() {
                let alt = rng.gen_range(0..filtered.len());
                msg = filtered[alt].to_string();
            }
        }
        self.current_status = msg;
        self.status_display_epochs = 1;
    }

    /// Get the cached status message. Call `generate_status()` once per epoch first.
    pub fn status_msg(&self) -> String {
        if self.current_status.is_empty() {
            return status_message(&self.context, &self.mood);
        }
        self.current_status.clone()
    }

    /// Apply RF environment observations to mood and face.
    /// Called once per epoch after QPU classification.
    pub fn apply_rf_environment(&mut self, rf: &crate::qpu::rf::RfEnvironment) {
        use crate::qpu::rf;

        if rf.total_frames > rf::BUSY_THRESHOLD {
            self.mood.adjust(mood_deltas::RF_BUSY);
        } else if rf.total_frames == 0 {
            self.mood.adjust(mood_deltas::RF_QUIET);
        }

        if rf.deauth_rate > rf::DEAUTH_STORM_RATE {
            self.mood.adjust(mood_deltas::RF_DEAUTH_STORM);
            self.override_face = Some(Face::Raging);
        }

        if rf.probe_rate > rf::PROBE_FLOOD_RATE {
            self.mood.adjust(mood_deltas::RF_PROBE_FLOOD);
        }

        if rf.unique_bssids > rf::RICH_BSSID_COUNT {
            self.override_face = Some(Face::Excited);
        }

        // Lonely: APs exist but nobody's talking
        if rf.beacon_rate > 0.0 && rf.data_rate == 0.0 && rf.probe_rate == 0.0 && rf.total_frames > 0 {
            self.override_face = Some(Face::Lonely);
        }
    }
}

impl Default for Personality {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// XP / Leveling system (Python: exp.py)
// ---------------------------------------------------------------------------

/// Persistent XP state saved to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XpSaveData {
    pub level: u32,
    pub xp: u64,
    pub xp_total: u64,
    /// Mood value at save time, so we can restore it.
    pub mood: f32,
}

/// Experience point tracker and leveling system.
///
/// XP values per event (matching Python exp.py spec):
///   - Handshake:   100 XP
///   - Deauth:       10 XP
///   - Association:  15 XP
///   - New AP seen:   5 XP
///
/// Level-up formula: XP needed = level * 100.
///   Level 1 → 100 XP, Level 2 → 200 XP, Level 3 → 300 XP, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XpTracker {
    /// XP accumulated toward the current level.
    pub xp: u64,
    /// Current level (starts at 1).
    pub level: u32,
    /// Total XP earned across all levels.
    pub xp_total: u64,
    /// Save file path.
    #[serde(skip)]
    pub save_path: PathBuf,
    /// Epoch counter for periodic saves.
    #[serde(skip)]
    pub epoch_counter: u64,
}

impl XpTracker {
    // XP award constants
    pub const XP_HANDSHAKE: u64 = 100;
    pub const XP_DEAUTH: u64 = 10;
    pub const XP_ASSOCIATION: u64 = 15;
    pub const XP_NEW_AP: u64 = 5;

    /// How many epochs between periodic saves.
    pub const SAVE_INTERVAL: u64 = 5;

    /// Create a new XP tracker at level 1 with zero XP.
    pub fn new() -> Self {
        Self {
            xp: 0,
            level: 1,
            xp_total: 0,
            save_path: PathBuf::from(DEFAULT_XP_SAVE_PATH),
            epoch_counter: 0,
        }
    }

    /// Create a new XP tracker with a custom save path.
    pub fn with_save_path(path: impl Into<PathBuf>) -> Self {
        Self {
            save_path: path.into(),
            ..Self::new()
        }
    }

    /// XP needed to complete the given level.
    ///
    /// Exponential curve: level^1.3 * 5. Early levels are fast,
    /// high levels are a serious grind. ~1 year of daily use to reach 999.
    /// Lv1=5, Lv10=99, Lv100=1990, Lv500=16129, Lv999=39664.
    pub fn xp_needed_for_level(level: u32) -> u64 {
        ((level as f64).powf(1.3) * 5.0).max(1.0) as u64
    }

    /// Maximum level.
    pub const MAX_LEVEL: u32 = 999;

    /// XP needed to complete the current level.
    pub fn xp_to_next_level(&self) -> u64 {
        Self::xp_needed_for_level(self.level)
    }

    /// Award XP for an event. Returns true if a level-up occurred.
    ///
    /// Handles multiple level-ups from a single large award.
    pub fn award(&mut self, base_xp: u64) -> bool {
        self.xp_total = self.xp_total.saturating_add(base_xp);
        self.xp = self.xp.saturating_add(base_xp);

        let mut leveled = false;
        loop {
            if self.level >= Self::MAX_LEVEL {
                self.xp = 0; // capped
                break;
            }
            let needed = self.xp_to_next_level();
            if self.xp >= needed {
                self.xp -= needed;
                self.level += 1;
                leveled = true;
            } else {
                break;
            }
        }
        leveled
    }

    /// Display string in format: "Lv.22 (1224/2200)"
    /// Display string with visual progress bar matching Python exp plugin.
    /// Format: "Lv N  Exp|████░░░░" (filled + empty blocks)
    pub fn display_str(&self) -> String {
        let needed = self.xp_to_next_level();
        let bar_width = 10u64;
        let filled = if needed > 0 {
            (self.xp * bar_width / needed).min(bar_width)
        } else {
            bar_width
        };
        let empty = bar_width - filled;
        let bar: String = "\u{2588}".repeat(filled as usize) + &"\u{2591}".repeat(empty as usize);
        format!("Lv {}  Exp|{}", self.level, bar)
    }

    /// Should we save this epoch? (every SAVE_INTERVAL epochs)
    pub fn should_save(&self) -> bool {
        self.epoch_counter > 0 && self.epoch_counter % Self::SAVE_INTERVAL == 0
    }

    /// Increment epoch counter and award passive XP. Call once per epoch.
    /// The bull gains XP just by being active — +1 per epoch base.
    pub fn tick_epoch(&mut self) {
        self.epoch_counter += 1;
        self.award(1); // passive XP for scanning
    }

    /// Award XP for seeing APs this epoch. +1 per AP seen.
    pub fn award_aps(&mut self, ap_count: u32) {
        if ap_count > 0 {
            self.award(ap_count as u64);
        }
    }

    /// Award XP for capturing a handshake. +100 per handshake.
    pub fn award_handshake(&mut self) {
        self.award(100);
    }

    /// Save XP state to disk. Uses atomic write (write .tmp, rename).
    pub fn save(&self, mood_value: f32) -> Result<(), String> {
        self.save_to_path(&self.save_path, mood_value)
    }

    /// Save XP state to a specific path. Uses atomic write.
    pub fn save_to_path(&self, path: &Path, mood_value: f32) -> Result<(), String> {
        let data = XpSaveData {
            level: self.level,
            xp: self.xp,
            xp_total: self.xp_total,
            mood: mood_value,
        };

        let json =
            serde_json::to_string_pretty(&data).map_err(|e| format!("serialize failed: {e}"))?;

        let tmp_path = path.with_extension("json.tmp");

        std::fs::write(&tmp_path, &json)
            .map_err(|e| format!("write to {:?} failed: {e}", tmp_path))?;

        std::fs::rename(&tmp_path, path)
            .map_err(|e| format!("rename {:?} -> {:?} failed: {e}", tmp_path, path))?;

        Ok(())
    }

    /// Load XP state from disk. Returns (XpTracker, mood_value).
    ///
    /// If the file is missing, returns a fresh tracker.
    /// If the file is corrupted, logs a warning and returns a fresh tracker.
    pub fn load(path: &Path) -> (Self, f32) {
        Self::load_impl(path)
    }

    fn load_impl(path: &Path) -> (Self, f32) {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    warn!("XP load: could not read {:?}: {e}", path);
                }
                let mut t = Self::new();
                t.save_path = path.to_path_buf();
                return (t, 0.5);
            }
        };

        match serde_json::from_str::<XpSaveData>(&content) {
            Ok(data) => {
                let mut t = Self::new();
                t.level = data.level.max(1);
                t.xp = data.xp;
                t.xp_total = data.xp_total;
                t.save_path = path.to_path_buf();
                (t, data.mood.clamp(0.0, 1.0))
            }
            Err(e) => {
                warn!("XP load: corrupted file {:?}: {e} — starting fresh", path);
                let mut t = Self::new();
                t.save_path = path.to_path_buf();
                (t, 0.5)
            }
        }
    }
}

impl Default for XpTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// System info (Python: memtemp-plus.py → display/sysinfo.rs)
// ---------------------------------------------------------------------------

/// A snapshot of /proc/stat CPU counters for computing CPU usage %.
#[derive(Debug, Clone, Default)]
pub struct CpuSample {
    /// Total busy ticks (user + nice + system + irq + softirq + steal).
    pub busy: u64,
    /// Total ticks (busy + idle + iowait).
    pub total: u64,
}

impl CpuSample {
    /// Read current CPU counters from /proc/stat.
    pub fn read() -> Option<Self> {
        #[cfg(target_os = "linux")]
        {
            if let Ok(content) = std::fs::read_to_string("/proc/stat") {
                if let Some(cpu_line) = content.lines().next() {
                    let fields: Vec<u64> = cpu_line
                        .split_whitespace()
                        .skip(1) // skip "cpu"
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    if fields.len() >= 8 {
                        // user(0) nice(1) system(2) idle(3) iowait(4) irq(5) softirq(6) steal(7)
                        let busy =
                            fields[0] + fields[1] + fields[2] + fields[5] + fields[6] + fields[7];
                        let total = busy + fields[3] + fields[4];
                        return Some(Self { busy, total });
                    }
                }
            }
            None
        }

        #[cfg(not(target_os = "linux"))]
        None
    }

    /// Compute CPU usage percentage from the delta between two samples.
    pub fn cpu_percent(&self, prev: &CpuSample) -> f32 {
        let delta_busy = self.busy.saturating_sub(prev.busy);
        let delta_total = self.total.saturating_sub(prev.total);
        if delta_total == 0 {
            return 0.0;
        }
        (delta_busy as f32 / delta_total as f32) * 100.0
    }
}

/// System stats: CPU temperature, memory usage, CPU load.
#[derive(Debug, Clone, Default)]
pub struct SystemInfo {
    /// CPU temperature in degrees Celsius.
    pub cpu_temp_c: f32,
    /// Memory used in MB.
    pub mem_used_mb: u32,
    /// Total memory in MB.
    pub mem_total_mb: u32,
    /// CPU usage percentage (0-100).
    pub cpu_percent: f32,
}

impl SystemInfo {
    /// Read system info from /proc and /sys on Linux.
    /// Pass a previous CpuSample to compute CPU usage %; None on first call.
    pub fn read(prev_cpu: &Option<CpuSample>) -> (Self, Option<CpuSample>) {
        #[cfg(target_os = "linux")]
        {
            let cpu_temp_c = if let Ok(content) =
                std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp")
            {
                content.trim().parse::<f32>().unwrap_or(0.0) / 1000.0
            } else {
                0.0
            };

            let (mem_used_mb, mem_total_mb) =
                if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
                    let mut total_kb: u64 = 0;
                    let mut available_kb: u64 = 0;
                    for line in content.lines() {
                        if line.starts_with("MemTotal:") {
                            total_kb = line
                                .split_whitespace()
                                .nth(1)
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0);
                        } else if line.starts_with("MemAvailable:") {
                            available_kb = line
                                .split_whitespace()
                                .nth(1)
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0);
                        }
                    }
                    (
                        (total_kb.saturating_sub(available_kb) / 1024) as u32,
                        (total_kb / 1024) as u32,
                    )
                } else {
                    (0, 0)
                };

            let sample = CpuSample::read();
            let cpu_percent = match (&sample, prev_cpu) {
                (Some(curr), Some(prev)) => curr.cpu_percent(prev),
                _ => 0.0,
            };

            return (
                Self {
                    cpu_temp_c,
                    mem_used_mb,
                    mem_total_mb,
                    cpu_percent,
                },
                sample,
            );
        }

        #[cfg(not(target_os = "linux"))]
        (Self::default(), None)
    }

    /// Format for display: "CPU 45C MEM 42/512MB"
    pub fn display_str(&self) -> String {
        if self.mem_total_mb == 0 {
            return "SYS N/A".to_string();
        }
        format!(
            "CPU {:.0}C MEM {}/{}MB",
            self.cpu_temp_c, self.mem_used_mb, self.mem_total_mb
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // ====================================================================
    // Face tests
    // ====================================================================

    #[test]
    fn test_face_as_str_all_unique() {
        let faces: Vec<&str> = Face::all().iter().map(|f| f.as_str()).collect();
        for (i, a) in faces.iter().enumerate() {
            for (j, b) in faces.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a,
                        b,
                        "Faces {:?} and {:?} share text",
                        Face::all()[i],
                        Face::all()[j]
                    );
                }
            }
        }
    }

    #[test]
    fn test_face_all_count() {
        assert_eq!(Face::all().len(), 26);
    }

    #[test]
    fn test_face_serialize() {
        let face = Face::Happy;
        let json = serde_json::to_string(&face).unwrap();
        assert_eq!(json, "\"Happy\"");
        let back: Face = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Face::Happy);
    }

    // ====================================================================
    // Mood tests
    // ====================================================================

    #[test]
    fn test_mood_clamp() {
        let mood = Mood::new(1.5);
        assert_eq!(mood.value(), 1.0);

        let mood = Mood::new(-0.5);
        assert_eq!(mood.value(), 0.0);
    }

    #[test]
    fn test_mood_adjust() {
        let mut mood = Mood::new(0.5);
        mood.adjust(0.3);
        assert!((mood.value() - 0.8).abs() < 0.001);

        mood.adjust(-1.0);
        assert_eq!(mood.value(), 0.0);

        mood.adjust(2.0);
        assert_eq!(mood.value(), 1.0);
    }

    #[test]
    fn test_mood_face_mapping() {
        assert_eq!(Mood::new(0.95).face(), Face::Excited);
        assert_eq!(Mood::new(0.75).face(), Face::Happy);
        assert_eq!(Mood::new(0.55).face(), Face::Awake);
        assert_eq!(Mood::new(0.35).face(), Face::Bored);
        assert_eq!(Mood::new(0.15).face(), Face::Sad);
        assert_eq!(Mood::new(0.05).face(), Face::Demotivated);
    }

    #[test]
    fn test_mood_default() {
        let mood = Mood::default();
        assert_eq!(mood.value(), 0.5);
        assert_eq!(mood.face(), Face::Awake);
    }

    #[test]
    fn test_mood_status_messages() {
        for v in [0.0, 0.1, 0.3, 0.5, 0.7, 0.9, 1.0] {
            let msg = Mood::new(v).status_message();
            assert!(!msg.is_empty());
        }
    }

    #[test]
    fn test_mood_at_exact_zero() {
        let mood = Mood::new(0.0);
        assert_eq!(mood.value(), 0.0);
        assert_eq!(mood.face(), Face::Demotivated);
        assert_eq!(mood.status_message(), "The bull contemplates the void...");
    }

    #[test]
    fn test_mood_at_exact_one() {
        let mood = Mood::new(1.0);
        assert_eq!(mood.value(), 1.0);
        assert_eq!(mood.face(), Face::Excited);
        assert_eq!(mood.status_message(), "So many handshakes!");
    }

    #[test]
    fn test_mood_adjust_at_floor_stays_zero() {
        let mut mood = Mood::new(0.0);
        mood.adjust(-0.5);
        assert_eq!(mood.value(), 0.0);
    }

    #[test]
    fn test_mood_adjust_at_ceiling_stays_one() {
        let mut mood = Mood::new(1.0);
        mood.adjust(0.5);
        assert_eq!(mood.value(), 1.0);
    }

    #[test]
    fn test_mood_face_at_boundaries() {
        assert_eq!(Mood::new(0.9).face(), Face::Excited);
        assert_eq!(Mood::new(0.7).face(), Face::Happy);
        assert_eq!(Mood::new(0.5).face(), Face::Awake);
        assert_eq!(Mood::new(0.3).face(), Face::Bored);
        assert_eq!(Mood::new(0.1).face(), Face::Sad);
        assert_eq!(Mood::new(0.09).face(), Face::Demotivated);
    }

    // ====================================================================
    // Mood engine delta tests
    // ====================================================================

    #[test]
    fn test_mood_handshake_delta() {
        let mut mood = Mood::new(0.5);
        mood.adjust(mood_deltas::HANDSHAKE);
        assert!((mood.value() - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_mood_new_ap_delta() {
        let mut mood = Mood::new(0.5);
        mood.adjust(mood_deltas::NEW_AP);
        assert!((mood.value() - 0.52).abs() < 0.001);
    }

    #[test]
    fn test_mood_level_up_delta() {
        let mut mood = Mood::new(0.5);
        mood.adjust(mood_deltas::LEVEL_UP);
        assert!((mood.value() - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_mood_blind_epoch_delta() {
        let mut mood = Mood::new(0.5);
        mood.adjust(mood_deltas::BLIND_EPOCH);
        assert!((mood.value() - 0.48).abs() < 0.001);
    }

    #[test]
    fn test_mood_crash_delta() {
        let mut mood = Mood::new(0.5);
        mood.adjust(mood_deltas::CRASH);
        assert!((mood.value() - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_mood_idle_decay_delta() {
        let mut mood = Mood::new(0.5);
        mood.adjust(mood_deltas::IDLE_DECAY);
        assert!((mood.value() - 0.49).abs() < 0.001);
    }

    // ====================================================================
    // Personality tests
    // ====================================================================

    #[test]
    fn test_personality_handshake() {
        let mut p = Personality::new();
        let initial = p.mood.value();
        p.on_handshake();
        assert!(p.mood.value() > initial);
        assert_eq!(p.total_handshakes, 1);
        assert_eq!(p.blind_epochs, 0);
    }

    #[test]
    fn test_personality_handshake_awards_xp() {
        let mut p = Personality::new();
        p.on_handshake();
        assert_eq!(p.xp.xp_total, XpTracker::XP_HANDSHAKE);
        // 100 XP from level 1: multi-level-up to level 5, xp=33, xp_needed(5)=40
        assert_eq!(p.xp.xp, 33);
        assert_eq!(p.xp.level, 5);
    }

    #[test]
    fn test_personality_deauth_awards_xp() {
        let mut p = Personality::new();
        p.on_deauth();
        assert_eq!(p.xp.xp_total, XpTracker::XP_DEAUTH);
    }

    #[test]
    fn test_personality_association_awards_xp() {
        let mut p = Personality::new();
        p.on_association();
        assert_eq!(p.xp.xp_total, XpTracker::XP_ASSOCIATION);
    }

    #[test]
    fn test_personality_aps_seen_awards_xp() {
        let mut p = Personality::new();
        p.on_aps_seen(3);
        assert_eq!(p.xp.xp_total, XpTracker::XP_NEW_AP * 3);
    }

    #[test]
    fn test_personality_handshake_levelup_boosts_mood() {
        let mut p = Personality::new();
        // Level 1 needs 5 XP. Handshake = 100 XP, so this should level up (multiple times).
        let leveled = p.on_handshake();
        assert!(leveled);
        assert_eq!(p.xp.level, 5);
        // Mood should have handshake boost + level up boost
        // 0.5 + 0.1 (handshake) + 0.2 (level up) = 0.8
        assert!((p.mood.value() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_personality_blind_epochs() {
        let mut p = Personality::new();
        p.on_blind_epoch();
        assert_eq!(p.blind_epochs, 1);
        assert!(p.mood.value() < 0.5);
    }

    #[test]
    fn test_personality_blind_epoch_graduated_penalty() {
        let mut p = Personality::new();
        // First 3 epochs: mild penalty (-0.02 each)
        for _ in 0..3 {
            p.on_blind_epoch();
        }
        let after_3 = p.mood.value();
        // 0.5 - 3*0.02 = 0.44
        assert!((after_3 - 0.44).abs() < 0.01);

        // Epochs 4-10: moderate penalty (-0.05 each)
        p.on_blind_epoch(); // epoch 4
        let after_4 = p.mood.value();
        assert!(after_4 < after_3 - 0.03); // penalty > 0.02
    }

    #[test]
    fn test_personality_idle_decay() {
        let mut p = Personality::new();
        let initial = p.mood.value();
        p.on_idle_epoch();
        assert!((p.mood.value() - (initial + mood_deltas::IDLE_DECAY)).abs() < 0.001);
    }

    #[test]
    fn test_personality_crash_penalty() {
        let mut p = Personality::new();
        let initial = p.mood.value();
        p.on_crash();
        assert!((p.mood.value() - (initial + mood_deltas::CRASH)).abs() < 0.001);
    }

    #[test]
    fn test_personality_override() {
        let mut p = Personality::new();
        assert_eq!(p.current_face(), Face::Awake);
        p.set_override(Face::BatteryCritical);
        assert_eq!(p.current_face(), Face::BatteryCritical);
        p.clear_override();
        assert_eq!(p.current_face(), Face::Awake);
    }

    #[test]
    fn test_personality_aps_seen() {
        let mut p = Personality::new();
        p.on_aps_seen(5);
        assert_eq!(p.total_aps_seen, 5);
        assert!(p.mood.value() > 0.5);
    }

    #[test]
    fn test_personality_many_blind_epochs_floors_mood() {
        let mut p = Personality::new();
        for _ in 0..200 {
            p.on_blind_epoch();
        }
        assert_eq!(p.mood.value(), 0.0);
        assert_eq!(p.current_face(), Face::Demotivated);
    }

    #[test]
    fn test_personality_many_handshakes_caps_mood() {
        let mut p = Personality::new();
        for _ in 0..200 {
            p.on_handshake();
        }
        assert_eq!(p.mood.value(), 1.0);
        assert_eq!(p.current_face(), Face::Excited);
    }

    #[test]
    fn test_personality_reset_epoch_context() {
        let mut p = Personality::new();
        p.context.last_handshake_ssid = Some("TestSSID".into());
        p.context.wifi_recovered = true;
        p.context.level_up = true;
        p.context.scan_channels = vec![1, 6, 11];
        p.reset_epoch_context();
        assert!(p.context.last_handshake_ssid.is_none());
        assert!(!p.context.wifi_recovered);
        assert!(!p.context.level_up);
        assert!(p.context.scan_channels.is_empty());
    }

    // ====================================================================
    // XP Tracker tests
    // ====================================================================

    #[test]
    fn test_xp_tracker_new() {
        let xp = XpTracker::new();
        assert_eq!(xp.level, 1);
        assert_eq!(xp.xp, 0);
        assert_eq!(xp.xp_total, 0);
        assert_eq!(xp.xp_to_next_level(), 5);
    }

    #[test]
    fn test_xp_needed_formula() {
        // ((level as f64).powf(1.3) * 5.0).max(1.0) as u64
        assert_eq!(XpTracker::xp_needed_for_level(1), 5);
        assert_eq!(XpTracker::xp_needed_for_level(2), 12);
        assert_eq!(XpTracker::xp_needed_for_level(10), 99);
        assert_eq!(XpTracker::xp_needed_for_level(100), 1990);
        assert_eq!(XpTracker::xp_needed_for_level(999), 39664);
    }

    #[test]
    fn test_xp_award_no_levelup() {
        let mut xp = XpTracker::new();
        // Level 1 needs 5 XP; award 4 to stay at level 1
        let leveled = xp.award(4);
        assert!(!leveled);
        assert_eq!(xp.xp, 4);
        assert_eq!(xp.level, 1);
        assert_eq!(xp.xp_total, 4);
    }

    #[test]
    fn test_xp_award_exact_levelup() {
        let mut xp = XpTracker::new();
        // Level 1 needs 5 XP exactly
        let leveled = xp.award(5);
        assert!(leveled);
        assert_eq!(xp.level, 2);
        assert_eq!(xp.xp, 0);
        assert_eq!(xp.xp_total, 5);
        assert_eq!(xp.xp_to_next_level(), 12); // xp_needed(2) = 2^1.3 * 5 ≈ 12
    }

    #[test]
    fn test_xp_award_with_remainder() {
        let mut xp = XpTracker::new();
        // cumsum(2)=5; award 12 = 5 + 7, arrives at level 2 with 7 remaining
        xp.award(12);
        assert_eq!(xp.level, 2);
        assert_eq!(xp.xp, 7);
        assert_eq!(xp.xp_total, 12);
    }

    #[test]
    fn test_xp_award_multi_levelup() {
        let mut xp = XpTracker::new();
        // Level 1: 5, Level 2: 12 = 17 total (cumsum to L3) for level 3 with no remainder
        let leveled = xp.award(17);
        assert!(leveled);
        assert_eq!(xp.level, 3);
        assert_eq!(xp.xp, 0);
        assert_eq!(xp.xp_total, 17);
    }

    #[test]
    fn test_xp_award_multi_levelup_with_remainder() {
        let mut xp = XpTracker::new();
        // cumsum to L3=17; award 20 = 17 + 3, arrives at level 3 with 3 remaining
        // (level 3 needs 20, so 3 < 20 stays as remainder)
        xp.award(20);
        assert_eq!(xp.level, 3);
        assert_eq!(xp.xp, 3);
        assert_eq!(xp.xp_total, 20);
    }

    #[test]
    fn test_xp_award_zero() {
        let mut xp = XpTracker::new();
        let leveled = xp.award(0);
        assert!(!leveled);
        assert_eq!(xp.xp, 0);
    }

    #[test]
    fn test_xp_display_str() {
        let xp = XpTracker::new();
        let s = xp.display_str();
        assert!(s.starts_with("Lv 1  Exp|"), "got: {s}");
        assert!(s.contains('\u{2591}'), "should have empty blocks");
    }

    #[test]
    fn test_xp_display_str_after_xp() {
        let mut xp = XpTracker::new();
        // award(50) from level 1: lands at level 4, xp=13, xp_needed(4)=30
        // 13/30 * 10 = 4 filled blocks
        xp.award(50);
        let s = xp.display_str();
        assert!(s.starts_with("Lv 4  Exp|"), "got: {s}");
        assert_eq!(
            s.matches('\u{2588}').count(),
            4,
            "should have 4 filled blocks: {s}"
        );
    }

    #[test]
    fn test_xp_display_str_level_22() {
        let mut xp = XpTracker::new();
        // cumsum_to_reach_level(22)=2510; add 139 remainder (139/278 = 50% → 5 filled blocks)
        // xp_needed(22) = 22^1.3 * 5 = 278
        xp.award(2510 + 139);
        assert_eq!(xp.level, 22);
        assert_eq!(xp.xp, 139);
        let s = xp.display_str();
        assert!(s.starts_with("Lv 22  Exp|"), "got: {s}");
        // 139/278 = 50% → 5 filled blocks
        assert_eq!(s.matches('\u{2588}').count(), 5, "got: {s}");
    }

    #[test]
    fn test_xp_handshake_values() {
        assert_eq!(XpTracker::XP_HANDSHAKE, 100);
        assert_eq!(XpTracker::XP_DEAUTH, 10);
        assert_eq!(XpTracker::XP_ASSOCIATION, 15);
        assert_eq!(XpTracker::XP_NEW_AP, 5);
    }

    #[test]
    fn test_xp_multiple_levelups_via_handshakes() {
        let mut xp = XpTracker::new();
        for _ in 0..20 {
            xp.award(XpTracker::XP_HANDSHAKE);
        }
        assert!(xp.level > 1);
        assert_eq!(xp.xp_total, 2000);
    }

    #[test]
    fn test_xp_should_save_interval() {
        let mut xp = XpTracker::new();
        assert!(!xp.should_save()); // epoch 0
        for i in 1..=10 {
            xp.tick_epoch();
            if i % 5 == 0 {
                assert!(xp.should_save(), "should save at epoch {i}");
            } else {
                assert!(!xp.should_save(), "should not save at epoch {i}");
            }
        }
    }

    #[test]
    fn test_xp_epoch_counter() {
        let mut xp = XpTracker::new();
        assert_eq!(xp.epoch_counter, 0);
        xp.tick_epoch();
        assert_eq!(xp.epoch_counter, 1);
        xp.tick_epoch();
        assert_eq!(xp.epoch_counter, 2);
    }

    #[test]
    fn test_xp_overflow_protection() {
        let mut xp = XpTracker::new();
        xp.xp_total = u64::MAX - 10;
        xp.award(100);
        // Should saturate, not panic
        assert_eq!(xp.xp_total, u64::MAX);
    }

    #[test]
    fn test_xp_level_zero_impossible() {
        // Even if loaded with level 0, it should be clamped to 1
        let data = XpSaveData {
            level: 0,
            xp: 50,
            xp_total: 50,
            mood: 0.5,
        };
        let json = serde_json::to_string(&data).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_xp.json");
        std::fs::write(&path, &json).unwrap();
        let (tracker, _) = XpTracker::load(&path);
        assert_eq!(tracker.level, 1);
    }

    // ====================================================================
    // Save/Load roundtrip tests
    // ====================================================================

    #[test]
    fn test_xp_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exp_stats.json");

        let mut xp = XpTracker::with_save_path(&path);
        // award(12): cumsum(2)=5 + 7 remainder → level 2, xp=7
        xp.award(12);
        xp.save(0.7).unwrap();

        let (loaded, mood) = XpTracker::load(&path);
        assert_eq!(loaded.level, 2);
        assert_eq!(loaded.xp, 7);
        assert_eq!(loaded.xp_total, 12);
        assert!((mood - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_xp_load_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");

        let (tracker, mood) = XpTracker::load(&path);
        assert_eq!(tracker.level, 1);
        assert_eq!(tracker.xp, 0);
        assert!((mood - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_xp_load_corrupted_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupted.json");
        std::fs::write(&path, "not valid json {{{").unwrap();

        let (tracker, mood) = XpTracker::load(&path);
        assert_eq!(tracker.level, 1);
        assert_eq!(tracker.xp, 0);
        assert!((mood - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_xp_load_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.json");
        std::fs::write(&path, "").unwrap();

        let (tracker, mood) = XpTracker::load(&path);
        assert_eq!(tracker.level, 1);
        assert_eq!(tracker.xp, 0);
        assert!((mood - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_xp_load_partial_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.json");
        // Valid JSON but missing required fields
        std::fs::write(&path, r#"{"level": 5}"#).unwrap();

        let (tracker, mood) = XpTracker::load(&path);
        // Should start fresh because deserialization fails (missing fields)
        assert_eq!(tracker.level, 1);
        assert!((mood - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_xp_save_atomic_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atomic_test.json");

        let xp = XpTracker::with_save_path(&path);
        xp.save(0.6).unwrap();

        // The .tmp file should NOT exist after save
        let tmp_path = path.with_extension("json.tmp");
        assert!(!tmp_path.exists(), ".tmp file should be cleaned up");

        // The main file should exist and be valid JSON
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        let data: XpSaveData = serde_json::from_str(&content).unwrap();
        assert_eq!(data.level, 1);
    }

    #[test]
    fn test_xp_save_load_high_level() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("high_level.json");

        let mut xp = XpTracker::with_save_path(&path);
        // Award enough for level 50: cumsum_to_reach_level(50) = sum(xp_needed(1..49))
        let cumsum_50: u64 = (1u32..50).map(XpTracker::xp_needed_for_level).sum();
        xp.award(cumsum_50);
        assert_eq!(xp.level, 50);
        assert_eq!(xp.xp, 0);
        xp.save(0.9).unwrap();

        let (loaded, mood) = XpTracker::load(&path);
        assert_eq!(loaded.level, 50);
        assert_eq!(loaded.xp, 0);
        assert_eq!(loaded.xp_total, cumsum_50);
        assert!((mood - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_xp_load_clamps_mood() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clamp_mood.json");
        let data = XpSaveData {
            level: 1,
            xp: 0,
            xp_total: 0,
            mood: 5.0, // out of range
        };
        let json = serde_json::to_string(&data).unwrap();
        std::fs::write(&path, &json).unwrap();

        let (_, mood) = XpTracker::load(&path);
        assert_eq!(mood, 1.0);
    }

    #[test]
    fn test_xp_load_negative_mood() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("neg_mood.json");
        let data = XpSaveData {
            level: 1,
            xp: 0,
            xp_total: 0,
            mood: -2.0,
        };
        let json = serde_json::to_string(&data).unwrap();
        std::fs::write(&path, &json).unwrap();

        let (_, mood) = XpTracker::load(&path);
        assert_eq!(mood, 0.0);
    }

    // ====================================================================
    // Status message tests
    // ====================================================================

    #[test]
    fn test_status_battery_low() {
        let ctx = SystemContext {
            battery_low: true,
            battery_percent: Some(15),
            ..Default::default()
        };
        let msg = status_message(&ctx, &Mood::new(0.5));
        assert_eq!(msg, "Battery low: 15%");
    }

    #[test]
    fn test_status_wifi_recovered() {
        let ctx = SystemContext {
            wifi_recovered: true,
            ..Default::default()
        };
        let msg = status_message(&ctx, &Mood::new(0.5));
        assert_eq!(msg, "WiFi recovered!");
    }

    #[test]
    fn test_status_handshake_captured() {
        let ctx = SystemContext {
            last_handshake_ssid: Some("MyNetwork".into()),
            ..Default::default()
        };
        let msg = status_message(&ctx, &Mood::new(0.5));
        assert_eq!(msg, "Captured MyNetwork!");
    }

    #[test]
    fn test_status_level_up() {
        let ctx = SystemContext {
            level_up: true,
            level: 5,
            ..Default::default()
        };
        let msg = status_message(&ctx, &Mood::new(0.5));
        assert_eq!(msg, "Level up! Lv.5");
    }

    #[test]
    fn test_status_scanning() {
        let ctx = SystemContext {
            scan_channels: vec![1, 6, 11],
            ..Default::default()
        };
        let msg = status_message(&ctx, &Mood::new(0.5));
        assert_eq!(msg, "Scanning channels 1,6,11...");
    }

    #[test]
    fn test_status_idle_bored() {
        let ctx = SystemContext {
            blind_epochs: 10,
            ..Default::default()
        };
        let mood = Mood::new(0.1); // low mood
        let msg = status_message(&ctx, &mood);
        assert!(!msg.is_empty());
        // Should be one of the "bored" messages
    }

    #[test]
    fn test_status_idle_patient() {
        let ctx = SystemContext {
            blind_epochs: 10,
            ..Default::default()
        };
        let mood = Mood::new(0.5); // OK mood
        let msg = status_message(&ctx, &mood);
        assert!(!msg.is_empty());
    }

    #[test]
    fn test_status_default_mood() {
        let ctx = SystemContext::default();
        let mood = Mood::new(0.5);
        let msg = status_message(&ctx, &mood);
        assert_eq!(msg, "Scanning...");
    }

    #[test]
    fn test_status_priority_battery_over_handshake() {
        // Battery low should take priority over handshake capture
        let ctx = SystemContext {
            battery_low: true,
            battery_percent: Some(10),
            last_handshake_ssid: Some("TestNet".into()),
            ..Default::default()
        };
        let msg = status_message(&ctx, &Mood::new(0.5));
        assert!(
            msg.contains("Battery"),
            "battery should take priority, got: {msg}"
        );
    }

    #[test]
    fn test_status_priority_wifi_over_handshake() {
        let ctx = SystemContext {
            wifi_recovered: true,
            last_handshake_ssid: Some("TestNet".into()),
            ..Default::default()
        };
        let msg = status_message(&ctx, &Mood::new(0.5));
        assert_eq!(msg, "WiFi recovered!");
    }

    #[test]
    fn test_status_idle_cycles_messages() {
        // Different blind_epochs values should cycle through messages
        let mood = Mood::new(0.5);
        let mut messages = Vec::new();
        for i in 10..14 {
            let ctx = SystemContext {
                blind_epochs: i,
                ..Default::default()
            };
            messages.push(status_message(&ctx, &mood));
        }
        // Should have at least 2 different messages in 4 tries
        messages.sort();
        messages.dedup();
        assert!(
            messages.len() >= 2,
            "idle messages should vary, got: {:?}",
            messages
        );
    }

    #[test]
    fn test_personality_status_msg() {
        let p = Personality::new();
        let msg = p.status_msg();
        assert!(!msg.is_empty());
    }

    #[test]
    fn test_personality_status_msg_with_context() {
        let mut p = Personality::new();
        p.context.last_handshake_ssid = Some("CapturedNet".into());
        let msg = p.status_msg();
        assert_eq!(msg, "Captured CapturedNet!");
    }

    // ====================================================================
    // SystemInfo tests
    // ====================================================================

    #[test]
    fn test_system_info_default() {
        let si = SystemInfo::default();
        assert_eq!(si.cpu_temp_c, 0.0);
        assert_eq!(si.display_str(), "SYS N/A");
    }

    #[test]
    fn test_system_info_display_str() {
        let si = SystemInfo {
            cpu_temp_c: 45.0,
            mem_used_mb: 42,
            mem_total_mb: 512,
            cpu_percent: 12.0,
        };
        assert_eq!(si.display_str(), "CPU 45C MEM 42/512MB");
    }

    // ====================================================================
    // Integration tests
    // ====================================================================

    #[test]
    fn test_personality_full_session_simulation() {
        let mut p = Personality::new();

        // Boot: start scanning
        p.context.scan_channels = vec![1, 6, 11];
        assert!(p.status_msg().contains("Scanning"));

        p.reset_epoch_context();

        // Epoch 1: see some APs
        p.on_aps_seen(5);
        assert!(p.mood.value() > 0.5);

        // Epoch 2: handshake!
        p.context.last_handshake_ssid = Some("CoffeeShop".into());
        let leveled = p.on_handshake();
        assert!(leveled); // 100 XP = level up (multiple times with new formula)
        assert_eq!(p.xp.level, 6);
        assert!(p.status_msg().contains("Battery") || p.status_msg().contains("Captured"));

        p.reset_epoch_context();

        // Blind epochs: need enough to push mood below 0.5.
        // With the new formula, on_aps_seen + handshake can drive mood to 1.0,
        // so use 15 blind epochs (well into the -0.08/epoch tier) to reliably get below 0.5.
        for _ in 0..15 {
            p.on_blind_epoch();
        }
        assert!(p.mood.value() < 0.5);

        // Crash
        p.on_crash();
        let mood_after_crash = p.mood.value();

        // Recovery
        p.context.wifi_recovered = true;
        assert_eq!(p.status_msg(), "WiFi recovered!");

        p.reset_epoch_context();

        // Another handshake to bring mood back up
        p.on_handshake();
        assert!(p.mood.value() > mood_after_crash);
    }

    #[test]
    fn test_xp_save_load_after_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");

        let mut p = Personality::new();
        p.xp.save_path = path.clone();

        // Simulate some activity
        p.on_handshake(); // 100 XP → level 2
        p.on_deauth(); // 10 XP
        p.on_association(); // 15 XP
        p.on_aps_seen(3); // 15 XP

        // Save
        p.xp.save(p.mood.value()).unwrap();

        // Load into fresh tracker
        let (loaded, mood) = XpTracker::load(&path);
        assert_eq!(loaded.level, p.xp.level);
        assert_eq!(loaded.xp, p.xp.xp);
        assert_eq!(loaded.xp_total, p.xp.xp_total);
        assert!((mood - p.mood.value()).abs() < 0.01);
    }

    #[test]
    fn test_xp_cumulative_total_across_levels() {
        let mut xp = XpTracker::new();
        // Award XP in small chunks across multiple levels
        let mut total_awarded: u64 = 0;
        for _ in 0..50 {
            xp.award(XpTracker::XP_HANDSHAKE);
            total_awarded += XpTracker::XP_HANDSHAKE;
        }
        assert_eq!(xp.xp_total, total_awarded);
        assert!(xp.level > 1);
        // Verify xp + sum of all completed levels = xp_total
        let mut completed_sum: u64 = 0;
        for lv in 1..xp.level {
            completed_sum += XpTracker::xp_needed_for_level(lv);
        }
        assert_eq!(completed_sum + xp.xp, xp.xp_total);
    }

    #[test]
    fn test_xp_max_level_boundary() {
        let mut xp = XpTracker::new();
        // Push to max level: cumsum_to_reach_level(999) = sum(xp_needed(1..998))
        let cumsum_999: u64 = (1u32..999).map(XpTracker::xp_needed_for_level).sum();
        xp.award(cumsum_999);
        assert_eq!(xp.level, 999);
        assert_eq!(xp.xp, 0);
        // Awarding more XP at MAX_LEVEL (999) is silently discarded (xp stays 0)
        xp.award(100_000);
        assert_eq!(xp.level, 999);
        assert_eq!(xp.xp, 0);
    }

    #[test]
    fn test_xp_concurrent_save_doesnt_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("concurrent.json");

        let mut xp = XpTracker::with_save_path(&path);
        // Save multiple times rapidly
        for _ in 0..10 {
            xp.award(50);
            xp.save(0.5).unwrap();
        }

        // Final load should be valid
        let (loaded, _) = XpTracker::load(&path);
        assert_eq!(loaded.xp_total, 500);
    }

    #[test]
    fn test_xp_with_save_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom.json");
        let xp = XpTracker::with_save_path(&path);
        assert_eq!(xp.save_path, path);
        assert_eq!(xp.level, 1);
    }

    // ====================================================================
    // SystemInfo tests
    // ====================================================================

    #[test]
    fn test_system_info_read_returns_struct() {
        let (info, _sample) = SystemInfo::read(&None);
        // On non-Linux, returns zeros (acceptable)
        // Just verify it doesn't panic and returns valid struct
        assert!(info.cpu_temp_c >= 0.0);
        assert!(info.cpu_percent >= 0.0);
    }

    #[test]
    fn test_system_info_display_str_no_data() {
        let info = SystemInfo::default();
        assert_eq!(info.display_str(), "SYS N/A");
    }

    // ====================================================================
    // CpuSample tests
    // ====================================================================

    #[test]
    fn test_cpu_sample_percent() {
        let prev = CpuSample {
            busy: 100,
            total: 200,
        };
        let curr = CpuSample {
            busy: 150,
            total: 300,
        };
        let pct = curr.cpu_percent(&prev);
        assert!((pct - 50.0).abs() < 0.01); // 50/100 = 50%
    }

    #[test]
    fn test_cpu_sample_percent_zero_delta() {
        let prev = CpuSample {
            busy: 100,
            total: 200,
        };
        let curr = CpuSample {
            busy: 100,
            total: 200,
        };
        assert_eq!(curr.cpu_percent(&prev), 0.0);
    }

    #[test]
    fn test_cpu_sample_read_on_platform() {
        let sample = CpuSample::read();
        // On Linux: Some with real values. On non-Linux: None.
        #[cfg(target_os = "linux")]
        {
            assert!(sample.is_some());
            let s = sample.unwrap();
            assert!(s.total > 0);
        }
        #[cfg(not(target_os = "linux"))]
        assert!(sample.is_none());
    }

    // ====================================================================
    // RF environment mood/face tests
    // ====================================================================

    #[test]
    fn test_rf_busy_mood() {
        let mut p = Personality::new();
        let initial = p.mood.value();
        let rf = crate::qpu::rf::RfEnvironment {
            total_frames: 200,
            ..Default::default()
        };
        p.apply_rf_environment(&rf);
        assert!((p.mood.value() - (initial + mood_deltas::RF_BUSY)).abs() < 0.001);
    }

    #[test]
    fn test_rf_quiet_mood() {
        let mut p = Personality::new();
        let initial = p.mood.value();
        let rf = crate::qpu::rf::RfEnvironment::default();
        p.apply_rf_environment(&rf);
        assert!((p.mood.value() - (initial + mood_deltas::RF_QUIET)).abs() < 0.001);
    }

    #[test]
    fn test_rf_deauth_storm_face() {
        let mut p = Personality::new();
        let rf = crate::qpu::rf::RfEnvironment {
            deauth_rate: 15.0,
            total_frames: 200,
            ..Default::default()
        };
        p.apply_rf_environment(&rf);
        assert_eq!(p.override_face, Some(Face::Raging));
    }

    #[test]
    fn test_rf_rich_environment_face() {
        let mut p = Personality::new();
        let rf = crate::qpu::rf::RfEnvironment {
            unique_bssids: 25,
            total_frames: 200,
            ..Default::default()
        };
        p.apply_rf_environment(&rf);
        assert_eq!(p.override_face, Some(Face::Excited));
    }

    #[test]
    fn test_joke_mood_boost() {
        let mut p = Personality::new();
        p.mood = Mood { value: 0.2 };
        let initial = p.mood.value();
        p.mood.adjust(mood_deltas::JOKE);
        assert!((p.mood.value() - (initial + mood_deltas::JOKE)).abs() < 0.001);
    }

    #[test]
    fn test_face_transition_picks_new_message() {
        let mut p = Personality::new();
        p.mood = Mood { value: 0.5 }; // awake face
        p.generate_status();
        assert!(!p.current_status.is_empty(), "should have a message after generate");

        // Change mood to trigger face change (sad)
        p.mood = Mood { value: 0.15 };
        p.generate_status();
        assert!(
            !p.current_status.is_empty(),
            "face transition should pick new message, not clear to empty"
        );
        // Should be from sad face pool, not the mood fallback
        let sad_msgs = messages::messages_for_face("sad");
        let jokes = jokes::jokes_for_face("sad");
        let is_sad_msg = sad_msgs.iter().any(|m| *m == p.current_status);
        let is_joke = jokes.iter().any(|j| j.0 == p.current_status || j.1 == p.current_status);
        assert!(
            is_sad_msg || is_joke,
            "message '{}' should be from sad face pool or jokes",
            p.current_status
        );
    }

    #[test]
    fn test_joke_rate_higher_at_low_mood() {
        // Statistical test: run 1000 iterations at low mood, count jokes
        let mut low_mood_jokes = 0;
        let mut high_mood_jokes = 0;

        for _ in 0..1000 {
            let mut p = Personality::new();
            p.mood = Mood { value: 0.15 }; // below 0.3
            p.joke_face = "sad".to_string();
            p.status_display_epochs = 3; // force new pick
            p.generate_status();
            if p.joke_index.is_some() {
                low_mood_jokes += 1;
            }
        }
        for _ in 0..1000 {
            let mut p = Personality::new();
            p.mood = Mood { value: 0.6 }; // above 0.3
            p.joke_face = "awake".to_string();
            p.status_display_epochs = 3;
            p.generate_status();
            if p.joke_index.is_some() {
                high_mood_jokes += 1;
            }
        }
        // Low mood should have significantly more jokes (45% vs 30%)
        assert!(
            low_mood_jokes > high_mood_jokes + 50,
            "low mood should produce more jokes: low={}, high={}",
            low_mood_jokes,
            high_mood_jokes
        );
    }

    #[test]
    fn test_joke_selection_boosts_mood() {
        // Run until a joke is selected (may take a few tries due to randomness)
        for _ in 0..100 {
            let mut p = Personality::new();
            p.mood = Mood { value: 0.15 };
            p.joke_face = "sad".to_string();
            p.status_display_epochs = 3;
            let before = p.mood.value();
            p.generate_status();
            if p.joke_index.is_some() {
                // Joke was selected — mood should have increased
                assert!(
                    p.mood.value() > before,
                    "joke should boost mood: before={}, after={}",
                    before,
                    p.mood.value()
                );
                assert!(
                    (p.mood.value() - (before + mood_deltas::JOKE)).abs() < 0.001,
                    "joke boost should be exactly JOKE constant"
                );
                return;
            }
        }
        panic!("no joke selected in 100 tries — probability too low");
    }
}
