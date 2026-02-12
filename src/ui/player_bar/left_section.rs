//! Left section of the player bar with artwork and track metadata.

use std::sync::atomic::{AtomicU64, Ordering::SeqCst};

use {
    libadwaita::{
        gdk::{Paintable, Texture},
        glib::Bytes,
        gtk::{
            AccessibleRole::Img,
            Align::Start,
            Box, Button,
            ContentFit::Contain,
            Label,
            Orientation::{Horizontal, Vertical},
            Picture,
            PolicyType::Never,
            Scale, ScrolledWindow, ToggleButton, Widget,
            pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExt, BoxExt, Cast, RangeExt, WidgetExt},
    },
    tracing::error,
};

use crate::audio::engine::TrackInfo;

/// Context struct for track information updates.
pub struct TrackInfoUpdateContext<'a> {
    /// Label widget displaying the track title.
    pub title_label: &'a Label,
    /// Label widget displaying the album name.
    pub album_label: &'a Label,
    /// Label widget displaying the artist name.
    pub artist_label: &'a Label,
    /// Label widget displaying the compact format info.
    pub format_label: &'a Label,
    /// Picture widget displaying the album artwork.
    pub artwork: &'a Picture,
    /// Label widget displaying the total track duration.
    pub total_duration_label: &'a Label,
    /// Toggle button for play/pause control.
    pub play_button: &'a ToggleButton,
    /// Button for skipping to previous track.
    pub prev_button: &'a Button,
    /// Button for skipping to next track.
    pub next_button: &'a Button,
}

/// Creates the left section containing artwork and track info.
///
/// # Returns
///
/// A tuple containing:
/// - The section container box
/// - The artwork picture widget
/// - The title label
/// - The album label
/// - The artist label
/// - The format info label
#[must_use]
pub fn create_left_section() -> (Box, Picture, Label, Label, Label, Label) {
    let left_section = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .hexpand(false)
        .vexpand(false)
        .build();

    let artwork = Picture::builder()
        .content_fit(Contain)
        .css_classes(["player-bar-artwork"])
        .build();

    artwork.set_accessible_role(Img);
    artwork.set_tooltip_text(Some("No artwork"));

    let artwork_container = ScrolledWindow::builder()
        .hscrollbar_policy(Never)
        .vscrollbar_policy(Never)
        .width_request(80)
        .height_request(80)
        .propagate_natural_width(false)
        .propagate_natural_height(false)
        .has_frame(false)
        .min_content_width(80)
        .min_content_height(80)
        .hexpand(false)
        .vexpand(false)
        .child(&artwork)
        .build();
    left_section.append(&artwork_container);

    let track_info_container = Box::builder()
        .orientation(Vertical)
        .spacing(2)
        .hexpand(false)
        .vexpand(false)
        .build();

    let title_label = create_metadata_label("No track loaded", &[]);
    track_info_container.append(title_label.upcast_ref::<Widget>());

    let album_label = create_metadata_label("", &["dim-label"]);
    track_info_container.append(album_label.upcast_ref::<Widget>());

    let artist_label = create_metadata_label("", &["dim-label"]);
    track_info_container.append(artist_label.upcast_ref::<Widget>());

    let format_label = create_metadata_label("", &["dim-label"]);
    track_info_container.append(format_label.upcast_ref::<Widget>());

    let track_info_wrapper = ScrolledWindow::builder()
        .hscrollbar_policy(Never)
        .vscrollbar_policy(Never)
        .width_request(320)
        .propagate_natural_width(false)
        .propagate_natural_height(false)
        .has_frame(false)
        .min_content_width(200)
        .hexpand(false)
        .vexpand(false)
        .child(&track_info_container)
        .build();

    left_section.append(&track_info_wrapper);

    (
        left_section,
        artwork,
        title_label,
        album_label,
        artist_label,
        format_label,
    )
}

/// Creates a metadata label with common properties.
///
/// # Arguments
///
/// * `initial_text` - The initial label text
/// * `css_classes` - CSS classes to apply to the label
///
/// # Returns
///
/// A configured `Label` widget.
#[must_use]
pub fn create_metadata_label(initial_text: &str, css_classes: &[&str]) -> Label {
    Label::builder()
        .label(initial_text)
        .halign(Start)
        .hexpand(false)
        .xalign(0.0)
        .css_classes(css_classes)
        .ellipsize(End)
        .tooltip_text(initial_text)
        .build()
}

