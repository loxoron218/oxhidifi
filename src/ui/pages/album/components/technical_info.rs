//! UI components for displaying technical information about albums.
//!
//! This module provides functions for building UI elements that display
//! technical metadata about music albums, including:
//! - Album cover images with proper scaling and fallbacks
//! - Audio format information (bit depth, sample rate, codec)
//! - Album metadata (year, track count, total duration)
//! - Quality indicators (Hi-Res, Lossy, CD) with appropriate icons

use std::path::PathBuf;

use gtk4::{Align::Start, Box, Image, Label, Orientation::Horizontal, Picture, gdk_pixbuf::Pixbuf};
use libadwaita::prelude::{BoxExt, WidgetExt};

use crate::{
    data::models::{Album, Track},
    ui::pages::album::helpers::album_helpers::{get_most_common_track_properties, is_lossy_format},
    utils::formatting::{format_album_year_display, format_bit_sample_rate, format_duration_hms},
};

/// Build the album cover widget, scaling and falling back if needed.
///
/// Creates a Picture widget displaying the album cover art, scaled to 300x300 pixels.
/// If the cover art path is not provided or loading fails, a default placeholder
/// with the same dimensions is returned.
///
/// # Arguments
///
/// * `path` - An optional path to the album cover image file
///
/// # Returns
///
/// A GTK Picture widget containing the album cover or a placeholder
pub fn build_album_cover(path: &Option<PathBuf>) -> Picture {
    if let Some(path) = path {
        if let Ok(pixbuf) = Pixbuf::from_file_at_scale(path, 300, 300, true) {
            let pic = Picture::for_pixbuf(&pixbuf);
            pic.set_size_request(300, 300);
            pic.add_css_class("album-cover-border");
            return pic;
        }
    }
    let pic = Picture::new();
    pic.set_size_request(300, 300);
    pic.add_css_class("album-cover-border");
    pic
}

/// Build a GTK label with optional CSS class.
///
/// Creates a GTK Label widget with the specified text and optional CSS styling.
/// The label is left-aligned by default.
///
/// # Arguments
///
/// * `label` - The text content for the label
/// * `css_class` - An optional CSS class name to apply to the label
///
/// # Returns
///
/// A configured GTK Label widget
pub fn build_info_label(label: &str, css_class: Option<&str>) -> Label {
    let l = Label::builder().label(label).halign(Start).build();
    if let Some(class) = css_class {
        l.add_css_class(class);
    }
    l
}

