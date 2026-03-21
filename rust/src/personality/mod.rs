use serde::{Deserialize, Serialize};

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
        ]
    }
}

/// Mood value ranging from 0.0 (miserable) to 1.0 (ecstatic).
#[derive(Debug, Clone)]
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
            _ => "...",
        }
    }
}

impl Default for Mood {
    fn default() -> Self {
        Self::new(0.5)
    }
}

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
        }
    }

    /// Get the face to display, considering overrides.
    pub fn current_face(&self) -> Face {
        self.override_face.unwrap_or_else(|| self.mood.face())
    }

    /// Called when a handshake is captured.
    pub fn on_handshake(&mut self) {
        self.total_handshakes += 1;
        self.blind_epochs = 0;
        self.mood.adjust(0.1);
    }

    /// Called when APs are seen in an epoch.
    pub fn on_aps_seen(&mut self, count: u32) {
        self.total_aps_seen += count;
        if count > 0 {
            self.mood.adjust(0.02);
        }
    }

    /// Called at the end of a blind epoch (no handshakes).
    pub fn on_blind_epoch(&mut self) {
        self.blind_epochs += 1;
        let penalty = match self.blind_epochs {
            1..=3 => -0.02,
            4..=10 => -0.05,
            _ => -0.08,
        };
        self.mood.adjust(penalty);
    }

    /// Set an override face (e.g., for hardware warnings).
    pub fn set_override(&mut self, face: Face) {
        self.override_face = Some(face);
    }

    /// Clear any face override.
    pub fn clear_override(&mut self) {
        self.override_face = None;
    }
}

impl Default for Personality {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// XP / Leveling system (Python: exp.py → personality/xp.rs)
// ---------------------------------------------------------------------------

/// Experience point tracker and leveling system.
#[derive(Debug, Clone)]
pub struct XpTracker {
    /// Total XP earned this session.
    pub xp: u64,
    /// Current level.
    pub level: u32,
    /// XP needed to reach the next level.
    pub xp_to_next_level: u64,
    /// XP multiplier (bonus from streaks, etc.).
    pub multiplier: f32,
}

impl XpTracker {
    /// Create a new XP tracker at level 1 with zero XP.
    pub fn new() -> Self {
        Self {
            xp: 0,
            level: 1,
            xp_to_next_level: 100,
            multiplier: 1.0,
        }
    }

    /// Award XP for an event. Returns true if a level-up occurred.
    pub fn award(&mut self, base_xp: u64) -> bool {
        let earned = (base_xp as f32 * self.multiplier) as u64;
        self.xp += earned;
        if self.xp >= self.xp_to_next_level {
            self.level += 1;
            self.xp -= self.xp_to_next_level;
            // XP curve: each level needs 20% more XP
            self.xp_to_next_level = (self.xp_to_next_level as f32 * 1.2) as u64;
            true
        } else {
            false
        }
    }

    /// XP awarded for various events.
    pub fn xp_for_handshake() -> u64 {
        50
    }
    pub fn xp_for_new_ap() -> u64 {
        10
    }
    pub fn xp_for_epoch() -> u64 {
        1
    }

    /// Display string: "LVL 3 (75/120 XP)"
    pub fn display_str(&self) -> String {
        format!("LVL {} ({}/{} XP)", self.level, self.xp, self.xp_to_next_level)
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
    ///
    /// TODO: Parse /sys/class/thermal/thermal_zone0/temp for CPU temp,
    /// /proc/meminfo for memory, /proc/stat for CPU usage.
    pub fn read() -> Self {
        // Stub: on non-Linux platforms, return zeros
        Self::default()
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

    #[test]
    fn test_face_as_str_all_unique() {
        let faces: Vec<&str> = Face::all().iter().map(|f| f.as_str()).collect();
        for (i, a) in faces.iter().enumerate() {
            for (j, b) in faces.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "Faces {:?} and {:?} share text", Face::all()[i], Face::all()[j]);
                }
            }
        }
    }

