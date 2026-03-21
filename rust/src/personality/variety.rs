//! Face variety engine — 8 features ported from Python angryoxide.py.
//!
//! 1. Achievement milestones (1st/10th/25th/50th/100th capture)
//! 2. Capture face cycling (happy→excited→cool after captures)
//! 3. Time-of-day faces (morning=awake, night=sleep)
//! 4. Idle rotation (Bored→Lonely→Demotivated→Angry→Sad)
//! 5. Friend detection (when peer is found)
//! 6. Upload face (when uploading captures)
//! 7. Debug face on boot
//! 8. Random rare face (5% per epoch)

use rand::Rng;

/// Milestone capture counts that trigger special faces.
const MILESTONES: &[(u32, &str)] = &[
    (1, "First capture! The bull charges!"),
    (10, "10 captures! Stampede!"),
    (25, "25 captures! Herd leader!"),
    (50, "50 captures! Legendary bull!"),
    (100, "100 captures! Bull God!"),
];

/// Idle face rotation thresholds (epochs without a capture).
/// Python: Bored→Lonely→Demotivated→Angry→Sad
const IDLE_THRESHOLDS: &[(u32, &str)] = &[
    (5, "bored"),
    (15, "lonely"),
    (30, "demotivated"),
    (50, "angry"),
    (75, "sad"),
];

/// Capture face cycling pool — rotates through these after each capture.
const CAPTURE_FACES: &[&str] = &["happy", "excited", "cool", "grateful", "motivated"];

/// Rare face pool — 5% chance per epoch.
const RARE_FACES: &[&str] = &["cool", "intense", "smart", "motivated"];

/// Face variety state machine.
#[derive(Debug)]
pub struct FaceVariety {
    /// Epochs remaining to show a milestone face.
    pub milestone_epochs_left: u32,
    /// Face to show during milestone.
    pub milestone_face: Option<&'static str>,
    /// Status to show during milestone.
    pub milestone_status: Option<&'static str>,
    /// Last milestone capture count triggered.
    pub last_milestone: u32,
    /// Consecutive epochs since last capture.
    pub idle_epochs: u32,
    /// Whether debug face was shown at boot.
    pub debug_shown: bool,
    /// Epochs remaining for debug face.
    pub debug_epochs_left: u32,
    /// Friend detection countdown epochs.
    pub friend_epochs_left: u32,
    /// Upload face countdown epochs.
    pub upload_epochs_left: u32,
    /// Capture face cycling index.
    capture_face_idx: usize,
    /// Epochs remaining for capture face.
    pub capture_face_epochs_left: u32,
    /// Current capture face override.
    pub capture_face: Option<&'static str>,
    /// Current hour of day (set from main loop for time-of-day faces).
    pub current_hour: u32,
    /// Rare face for this epoch (set by rare_face_roll, cleared each epoch).
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
            capture_face_idx: 0,
            capture_face_epochs_left: 0,
            capture_face: None,
            current_hour: 12,
            rare_face: None,
        }
    }

    /// Called when a capture occurs. Returns Some((face, status)) if milestone hit.
    pub fn on_capture(&mut self, total_captures: u32) -> Option<(&'static str, &'static str)> {
        self.idle_epochs = 0;

        // Check milestones first
        for &(count, status) in MILESTONES {
            if total_captures == count && self.last_milestone < count {
                self.last_milestone = count;
                self.milestone_epochs_left = 3;
                self.milestone_face = Some("excited");
                self.milestone_status = Some(status);
                return Some(("excited", status));
            }
        }

        // Non-milestone capture: cycle through capture faces
        let face = CAPTURE_FACES[self.capture_face_idx % CAPTURE_FACES.len()];
        self.capture_face_idx += 1;
        self.capture_face = Some(face);
        self.capture_face_epochs_left = 2;

        None
    }

    /// Increment idle epoch counter.
    pub fn tick_idle(&mut self) {
        self.idle_epochs += 1;
    }

    /// Get the idle face based on how many epochs since last capture.
    /// Returns None if not idle enough for a special face.
    pub fn idle_face(&self) -> Option<&'static str> {
        let mut result = None;
        for &(threshold, face) in IDLE_THRESHOLDS {
            if self.idle_epochs >= threshold {
                result = Some(face);
            }
        }
        result
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

    /// Roll for a random rare face this epoch (5% chance).
    pub fn rare_face_roll(&self) -> Option<&'static str> {
        let mut rng = rand::thread_rng();
        if rng.r#gen::<f32>() < 0.05 {
            let idx = rng.gen_range(0..RARE_FACES.len());
            Some(RARE_FACES[idx])
        } else {
            None
        }
    }

    /// Tick down all active countdowns. Call once per epoch.
    /// Also rolls for rare face (5% chance) each tick.
    /// Returns true if any countdown expired this tick.
    pub fn tick_countdowns(&mut self) -> bool {
        let mut expired = false;

        // Roll for rare face this epoch (clears previous)
        self.rare_face = self.rare_face_roll();

        if self.milestone_epochs_left > 0 {
            self.milestone_epochs_left -= 1;
            if self.milestone_epochs_left == 0 {
                self.milestone_face = None;
                self.milestone_status = None;
                expired = true;
            }
        }
        if self.debug_epochs_left > 0 {
            self.debug_epochs_left -= 1;
            expired = true;
        }
        if self.friend_epochs_left > 0 {
            self.friend_epochs_left -= 1;
            expired = true;
        }
        if self.upload_epochs_left > 0 {
            self.upload_epochs_left -= 1;
            expired = true;
        }
        if self.capture_face_epochs_left > 0 {
            self.capture_face_epochs_left -= 1;
            if self.capture_face_epochs_left == 0 {
                self.capture_face = None;
                expired = true;
            }
        }

        expired
    }

    /// Trigger friend face for N epochs.
    pub fn on_friend_detected(&mut self, epochs: u32) {
        self.friend_epochs_left = epochs;
    }

    /// Trigger upload face for N epochs.
    pub fn on_upload(&mut self, epochs: u32) {
        self.upload_epochs_left = epochs;
    }

    /// Get the current face override from the variety engine, if any.
    /// Priority: milestone > debug > friend > upload > capture > rare > time-of-day > idle.
    pub fn current_override(&self) -> Option<&'static str> {
        // Milestone is highest priority
        if self.milestone_epochs_left > 0 {
            return self.milestone_face;
        }
        // Debug face on boot
        if self.debug_epochs_left > 0 {
            return Some("debug");
        }
        // Friend detected
        if self.friend_epochs_left > 0 {
            return Some("friend");
        }
        // Upload in progress
        if self.upload_epochs_left > 0 {
            return Some("upload");
        }
        // Recent capture face
        if self.capture_face_epochs_left > 0 {
            return self.capture_face;
        }
        // Random rare face (5% per epoch, set by tick_countdowns)
        if let Some(rare) = self.rare_face {
            return Some(rare);
        }
        // Time-of-day face (morning=awake, night=sleep)
        if let Some(tod) = time_of_day_face(self.current_hour) {
            return Some(tod);
        }
        // Idle rotation
        if let Some(face) = self.idle_face() {
            return Some(face);
        }
        // No override
        None
    }
}

