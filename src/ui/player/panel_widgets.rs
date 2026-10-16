//! Player panel widget handles, cover sizing, and time formatting.
//!
//! Shared handles for the sidebar player panel: track labels, playback
//! widgets, cover-size floors, and the `MM:SS` formatter. The transport
//! builders in [`deck`](crate::ui::player::deck), the metadata handlers in
//! [`now_playing`](crate::ui::player::now_playing), and the event listeners in
//! [`playback_events`](crate::ui::player::playback_events) all import from
//! this leaf, so none of them import each other through
//! [`sidebar`](crate::ui::player::sidebar).

use libadwaita::gtk::{Button, Label, Picture, Scale};

/// Logical display size of the player cover in pixels.
///
/// The `Picture` widget requests this size; the decoded texture is larger
/// (see [`PLAYER_COVER_SIZE`]) so `HiDPI` displays stay sharp.
pub const PLAYER_COVER_LOGICAL_SIZE: i32 = 280;

/// Decoded size for the player cover in pixels.
///
/// Twice the logical size so a 2x scale factor still renders 1:1 source
/// pixels instead of upscaling a 280 px texture.
pub const PLAYER_COVER_SIZE: i32 = 560;

/// Minimum texture dimension (width or height) required to use a cached
/// cover in the player panel (displayed at 280×280 logical px).
///
/// Textures decoded for the column/grid views are rejected, forcing a
/// proper-sized decode via the async metadata path.
pub const COVER_MIN_SIZE: i32 = PLAYER_COVER_LOGICAL_SIZE;

/// Tuple of resolved metadata: `(title, artist, album, artwork_path, format_info, album_id)`.
pub type MetaResult = (String, String, String, Option<String>, String, i64);

/// Widget references for playback control updates.
#[derive(Debug, Clone)]
pub struct PlaybackWidgets {
    /// Track title, artist, album, format labels.
    pub labels: TrackLabels,
    /// Album artwork display widget.
    pub artwork_image: Picture,
    /// Play/pause transport button.
    pub play_button: Button,
    /// Seek slider for position control.
    pub seek_scale: Scale,
    /// Label showing current playback position.
    pub current_time: Label,
    /// Label showing total track duration.
    pub total_time: Label,
    /// Output-mode toggle button (BP / RM).
    pub output_mode_btn: Button,
    /// Volume scale slider, greyed out in bit-perfect mode.
    pub volume_scale: Scale,
}

/// Labels for track metadata display.
#[derive(Debug, Clone)]
pub struct TrackLabels {
    /// Title label.
    pub title: Label,
    /// Artist label.
    pub artist: Label,
    /// Album label.
    pub album: Label,
    /// Format label.
    pub format: Label,
}

/// Format seconds into `MM:SS` display string.
#[must_use]
pub fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0).floor();
    let mins = (total / 60.0).floor();
    let secs = mins.mul_add(-60.0, total);
    format!("{mins:02.0}:{secs:02.0}")
}
