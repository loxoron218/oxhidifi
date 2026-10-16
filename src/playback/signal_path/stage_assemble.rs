//! Shared stage-assembly primitives for signal path snapshots.
//!
//! Positional stage appending plus shared-mixer routing used by both the
//! conversion/volume constructors in
//! [`stage_build`](crate::playback::signal_path::stage_build) and the
//! transport/output constructors in
//! [`stage_output`](crate::playback::signal_path::stage_output). Living in one
//! leaf keeps those siblings free of mutual imports.

use crate::playback::signal_path::{PathStage, QualityVerdict, StageKind};

/// Append one stage, assigning the next `position` in chain order.
pub fn push_stage(
    stages: &mut Vec<PathStage>,
    kind: StageKind,
    title: String,
    detail: String,
    explanation: String,
    verdict: QualityVerdict,
    badge_icon: &'static str,
) {
    let position = u32::try_from(stages.len()).unwrap_or(u32::MAX);
    stages.push(PathStage {
        position,
        kind,
        title,
        detail,
        explanation,
        verdict,
        badge_icon,
    });
}

/// Check whether the output targets the shared system mixer.
///
/// # Arguments
///
/// * `device_id` - Stable output device identifier.
/// * `device_name` - Human-readable output device name.
///
/// # Returns
///
/// * `bool` - Whether the path runs through the shared system mixer.
#[must_use]
pub fn is_shared_mixer(device_id: &str, device_name: &str) -> bool {
    if device_id == "default" || device_id.starts_with("default:") {
        return true;
    }
    let lower = device_name.to_lowercase();
    lower.contains("pipewire")
        || lower.contains("pulse")
        || lower.contains("system mixer")
        || lower.contains("default")
}
