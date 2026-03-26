use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::gpu::runtime::trace::GpuRuntimeSummary;

const DEFAULT_ENV_SOURCE: &str = "OXIGOTCHI_GPU_SUMMARY_SOURCE";

#[derive(Debug, Default)]
pub struct GpuRuntimeIngestor {
    last_loaded_from: Option<PathBuf>,
    last_modified: Option<SystemTime>,
    cached: Option<GpuRuntimeSummary>,
}

impl GpuRuntimeIngestor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(&mut self, configured_source: &str) -> io::Result<Option<GpuRuntimeSummary>> {
        let Some(summary_path) = resolve_summary_path(configured_source)? else {
            return Ok(None);
        };

        let modified = fs::metadata(&summary_path)?.modified()?;
        if self
            .last_loaded_from
            .as_ref()
            .is_some_and(|p| p == &summary_path)
            && self.last_modified == Some(modified)
        {
            return Ok(self.cached.clone());
        }

        let mut summary = load_summary_from_path(&summary_path)?;
        summary.classify();

        self.last_loaded_from = Some(summary_path);
        self.last_modified = Some(modified);
        self.cached = Some(summary.clone());

        Ok(Some(summary))
    }
}

fn resolve_summary_path(configured_source: &str) -> io::Result<Option<PathBuf>> {
    let source = if configured_source.trim().is_empty() {
        std::env::var(DEFAULT_ENV_SOURCE).unwrap_or_default()
    } else {
        configured_source.to_string()
    };

    if source.trim().is_empty() {
        return Ok(None);
    }

    let source_path = PathBuf::from(source);
    if source_path.is_file() {
        return Ok(Some(source_path));
    }

    if source_path.is_dir() {
        return newest_summary_under_dir(&source_path);
    }

    Ok(None)
}

fn newest_summary_under_dir(root: &Path) -> io::Result<Option<PathBuf>> {
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            if path.file_name().and_then(|n| n.to_str()) != Some("bridge_trace_summary.json") {
                continue;
            }

            let modified = entry.metadata()?.modified()?;
            match &newest {
                Some((current, _)) if modified <= *current => {}
                _ => newest = Some((modified, path)),
            }
        }
    }

    Ok(newest.map(|(_, path)| path))
}

fn load_summary_from_path(summary_path: &Path) -> io::Result<GpuRuntimeSummary> {
    let raw = fs::read_to_string(summary_path)?;
    let bridge: BridgeTraceSummaryFile = serde_json::from_str(&raw).map_err(io::Error::other)?;

    let mut summary = GpuRuntimeSummary {
        last_seen_at: Some(DateTime::<Utc>::from(
            fs::metadata(summary_path)?.modified()?,
        )),
        ..GpuRuntimeSummary::default()
    };

    for phase in bridge.phases.values() {
        summary.vc4_setup_seen |= phase.pattern_count("vc4") > 0
            || phase.pattern_count("v3d") > 0
            || phase.pattern_count("egl") > 0
            || phase.pattern_count("gl") > 0
            || interesting_lines_match(&phase.interesting_lines, &["VC4", "V3D", "EGL"]);
    }

    let capture_dir = PathBuf::from(&bridge.capture_dir);
    let trace_signals = scan_trace_signals(&capture_dir)?;
    summary.card0_seen |= trace_signals.card0_seen;
    summary.renderd128_seen |= trace_signals.renderd128_seen;
    summary.vc4_setup_seen |= trace_signals.vc4_setup_seen;
    summary.vc4_submit_cl_seen |= trace_signals.vc4_submit_cl_seen;

    Ok(summary)
}

fn interesting_lines_match(lines: &[String], patterns: &[&str]) -> bool {
    lines
        .iter()
        .any(|line| patterns.iter().any(|pattern| line.contains(pattern)))
}