/// Build the technical information section for the album header.
///
/// Creates a UI component displaying technical audio information about the album,
/// including bit depth, sample rate, and format. Also shows quality indicators
/// with appropriate icons:
/// - Hi-Res icon for high-resolution audio (24-bit/88.2kHz or higher)
/// - Audio icon for lossy formats (MP3, AAC, etc.)
/// - Optical disc icon for standard CD quality
///
/// # Arguments
///
/// * `tracks` - A slice of Track objects containing the album's tracks
/// * `_album` - The Album object (currently unused but kept for API consistency)
///
/// # Returns
///
/// An optional GTK Box containing the technical information UI elements,
/// or None if no relevant technical information is available
pub fn build_technical_info(tracks: &[Track], _album: &Album) -> Option<Box> {
    let (most_common_bit_depth, most_common_freq, most_common_format_opt) =
        get_most_common_track_properties(tracks);

    // Calculate if the album is mainly in a lossy format
    let total_tracks = tracks.len();
    let lossy_tracks_count = tracks.iter().filter(|t| is_lossy_format(&t.format)).count();
    let is_lossy_album =
        total_tracks > 0 && (lossy_tracks_count as f64 / total_tracks as f64) > 0.5;

    // Calculate if the album is mainly Hi-Res
    let hires_tracks_count = tracks.iter()
        .filter(|t| matches!((t.bit_depth, t.sample_rate), (Some(bd), Some(fq)) if bd >= 24 && fq >= 8_200))
        .count();
    let show_hires = total_tracks > 0 && (hires_tracks_count as f64 / total_tracks as f64) > 0.5;

    // Bit depth / Sample Rate and Format, with Hi-Res icon aligned to both lines
    let bit_freq_str = format_bit_sample_rate(most_common_bit_depth, most_common_freq);

    // Only build this row if any content
    if show_hires || is_lossy_album || !bit_freq_str.is_empty() || most_common_format_opt.is_some()
    {
        let outer_row = Box::builder()
            .orientation(Horizontal)
            .spacing(8)
            .halign(Start)
            .margin_start(3)
            .build();

        // Hi-Res, Lossy, or CD icon (tall, left)
        // Use a match statement on the conditions for a clear, exhaustive check.
        match (show_hires, is_lossy_album) {
            // Case 1: Show Hi-Res icon, regardless of whether it's lossy.
            (true, _) => {
                match Pixbuf::from_file_at_scale("assets/hires.png", -1, 40, true) {
                    Ok(pixbuf) => {
                        let hires_pic = Picture::for_pixbuf(&pixbuf);
                        hires_pic.set_size_request(40, 40);
                        hires_pic.set_halign(Start);
                        outer_row.append(&hires_pic);
                    }
                    Err(e) => {
                        // Log the error
                        eprintln!("Failed to load Hi-Res icon: {}", e);

                        // Fallback to a symbolic icon
                        let fallback_icon = Image::from_icon_name("image-missing-symbolic");
                        fallback_icon.set_pixel_size(44);
                        fallback_icon.set_halign(Start);
                        outer_row.append(&fallback_icon);
                    }
                }
            }

            // Case 2 & 3: Not showing Hi-Res, so show a symbolic icon.
            (false, is_lossy) => {
                // First, determine the correct icon name.
                let icon_name = if is_lossy {
                    "audio-x-generic-symbolic"
                } else {
                    "media-optical-symbolic"
                };

                // Now, create and configure the icon just once.
                let icon = Image::from_icon_name(icon_name);
                icon.set_pixel_size(44);
                icon.set_halign(Start);
                outer_row.append(&icon);
            }
        }

        // Right: vertical box with bit/freq and format
        let lines_box = Box::builder()
            .orientation(Horizontal)
            .spacing(0)
            .halign(Start)
            .margin_start(12)
            .build();
        if !bit_freq_str.is_empty() {
            lines_box.append(&build_info_label(
                &bit_freq_str,
                Some("album-technical-label"),
            ));
        }
        if let Some(format) = most_common_format_opt {
            if !format.is_empty() {
                // Add separator if we already have bit/freq info
                if !bit_freq_str.is_empty() {
                    lines_box.append(&build_info_label(" · ", Some("album-technical-label")));
                }
                lines_box.append(&build_info_label(
                    &format.to_uppercase(),
                    Some("album-technical-label"),
                ));
            }
        }
        outer_row.append(&lines_box);
        Some(outer_row)
    } else {
        None
    }
}

/// Build the album metadata section (year, track count, duration).
///
/// Creates a UI component displaying basic album metadata including:
/// - Release year (with original release date if different)
/// - Total number of tracks
/// - Total duration of all tracks
///
/// # Arguments
///
/// * `tracks` - A slice of Track objects containing the album's tracks
/// * `album` - The Album object containing metadata
///
/// # Returns
///
/// An optional GTK Box containing the metadata UI elements,
/// or None if no metadata is available
pub fn build_album_metadata(tracks: &[Track], album: &Album) -> Option<Box> {
    let year_display_text =
        format_album_year_display(album.year, album.original_release_date.as_deref());
    if !year_display_text.is_empty() {
        let meta_box = Box::builder()
            .orientation(Horizontal)
            .spacing(8)
            .halign(Start)
            .build();
        let mut meta_fields = Vec::with_capacity(3);
        if !year_display_text.is_empty() {
            meta_fields.push(year_display_text);
        }

        // Number of songs in the album
        let total_songs_count = tracks.len();
        if total_songs_count > 0 {
            meta_fields.push(format!("{} Songs", total_songs_count));
        }

        // Duration as HH:MM:SS
        let total_length: u32 = tracks.iter().filter_map(|t| t.duration).sum();
        meta_fields.push(format_duration_hms(total_length));
        let meta_text = meta_fields.join(" · ");
        if !meta_text.is_empty() {
            meta_box.append(&build_info_label(&meta_text, Some("album-meta-label")));
        }
        Some(meta_box)
    } else {
        None
    }
}
