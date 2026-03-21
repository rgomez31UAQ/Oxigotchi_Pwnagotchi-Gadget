use crate::personality::{Face, Personality};
use std::time::{Duration, Instant};

/// Metrics tracked across epochs.
#[derive(Debug, Clone, Default)]
pub struct EpochMetrics {
    /// Current epoch number (monotonically increasing).
    pub epoch: u64,
    /// Number of consecutive blind epochs (reset on handshake).
    pub blind_epochs: u32,
    /// Total handshakes captured this session.
    pub handshakes: u32,
    /// APs seen in the current epoch.
    pub aps_this_epoch: u32,
    /// Total unique APs seen this session.
    pub total_aps: u32,
    /// Current WiFi channel.
    pub channel: u8,
    /// Time since the daemon started.
    pub uptime: Duration,
    /// Number of deauths sent this epoch.
    pub deauths_this_epoch: u32,
    /// Number of associations this epoch.
    pub assocs_this_epoch: u32,
}

/// The result of one scan epoch.
#[derive(Debug, Clone, Default)]
pub struct EpochResult {
    pub aps_seen: u32,
    pub handshakes_captured: u32,
    pub deauths_sent: u32,
    pub associations: u32,
    pub channel: u8,
}

/// Epoch state machine phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpochPhase {
    /// Scanning for APs and clients.
    Scan,
    /// Running attacks (deauth, PMKID, etc.).
    Attack,
    /// Processing captures.
    Capture,
    /// Updating display.
    Display,
    /// Sleeping before next epoch.
    Sleep,
}

/// The main epoch loop controller.
pub struct EpochLoop {
    pub metrics: EpochMetrics,
    pub personality: Personality,
    pub phase: EpochPhase,
    pub epoch_duration: Duration,
    start_time: Instant,
}

impl EpochLoop {
    /// Create a new epoch loop with the given duration per epoch.
    pub fn new(epoch_duration: Duration) -> Self {
        Self {
            metrics: EpochMetrics::default(),
            personality: Personality::new(),
            phase: EpochPhase::Scan,
            epoch_duration,
            start_time: Instant::now(),
        }
    }

    /// Advance to the next phase in the epoch cycle.
    pub fn next_phase(&mut self) -> EpochPhase {
        self.phase = match self.phase {
            EpochPhase::Scan => EpochPhase::Attack,
            EpochPhase::Attack => EpochPhase::Capture,
            EpochPhase::Capture => EpochPhase::Display,
            EpochPhase::Display => EpochPhase::Sleep,
            EpochPhase::Sleep => {
                self.finish_epoch();
                EpochPhase::Scan
            }
        };
        self.phase
    }

    /// Record the result of a completed epoch and update personality/metrics.
    pub fn record_result(&mut self, result: &EpochResult) {
        self.metrics.aps_this_epoch = result.aps_seen;
        self.metrics.total_aps += result.aps_seen;
        self.metrics.channel = result.channel;
        self.metrics.deauths_this_epoch = result.deauths_sent;
        self.metrics.assocs_this_epoch = result.associations;

        if result.handshakes_captured > 0 {
            self.metrics.handshakes += result.handshakes_captured;
            self.metrics.blind_epochs = 0;
            for _ in 0..result.handshakes_captured {
                self.personality.on_handshake();
            }
        } else {
            self.metrics.blind_epochs += 1;
            self.personality.on_blind_epoch();
        }

        if result.aps_seen > 0 {
            self.personality.on_aps_seen(result.aps_seen);
        }
    }

    /// Called when the epoch's Sleep phase completes — prepares for next epoch.
    fn finish_epoch(&mut self) {
        self.metrics.epoch += 1;
        self.metrics.uptime = self.start_time.elapsed();
        self.metrics.aps_this_epoch = 0;
        self.metrics.deauths_this_epoch = 0;
        self.metrics.assocs_this_epoch = 0;
    }

    /// Get the face to display this epoch.
    pub fn current_face(&self) -> Face {
        self.personality.current_face()
    }

    /// Get a status message for the current epoch.
    pub fn status_message(&self) -> String {
        match self.phase {
            EpochPhase::Scan => format!(
                "Scanning... CH {} | {} APs",
                self.metrics.channel, self.metrics.aps_this_epoch
            ),
            EpochPhase::Attack => format!(
                "Attacking... {} deauths",
                self.metrics.deauths_this_epoch
            ),
            EpochPhase::Capture => format!(
                "Capturing... {} handshakes",
                self.metrics.handshakes
            ),
            EpochPhase::Display => self.personality.mood.status_message().to_string(),
            EpochPhase::Sleep => "Sleeping...".to_string(),
        }
    }

