//! Player panel content with artwork, track info, and playback controls.
//!
//! Displays album artwork, track title, artist, seek slider, playback
//! controls, and volume slider. Used as the content of the sidebar pane.
//! Subscribes to `PlaybackEvent` for fully event-driven updates.
//! Event handling lives in the sibling [`playback_events`] module.

use std::sync::Arc;

use libadwaita::{
    gtk::{
        Align::{Center, Start},
        Button,
        ContentFit::Cover,
        Label, Picture, Scale, ScrolledWindow,
        accessible::Property::Label as PropertyLabel,
        pango::EllipsizeMode::End,
    },
    prelude::{AccessibleExtManual, BoxExt},
};

use crate::{
    app::runtime::AppState,
    ui::{
        detail::page::build_scroll_content,
        player::{
            acoustic_fader::build_volume,
            deck::{build_queue_section, build_seek_section, build_transport},
            playback_events::spawn_async_listeners,
        },
    },
};

/// Minimum texture dimension (width or height) required to use a cached
/// cover in the player panel (displayed at 280×280).
///
/// Textures decoded at 36 px by the column view are rejected, forcing a
/// proper-sized decode via the async metadata path.
pub const COVER_MIN_SIZE: i32 = 180;

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

/// Build the player panel content area.
///
/// Returns a `ScrolledWindow` containing album artwork, track info,
/// seek slider, playback controls, volume control, and queue view.
/// Used as the content child of the sidebar's `AdwToolbarView`.
/// Listens to `PlaybackEvent` stream for fully event-driven updates.
pub fn build_player_content(state: &Arc<AppState>) -> ScrolledWindow {
    let (scroll, content) = build_scroll_content();

    let artwork_image = build_artwork_placeholder();
    content.append(&artwork_image);

    let (title_label, artist_label, album_label, format_label) = build_track_info();
    content.append(&title_label);
    content.append(&artist_label);
    content.append(&album_label);
    content.append(&format_label);

    let (seek_section, seek_scale, current_time, total_time) = build_seek_section(state);
    content.append(&seek_section);
    let (controls_section, play_button) = build_transport(state);
    content.append(&controls_section);
    let (vol_section, mode_btn, vol_scale) = build_volume(state);
    content.append(&vol_section);
    content.append(&build_queue_section(state));

    scroll.set_child(Some(&content));

    let widgets = PlaybackWidgets {
        labels: TrackLabels {
            title: title_label,
            artist: artist_label,
            album: album_label,
            format: format_label,
        },
        artwork_image,
        play_button,
        seek_scale,
        current_time,
        total_time,
        output_mode_btn: mode_btn,
        volume_scale: vol_scale,
    };

    spawn_async_listeners(state, widgets);
    scroll
}

/// Build the album artwork placeholder.
fn build_artwork_placeholder() -> Picture {
    let artwork = Picture::builder()
        .content_fit(Cover)
        .can_shrink(true)
        .halign(Center)
        .width_request(280)
        .height_request(280)
        .css_classes(["album-cover"])
        .build();
    artwork.update_property(&[PropertyLabel("Album artwork")]);
    artwork
}

/// Build the track info section (title, artist, album, and format labels).
///
/// Returns the title, artist, album, and format `Label` widgets for dynamic updates.
fn build_track_info() -> (Label, Label, Label, Label) {
    let title = Label::builder()
        .label("No track playing")
        .css_classes(["title-3", "heading"])
        .ellipsize(End)
        .max_width_chars(35)
        .halign(Start)
        .build();
    title.update_property(&[PropertyLabel("Track title")]);

    let artist = Label::builder()
        .label("")
        .css_classes(["dim-label", "body"])
        .ellipsize(End)
        .max_width_chars(35)
        .halign(Start)
        .build();
    artist.update_property(&[PropertyLabel("Artist name")]);

    let album = Label::builder()
        .label("")
        .css_classes(["dim-label", "body"])
        .ellipsize(End)
        .max_width_chars(35)
        .halign(Start)
        .build();
    album.update_property(&[PropertyLabel("Album name")]);

    let format = Label::builder()
        .label("")
        .css_classes(["dim-label", "caption"])
        .ellipsize(End)
        .max_width_chars(35)
        .halign(Start)
        .build();
    format.update_property(&[PropertyLabel("Audio format information")]);

    (title, artist, album, format)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::{Result, ensure};

    use crate::{
        app::runtime::AppState,
        ui::player::sidebar::{
            COVER_MIN_SIZE, MetaResult, PlaybackWidgets, TrackLabels, build_player_content,
            format_time,
        },
    };

    #[test]
    fn format_time_zero() {
        assert_eq!(format_time(0.0), "00:00");
    }

    #[test]
    fn format_time_minutes() {
        assert_eq!(format_time(90.0), "01:30");
    }

    #[test]
    fn format_time_hours() {
        assert_eq!(format_time(3661.0), "61:01");
    }

    #[test]
    fn cover_min_size_is_documented_floor() {
        assert_eq!(
            COVER_MIN_SIZE, 180,
            "cover size floor must stay 180 px per the documented threshold"
        );
    }

    #[test]
    fn meta_result_fields_are_accessible() {
        let meta: MetaResult = (
            String::from("Title"),
            String::from("Artist"),
            String::from("Album"),
            None,
            String::from("FLAC"),
            7,
        );
        assert_eq!(meta.0, "Title", "title field must round-trip");
        assert_eq!(meta.5, 7, "album id field must round-trip");
    }

    #[test]
    fn track_labels_fields_are_accessible() {
        let labels = TrackLabels::test_fixture();
        assert_eq!(labels.title.label(), "Title", "title label must round-trip");
        assert_eq!(
            labels.format.label(),
            "FLAC",
            "format label must round-trip"
        );
    }

    #[test]
    fn playback_widgets_fields_are_accessible() {
        let widgets = PlaybackWidgets::test_fixture();
        assert_eq!(
            widgets.current_time.label(),
            "00:00",
            "current time label must round-trip"
        );
        assert_eq!(
            widgets.total_time.label(),
            "00:00",
            "total time label must round-trip"
        );
    }

    #[test]
    fn player_content_installs_child() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let scroll = build_player_content(&state);
        ensure!(
            scroll.child().is_some(),
            "player content must be installed in the scrolled window"
        );
        Ok(())
    }
}
