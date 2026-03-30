//! RAGE slider preset table.
//!
//! 3 aggression levels mapping to stress-test-validated combinations
//! of attack rate, dwell time, and channel list.

/// A single RAGE preset level.
#[derive(Debug, Clone, Copy)]
pub struct RagePreset {
    pub level: u8,
    pub name: &'static str,
    pub rate: u32,
    pub dwell_ms: u64,
    pub channels: &'static [u8],
}

const ALL_13: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
const SAFE_3: &[u8] = &[1, 6, 11];

/// Stress-test-validated presets (2026-03-30).
/// 3 aggression levels: Conservative, Balanced, Maximum.
pub const PRESETS: [RagePreset; 3] = [
    RagePreset {
        level: 1,
        name: "Chill",
        rate: 1,
        dwell_ms: 5000,
        channels: SAFE_3,
    },
    RagePreset {
        level: 2,
        name: "Hunt",
        rate: 2,
        dwell_ms: 1000,
        channels: ALL_13,
    },
    RagePreset {
        level: 3,
        name: "RAGE",
        rate: 3,
        dwell_ms: 500,
        channels: ALL_13,
    },
];

/// Look up a preset by level (1-3). Returns `None` for out-of-range.
pub fn preset(level: u8) -> Option<&'static RagePreset> {
    PRESETS.iter().find(|p| p.level == level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_presets_exist() {
        for level in 1..=3 {
            assert!(preset(level).is_some(), "missing preset for level {level}");
        }
    }

    #[test]
    fn test_out_of_range_returns_none() {
        assert!(preset(0).is_none());
        assert!(preset(4).is_none());
    }

    #[test]
    fn test_preset_values() {
        let p = preset(1).unwrap();
        assert_eq!(p.name, "Chill");
        assert_eq!(p.rate, 1);
        assert_eq!(p.dwell_ms, 5000);
        assert_eq!(p.channels, &[1, 6, 11]);

        let p = preset(3).unwrap();
        assert_eq!(p.name, "RAGE");
        assert_eq!(p.rate, 3);
        assert_eq!(p.dwell_ms, 500);
        assert_eq!(p.channels, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
    }
}
