// QPU RF environment — computes per-epoch RF statistics from classified
// frame batches and provides the personality integration interface.
//
// This module is NOT cfg-gated — it's pure computation, works on all platforms.

use std::collections::HashSet;

use super::classifier::FrameClass;
use super::ringbuf::FrameEntry;

// ---------------------------------------------------------------------------
// Threshold constants — used by personality to classify RF conditions
// ---------------------------------------------------------------------------

/// Frames per epoch to consider the environment RF_BUSY.
pub const BUSY_THRESHOLD: u32 = 100;
/// Deauths per second indicating a deauth storm.
pub const DEAUTH_STORM_RATE: f32 = 10.0;
/// Probe requests per second indicating a probe flood.
pub const PROBE_FLOOD_RATE: f32 = 20.0;
/// Unique BSSID count that triggers Excited personality state.
pub const RICH_BSSID_COUNT: u32 = 20;

// ---------------------------------------------------------------------------
// RfEnvironment — per-epoch RF statistics
// ---------------------------------------------------------------------------

/// Per-epoch RF statistics computed from classified frame batches.
/// Consumed by the personality engine to adjust mood and behavior.
pub struct RfEnvironment {
    /// Beacons per second.
    pub beacon_rate: f32,
    /// Probe requests per second.
    pub probe_rate: f32,
    /// Deauths per second.
    pub deauth_rate: f32,
    /// Data frames per second.
    pub data_rate: f32,
    /// Control frames per second.
    pub control_rate: f32,
    /// Distinct BSSIDs seen in the batch.
    pub unique_bssids: u32,
    /// Total frames classified in the batch.
    pub total_frames: u32,
    /// Frame class with the highest count (ties broken by enum order).
    pub dominant_class: FrameClass,
    /// Fraction of frames whose BSSID is in the AO target set (0.0-1.0).
    pub ao_target_ratio: f32,
}

impl Default for RfEnvironment {
    fn default() -> Self {
        RfEnvironment {
            beacon_rate: 0.0,
            probe_rate: 0.0,
            deauth_rate: 0.0,
            data_rate: 0.0,
            control_rate: 0.0,
            unique_bssids: 0,
            total_frames: 0,
            dominant_class: FrameClass::Unknown,
            ao_target_ratio: 0.0,
        }
    }
}

