//! WiFi monitor mode and channel scanning.
//!
//! This module provides stubs for WiFi integration. The actual monitor mode
//! and packet capture will be handled by AngryOxide as a Rust crate.
//! These types define the interface that the epoch loop uses.

use std::collections::HashMap;
use std::time::Instant;

/// Represents a discovered access point.
#[derive(Debug, Clone)]
pub struct AccessPoint {
    /// BSSID (MAC address) as 6 bytes.
    pub bssid: [u8; 6],
    /// SSID (network name), may be empty for hidden networks.
    pub ssid: String,
    /// WiFi channel.
    pub channel: u8,
    /// Signal strength in dBm.
    pub rssi: i8,
    /// Last time this AP was seen.
    pub last_seen: Instant,
    /// Number of associated clients observed.
    pub client_count: u32,
    /// Whether this AP is in the whitelist.
    pub whitelisted: bool,
}

impl AccessPoint {
    pub fn bssid_str(&self) -> String {
        self.bssid
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    }
}

/// Channel hopping configuration.
#[derive(Debug, Clone)]
pub struct ChannelConfig {
    /// Channels to hop through. Default: 1-11 for 2.4GHz.
    pub channels: Vec<u8>,
    /// Dwell time on each channel in milliseconds.
    pub dwell_ms: u64,
    /// Current channel index in the list.
    pub current_index: usize,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            channels: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            dwell_ms: 250,
            current_index: 0,
        }
    }
}

impl ChannelConfig {
    /// Advance to the next channel and return it.
    pub fn next_channel(&mut self) -> u8 {
        self.current_index = (self.current_index + 1) % self.channels.len();
        self.channels[self.current_index]
    }

    /// Get the current channel.
    pub fn current_channel(&self) -> u8 {
        self.channels[self.current_index]
    }
}

/// Tracks all discovered APs by BSSID.
#[derive(Debug, Default)]
pub struct ApTracker {
    aps: HashMap<[u8; 6], AccessPoint>,
}

impl ApTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update or insert an AP. Returns true if this is a new AP.
    pub fn update(&mut self, ap: AccessPoint) -> bool {
        let is_new = !self.aps.contains_key(&ap.bssid);
        self.aps.insert(ap.bssid, ap);
        is_new
    }

    /// Get number of tracked APs.
    pub fn count(&self) -> usize {
        self.aps.len()
    }

    /// Get an AP by BSSID.
    pub fn get(&self, bssid: &[u8; 6]) -> Option<&AccessPoint> {
        self.aps.get(bssid)
    }

    /// Return all tracked APs sorted by signal strength (strongest first).
    pub fn sorted_by_rssi(&self) -> Vec<&AccessPoint> {
        let mut aps: Vec<_> = self.aps.values().collect();
        aps.sort_by(|a, b| b.rssi.cmp(&a.rssi));
        aps
    }

    /// Remove APs not seen for more than `max_age` seconds.
    pub fn prune(&mut self, max_age_secs: u64) {
        let cutoff = Instant::now() - std::time::Duration::from_secs(max_age_secs);
        self.aps.retain(|_, ap| ap.last_seen >= cutoff);
    }

    /// Filter out whitelisted APs, returning only attackable ones.
    pub fn attackable(&self) -> Vec<&AccessPoint> {
        self.aps.values().filter(|ap| !ap.whitelisted).collect()
    }
}

/// WiFi interface state (stub — actual implementation will use netlink/AO).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiState {
    /// Interface not initialized.
    Down,
    /// In managed mode.
    Managed,
    /// In monitor mode, ready to scan.
    Monitor,
    /// Error state (e.g., firmware crash).
    Error,
}

/// Stub WiFi manager. On Pi, this will wrap AngryOxide's WiFi functionality.
pub struct WifiManager {
    pub state: WifiState,
    pub channel_config: ChannelConfig,
    pub tracker: ApTracker,
}

impl WifiManager {
    pub fn new() -> Self {
        Self {
            state: WifiState::Down,
            channel_config: ChannelConfig::default(),
            tracker: ApTracker::new(),
        }
    }