    /// Format uptime as "HH:MM:SS".
    pub fn uptime_str(&self) -> String {
        let secs = self.start_time.elapsed().as_secs();
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        format!("{h:02}:{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_phase_cycle() {
        let mut el = EpochLoop::new(Duration::from_secs(1));
        assert_eq!(el.phase, EpochPhase::Scan);
        assert_eq!(el.next_phase(), EpochPhase::Attack);
        assert_eq!(el.next_phase(), EpochPhase::Capture);
        assert_eq!(el.next_phase(), EpochPhase::Display);
        assert_eq!(el.next_phase(), EpochPhase::Sleep);
        // Sleep -> Scan starts a new epoch
        assert_eq!(el.next_phase(), EpochPhase::Scan);
        assert_eq!(el.metrics.epoch, 1);
    }

    #[test]
    fn test_record_handshake() {
        let mut el = EpochLoop::new(Duration::from_secs(1));
        let result = EpochResult {
            aps_seen: 5,
            handshakes_captured: 2,
            deauths_sent: 3,
            associations: 1,
            channel: 6,
        };
        el.record_result(&result);
        assert_eq!(el.metrics.handshakes, 2);
        assert_eq!(el.metrics.total_aps, 5);
        assert_eq!(el.metrics.channel, 6);
        assert_eq!(el.metrics.blind_epochs, 0);
    }

    #[test]
    fn test_blind_epochs_increment() {
        let mut el = EpochLoop::new(Duration::from_secs(1));
        let blind = EpochResult::default();
        el.record_result(&blind);
        assert_eq!(el.metrics.blind_epochs, 1);
        el.record_result(&blind);
        assert_eq!(el.metrics.blind_epochs, 2);

        // Handshake resets blind count
        let good = EpochResult {
            handshakes_captured: 1,
            ..Default::default()
        };
        el.record_result(&good);
        assert_eq!(el.metrics.blind_epochs, 0);
    }

    #[test]
    fn test_personality_updates_on_epoch() {
        let mut el = EpochLoop::new(Duration::from_secs(1));
        let initial_mood = el.personality.mood.value();

        // Multiple blind epochs should decrease mood
        for _ in 0..5 {
            el.record_result(&EpochResult::default());
        }
        assert!(el.personality.mood.value() < initial_mood);

        // Handshakes should increase mood
        let mood_before = el.personality.mood.value();
        el.record_result(&EpochResult {
            handshakes_captured: 3,
            aps_seen: 10,
            ..Default::default()
        });
        assert!(el.personality.mood.value() > mood_before);
    }

    #[test]
    fn test_status_message_per_phase() {
        let mut el = EpochLoop::new(Duration::from_secs(1));
        el.metrics.channel = 11;

        // Each phase should produce a different status
        let scan_msg = el.status_message();
        assert!(scan_msg.contains("Scanning"), "got: {scan_msg}");

        el.next_phase();
        let atk_msg = el.status_message();
        assert!(atk_msg.contains("Attacking"), "got: {atk_msg}");

        el.next_phase();
        let cap_msg = el.status_message();
        assert!(cap_msg.contains("Capturing"), "got: {cap_msg}");
    }

    #[test]
    fn test_finish_epoch_resets_per_epoch_counters() {
        let mut el = EpochLoop::new(Duration::from_secs(1));
        el.record_result(&EpochResult {
            aps_seen: 5,
            deauths_sent: 3,
            associations: 2,
            ..Default::default()
        });
        assert_eq!(el.metrics.aps_this_epoch, 5);

        // Cycle through to Sleep -> Scan (triggers finish_epoch)
        for _ in 0..5 {
            el.next_phase();
        }
        assert_eq!(el.metrics.aps_this_epoch, 0);
        assert_eq!(el.metrics.deauths_this_epoch, 0);
        assert_eq!(el.metrics.assocs_this_epoch, 0);
        // But total APs should persist
        assert_eq!(el.metrics.total_aps, 5);
    }

    #[test]
    fn test_uptime_str_format() {
        let el = EpochLoop::new(Duration::from_secs(1));
        let s = el.uptime_str();
        // Should be HH:MM:SS format
        assert_eq!(s.len(), 8);
        assert_eq!(&s[2..3], ":");
        assert_eq!(&s[5..6], ":");
    }

    #[test]
    fn test_current_face() {
        let mut el = EpochLoop::new(Duration::from_secs(1));
        // Default mood (0.5) -> Awake
        assert_eq!(el.current_face(), Face::Awake);

        // Override takes priority
        el.personality.set_override(Face::BatteryCritical);
        assert_eq!(el.current_face(), Face::BatteryCritical);
    }
}
