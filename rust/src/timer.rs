//! Wall-clock timers for periodic scheduling, independent of epoch count.

use std::time::{Duration, Instant};

/// A timer that fires at wall-clock intervals.
/// Call `due()` each iteration — returns `true` when the interval has elapsed
/// and resets the timer. Skips missed intervals (no catch-up).
pub struct WallTimer {
    last: Instant,
    interval: Duration,
}

impl WallTimer {
    /// Create a timer that first fires after `interval` elapses.
    pub fn new(interval: Duration) -> Self {
        Self {
            last: Instant::now(),
            interval,
        }
    }

    /// Create a timer that fires immediately on first `due()` call,
    /// then every `interval` after that.
    pub fn ready(interval: Duration) -> Self {
        Self {
            last: Instant::now() - interval,
            interval,
        }
    }

    /// Returns true if the interval has elapsed, and resets the timer.
    pub fn due(&mut self) -> bool {
        if self.last.elapsed() >= self.interval {
            self.last = Instant::now();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_wall_timer_not_due_immediately() {
        let mut t = WallTimer::new(Duration::from_secs(10));
        assert!(!t.due());
    }

    #[test]
    fn test_wall_timer_due_after_zero_interval() {
        let mut t = WallTimer::new(Duration::from_secs(0));
        assert!(t.due());
    }

    #[test]
    fn test_wall_timer_due_resets() {
        let mut t = WallTimer::new(Duration::from_millis(0));
        assert!(t.due());
        // Immediately due again with 0ms interval
        assert!(t.due());
    }

    #[test]
    fn test_wall_timer_ready_starts_due() {
        let mut t = WallTimer::ready(Duration::from_secs(60));
        assert!(t.due()); // First call should fire
        assert!(!t.due()); // Second should not (60s hasn't passed)
    }
}