    /// Stub: would create monitor mode interface.
    pub fn start_monitor(&mut self) -> Result<(), String> {
        self.state = WifiState::Monitor;
        Ok(())
    }

    /// Stub: would destroy monitor mode interface.
    pub fn stop_monitor(&mut self) -> Result<(), String> {
        self.state = WifiState::Managed;
        Ok(())
    }

    /// Hop to the next channel.
    pub fn hop_channel(&mut self) -> u8 {
        self.channel_config.next_channel()
    }
}

impl Default for WifiManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ap(bssid_last: u8, ssid: &str, rssi: i8) -> AccessPoint {
        AccessPoint {
            bssid: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, bssid_last],
            ssid: ssid.into(),
            channel: 6,
            rssi,
            last_seen: Instant::now(),
            client_count: 0,
            whitelisted: false,
        }
    }

    #[test]
    fn test_channel_config_default() {
        let cc = ChannelConfig::default();
        assert_eq!(cc.channels.len(), 11);
        assert_eq!(cc.current_channel(), 1);
    }

    #[test]
    fn test_channel_hop_wraps() {
        let mut cc = ChannelConfig::default();
        for _ in 0..11 {
            cc.next_channel();
        }
        // After 11 hops, should wrap back to channel 1
        assert_eq!(cc.current_channel(), 1);
    }

    #[test]
    fn test_ap_tracker_insert() {
        let mut tracker = ApTracker::new();
        let ap = make_ap(0x01, "TestNet", -50);
        assert!(tracker.update(ap));
        assert_eq!(tracker.count(), 1);
    }

    #[test]
    fn test_ap_tracker_dedup() {
        let mut tracker = ApTracker::new();
        let ap1 = make_ap(0x01, "TestNet", -50);
        let ap2 = make_ap(0x01, "TestNet", -40); // Same BSSID, updated RSSI
        assert!(tracker.update(ap1));
        assert!(!tracker.update(ap2)); // Not new
        assert_eq!(tracker.count(), 1);
        // Should have updated RSSI
        let bssid = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01];
        assert_eq!(tracker.get(&bssid).unwrap().rssi, -40);
    }

    #[test]
    fn test_ap_tracker_sorted_by_rssi() {
        let mut tracker = ApTracker::new();
        tracker.update(make_ap(0x01, "Weak", -80));
        tracker.update(make_ap(0x02, "Strong", -30));
        tracker.update(make_ap(0x03, "Medium", -55));

        let sorted = tracker.sorted_by_rssi();
        assert_eq!(sorted[0].ssid, "Strong");
        assert_eq!(sorted[1].ssid, "Medium");
        assert_eq!(sorted[2].ssid, "Weak");
    }

    #[test]
    fn test_ap_tracker_whitelist_filter() {
        let mut tracker = ApTracker::new();
        tracker.update(make_ap(0x01, "MyHome", -50));
        let mut wl_ap = make_ap(0x02, "Whitelisted", -40);
        wl_ap.whitelisted = true;
        tracker.update(wl_ap);

        let attackable = tracker.attackable();
        assert_eq!(attackable.len(), 1);
        assert_eq!(attackable[0].ssid, "MyHome");
    }

    #[test]
    fn test_bssid_str() {
        let ap = make_ap(0xFF, "Test", -50);
        assert_eq!(ap.bssid_str(), "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn test_wifi_manager_state() {
        let mut wm = WifiManager::new();
        assert_eq!(wm.state, WifiState::Down);
        wm.start_monitor().unwrap();
        assert_eq!(wm.state, WifiState::Monitor);
        wm.stop_monitor().unwrap();
        assert_eq!(wm.state, WifiState::Managed);
    }

    #[test]
    fn test_wifi_manager_hop() {
        let mut wm = WifiManager::new();
        let ch = wm.hop_channel();
        assert_eq!(ch, 2); // First hop from ch1 to ch2
    }
}
