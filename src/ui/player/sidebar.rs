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
        ContentFit::Cover,
        Label, Picture, ScrolledWindow,
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
            panel_widgets::{PLAYER_COVER_LOGICAL_SIZE, PlaybackWidgets, TrackLabels},
            playback_events::spawn_async_listeners,
            signal_badge::{build_signal_badge, start_badge_poll},
        },
        signal_view::{signal_poll::create_mailbox, signal_publish::spawn_snapshot_publisher},
    },
};

/// Build the player panel content area.
///
/// Returns a `ScrolledWindow` containing album artwork, track info,
/// seek slider, playback controls, volume control, and queue view.
/// Used as the content child of the sidebar's `AdwToolbarView`.
/// Listens to `PlaybackEvent` stream for fully event-driven updates.
pub fn build_player_content(state: &Arc<AppState>) -> ScrolledWindow {
    let (scroll, content) = build_scroll_content();

    let badge = build_signal_badge(state);
    content.prepend(badge.widget());
    let (snapshot_tx, snapshot_rx) = create_mailbox();
    spawn_snapshot_publisher(&state.playback, &state.storage, snapshot_tx);
    start_badge_poll(state, &badge, snapshot_rx);

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
        .width_request(PLAYER_COVER_LOGICAL_SIZE)
        .height_request(PLAYER_COVER_LOGICAL_SIZE)
        .css_classes(["album-cover"])
        .tooltip_text("Album artwork")
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
        .tooltip_text("Track title")
        .build();
    title.update_property(&[PropertyLabel("Track title")]);

    let artist = Label::builder()
        .label("")
        .css_classes(["dim-label", "body"])
        .ellipsize(End)
        .max_width_chars(35)
        .halign(Start)
        .tooltip_text("Artist name")
        .build();
    artist.update_property(&[PropertyLabel("Artist name")]);

    let album = Label::builder()
        .label("")
        .css_classes(["dim-label", "body"])
        .ellipsize(End)
        .max_width_chars(35)
        .halign(Start)
        .tooltip_text("Album name")
        .build();
    album.update_property(&[PropertyLabel("Album name")]);

    let format = Label::builder()
        .label("")
        .css_classes(["dim-label", "caption"])
        .ellipsize(End)
        .max_width_chars(35)
        .halign(Start)
        .tooltip_text("Audio format information")
        .build();
    format.update_property(&[PropertyLabel("Audio format information")]);

    (title, artist, album, format)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::gtk::{self, test as gtk_test},
    };

    use crate::{
        app::{mocks::pump_in_test_runtime, runtime::AppState},
        ui::player::{
            panel_widgets::{
                COVER_MIN_SIZE, MetaResult, PLAYER_COVER_LOGICAL_SIZE, PLAYER_COVER_SIZE,
                PlaybackWidgets, TrackLabels, format_time,
            },
            sidebar::build_player_content,
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
            COVER_MIN_SIZE, PLAYER_COVER_LOGICAL_SIZE,
            "cover size floor must match the player logical size to avoid upscaling thumbnails"
        );
        assert_eq!(
            PLAYER_COVER_SIZE, 560,
            "player decode size must stay 2x logical for HiDPI sharpness"
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

    #[gtk_test]
    fn track_labels_fields_are_accessible() {
        let labels = TrackLabels::test_fixture();
        assert_eq!(labels.title.label(), "Title", "title label must round-trip");
        assert_eq!(
            labels.format.label(),
            "FLAC",
            "format label must round-trip"
        );
    }

    #[gtk_test]
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

    #[gtk_test]
    fn player_content_installs_child() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let scroll = pump_in_test_runtime(|| build_player_content(&state))?;
        ensure!(
            scroll.child().is_some(),
            "player content must be installed in the scrolled window"
        );
        Ok(())
    }
}