impl Default for FaceVariety {
    fn default() -> Self {
        Self::new()
    }
}

/// Get a time-of-day face override (if applicable).
/// Morning (6-9) = awake, Night (23, 0-5) = sleep, else None.
pub fn time_of_day_face(hour: u32) -> Option<&'static str> {
    match hour {
        6..=9 => Some("awake"),
        23 | 0..=5 => Some("sleep"),
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
        let (_, status) = result.unwrap();
        assert!(status.contains("First"), "should mention 'First' capture");
    }

    #[test]
    fn test_milestone_10th_capture() {
        let mut fv = FaceVariety::new();
        // First trigger milestone 1
        fv.on_capture(1);
        // Then trigger milestone 10
        let result = fv.on_capture(10);
        assert!(result.is_some(), "10th capture should trigger milestone");
    }

    #[test]
    fn test_milestone_non_milestone_count() {
        let mut fv = FaceVariety::new();
        // Trigger milestone 1 first
        fv.on_capture(1);
        let result = fv.on_capture(7);
        assert!(result.is_none(), "7th capture is not a milestone");
    }

    #[test]
    fn test_milestone_no_double_trigger() {
        let mut fv = FaceVariety::new();
        let r1 = fv.on_capture(1);
        assert!(r1.is_some());
        let r2 = fv.on_capture(1);
        assert!(r2.is_none(), "same milestone should not trigger twice");
    }

    #[test]
    fn test_idle_rotation() {
        let mut fv = FaceVariety::new();
        // No idle face before threshold
        assert_eq!(fv.idle_face(), None);
        // After 5 idle epochs -> Bored
        for _ in 0..5 {
            fv.tick_idle();
        }
        assert_eq!(fv.idle_face(), Some("bored"));
        // After 15 total -> Lonely
        for _ in 0..10 {
            fv.tick_idle();
        }
        assert_eq!(fv.idle_face(), Some("lonely"));
        // After 30 total -> Demotivated
        for _ in 0..15 {
            fv.tick_idle();
        }
        assert_eq!(fv.idle_face(), Some("demotivated"));
        // After 50 total -> Angry
        for _ in 0..20 {
            fv.tick_idle();
        }
        assert_eq!(fv.idle_face(), Some("angry"));
        // After 75 total -> Sad
        for _ in 0..25 {
            fv.tick_idle();
        }
        assert_eq!(fv.idle_face(), Some("sad"));
    }

    #[test]
    fn test_idle_resets_on_capture() {
        let mut fv = FaceVariety::new();
        for _ in 0..20 {
            fv.tick_idle();
        }
        assert!(fv.idle_face().is_some());
        fv.on_capture(1);
        assert_eq!(fv.idle_epochs, 0);
        assert_eq!(fv.idle_face(), None);
    }

    #[test]
    fn test_time_of_day_faces() {
        assert_eq!(time_of_day_face(7), Some("awake")); // morning
        assert_eq!(time_of_day_face(14), None); // midday
        assert_eq!(time_of_day_face(23), Some("sleep")); // late night
        assert_eq!(time_of_day_face(3), Some("sleep")); // early morning
        assert_eq!(time_of_day_face(6), Some("awake")); // boundary
        assert_eq!(time_of_day_face(10), None); // just after morning
    }

    #[test]
    fn test_debug_on_boot() {
        let mut fv = FaceVariety::new();
        assert!(!fv.debug_shown);
        let face = fv.boot_face();
        assert_eq!(face, "debug");
        assert!(fv.debug_shown);
        // Second call should not return debug
        let face2 = fv.boot_face();
        assert_eq!(face2, "awake");
    }

    #[test]
    fn test_friend_countdown() {
        let mut fv = FaceVariety::new();
        fv.on_friend_detected(3);
        assert_eq!(fv.friend_epochs_left, 3);
        assert_eq!(fv.current_override(), Some("friend"));
        fv.tick_countdowns();
        assert_eq!(fv.friend_epochs_left, 2);
        fv.tick_countdowns();
        fv.tick_countdowns();
        assert_eq!(fv.friend_epochs_left, 0);
    }

    #[test]
    fn test_upload_countdown() {
        let mut fv = FaceVariety::new();
        fv.on_upload(2);
        assert_eq!(fv.current_override(), Some("upload"));
        fv.tick_countdowns();
        fv.tick_countdowns();
        assert_eq!(fv.upload_epochs_left, 0);
    }

    #[test]
    fn test_milestone_priority_over_idle() {
        let mut fv = FaceVariety::new();
        // Build up idle
        for _ in 0..20 {
            fv.tick_idle();
        }
        assert!(fv.idle_face().is_some());
        // Capture triggers milestone — should override idle
        fv.on_capture(1);
        assert_eq!(fv.current_override(), Some("excited"));
    }

    #[test]
    fn test_capture_face_cycling() {
        let mut fv = FaceVariety::new();
        // First non-milestone capture (after milestone 1)
        fv.on_capture(1); // milestone
        fv.milestone_epochs_left = 0; // clear milestone
        fv.milestone_face = None;

        fv.on_capture(2); // non-milestone, should cycle
        assert!(fv.capture_face.is_some());
        let face1 = fv.capture_face.unwrap();

        fv.capture_face_epochs_left = 0;
        fv.capture_face = None;
        fv.on_capture(3);
        let face2 = fv.capture_face.unwrap();

        // Should cycle through different faces
        assert!(
            CAPTURE_FACES.contains(&face1),
            "capture face should be from pool"
        );
        assert!(
            CAPTURE_FACES.contains(&face2),
            "capture face should be from pool"
        );
    }

    #[test]
    fn test_tick_countdowns_clears_milestone() {
        let mut fv = FaceVariety::new();
        fv.on_capture(1); // triggers milestone with 3 epochs
        assert_eq!(fv.milestone_epochs_left, 3);
        fv.tick_countdowns(); // 2
        fv.tick_countdowns(); // 1
        fv.tick_countdowns(); // 0 — cleared
        assert_eq!(fv.milestone_epochs_left, 0);
        assert!(fv.milestone_face.is_none());
    }

    #[test]
    fn test_current_override_priority() {
        let mut fv = FaceVariety::new();

        // No overrides
        assert_eq!(fv.current_override(), None);

        // Add idle
        for _ in 0..10 {
            fv.tick_idle();
        }
        assert_eq!(fv.current_override(), Some("bored"));

        // Friend overrides idle
        fv.on_friend_detected(2);
        assert_eq!(fv.current_override(), Some("friend"));

        // Milestone overrides friend
        fv.on_capture(1);
        assert_eq!(fv.current_override(), Some("excited"));
    }
}
