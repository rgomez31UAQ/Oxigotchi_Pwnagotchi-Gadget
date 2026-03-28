//! Face variety engine — 8 features ported from Python angryoxide.py.
//!
//! Priority order (first match wins):
//! 7. Debug face on boot (1 epoch)
//! 1. Achievement milestones (1/10/25/50/100 captures, each with unique face)
//! 1c. Level-up (every 10 captures, not at milestone counts)
//! 2. Capture variety (random face on new capture this epoch)
//! 5. Friend detection (when peer is found)
//! 6. Upload face (when uploading captures)
//! 3. Time-of-day (2-5am=sleep, 6-8am=motivated once, 22-1am=cool)
//! 8. Random rare face (12% per epoch — breaks up idle loops)
//! 4. Idle rotation (modulo 25 cycle: bored→lonely→demotivated→angry→sad)
//! Default: awake

use rand::Rng;

/// Milestone capture counts → (count, face, status_text).
/// Each milestone has its own face — matches Python exactly.
const MILESTONES: &[(u32, &str, &str)] = &[
    (1, "excited", "First capture! The bull charges!"),
    (10, "cool", "10 captures! Stampede!"),
    (25, "intense", "25 captures! Herd leader!"),
    (50, "smart", "50 captures! Legendary bull!"),
    (100, "grateful", "100 captures! Bull God!"),
];

/// Capture face variety pool — random choice on capture.
const CAPTURE_FACES: &[&str] = &["happy", "cool", "grateful", "excited"];

/// Rare face pool — 5% chance per epoch.
const RARE_FACES: &[&str] = &["cool", "intense", "smart", "grateful", "motivated"];

/// Face variety state machine.
#[derive(Debug)]
pub struct FaceVariety {
    // -- Milestone state --
    /// Epochs remaining to show a milestone face.
    pub milestone_epochs_left: u32,
    /// Face to show during milestone.
    pub milestone_face: Option<&'static str>,
    /// Status to show during milestone.
    pub milestone_status: Option<&'static str>,
    /// Last milestone capture count triggered.
    pub last_milestone: u32,

    // -- Idle state --
    /// Consecutive epochs since last capture.
    pub idle_epochs: u32,

    // -- Debug on boot --
    /// Whether debug face was shown at boot.
    pub debug_shown: bool,
    /// Epochs remaining for debug face.
    pub debug_epochs_left: u32,

    // -- Friend/Upload countdowns --
    /// Friend detection countdown epochs.
    pub friend_epochs_left: u32,
    /// Upload face countdown epochs.
    pub upload_epochs_left: u32,

    // -- Capture variety --
    /// Whether a capture happened this epoch (set externally).
    pub captures_this_epoch: u32,
    /// Face chosen for capture variety (random per capture).
    pub capture_face: Option<&'static str>,

    // -- Time-of-day --
    /// Current hour of day (0-23, set from main loop).
    pub current_hour: u32,
    /// Whether morning greeting was shown (once per boot).
    pub morning_greeted: bool,

    // -- Rare face --
    /// Rare face for this epoch (rolled in tick_countdowns).
    pub rare_face: Option<&'static str>,
}

impl FaceVariety {
    pub fn new() -> Self {
        Self {
            milestone_epochs_left: 0,
            milestone_face: None,
            milestone_status: None,
            last_milestone: 0,
            idle_epochs: 0,
            debug_shown: false,
            debug_epochs_left: 0,
            friend_epochs_left: 0,
            upload_epochs_left: 0,
            captures_this_epoch: 0,
            capture_face: None,
            current_hour: 12,
            morning_greeted: false,
            rare_face: None,
        }
    }