/// Updates track information display.
///
/// # Arguments
///
/// * `track_info` - Optional track information
/// * `ctx` - Context struct with widget references
/// * `progress_scale` - Progress scale widget
/// * `current_time_label` - Current time label widget
/// * `track_duration_ms` - Atomic duration in milliseconds
pub fn update_track_info(
    track_info: Option<&TrackInfo>,
    ctx: &TrackInfoUpdateContext,
    progress_scale: &Scale,
    current_time_label: &Label,
    track_duration_ms: &AtomicU64,
) {
    if let Some(info) = track_info {
        let title = info
            .metadata
            .standard
            .title
            .clone()
            .unwrap_or_else(|| "Unknown Track".to_string());
        ctx.title_label.set_label(&title);
        ctx.title_label.set_tooltip_text(Some(&title));

        let album = info
            .metadata
            .standard
            .album
            .clone()
            .unwrap_or_else(|| "Unknown Album".to_string());
        ctx.album_label.set_label(&album);
        ctx.album_label.set_tooltip_text(Some(&album));

        let artist = info
            .metadata
            .standard
            .artist
            .clone()
            .unwrap_or_else(|| "Unknown Artist".to_string());
        ctx.artist_label.set_label(&artist);
        ctx.artist_label.set_tooltip_text(Some(&artist));

        let channels_text = match info.format.channels {
            1 => "Mono".to_string(),
            2 => "Stereo".to_string(),
            n => format!("{n} ch"),
        };
        let sample_rate_display = if info.format.sample_rate % 1000 == 0 {
            format!("{}", info.format.sample_rate / 1000)
        } else {
            format!("{:.1}", f64::from(info.format.sample_rate) / 1000.0)
        };
        let format_str = format!(
            "{} {}/{sample_rate_display} {channels_text}",
            info.metadata.technical.format, info.metadata.technical.bits_per_sample
        );
        ctx.format_label.set_label(&format_str);
        ctx.format_label.set_tooltip_text(Some(&format_str));

        if let Some(artwork_data) = &info.metadata.artwork {
            let bytes = Bytes::from(&artwork_data[..]);

            match Texture::from_bytes(&bytes) {
                Ok(texture) => {
                    ctx.artwork.set_paintable(Some(&texture));
                    ctx.artwork.set_tooltip_text(Some("Embedded album artwork"));
                }
                Err(e) => {
                    error!(error = %e, "Failed to load artwork from bytes");
                    ctx.artwork.set_paintable(None::<&Paintable>);
                    ctx.artwork.set_tooltip_text(Some("Failed to load artwork"));
                }
            }
        } else {
            ctx.artwork.set_paintable(None::<&Paintable>);
            ctx.artwork.set_tooltip_text(Some("No artwork"));
        }

        track_duration_ms.store(info.duration_ms, SeqCst);
        let duration_seconds = info.duration_ms / 1000;
        let duration_minutes = duration_seconds / 60;
        let duration_remaining = duration_seconds % 60;
        let duration_text = format!("{duration_minutes:02}:{duration_remaining:02}");
        ctx.total_duration_label.set_label(&duration_text);

        progress_scale.set_value(0.0);
        current_time_label.set_label("00:00");

        ctx.play_button.set_sensitive(true);
        ctx.prev_button.set_sensitive(true);
        ctx.next_button.set_sensitive(true);
    } else {
        ctx.title_label.set_label("No track loaded");
        ctx.title_label.set_tooltip_text(Some("No track loaded"));
        ctx.album_label.set_label("");
        ctx.album_label.set_tooltip_text(Some(""));
        ctx.artist_label.set_label("");
        ctx.artist_label.set_tooltip_text(Some(""));
        ctx.format_label.set_label("");
        ctx.format_label.set_tooltip_text(Some(""));
        ctx.artwork.set_paintable(None::<&Paintable>);
        ctx.artwork.set_tooltip_text(Some("No artwork"));
        ctx.total_duration_label.set_label("00:00");
        track_duration_ms.store(0, SeqCst);

        progress_scale.set_value(0.0);
        current_time_label.set_label("00:00");

        ctx.play_button.set_sensitive(false);
        ctx.prev_button.set_sensitive(false);
        ctx.next_button.set_sensitive(false);
    }
}
