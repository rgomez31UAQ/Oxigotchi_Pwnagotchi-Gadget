//! Attack scheduling and rate limiting.
//!
//! Stub module — actual attacks are handled by AngryOxide.
//! This module defines the interface for attack scheduling, rate limiting,
//! and whitelist filtering that the epoch loop uses.

use std::time::{Duration, Instant};

/// Types of attacks supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackType {
    /// Deauthentication attack.
    Deauth,
    /// PMKID capture (RSN/PMKID from EAPOL).
    Pmkid,
    /// Channel Switch Announcement (CSA).
    Csa,
    /// Disassociation attack.
    Disassoc,
}

/// Result of a single attack attempt.
#[derive(Debug, Clone)]
pub struct AttackResult {
    pub attack_type: AttackType,
    pub target_bssid: [u8; 6],
    pub success: bool,
    pub handshake_captured: bool,
    pub timestamp: Instant,
}

/// Rate limiter to prevent firmware crashes (BCM43436B0 crashes at rate 2+).
#[derive(Debug)]
pub struct RateLimiter {
    /// Maximum attacks per second.
    pub max_rate: u32,
    /// Attacks sent in the current window.
    attacks_in_window: u32,
    /// Start of the current rate window.
    window_start: Instant,
    /// Window duration.
    window_duration: Duration,
}

impl RateLimiter {
    /// Create a rate limiter with the given max attacks per second.
    pub fn new(max_rate: u32) -> Self {
        Self {
            max_rate,
            attacks_in_window: 0,
            window_start: Instant::now(),
            window_duration: Duration::from_secs(1),
        }
    }

    /// Check if an attack is allowed. Returns true if under the rate limit.
    pub fn allow(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.window_start) >= self.window_duration {
            // New window
            self.window_start = now;
            self.attacks_in_window = 0;
        }
        if self.attacks_in_window < self.max_rate {
            self.attacks_in_window += 1;
            true
        } else {
            false
        }
    }

    /// Get attacks remaining in the current window.
    pub fn remaining(&self) -> u32 {
        self.max_rate.saturating_sub(self.attacks_in_window)
    }
}

/// Attack scheduler deciding which APs to target and when.
#[derive(Debug)]
pub struct AttackScheduler {
    pub rate_limiter: RateLimiter,
    /// BSSIDs to skip (whitelisted).
    pub whitelist: Vec<[u8; 6]>,
    /// Total attacks sent this session.
    pub total_attacks: u64,
    /// Total handshakes captured via attacks.
    pub total_handshakes: u64,
}

impl AttackScheduler {
    pub fn new(rate: u32) -> Self {
        Self {
            rate_limiter: RateLimiter::new(rate),
            whitelist: Vec::new(),
            total_attacks: 0,
            total_handshakes: 0,
        }
    }

    /// Check if a BSSID is whitelisted.
    pub fn is_whitelisted(&self, bssid: &[u8; 6]) -> bool {
        self.whitelist.contains(bssid)
    }

    /// Record an attack result.
    pub fn record(&mut self, result: &AttackResult) {
        self.total_attacks += 1;
        if result.handshake_captured {
            self.total_handshakes += 1;
        }
    }

    /// Stub: schedule the next attack. Returns the attack type to run, or None.
    pub fn next_attack(&mut self, _target_bssid: &[u8; 6]) -> Option<AttackType> {
        if self.rate_limiter.allow() {
            Some(AttackType::Deauth) // Stub: always deauth
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_under_limit() {
        let mut rl = RateLimiter::new(3);
        assert!(rl.allow());
        assert!(rl.allow());
        assert!(rl.allow());
        assert!(!rl.allow()); // 4th should be blocked
    }

    #[test]
    fn test_rate_limiter_remaining() {
        let mut rl = RateLimiter::new(5);
        assert_eq!(rl.remaining(), 5);
        rl.allow();
        assert_eq!(rl.remaining(), 4);
    }

    #[test]
    fn test_rate_limiter_window_reset() {
        let mut rl = RateLimiter::new(1);
        assert!(rl.allow());
        assert!(!rl.allow());

        // Simulate window expiry by backdating start
        rl.window_start = Instant::now() - Duration::from_secs(2);
        assert!(rl.allow()); // New window
    }

    #[test]
    fn test_whitelist_check() {
        let mut scheduler = AttackScheduler::new(1);
        let bssid = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        assert!(!scheduler.is_whitelisted(&bssid));
        scheduler.whitelist.push(bssid);
        assert!(scheduler.is_whitelisted(&bssid));
    }

    #[test]
    fn test_record_attack() {
        let mut scheduler = AttackScheduler::new(10);
        let result = AttackResult {
            attack_type: AttackType::Deauth,
            target_bssid: [0; 6],
            success: true,
            handshake_captured: true,
            timestamp: Instant::now(),
        };
        scheduler.record(&result);
        assert_eq!(scheduler.total_attacks, 1);
        assert_eq!(scheduler.total_handshakes, 1);
    }

    #[test]
    fn test_next_attack_respects_rate() {
        let mut scheduler = AttackScheduler::new(1);
        let bssid = [0; 6];
        assert!(scheduler.next_attack(&bssid).is_some());
        assert!(scheduler.next_attack(&bssid).is_none()); // rate limited
    }

    #[test]
    fn test_attack_types() {
        // Ensure all variants are distinct
        assert_ne!(AttackType::Deauth, AttackType::Pmkid);
        assert_ne!(AttackType::Csa, AttackType::Disassoc);
    }
}
