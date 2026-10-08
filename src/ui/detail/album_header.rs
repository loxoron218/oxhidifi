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
    prelude::{AccessibleExtManual, BoxExt, WidgetExt},
};

use crate::{
    app::runtime::AppState,
    storage::{catalog::Album, formats::FormatInfo},
    ui::detail::{
        album_meta::{
            album_format_label, album_meta_label, album_meta_primary, album_meta_secondary,
        },
        cover_art::decode_cover_into_picture,
    },
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
/// Shows the album title plus two single-line metadata labels: primary
/// `"{year} • N tracks"` and secondary `{format details}`, each ellipsized
/// independently so the year and track count stay visible on narrow windows.
/// The format line is hidden when empty.
///
/// # Arguments
///
/// * `album` - Album to display.
/// * `format_info` - Precomputed format summary for the album.
///
/// # Returns
///
/// * `Box` - Vertical box with title and two metadata labels.
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
    let primary = build_meta_label(&album_meta_primary(album), &album_meta_label(album));
    info_box.append(&primary);
    let secondary_text = album_meta_secondary(format_info);
    let secondary = build_meta_label(&secondary_text, &album_format_label(album, &secondary_text));
    secondary.set_visible(!secondary_text.is_empty());
    info_box.append(&secondary);
    info_box
}

/// Build a single-line dimmed metadata label.
///
/// # Arguments
///
/// * `text` - Visible label text.
/// * `accessible` - Accessible label text.
///
/// # Returns
///
/// * `Label` - Ellipsized metadata label.
fn build_meta_label(text: &str, accessible: &str) -> Label {
    let meta = Label::builder()
        .label(text)
        .css_classes(["dim-label", "caption"])
        .ellipsize(End)
        .hexpand(true)
        .halign(Start)
        .build();
    meta.update_property(&[PropertyLabel(accessible)]);
    meta
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
        anyhow::{Result, anyhow, bail, ensure},
        libadwaita::{
            gtk::{self, Box, Label, test},
            prelude::{Cast, WidgetExt},
        },
    };

    use crate::{
        app::runtime::AppState,
        storage::formats::FormatInfo,
        ui::detail::{
            album_header::build_album_header,
            album_meta::tests::{fixture_album, flac_info},
            min_width::assert_fits_minimum_window,
        },
    };

    fn header_meta_lines(header: &Box) -> Result<(String, String, bool)> {
        let Some(toggle) = header.first_child() else {
            bail!("header must start with a toggle")
        };
        let Some(info_widget) = toggle.next_sibling() else {
            bail!("header must hold an info box")
        };
        let Ok(info) = info_widget.downcast::<Box>() else {
            bail!("header info must be a box")
        };
        let Some(title) = info.first_child() else {
            bail!("info must start with a title")
        };
        let Some(primary_widget) = title.next_sibling() else {
            bail!("info must hold a primary label")
        };
        let Some(secondary_widget) = primary_widget.next_sibling() else {
            bail!("info must hold a secondary label")
        };
        let Ok(primary) = primary_widget.downcast::<Label>() else {
            bail!("primary must be a label")
        };
        let Ok(secondary) = secondary_widget.downcast::<Label>() else {
            bail!("secondary must be a label")
        };
        Ok((
            primary.label().to_string(),
            secondary.label().to_string(),
            secondary.is_visible(),
        ))
    }

    fn header_box(state: &Arc<AppState>, year: Option<i32>, info: &FormatInfo) -> Result<Box> {
        let (header, _, _) = build_album_header(state, &fixture_album(year), info, true);
        header
            .downcast::<Box>()
            .map_err(|widget| anyhow!("header must be a box, got {widget:?}"))
    }

    #[test]
    fn header_widget_shows_two_lines() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let (primary, secondary, visible) =
            header_meta_lines(&header_box(&state, Some(1994), &flac_info())?)?;
        ensure!(primary == "1994 \u{2022} 25 tracks", "primary: {primary}");
        ensure!(secondary.contains("FLAC"), "secondary: {secondary}");
        ensure!(visible, "format line must be visible");
        Ok(())
    }

    #[test]
    fn header_widget_hides_empty_format() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let (primary, secondary, visible) =
            header_meta_lines(&header_box(&state, Some(2024), &FormatInfo::default())?)?;
        ensure!(primary == "2024 \u{2022} 25 tracks", "primary: {primary}");
        ensure!(secondary.is_empty() && !visible, "empty format must hide");
        Ok(())
    }

    #[test]
    fn album_header_fits_minimum_window() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let mut album = fixture_album(Some(1994));
        album.title = "Selected Ambient Works Volume II With A Very Long Title".into();
        let (header, _, _) = build_album_header(&state, &album, &flac_info(), true);
        assert_fits_minimum_window(&header, "album header")
    }
}