    /// Called when a capture occurs. Checks for milestones and level-ups.
    /// Returns Some((face, status)) if milestone or level-up hit.
    pub fn on_capture(&mut self, total_captures: u32) -> Option<(&'static str, &'static str)> {
        self.idle_epochs = 0;
        self.captures_this_epoch += 1;

        // Pick random capture variety face
        let mut rng = rand::thread_rng();
        let idx = rng.gen_range(0..CAPTURE_FACES.len());
        self.capture_face = Some(CAPTURE_FACES[idx]);

        // Check milestones (1, 10, 25, 50, 100)
        for &(count, face, status) in MILESTONES {
            if total_captures == count && self.last_milestone < count {
                self.last_milestone = count;
                self.milestone_epochs_left = 2;
                self.milestone_face = Some(face);
                self.milestone_status = Some(status);
                return Some((face, status));
            }
        }

        // Level-up: every 10 captures, not at milestone counts
        let level = total_captures / 10;
        let prev_level = if self.last_milestone > 0 {
            self.last_milestone / 10
        } else {
            0
        };
        if level > prev_level && !matches!(total_captures, 10 | 25 | 50 | 100) {
            self.last_milestone = total_captures;
            self.milestone_epochs_left = 2;
            self.milestone_face = Some("motivated");
            self.milestone_status = Some("Level up! The bull grows stronger!");
            return Some(("motivated", "Level up! The bull grows stronger!"));
        }

        None
    }

    /// Increment idle epoch counter.
    pub fn tick_idle(&mut self) {
        self.idle_epochs += 1;
    }

    /// Get the idle face based on epochs since last capture.
    /// Uses modulo 25 cycle: 1-5=bored, 6-10=lonely,
    /// 11-15=demotivated, 16-20=angry, 21-25=sad.
    /// Faster cycling keeps the face feeling alive.
    pub fn idle_face(&self) -> Option<&'static str> {
        if self.idle_epochs == 0 {
            return None;
        }
        let cycle = self.idle_epochs % 25;
        match cycle {
            0..=5 => Some("bored"),
            6..=10 => Some("lonely"),
            11..=15 => Some("demotivated"),
            16..=20 => Some("angry"),
            _ => Some("sad"),
        }
    }

    /// Get the boot face (debug face shown once on startup).
    pub fn boot_face(&mut self) -> &'static str {
        if !self.debug_shown {
            self.debug_shown = true;
            self.debug_epochs_left = 1;
            return "debug";
        }
        "awake"
    }

    /// Roll for a random rare face this epoch (12% chance).
    fn rare_face_roll(&self) -> Option<&'static str> {
        let mut rng = rand::thread_rng();
        if rng.r#gen::<f32>() < 0.12 {
            let idx = rng.gen_range(0..RARE_FACES.len());
            Some(RARE_FACES[idx])
        } else {
            None
        }
    }

    /// Tick down all active countdowns. Call once per epoch.
    /// Also rolls for rare face and resets per-epoch state.
    pub fn tick_countdowns(&mut self) {
        // Roll for rare face this epoch
        self.rare_face = self.rare_face_roll();

        // Reset per-epoch capture state
        self.captures_this_epoch = 0;
        self.capture_face = None;

        if self.milestone_epochs_left > 0 {
            self.milestone_epochs_left -= 1;
            if self.milestone_epochs_left == 0 {
                self.milestone_face = None;
                self.milestone_status = None;
            }
        }
        if self.debug_epochs_left > 0 {
            self.debug_epochs_left -= 1;
        }
        if self.friend_epochs_left > 0 {
            self.friend_epochs_left -= 1;
        }
        if self.upload_epochs_left > 0 {
            self.upload_epochs_left -= 1;
        }
    }

    /// Trigger friend face for N epochs.
    pub fn on_friend_detected(&mut self, epochs: u32) {
        self.friend_epochs_left = epochs;
    }

    /// Trigger upload face for N epochs.
    pub fn on_upload(&mut self, epochs: u32) {
        self.upload_epochs_left = epochs;
    }

    /// Get the current face override from the variety engine.
    /// Matches Python priority order exactly (first match wins).
    pub fn current_override(&self) -> Option<&'static str> {
        // 7. Debug on boot
        if self.debug_epochs_left > 0 {
            return Some("debug");
        }
        // 1a. Active milestone display
        if self.milestone_epochs_left > 0 {
            return self.milestone_face;
        }
        // 2. Capture variety (new capture this epoch)
        if self.captures_this_epoch > 0 {
            return self.capture_face;
        }
        // 5. Friend detected
        if self.friend_epochs_left > 0 {
            return Some("friend");
        }
        // 6. Upload in progress
        if self.upload_epochs_left > 0 {
            return Some("upload");
        }
        // 3. Time-of-day faces
        if let Some(tod) = self.time_of_day_face() {
            return Some(tod);
        }
        // 8. Random rare face — checked before idle so it can break up dry-spell loops
        if let Some(rare) = self.rare_face {
            return Some(rare);
        }
        // 4. Idle rotation
        if let Some(idle) = self.idle_face() {
            return Some(idle);
        }
        // Default: no override (caller uses "awake")
        None
    }

    /// Time-of-day face. Matches Python exactly:
    /// 2-5am = sleep (if no captures), 6-8am = motivated (once per boot),
    /// 22pm-1am = cool.
    fn time_of_day_face(&self) -> Option<&'static str> {
        match self.current_hour {
            2..=5 if self.captures_this_epoch == 0 => Some("sleep"),
            6..=8 if !self.morning_greeted => Some("motivated"),
            22..=23 | 0..=1 => Some("cool"),
            _ => None,
        }
    }
}

