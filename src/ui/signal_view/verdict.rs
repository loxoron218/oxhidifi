//! Whole-path verdict presentation shared by the player badge and Signal tab.
//!
//! Dedicated indicator icons plus structured flip tracing for the live
//! verdict surfaces. Used by the badge in
//! [`signal_badge`](crate::ui::player::signal_badge) and the chain rows in
//! [`signal_chain`](crate::ui::signal_view::signal_chain) plus the header in
//! [`signal_header`](crate::ui::signal_view::signal_header); defined here so
//! the `Signal` tab never imports from the player panel.

use tracing::info;

use crate::playback::signal_path::QualityVerdict::{self, BitPerfect, Limited, Processed};

/// Dedicated indicator icon per verdict state (never color alone).
///
/// # Arguments
///
/// * `verdict` - Whole-path verdict to illustrate.
///
/// # Returns
///
/// * `&'static str` - Symbolic icon name with a distinct shape per state.
#[must_use]
pub const fn verdict_icon(verdict: QualityVerdict) -> &'static str {
    match verdict {
        BitPerfect => "media-optical-cd-audio-symbolic",
        Processed => "audio-card-symbolic",
        Limited => "dialog-warning-symbolic",
    }
}

/// Emit structured tracing fields for a whole-path verdict flip.
///
/// Both live surfaces (player badge and tab header) report through here so
/// DSP-volume and output-mode changes are observable without leaving the tab.
///
/// # Arguments
///
/// * `previous` - Verdict previously shown on the surface.
/// * `next` - Verdict carried by the incoming snapshot.
/// * `generation` - Snapshot generation carrying the flip.
/// * `track_id` - Track carried by the snapshot, if any.
/// * `surface` - UI surface reporting the flip.
pub fn trace_verdict_flip(
    previous: QualityVerdict,
    next: QualityVerdict,
    generation: u64,
    track_id: Option<i64>,
    surface: &str,
) {
    info!(
        previous_verdict = previous.label(),
        new_verdict = next.label(),
        generation,
        track_id,
        surface,
        "Signal path verdict changed",
    );
}
