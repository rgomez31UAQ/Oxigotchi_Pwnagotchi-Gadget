pub mod classifier;
pub mod engine;
pub mod mailbox;
pub mod rf;
pub mod ringbuf;

use serde::{Deserialize, Serialize};

/// QPU feature configuration — loaded from TOML config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QpuFeatureConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_ring_capacity")]
    pub ring_capacity: u32,
    #[serde(default = "default_batch_capacity")]
    pub batch_capacity: u32,
    #[serde(default = "default_ring_alloc_size")]
    pub ring_alloc_size: u32,
}

fn default_enabled() -> bool { false }
fn default_ring_capacity() -> u32 { 256 }
fn default_batch_capacity() -> u32 { 256 }
fn default_ring_alloc_size() -> u32 { 16384 }

impl Default for QpuFeatureConfig {
    fn default() -> Self {
        QpuFeatureConfig {
            enabled: false,
            ring_capacity: default_ring_capacity(),
            batch_capacity: default_batch_capacity(),
            ring_alloc_size: default_ring_alloc_size(),
        }
    }
}

impl QpuFeatureConfig {
    /// Convert to the engine's QpuConfig.
    pub fn to_engine_config(&self) -> engine::QpuConfig {
        engine::QpuConfig {
            ring_capacity: self.ring_capacity,
            batch_capacity: self.batch_capacity,
            ring_alloc_size: self.ring_alloc_size,
        }
    }
}