impl RfEnvironment {
    /// Compute RF environment statistics from a batch of classified frames.
    ///
    /// `results` — pre-classified (FrameClass, FrameEntry) pairs from the
    ///              classifier batch output.
    /// `epoch_secs` — duration of the epoch in seconds (used as rate divisor).
    /// `ao_bssids` — set of BSSIDs currently targeted by angryoxide.
    pub fn compute(
        results: &[(FrameClass, FrameEntry)],
        epoch_secs: f32,
        ao_bssids: &HashSet<[u8; 6]>,
    ) -> Self {
        if results.is_empty() || epoch_secs <= 0.0 {
            return Self::default();
        }

        // Count frames per class. FrameClass variants are 0..=9.
        let mut counts = [0u32; 10];
        let mut bssid_set = HashSet::new();
        let mut ao_hit_count: u32 = 0;

        for (class, entry) in results {
            counts[*class as u8 as usize] += 1;

            // SAFETY: FrameEntry is packed — read bssid via addr_of! to
            // avoid creating a misaligned reference.
            let bssid = unsafe { std::ptr::addr_of!(entry.bssid).read_unaligned() };
            bssid_set.insert(bssid);

            if ao_bssids.contains(&bssid) {
                ao_hit_count += 1;
            }
        }

        let total = results.len() as u32;

        // Dominant class: highest count, ties broken by lowest enum discriminant
        // (i.e., first one encountered in enum order wins).
        let mut dominant_idx: u8 = 0;
        let mut dominant_count: u32 = counts[0];
        for i in 1u8..10 {
            if counts[i as usize] > dominant_count {
                dominant_count = counts[i as usize];
                dominant_idx = i;
            }
        }

        // Map dominant_idx back to FrameClass. The discriminant values are
        // contiguous 0..=9, matching the array indices.
        let dominant_class = match dominant_idx {
            0 => FrameClass::Unknown,
            1 => FrameClass::Beacon,
            2 => FrameClass::ProbeReq,
            3 => FrameClass::ProbeResp,
            4 => FrameClass::Auth,
            5 => FrameClass::Deauth,
            6 => FrameClass::AssocReq,
            7 => FrameClass::AssocResp,
            8 => FrameClass::Data,
            9 => FrameClass::Control,
            _ => FrameClass::Unknown,
        };

        let ao_target_ratio = ao_hit_count as f32 / total as f32;

        RfEnvironment {
            beacon_rate: counts[FrameClass::Beacon as usize] as f32 / epoch_secs,
            probe_rate: counts[FrameClass::ProbeReq as usize] as f32 / epoch_secs,
            deauth_rate: counts[FrameClass::Deauth as usize] as f32 / epoch_secs,
            data_rate: counts[FrameClass::Data as usize] as f32 / epoch_secs,
            control_rate: counts[FrameClass::Control as usize] as f32 / epoch_secs,
            unique_bssids: bssid_set.len() as u32,
            total_frames: total,
            dominant_class,
            ao_target_ratio,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a zeroed FrameEntry with just bssid, frame_type, and
    /// frame_subtype populated (all other fields zeroed).
    fn make_entry(bssid: [u8; 6], frame_type: u8, frame_subtype: u8) -> FrameEntry {
        FrameEntry {
            bssid,
            frame_type,
            frame_subtype,
            channel: 0,
            rssi: 0,
            flags: 0,
            _pad: 0,
            seq_num: 0,
            timestamp_ms: 0,
            ssid_hash: 0,
            frame_len: 0,
            _reserved: [0; 6],
        }
    }

    #[test]
    fn test_empty_batch() {
        let env = RfEnvironment::compute(&[], 1.0, &HashSet::new());
        assert_eq!(env.beacon_rate, 0.0);
        assert_eq!(env.probe_rate, 0.0);
        assert_eq!(env.deauth_rate, 0.0);
        assert_eq!(env.data_rate, 0.0);
        assert_eq!(env.control_rate, 0.0);
        assert_eq!(env.unique_bssids, 0);
        assert_eq!(env.total_frames, 0);
        assert_eq!(env.dominant_class, FrameClass::Unknown);
        assert_eq!(env.ao_target_ratio, 0.0);
    }

    #[test]
    fn test_beacon_only_batch() {
        let bssid = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let entry = make_entry(bssid, 0, 8);
        let results: Vec<(FrameClass, FrameEntry)> = vec![
            (FrameClass::Beacon, entry),
            (FrameClass::Beacon, entry),
            (FrameClass::Beacon, entry),
            (FrameClass::Beacon, entry),
            (FrameClass::Beacon, entry),
        ];

        let env = RfEnvironment::compute(&results, 2.5, &HashSet::new());

        // 5 beacons / 2.5 seconds = 2.0 beacons/sec
        assert!((env.beacon_rate - 2.0).abs() < f32::EPSILON);
        assert_eq!(env.probe_rate, 0.0);
        assert_eq!(env.deauth_rate, 0.0);
        assert_eq!(env.total_frames, 5);
        assert_eq!(env.unique_bssids, 1);
        assert_eq!(env.dominant_class, FrameClass::Beacon);
    }

    #[test]
    fn test_mixed_batch() {
        let bssid_a = [1, 2, 3, 4, 5, 6];
        let bssid_b = [7, 8, 9, 10, 11, 12];

        let results: Vec<(FrameClass, FrameEntry)> = vec![
            // 3 beacons
            (FrameClass::Beacon, make_entry(bssid_a, 0, 8)),
            (FrameClass::Beacon, make_entry(bssid_a, 0, 8)),
            (FrameClass::Beacon, make_entry(bssid_b, 0, 8)),
            // 5 data frames — should be dominant
            (FrameClass::Data, make_entry(bssid_a, 2, 0)),
            (FrameClass::Data, make_entry(bssid_a, 2, 0)),
            (FrameClass::Data, make_entry(bssid_b, 2, 0)),
            (FrameClass::Data, make_entry(bssid_b, 2, 0)),
            (FrameClass::Data, make_entry(bssid_b, 2, 0)),
            // 2 deauths
            (FrameClass::Deauth, make_entry(bssid_a, 0, 12)),
            (FrameClass::Deauth, make_entry(bssid_b, 0, 12)),
        ];

        let env = RfEnvironment::compute(&results, 5.0, &HashSet::new());

        // Rates: beacons=3/5=0.6, data=5/5=1.0, deauth=2/5=0.4
        assert!((env.beacon_rate - 0.6).abs() < f32::EPSILON);
        assert!((env.data_rate - 1.0).abs() < f32::EPSILON);
        assert!((env.deauth_rate - 0.4).abs() < f32::EPSILON);
        assert_eq!(env.probe_rate, 0.0);
        assert_eq!(env.control_rate, 0.0);
        assert_eq!(env.total_frames, 10);
        assert_eq!(env.unique_bssids, 2);
        assert_eq!(env.dominant_class, FrameClass::Data);
    }

    #[test]
    fn test_unique_bssids() {
        let bssid_a = [1, 0, 0, 0, 0, 0];
        let bssid_b = [2, 0, 0, 0, 0, 0];
        let bssid_c = [3, 0, 0, 0, 0, 0];

        // 6 frames from 3 BSSIDs (with duplicates)
        let results: Vec<(FrameClass, FrameEntry)> = vec![
            (FrameClass::Beacon, make_entry(bssid_a, 0, 8)),
            (FrameClass::Beacon, make_entry(bssid_a, 0, 8)), // dup
            (FrameClass::Beacon, make_entry(bssid_b, 0, 8)),
            (FrameClass::Beacon, make_entry(bssid_b, 0, 8)), // dup
            (FrameClass::Beacon, make_entry(bssid_c, 0, 8)),
            (FrameClass::Beacon, make_entry(bssid_a, 0, 8)), // dup
        ];

        let env = RfEnvironment::compute(&results, 1.0, &HashSet::new());
        assert_eq!(env.unique_bssids, 3);
        assert_eq!(env.total_frames, 6);
    }

    #[test]
    fn test_ao_target_ratio() {
        let target_bssid = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01];
        let other_bssid = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];

        let mut ao_bssids = HashSet::new();
        ao_bssids.insert(target_bssid);

        // 2 frames from target, 3 from other => ratio = 2/5 = 0.4
        let results: Vec<(FrameClass, FrameEntry)> = vec![
            (FrameClass::Beacon, make_entry(target_bssid, 0, 8)),
            (FrameClass::Data, make_entry(other_bssid, 2, 0)),
            (FrameClass::Beacon, make_entry(target_bssid, 0, 8)),
            (FrameClass::Data, make_entry(other_bssid, 2, 0)),
            (FrameClass::Data, make_entry(other_bssid, 2, 0)),
        ];

        let env = RfEnvironment::compute(&results, 1.0, &ao_bssids);

        assert!((env.ao_target_ratio - 0.4).abs() < f32::EPSILON);
        assert_eq!(env.unique_bssids, 2);
        assert_eq!(env.total_frames, 5);
    }

    #[test]
    fn test_deauth_storm_threshold() {
        // Verify threshold constants have the expected values.
        assert_eq!(BUSY_THRESHOLD, 100);
        assert!((DEAUTH_STORM_RATE - 10.0).abs() < f32::EPSILON);
        assert!((PROBE_FLOOD_RATE - 20.0).abs() < f32::EPSILON);
        assert_eq!(RICH_BSSID_COUNT, 20);
    }
}
