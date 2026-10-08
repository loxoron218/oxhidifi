//! Album header row for artist detail sections.
//!
//! Builds the disclosure toggle, artwork thumbnail, title/metadata labels,
//! and open button atop each album section, split from `artist_album_section`
//! to keep each module under the file-size limit.

use std::sync::Arc;

use libadwaita::{
    gtk::{
        Align::{Center, Start},
        Box, Button,
        ContentFit::Cover,
        Label,
        Orientation::{Horizontal, Vertical},
        Picture, ToggleButton,
        accessible::Property::Label as PropertyLabel,
        pango::EllipsizeMode::End,
    },
    prelude::{AccessibleExtManual, BoxExt},
};

use crate::{
    app::runtime::AppState,
    storage::{catalog::Album, formats::FormatInfo},
    ui::detail::cover_art::decode_cover_into_picture,
};

/// Icon for an expanded (revealed) album section.
pub const EXPANDED_ICON: &str = "pan-down-symbolic";

/// Icon for a collapsed album section.
pub const COLLAPSED_ICON: &str = "pan-end-symbolic";

/// Build the album header row with toggle, artwork, labels, and open button.
///
/// # Arguments
///
/// * `state` - Application state for cover-art decoding.
/// * `album` - Album to display.
/// * `format_info` - Precomputed format summary for the album.
/// * `expanded` - Whether the section starts revealed.
///
/// # Returns
///
/// * `(Box, ToggleButton, Button)` - Header row, disclosure toggle, open button.
pub fn build_album_header(
    state: &Arc<AppState>,
    album: &Album,
    format_info: &FormatInfo,
    expanded: bool,
) -> (Box, ToggleButton, Button) {
    let header = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .hexpand(true)
        .build();
    header.update_property(&[PropertyLabel(&format!("Album {}", album.title))]);

    let toggle = ToggleButton::builder()
        .icon_name(if expanded {
            EXPANDED_ICON
        } else {
            COLLAPSED_ICON
        })
        .active(expanded)
        .tooltip_text(toggle_tooltip(&album.title, expanded))
        .css_classes(["flat", "circular"])
        .valign(Center)
        .can_focus(true)
        .build();
    toggle.update_property(&[PropertyLabel(&toggle_label(&album.title, expanded))]);
    header.append(&toggle);

    if let Some(art_path) = &album.artwork_path {
        let thumb = Picture::builder()
            .content_fit(Cover)
            .can_shrink(true)
            .width_request(60)
            .height_request(60)
            .css_classes(["album-cover"])
            .build();
        thumb.update_property(&[PropertyLabel(&format!("Artwork for {}", album.title))]);
        header.append(&thumb);
        decode_cover_into_picture(state, album.id, art_path.clone(), 60, &thumb);
    }

    header.append(&build_album_info(album, format_info));

    let open_button = Button::builder()
        .icon_name("go-next-symbolic")
        .tooltip_text(format!("Open {}", album.title))
        .css_classes(["flat", "circular"])
        .valign(Center)
        .can_focus(true)
        .build();
    open_button.update_property(&[PropertyLabel(&format!("Open album {}", album.title))]);
    header.append(&open_button);
    (header, toggle, open_button)
}

/// Build the title/metadata box for the album header.
///
/// # Arguments
///
/// * `album` - Album to display.
/// * `format_info` - Precomputed format summary for the album.
///
/// # Returns
///
/// * `Box` - Vertical box with title and metadata labels.
fn build_album_info(album: &Album, format_info: &FormatInfo) -> Box {
    let info_box = Box::builder()
        .orientation(Vertical)
        .spacing(3)
        .hexpand(true)
        .build();
    let title = Label::builder()
        .label(&album.title)
        .css_classes(["title-4", "heading"])
        .ellipsize(End)
        .hexpand(true)
        .halign(Start)
        .build();
    title.update_property(&[PropertyLabel(&format!("Album: {}", album.title))]);
    info_box.append(&title);
    let meta = Label::builder()
        .label(format!(
            "{} tracks \u{2022} {}",
            album.track_count,
            format_info.summary_detailed()
        ))
        .css_classes(["dim-label", "caption"])
        .ellipsize(End)
        .hexpand(true)
        .halign(Start)
        .build();
    meta.update_property(&[PropertyLabel(&format!(
        "{} tracks in {}",
        album.track_count, album.title
    ))]);
    info_box.append(&meta);
    info_box
}

/// Tooltip for the disclosure toggle.
///
/// # Arguments
///
/// * `title` - Album title.
/// * `expanded` - Whether the section is currently revealed.
///
/// # Returns
///
/// * `String` - Collapse tooltip when revealed, expand tooltip when hidden.
#[must_use]
pub fn toggle_tooltip(title: &str, expanded: bool) -> String {
    if expanded {
        format!("Collapse {title}")
    } else {
        format!("Expand {title}")
    }
}

/// Accessible label for the disclosure toggle.
///
/// # Arguments
///
/// * `title` - Album title.
/// * `expanded` - Whether the section is currently revealed.
///
/// # Returns
///
/// * `String` - Collapse label when revealed, expand label when hidden.
#[must_use]
pub fn toggle_label(title: &str, expanded: bool) -> String {
    if expanded {
        format!("Collapse album {title}")
    } else {
        format!("Expand album {title}")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::Result,
        libadwaita::gtk::{self, test},
    };

    use crate::{
        app::runtime::AppState,
        storage::{catalog::Album, formats::FormatInfo},
        ui::detail::{album_header::build_album_header, min_width::assert_fits_minimum_window},
    };

    #[test]
    fn album_header_fits_minimum_window() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let album = Album {
            id: 1,
            title: "Selected Ambient Works Volume II With A Very Long Title".into(),
            artist_id: 1,
            year: Some(1994),
            genre: None,
            artwork_path: None,
            track_count: 25,
            total_duration: 3000.0,
            format_summary: String::new(),
            lossless: true,
            format: "FLAC".into(),
            bit_depth: Some(24),
            sample_rate: Some(96_000),
        };
        let format_info = FormatInfo {
            formats: vec!["FLAC".into()],
            sample_rates: vec![96_000],
            bit_depths: vec![24],
            channels: vec![2],
        };
        let (header, _, _) = build_album_header(&state, &album, &format_info, true);
        assert_fits_minimum_window(&header, "album header")
    }
}
