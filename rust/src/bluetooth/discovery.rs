use std::collections::HashMap;

use chrono::Utc;

use crate::bluetooth::model::observation::{BtDeviceObservation, BtDiscoveryObservation};
use crate::bluetooth::model::state::BtSummaryState;

#[derive(Debug, Default)]
pub struct BtDiscoveryWorker {
    devices: HashMap<String, BtDeviceObservation>,
}

impl BtDiscoveryWorker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.devices.clear();
    }

    pub fn apply(&mut self, observation: BtDiscoveryObservation) -> BtSummaryState {
        match observation {
            BtDiscoveryObservation::DeviceSeen(device) => {
                if let Some(existing) = self.devices.get_mut(&device.id) {
                    // Update existing device
                    existing.ts = device.ts;
                    existing.seen_count += 1;
                    existing.rssi = device.rssi;

                    // Update rssi_best if the new reading is stronger
                    if let Some(new_rssi) = device.rssi {
                        match existing.rssi_best {
                            Some(best) if new_rssi > best => {
                                existing.rssi_best = Some(new_rssi);
                            }
                            None => {
                                existing.rssi_best = Some(new_rssi);
                            }
                            _ => {}
                        }
                    }

                    // Update name if the new observation has one
                    if device.name.is_some() {
                        existing.name = device.name;
                    }
                    // Promote to connectable if ever seen as connectable
                    if device.connectable {
                        existing.connectable = true;
                    }
                } else {
                    // New device — insert as-is
                    self.devices.insert(device.id.clone(), device);
                }
            }
            BtDiscoveryObservation::DeviceLost { id, .. } => {
                self.devices.remove(&id);
            }
            BtDiscoveryObservation::ScanStarted | BtDiscoveryObservation::ScanStopped => {}
        }

        let strongest = self.devices.values().filter_map(|d| d.rssi).max();
        BtSummaryState {
            devices_now: self.devices.len() as u32,
            strongest_rssi_recent: strongest,
        }
    }

    pub fn summary(&self) -> BtSummaryState {
        let strongest = self.devices.values().filter_map(|d| d.rssi).max();
        BtSummaryState {
            devices_now: self.devices.len() as u32,
            strongest_rssi_recent: strongest,
        }
    }

    /// Return all devices sorted by RSSI, strongest (closest to 0) first.
    /// Devices with no RSSI are placed at the end.
    pub fn devices_by_rssi(&self) -> Vec<&BtDeviceObservation> {
        let mut devs: Vec<&BtDeviceObservation> = self.devices.values().collect();
        devs.sort_by(|a, b| {
            match (a.rssi, b.rssi) {
                (Some(ra), Some(rb)) => rb.cmp(&ra), // higher (closer to 0) = stronger
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });
        devs
    }

    /// Get a mutable reference to a device by ID (for updating attack state, etc.)
    pub fn get_device_mut(&mut self, id: &str) -> Option<&mut BtDeviceObservation> {
        self.devices.get_mut(id)
    }

    /// Get the L2CAP address type byte for a device.
    /// Returns: 0 = BR/EDR, 1 = LE public, 2 = LE random. Defaults to 1 (LE public).
    pub fn get_device_addr_type(&self, device_id: &str) -> u8 {
        if let Some(dev) = self.devices.get(device_id) {
            match dev.address_type.as_deref() {
                Some("public") => 1,
                Some("random") => 2,
                _ => match dev.transport {
                    crate::bluetooth::model::observation::BtTransport::Classic => 0,
                    _ => 1,
                },
            }
        } else {
            1
        }
    }

    /// Remove devices not seen since the cutoff (now - max_age_secs).
    pub fn prune(&mut self, max_age_secs: u64) {
        let cutoff = Utc::now() - chrono::Duration::seconds(max_age_secs as i64);
        self.devices.retain(|_, d| d.ts >= cutoff);
    }

    /// LRU eviction: if more than `limit` devices, remove the ones with the oldest `ts`.
    pub fn enforce_limit(&mut self, limit: usize) {
        if self.devices.len() <= limit {
            return;
        }

        // Collect (id, ts) and sort by ts ascending (oldest first)
        let mut entries: Vec<(String, chrono::DateTime<chrono::Utc>)> = self
            .devices
            .iter()
            .map(|(id, d)| (id.clone(), d.ts))
            .collect();
        entries.sort_by_key(|(_, ts)| *ts);

        // Remove the oldest entries until we're at the limit
        let to_remove = self.devices.len() - limit;
        for (id, _) in entries.into_iter().take(to_remove) {
            self.devices.remove(&id);
        }
    }
}
