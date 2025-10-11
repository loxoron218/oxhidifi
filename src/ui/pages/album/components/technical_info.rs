use std::path::PathBuf;

use gtk4::{Align::Start, Box, Image, Label, Orientation::Horizontal, Picture, gdk_pixbuf::Pixbuf};
use libadwaita::prelude::{BoxExt, WidgetExt};

use crate::{
    data::models::{Album, Song},
    ui::pages::album::helpers::album_helpers::{
        get_most_common_song_properties, has_mixed_audio_properties, is_lossy_format,
    },
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
    if let Some(path) = path
        && let Ok(pixbuf) = Pixbuf::from_file_at_scale(path, 300, 300, true)
    {
        let pic = Picture::for_pixbuf(&pixbuf);
        pic.set_size_request(300, 300);
        pic.add_css_class("album-cover-border");
        return pic;
    }
    let pic = Picture::new();
    pic.set_size_request(300, 300);
    pic.add_css_class("album-cover-border");
    pic
}

/// Build a GTK label with optional CSS class and tooltip.
///
/// Creates a GTK Label widget with the specified text and optional CSS styling.
/// The label is left-aligned by default.
///
/// # Arguments
///
/// * `label` - The text content for the label
/// * `css_class` - An optional CSS class name to apply to the label
/// * `tooltip` - An optional tooltip text to show on hover
///
/// # Returns
///
/// A configured GTK Label widget
pub fn build_info_label(label: &str, css_class: Option<&str>, tooltip: Option<&str>) -> Label {
    let l = Label::builder().label(label).halign(Start).build();
    if let Some(class) = css_class {
        l.add_css_class(class);
    }
    if let Some(tooltip_text) = tooltip {
        l.set_tooltip_text(Some(tooltip_text));
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
/// * `songs` - A slice of Song objects containing the album's songs
/// * `_album` - The Album object (currently unused but kept for API consistency)
///
/// # Returns
///
/// An optional GTK Box containing the technical information UI elements,
/// or None if no relevant technical information is available
pub fn build_technical_info(songs: &[Song], _album: &Album) -> Option<Box> {
    let (most_common_bit_depth, most_common_freq, most_common_format_opt) =
        get_most_common_song_properties(songs);

    // Check if the album has mixed audio properties
    let (has_mixed_bit_depths, has_mixed_sample_rates, has_mixed_formats) =
        has_mixed_audio_properties(songs);

    // Calculate if the album is mainly in a lossy format
    let total_songs = songs.len();
    let lossy_songs_count = songs.iter().filter(|t| is_lossy_format(&t.format)).count();
    let is_lossy_album = total_songs > 0 && (lossy_songs_count as f64 / total_songs as f64) > 0.5;

    // Calculate if the album is mainly Hi-Res
    let hires_songs_count = songs.iter()
        .filter(|t| matches!((t.bit_depth, t.sample_rate), (Some(bd), Some(fq)) if bd >= 24 && fq >= 8_200))
        .count();
    let show_hires = total_songs > 0 && (hires_songs_count as f64 / total_songs as f64) > 0.5;

    // Determine if we should show "Mixed" instead of individual values
    let show_mixed_indicator = has_mixed_bit_depths || has_mixed_sample_rates || has_mixed_formats;

    // Bit depth / Sample Rate and Format, with Hi-Res icon aligned to both lines
    let bit_freq_str = if show_mixed_indicator {
        if has_mixed_bit_depths && has_mixed_sample_rates {
            "Mixed".to_string()
        } else if has_mixed_bit_depths {
            format!("Mixed/{}", format_bit_sample_rate(None, most_common_freq))
        } else if has_mixed_sample_rates {
            format!(
                "{} (Mixed)",
                format_bit_sample_rate(most_common_bit_depth, None)
            )
        } else {
            format_bit_sample_rate(most_common_bit_depth, most_common_freq)
        }
    } else {
        format_bit_sample_rate(most_common_bit_depth, most_common_freq)
    };

    // Format string with mixed indicator if needed
    let format_str = if has_mixed_formats {
        Some("Mixed".to_string())
    } else {
        most_common_format_opt.clone()
    };

    // Add tooltip to indicate what "Mixed" means with specific details
    let tooltip_text = if show_mixed_indicator {
        let mut mixed_parts = Vec::new();
        if has_mixed_bit_depths {
            mixed_parts.push("different bit depths");
        }
        if has_mixed_sample_rates {
            mixed_parts.push("different sample rates");
        }
        if has_mixed_formats {
            mixed_parts.push("different formats");
        }
        if !mixed_parts.is_empty() {
            Some(format!("Mixed: {}", mixed_parts.join(", ")))
        } else {
            None
        }
    } else {
        None
    };

    // Only build this row if any content
    if show_hires || is_lossy_album || !bit_freq_str.is_empty() || format_str.is_some() {
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
                        hires_pic.set_tooltip_text(Some("Hi-Res Audio: 24-bit/88.2kHz or higher"));
                        outer_row.append(&hires_pic);
                    }
                    Err(e) => {
                        // Log the error
                        eprintln!("Failed to load Hi-Res icon: {}", e);

                        // Fallback to a symbolic icon
                        let fallback_icon = Image::from_icon_name("image-missing-symbolic");
                        fallback_icon.set_pixel_size(44);
                        fallback_icon.set_halign(Start);
                        fallback_icon
                            .set_tooltip_text(Some("Hi-Res Audio: 24-bit/88.2kHz or higher"));
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

                // Add tooltip based on the icon type
                match icon_name {
                    "audio-x-generic-symbolic" => {
                        icon.set_tooltip_text(Some(
                            "Lossy Audio: Compressed format (MP3, AAC, etc.)",
                        ));
                    }
                    "media-optical-symbolic" => {
                        icon.set_tooltip_text(Some("CD Quality: 16-bit/44.1kHz"));
                    }
                    _ => {
                        icon.set_tooltip_text(Some("Audio Quality Indicator"));
                    }
                }
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
            let bit_freq_tooltip =
                if bit_freq_str == "Mixed" || (has_mixed_bit_depths && has_mixed_sample_rates) {
                    tooltip_text.as_deref()
                } else if has_mixed_bit_depths {
                    Some("Mixed: different bit depths across songs")
                } else if has_mixed_sample_rates {
                    Some("Mixed: different sample rates across songs")
                } else {
                    None
                };
            lines_box.append(&build_info_label(
                &bit_freq_str,
                Some("album-technical-label"),
                bit_freq_tooltip,
            ));
        }
        if let Some(format) = format_str
            && !format.is_empty()
        {
            // Add separator if we already have bit/freq info
            if !bit_freq_str.is_empty() && bit_freq_str != "Mixed" {
                lines_box.append(&build_info_label(
                    " · ",
                    Some("album-technical-label"),
                    None,
                ));
            }
            let format_specific_tooltip = if format == "Mixed" {
                tooltip_text.as_deref()
            } else {
                None
            };
            lines_box.append(&build_info_label(
                &format.to_uppercase(),
                Some("album-technical-label"),
                format_specific_tooltip,
            ));
        }
        outer_row.append(&lines_box);
        Some(outer_row)
    } else {
        None
    }
}

/// Build the album metadata section (year, song count, duration).
///
/// Creates a UI component displaying basic album metadata including:
/// - Release year (with original release date if different)
/// - Total number of songs
/// - Total duration of all songs
///
/// # Arguments
///
/// * `songs` - A slice of Song objects containing the album's songs
/// * `album` - The Album object containing metadata
///
/// # Returns
///
/// An optional GTK Box containing the metadata UI elements,
/// or None if no metadata is available
pub fn build_album_metadata(songs: &[Song], album: &Album) -> Option<Box> {
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
        let total_songs_count = songs.len();
        if total_songs_count > 0 {
            meta_fields.push(format!("{} Songs", total_songs_count));
        }

        // Duration as HH:MM:SS
        let total_length: u32 = songs.iter().filter_map(|t| t.duration).sum();
        meta_fields.push(format_duration_hms(total_length));
        let meta_text = meta_fields.join(" · ");
        if !meta_text.is_empty() {
            meta_box.append(&build_info_label(
                &meta_text,
                Some("album-meta-label"),
                None,
            ));
        }
        Some(meta_box)
    } else {
        None
    }
}