impl Default for FaceVariety {
    fn default() -> Self {
        Self::new()
    }
}

/// Standalone time-of-day check (for external callers).
pub fn time_of_day_face(hour: u32) -> Option<&'static str> {
    match hour {
        2..=5 => Some("sleep"),
        6..=8 => Some("motivated"),
        22..=23 | 0..=1 => Some("cool"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_milestone_1st_capture() {
        let mut fv = FaceVariety::new();
        let result = fv.on_capture(1);
        assert!(result.is_some(), "1st capture should trigger milestone");
        let (face, _) = result.unwrap();
        assert_eq!(face, "excited", "1st capture = excited face");
    }

    #[test]
    fn test_milestone_10th_capture() {
        let mut fv = FaceVariety::new();
        fv.on_capture(1); // trigger milestone 1
        fv.last_milestone = 1;
        let result = fv.on_capture(10);
        assert!(result.is_some());
        let (face, _) = result.unwrap();
        assert_eq!(face, "cool", "10th capture = cool face");
    }

    #[test]
    fn test_milestone_25th_capture() {
        let mut fv = FaceVariety::new();
        fv.last_milestone = 10;
        let result = fv.on_capture(25);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "intense");
    }

    #[test]
    fn test_milestone_50th_capture() {
        let mut fv = FaceVariety::new();
        fv.last_milestone = 25;
        let result = fv.on_capture(50);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "smart");
    }

    #[test]
    fn test_milestone_100th_capture() {
        let mut fv = FaceVariety::new();
        fv.last_milestone = 50;
        let result = fv.on_capture(100);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "grateful");
    }

    #[test]
    fn test_level_up_at_20() {
        let mut fv = FaceVariety::new();
        fv.last_milestone = 10; // milestone 10 already hit
        let result = fv.on_capture(20);
        assert!(result.is_some(), "20 captures should trigger level-up");
        assert_eq!(result.unwrap().0, "motivated");
    }

    #[test]
    fn test_no_level_up_at_milestone_counts() {
        let mut fv = FaceVariety::new();
        fv.last_milestone = 0;
        // 10 is a milestone, not a level-up
        let result = fv.on_capture(10);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "cool"); // milestone face, not motivated
    }

    #[test]
    fn test_non_milestone_capture() {
        let mut fv = FaceVariety::new();
        fv.last_milestone = 1;
        let result = fv.on_capture(7);
        assert!(result.is_none(), "7th capture is not a milestone");
        assert!(fv.capture_face.is_some(), "should set capture variety face");
        assert!(CAPTURE_FACES.contains(&fv.capture_face.unwrap()));
    }

    #[test]
    fn test_idle_rotation_modulo_25() {
        let mut fv = FaceVariety::new();

        // No idle face at 0
        assert_eq!(fv.idle_face(), None);

        // 1-5 = bored
        fv.idle_epochs = 3;
        assert_eq!(fv.idle_face(), Some("bored"));

        // 6-10 = lonely
        fv.idle_epochs = 8;
        assert_eq!(fv.idle_face(), Some("lonely"));

        // 11-15 = demotivated
        fv.idle_epochs = 13;
        assert_eq!(fv.idle_face(), Some("demotivated"));

        // 16-20 = angry
        fv.idle_epochs = 18;
        assert_eq!(fv.idle_face(), Some("angry"));

        // 21-24 = sad
        fv.idle_epochs = 23;
        assert_eq!(fv.idle_face(), Some("sad"));

        // 25 = wraps to 0 = bored (modulo cycle)
        fv.idle_epochs = 25;
        assert_eq!(fv.idle_face(), Some("bored"));

        // 33 = 33%25=8 = lonely
        fv.idle_epochs = 33;
        assert_eq!(fv.idle_face(), Some("lonely"));
    }

    #[test]
    fn test_idle_resets_on_capture() {
        let mut fv = FaceVariety::new();
        fv.idle_epochs = 20;
        fv.last_milestone = 1; // already hit milestone 1
        fv.on_capture(7);
        assert_eq!(fv.idle_epochs, 0);
    }

    #[test]
    fn test_time_of_day() {
        assert_eq!(time_of_day_face(3), Some("sleep")); // 3am
        assert_eq!(time_of_day_face(7), Some("motivated")); // 7am morning
        assert_eq!(time_of_day_face(14), None); // midday
        assert_eq!(time_of_day_face(22), Some("cool")); // 10pm
        assert_eq!(time_of_day_face(0), Some("cool")); // midnight
        assert_eq!(time_of_day_face(5), Some("sleep")); // 5am
        assert_eq!(time_of_day_face(9), None); // 9am (past morning)
    }

    #[test]
    fn test_debug_on_boot() {
        let mut fv = FaceVariety::new();
        assert!(!fv.debug_shown);
        let face = fv.boot_face();
        assert_eq!(face, "debug");
        assert!(fv.debug_shown);
        assert_eq!(fv.boot_face(), "awake"); // second call
    }

    #[test]
    fn test_morning_greeting_once() {
        let mut fv = FaceVariety::new();
        fv.current_hour = 7;
        assert_eq!(fv.time_of_day_face(), Some("motivated"));
        fv.morning_greeted = true;
        assert_eq!(fv.time_of_day_face(), None); // only once
    }

    #[test]
    fn test_current_override_priority() {
        let mut fv = FaceVariety::new();

        // Default: no override
        assert_eq!(fv.current_override(), None);

        // Idle kicks in
        fv.idle_epochs = 5;
        assert_eq!(fv.current_override(), Some("bored"));

        // Friend overrides idle
        fv.friend_epochs_left = 2;
        assert_eq!(fv.current_override(), Some("friend"));

        // Capture overrides friend
        fv.captures_this_epoch = 1;
        fv.capture_face = Some("happy");
        assert!(
            fv.current_override() == Some("happy"),
            "capture should override friend"
        );

        // Milestone overrides capture
        fv.milestone_epochs_left = 1;
        fv.milestone_face = Some("cool");
        assert_eq!(fv.current_override(), Some("cool"));

        // Debug overrides milestone
        fv.debug_epochs_left = 1;
        assert_eq!(fv.current_override(), Some("debug"));
    }

    #[test]
    fn test_tick_countdowns_clears_milestone() {
        let mut fv = FaceVariety::new();
        fv.milestone_epochs_left = 1;
        fv.milestone_face = Some("excited");
        fv.milestone_status = Some("test");
        fv.tick_countdowns();
        assert_eq!(fv.milestone_epochs_left, 0);
        assert!(fv.milestone_face.is_none());
    }

    #[test]
    fn test_tick_resets_capture_state() {
        let mut fv = FaceVariety::new();
        fv.captures_this_epoch = 3;
        fv.capture_face = Some("happy");
        fv.tick_countdowns();
        assert_eq!(fv.captures_this_epoch, 0);
        assert!(fv.capture_face.is_none());
    }
}
