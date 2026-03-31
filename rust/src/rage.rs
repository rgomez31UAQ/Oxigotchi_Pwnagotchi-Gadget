//! RAGE slider preset table.
//!
//! 7 aggression levels mapping to stress-test-validated combinations
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

/// Stress-test-validated presets (2026-03-26).
/// Each step changes exactly one variable from the previous level.
pub const PRESETS: [RagePreset; 7] = [
    RagePreset {
        level: 1,
        name: "Chill",
        rate: 1,
        dwell_ms: 5000,
        channels: SAFE_3,
    },
    RagePreset {
        level: 2,
        name: "Lurk",
        rate: 1,
        dwell_ms: 2000,
        channels: SAFE_3,
    },
    RagePreset {
        level: 3,
        name: "Prowl",
        rate: 1,
        dwell_ms: 2000,
        channels: ALL_13,
    },
    RagePreset {
        level: 4,
        name: "Hunt",
        rate: 2,
        dwell_ms: 2000,
        channels: ALL_13,
    },
    RagePreset {
        level: 5,
        name: "RAGE",
        rate: 2,
        dwell_ms: 1000,
        channels: ALL_13,
    },
    RagePreset {
        level: 6,
        name: "FURY",
        rate: 3,
        dwell_ms: 1000,
        channels: ALL_13,
    },
    RagePreset {
        level: 7,
        name: "YOLO",
        rate: 3,
        dwell_ms: 500,
        channels: ALL_13,
    },
];

/// Look up a preset by level (1-7). Returns `None` for out-of-range.
pub fn preset(level: u8) -> Option<&'static RagePreset> {
    PRESETS.iter().find(|p| p.level == level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_presets_exist() {
        for level in 1..=7 {
            assert!(preset(level).is_some(), "missing preset for level {level}");
        }
    }

    #[test]
    fn test_out_of_range_returns_none() {
        assert!(preset(0).is_none());
        assert!(preset(8).is_none());
    }

    #[test]
    fn test_preset_values() {
        let p = preset(1).unwrap();
        assert_eq!(p.name, "Chill");
        assert_eq!(p.rate, 1);
        assert_eq!(p.dwell_ms, 5000);
        assert_eq!(p.channels, &[1, 6, 11]);

        let p = preset(7).unwrap();
        assert_eq!(p.name, "YOLO");
        assert_eq!(p.rate, 3);
        assert_eq!(p.dwell_ms, 500);
        assert_eq!(p.channels, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
    }

    #[test]
    fn test_each_step_changes_one_variable() {
        for i in 2..=7u8 {
            let prev = preset(i - 1).unwrap();
            let curr = preset(i).unwrap();
            let diffs = [
                prev.rate != curr.rate,
                prev.dwell_ms != curr.dwell_ms,
                prev.channels != curr.channels,
            ];
            let diff_count: usize = diffs.iter().filter(|&&d| d).count();
            assert_eq!(
                diff_count,
                1,
                "level {i} changes {} variables from level {} (expected 1)",
                diff_count,
                i - 1
            );
        }
    }
}
