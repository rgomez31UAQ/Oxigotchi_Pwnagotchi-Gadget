// QPU engine — top-level orchestrator that owns all QPU resources and provides
// the frame submission + batch classification API for the rest of the daemon.
//
// QpuEngine manages:
//   - Mailbox (vcio) for GPU memory allocation
//   - V3dRegs for QPU execution via register poke
//   - RingBuf (SPSC in GPU memory) for staging frame headers
//   - Classifier for QPU/CPU frame classification
//
// Lifecycle: init() -> submit_frame() + process_batch() in a loop -> Drop

use super::classifier::FrameClass;
use super::ringbuf::FrameEntry;

#[cfg(target_os = "linux")]
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::time::Instant;
#[cfg(target_os = "linux")]
use super::classifier::Classifier;
#[cfg(target_os = "linux")]
use super::mailbox::{GpuMem, Mailbox, V3dRegs};
#[cfg(target_os = "linux")]
use super::ringbuf::{extract_frame_entry, RingBuf};

// ---------------------------------------------------------------------------
// QpuStats — throughput and overflow statistics
// ---------------------------------------------------------------------------

/// QPU engine statistics.
#[derive(Debug, Clone, Default)]
pub struct QpuStats {
    pub frames_submitted: u64,
    pub frames_classified: u64,
    pub batches_processed: u64,
    pub overflow_count: u64,
    pub qpu_available: bool,
    pub num_qpus: u32,
    pub last_batch_size: u32,
    pub last_batch_duration_us: u64,
}

// ---------------------------------------------------------------------------
// QpuConfig — engine configuration
// ---------------------------------------------------------------------------

/// Configuration for QpuEngine.
pub struct QpuConfig {
    pub ring_capacity: u32,   // Ring buffer entries (power of 2, default 256)
    pub batch_capacity: u32,  // Max frames per classify batch (default 256)
    pub ring_alloc_size: u32, // GPU alloc for ring buffer (default 16384)
}

impl Default for QpuConfig {
    fn default() -> Self {
        QpuConfig {
            ring_capacity: 256,
            batch_capacity: 256,
            ring_alloc_size: 16384, // 64-byte header + 256 * 32-byte entries = 8256, round up
        }
    }
}

// ---------------------------------------------------------------------------
// QpuEngine — Linux implementation
// ---------------------------------------------------------------------------

/// Top-level QPU engine — owns all GPU resources and provides the
/// frame submission + batch classification API.
#[cfg(target_os = "linux")]
pub struct QpuEngine {
    mbox: Arc<Mailbox>,
    v3d: V3dRegs,
    ring: RingBuf,
    classifier: Classifier,
    stats: QpuStats,
    epoch_start: Instant,
}

#[cfg(target_os = "linux")]
impl QpuEngine {
    /// Initialize the QPU engine: open mailbox, enable QPUs, map V3D,
    /// allocate ring buffer and classifier.
    pub fn init(config: QpuConfig) -> Result<Self, String> {
        // 1. Open mailbox
        let mbox = Arc::new(Mailbox::open()?);

        // 2. Enable QPUs
        mbox.qpu_enable(true)?;

        // 3. Map V3D registers (verifies IDENT0 "V3D" signature)
        let v3d = V3dRegs::map()?;

        // 4. Allocate ring buffer in GPU memory
        let ring_mem = GpuMem::alloc(mbox.clone(), config.ring_alloc_size)?;
        let ring = RingBuf::new(ring_mem, config.ring_capacity)?;

        // 5. Create classifier (allocates code + output GPU memory)
        let classifier = Classifier::new(mbox.clone(), config.batch_capacity)?;

        let stats = QpuStats {
            qpu_available: true,
            num_qpus: v3d.num_qpus(),
            ..Default::default()
        };

        Ok(QpuEngine {
            mbox,
            v3d,
            ring,
            classifier,
            stats,
            epoch_start: Instant::now(),
        })
    }

    /// Submit a raw 802.11 frame (radiotap + MAC header) for classification.
    /// Extracts the frame entry and pushes it to the ring buffer.
    /// Returns true if the frame was accepted, false if the buffer is full.
    pub fn submit_frame(&mut self, raw: &[u8], channel: u8, rssi: i8) -> bool {
        let timestamp_ms = self.epoch_start.elapsed().as_millis() as u32;

        let entry = match extract_frame_entry(raw, channel, rssi, timestamp_ms) {
            Some(e) => e,
            None => return false, // malformed frame, skip
        };

        let accepted = self.ring.push(&entry);
        if accepted {
            self.stats.frames_submitted += 1;
        } else {
            self.stats.overflow_count += 1;
        }
        accepted
    }

