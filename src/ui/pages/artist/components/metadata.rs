use gtk4::{
    Align::{End, Start},
    Box, Label,
    Orientation::{Horizontal, Vertical},
    pango::{EllipsizeMode, WrapMode::WordChar},
};
use libadwaita::prelude::{BoxExt, WidgetExt};

use crate::{
    ui::{
        components::{
            tiles::helpers::create_album_label,
            view_controls::ZoomLevel::{self, ExtraSmall, Small},
        },
        pages::artist::data::artist_data::AlbumDisplayInfoWithYear,
    },
    utils::formatting::{format_sample_rate_khz, format_sample_rate_value, format_year_info},
};

/// Creates and styles the album title label with search highlighting
pub fn create_title_label(highlighted_title: &str, cover_size: i32) -> Label {
    let title_label = create_album_label(
        highlighted_title,
        &["album-title-label"],
        Some(((cover_size - 16) / 10).max(8)),
        Some(EllipsizeMode::End),
        true,
        Some(WordChar),
        Some(2),
        // use_markup: true because highlight is used
        true,
    );
    title_label.set_size_request(cover_size - 16, -1);

    // Align to the bottom of its allocated space
    title_label.set_valign(End);
    title_label
}

/// Creates and styles the year label for displaying release year information
pub fn create_year_label(
    album: &AlbumDisplayInfoWithYear,
    use_original_year: bool,
    zoom_level: ZoomLevel,
) -> Label {
    // Extract and format the release year based on user preference for original vs. release year
    let year_text = format_year_info(
        album.year,
        album.original_release_date.as_deref(),
        use_original_year,
    );
    let year_label = create_album_label(
        &year_text,
        &["album-format-label"],
        Some(8),
        Some(EllipsizeMode::End),
        false,
        None,
        None,
        // use_markup: false for plain text
        false,
    );
    year_label.set_halign(End);
    year_label.set_hexpand(false);
    year_label.set_visible(!matches!(zoom_level, ExtraSmall | Small));
    year_label
}

/// Creates and styles the format label for displaying audio format information
pub fn create_format_label(
    album: &AlbumDisplayInfoWithYear,
    zoom_level: ZoomLevel,
    cover_size: i32,
) -> Label {
    // Format the audio quality line (e.g., "FLAC 24/96")
    let format_line = album
        .format
        .as_ref()
        .map(|format_str: &String| {
            // Convert format to uppercase for consistent display
            let format_caps = format_str.to_uppercase();

            // For ExtraSmall zoom level, only show the format without bit depth/sample rate
            if zoom_level == ExtraSmall {
                format_caps
            } else {
                // First, determine only the part of the string that changes.
                let tech_details = match (album.bit_depth, album.sample_rate) {
                    (Some(bit), Some(freq)) => {
                        // For Small zoom level, don't show "kHz" suffix
                        match zoom_level {
                            Small => {
                                format!(" {}/{}", bit, format_sample_rate_value(freq))
                            }
                            _ => {
                                format!(" {}/{}", bit, format_sample_rate_khz(freq))
                            }
                        }
                    }
                    (None, Some(freq)) => {
                        // For Small zoom level, don't show "kHz" suffix
                        match zoom_level {
                            Small => {
                                format!(" {}", format_sample_rate_value(freq))
                            }
                            _ => {
                                format!(" {}", format_sample_rate_khz(freq))
                            }
                        }
                    }
                    _ => String::new(),
                };

                // Combine the static and dynamic parts in one place.
                format!("{}{}", format_caps, tech_details)
            }
        })
        // If `album.format` was None, this provides an empty String.
        .unwrap_or_default();

    let format_label = create_album_label(
        &format_line,
        &["album-format-label"],
        Some(((cover_size - 16) / 10).max(8)),
        // Only ellipsize at ExtraSmall zoom level, not at Small or larger
        match zoom_level {
            ExtraSmall => Some(EllipsizeMode::End),
            _ => None,
        },
        false,
        None,
        None,
        // use_markup: false for plain text
        false,
    );
    format_label.set_halign(Start);
    format_label.set_hexpand(true);
    format_label
}

/// Creates the metadata container box that holds both format and year labels
pub fn create_metadata_container(format_label: &Label, year_label: &Label, cover_size: i32) -> Box {
    // Container to constrain metadata box width
    let metadata_container = Box::builder().orientation(Vertical).hexpand(false).build();
    metadata_container.set_size_request(cover_size - 16, -1);
    let metadata_box = Box::builder()
        .orientation(Horizontal)
        .spacing(0)
        .hexpand(false)
        .build();
    metadata_box.append(format_label);
    metadata_box.append(year_label);
    metadata_container.append(&metadata_box);
    metadata_container
}

/// Creates the title area box that contains the title label
pub fn create_title_area_box(title_label: &Label) -> Box {
    let title_area_box = Box::builder()
        .orientation(Vertical)
        .height_request(40)
        .margin_top(12)
        .build();
    title_area_box.append(title_label);
    title_area_box
}