    #[test]
    fn test_face_all_count() {
        assert_eq!(Face::all().len(), 24);
    }

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
    fn test_personality_handshake() {
        let mut p = Personality::new();
        let initial = p.mood.value();
        p.on_handshake();
        assert!(p.mood.value() > initial);
        assert_eq!(p.total_handshakes, 1);
        assert_eq!(p.blind_epochs, 0);
    }

    #[test]
    fn test_personality_blind_epochs() {
        let mut p = Personality::new();
        p.on_blind_epoch();
        assert_eq!(p.blind_epochs, 1);
        assert!(p.mood.value() < 0.5);
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
    fn test_mood_status_messages() {
        // Just verify they're non-empty and don't panic
        for v in [0.0, 0.1, 0.3, 0.5, 0.7, 0.9, 1.0] {
            let msg = Mood::new(v).status_message();
            assert!(!msg.is_empty());
        }
    }

    #[test]
    fn test_face_serialize() {
        let face = Face::Happy;
        let json = serde_json::to_string(&face).unwrap();
        assert_eq!(json, "\"Happy\"");
        let back: Face = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Face::Happy);
    }

    // ---- XP Tracker tests ----

    #[test]
    fn test_xp_tracker_new() {
        let xp = XpTracker::new();
        assert_eq!(xp.level, 1);
        assert_eq!(xp.xp, 0);
        assert_eq!(xp.xp_to_next_level, 100);
    }

    #[test]
    fn test_xp_award_no_levelup() {
        let mut xp = XpTracker::new();
        let leveled = xp.award(50);
        assert!(!leveled);
        assert_eq!(xp.xp, 50);
        assert_eq!(xp.level, 1);
    }

    #[test]
    fn test_xp_award_levelup() {
        let mut xp = XpTracker::new();
        let leveled = xp.award(100);
        assert!(leveled);
        assert_eq!(xp.level, 2);
        assert_eq!(xp.xp, 0);
        // Next level requires 20% more: 120
        assert_eq!(xp.xp_to_next_level, 120);
    }

    #[test]
    fn test_xp_display_str() {
        let xp = XpTracker::new();
        assert_eq!(xp.display_str(), "LVL 1 (0/100 XP)");
    }

    #[test]
    fn test_xp_multiplier() {
        let mut xp = XpTracker::new();
        xp.multiplier = 2.0;
        xp.award(30); // 30 * 2.0 = 60
        assert_eq!(xp.xp, 60);
    }

    // ---- SystemInfo tests ----

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

    // ---- Mood boundary tests ----

    #[test]
    fn test_mood_at_exact_zero() {
        let mood = Mood::new(0.0);
        assert_eq!(mood.value(), 0.0);
        assert_eq!(mood.face(), Face::Demotivated);
        assert_eq!(mood.status_message(), "...");
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
        mood.adjust(-0.5); // Can't go below 0
        assert_eq!(mood.value(), 0.0);
    }

    #[test]
    fn test_mood_adjust_at_ceiling_stays_one() {
        let mut mood = Mood::new(1.0);
        mood.adjust(0.5); // Can't go above 1
        assert_eq!(mood.value(), 1.0);
    }

    #[test]
    fn test_mood_face_at_boundaries() {
        // Exactly on each threshold boundary
        assert_eq!(Mood::new(0.9).face(), Face::Excited);
        assert_eq!(Mood::new(0.7).face(), Face::Happy);
        assert_eq!(Mood::new(0.5).face(), Face::Awake);
        assert_eq!(Mood::new(0.3).face(), Face::Bored);
        assert_eq!(Mood::new(0.1).face(), Face::Sad);
        // Just below lowest threshold
        assert_eq!(Mood::new(0.09).face(), Face::Demotivated);
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
    fn test_xp_award_zero() {
        let mut xp = XpTracker::new();
        let leveled = xp.award(0);
        assert!(!leveled);
        assert_eq!(xp.xp, 0);
    }

    #[test]
    fn test_xp_multiple_levelups() {
        let mut xp = XpTracker::new();
        // Award enough to skip multiple levels
        for _ in 0..20 {
            xp.award(XpTracker::xp_for_handshake());
        }
        assert!(xp.level > 1);
    }
}