    /// Process accumulated frames in the ring buffer.
    /// Uses QPU if available, falls back to CPU classification.
    /// Returns the classified frames.
    pub fn process_batch(&mut self) -> Vec<(FrameClass, FrameEntry)> {
        if self.ring.available() == 0 {
            return Vec::new();
        }

        let batch_start = Instant::now();

        // Try QPU classification first
        let results = match self.classifier.classify_batch(&mut self.ring, &self.v3d) {
            Ok(r) if !r.is_empty() => r,
            _ => {
                // CPU fallback: drain entries from ring and classify on ARM
                let entries = self.ring.drain(self.ring.available());
                if entries.is_empty() {
                    Vec::new()
                } else {
                    let classes = Classifier::classify_cpu(&entries);
                    classes.into_iter().zip(entries).map(|(c, e)| (c, e)).collect()
                }
            }
        };

        let batch_duration = batch_start.elapsed();
        self.stats.batches_processed += 1;
        self.stats.last_batch_size = results.len() as u32;
        self.stats.last_batch_duration_us = batch_duration.as_micros() as u64;
        self.stats.frames_classified += results.len() as u64;

        results
    }

    /// Get current statistics.
    pub fn stats(&self) -> &QpuStats {
        &self.stats
    }

    /// Get mutable reference to stats (for updating from external sources).
    pub fn stats_mut(&mut self) -> &mut QpuStats {
        &mut self.stats
    }

    /// Number of frames waiting in the ring buffer.
    pub fn pending_count(&self) -> u32 {
        self.ring.available()
    }

    /// Reset the ring buffer (call between epochs).
    pub fn reset_ring(&mut self) {
        self.ring.reset();
    }

    /// Check if QPU hardware is available.
    pub fn is_qpu_available(&self) -> bool {
        self.stats.qpu_available
    }

    /// Get the number of QPU cores detected.
    pub fn num_qpus(&self) -> u32 {
        self.stats.num_qpus
    }
}

#[cfg(target_os = "linux")]
impl Drop for QpuEngine {
    fn drop(&mut self) {
        // Disable QPUs on cleanup.
        // Ring buffer, classifier, and V3D are cleaned up by their own Drop impls.
        let _ = self.mbox.qpu_enable(false);
    }
}

// ---------------------------------------------------------------------------
// QpuEngine — non-Linux stub
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "linux"))]
pub struct QpuEngine;

#[cfg(not(target_os = "linux"))]
impl QpuEngine {
    pub fn init(_config: QpuConfig) -> Result<Self, String> {
        Err("QpuEngine requires Linux with VideoCore IV".into())
    }

    pub fn submit_frame(&mut self, _raw: &[u8], _channel: u8, _rssi: i8) -> bool {
        false
    }

    pub fn process_batch(&mut self) -> Vec<(FrameClass, FrameEntry)> {
        Vec::new()
    }

    pub fn stats(&self) -> &QpuStats {
        static DEFAULT_STATS: QpuStats = QpuStats {
            frames_submitted: 0,
            frames_classified: 0,
            batches_processed: 0,
            overflow_count: 0,
            qpu_available: false,
            num_qpus: 0,
            last_batch_size: 0,
            last_batch_duration_us: 0,
        };
        &DEFAULT_STATS
    }

    pub fn stats_mut(&mut self) -> &mut QpuStats {
        unimplemented!("QpuEngine not available on this platform")
    }

    pub fn pending_count(&self) -> u32 {
        0
    }

    pub fn reset_ring(&mut self) {}

    pub fn is_qpu_available(&self) -> bool {
        false
    }

    pub fn num_qpus(&self) -> u32 {
        0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qpu_config_default() {
        let config = QpuConfig::default();
        assert_eq!(config.ring_capacity, 256);
        assert_eq!(config.batch_capacity, 256);
        assert_eq!(config.ring_alloc_size, 16384);
    }

    #[test]
    fn test_qpu_stats_default() {
        let stats = QpuStats::default();
        assert_eq!(stats.frames_submitted, 0);
        assert_eq!(stats.frames_classified, 0);
        assert_eq!(stats.batches_processed, 0);
        assert_eq!(stats.overflow_count, 0);
        assert!(!stats.qpu_available);
        assert_eq!(stats.num_qpus, 0);
        assert_eq!(stats.last_batch_size, 0);
        assert_eq!(stats.last_batch_duration_us, 0);
    }

    #[test]
    fn test_init_fails_on_non_pi() {
        // Should fail gracefully on non-Pi hardware
        let result = QpuEngine::init(QpuConfig::default());
        // We just verify it returns a Result
        let _ = result;
    }

    #[test]
    fn test_qpu_stats_clone() {
        let mut stats = QpuStats::default();
        stats.frames_submitted = 42;
        stats.qpu_available = true;
        let cloned = stats.clone();
        assert_eq!(cloned.frames_submitted, 42);
        assert!(cloned.qpu_available);
    }

    #[test]
    fn test_qpu_config_custom() {
        let config = QpuConfig {
            ring_capacity: 512,
            batch_capacity: 128,
            ring_alloc_size: 32768,
        };
        assert_eq!(config.ring_capacity, 512);
        assert_eq!(config.batch_capacity, 128);
        assert_eq!(config.ring_alloc_size, 32768);
    }
}