#[derive(Debug, Default)]
struct TraceSignals {
    card0_seen: bool,
    renderd128_seen: bool,
    vc4_setup_seen: bool,
    vc4_submit_cl_seen: bool,
}

fn scan_trace_signals(root: &Path) -> io::Result<TraceSignals> {
    if !root.exists() {
        return Ok(TraceSignals::default());
    }

    let mut signals = TraceSignals::default();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };

            signals.card0_seen |= contents.contains("/dev/dri/card0");
            signals.renderd128_seen |= contents.contains("/dev/dri/renderD128");
            signals.vc4_setup_seen |= contents.contains("DRM_IOCTL_VC4_GET_PARAM")
                || contents.contains("DRM_IOCTL_GET_CAP")
                || contents.contains("DRM_IOCTL_SYNCOBJ_CREATE")
                || contents.contains("DRM_IOCTL_VC4_CREATE_BO")
                || contents.contains("DRM_IOCTL_VC4_GEM_MADVISE");
            signals.vc4_submit_cl_seen |= contents.contains("DRM_IOCTL_VC4_SUBMIT_CL");
        }
    }

    Ok(signals)
}

#[derive(Debug, Deserialize)]
struct BridgeTraceSummaryFile {
    capture_dir: String,
    phases: HashMap<String, BridgeTracePhase>,
}

#[derive(Debug, Deserialize)]
struct BridgeTracePhase {
    #[serde(default)]
    pattern_counts: HashMap<String, u64>,
    #[serde(default)]
    interesting_lines: Vec<String>,
}

impl BridgeTracePhase {
    fn pattern_count(&self, name: &str) -> u64 {
        self.pattern_counts.get(name).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingests_summary_file_and_trace_signals() {
        let dir = tempfile::tempdir().unwrap();
        let capture_dir = dir.path().join("capture");
        let phase_dir = capture_dir.join("gpu_runtime_bridge").join("graphics");
        fs::create_dir_all(&phase_dir).unwrap();
        fs::write(
            phase_dir.join("strace.txt"),
            "openat(AT_FDCWD, \"/dev/dri/renderD128\", O_RDWR)\nDRM_IOCTL_VC4_GET_PARAM\nDRM_IOCTL_VC4_SUBMIT_CL\n",
        )
        .unwrap();

        let summary_path = dir.path().join("bridge_trace_summary.json");
        fs::write(
            &summary_path,
            serde_json::json!({
                "capture_dir": capture_dir.display().to_string(),
                "phases": {
                    "graphics": {
                        "pattern_counts": {"vc4": 2, "v3d": 2, "egl": 4, "gl": 8},
                        "interesting_lines": ["OpenGL ES profile renderer: VC4 V3D 2.1"]
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let mut ingestor = GpuRuntimeIngestor::new();
        let summary = ingestor
            .load(summary_path.to_str().unwrap())
            .unwrap()
            .unwrap();

        assert!(summary.renderd128_seen);
        assert!(summary.vc4_setup_seen);
        assert!(summary.vc4_submit_cl_seen);
        assert_eq!(
            summary.strongest_signal,
            Some(crate::gpu::runtime::signals::GpuRuntimeSignal::GpuSubmissionObserved)
        );
    }

    #[test]
    fn finds_newest_summary_under_directory() {
        let dir = tempfile::tempdir().unwrap();
        let older_dir = dir.path().join("older");
        let newer_dir = dir.path().join("newer");
        fs::create_dir_all(&older_dir).unwrap();
        fs::create_dir_all(&newer_dir).unwrap();
        fs::write(
            older_dir.join("bridge_trace_summary.json"),
            r#"{"capture_dir":"","phases":{}}"#,
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let newer = newer_dir.join("bridge_trace_summary.json");
        fs::write(&newer, r#"{"capture_dir":"","phases":{}}"#).unwrap();

        let path = newest_summary_under_dir(dir.path()).unwrap().unwrap();
        assert_eq!(path, newer);
    }
}
