//! Album and artist card label sizing for in-place zoom.
//!
//! Provides character-limit helpers and card resize logic
//! used to scale album and artist card layouts with cover size.

use libadwaita::{
    gtk::{Box as GtkBox, Label, Overlay, Widget},
    prelude::{Cast, WidgetExt},
};

/// Calculate the title label's character limit for a cover size.
#[must_use]
pub fn title_max_chars(size: i32) -> i32 {
    (size / 10).max(6)
}

/// Calculate the artist label's character limit for a cover size.
#[must_use]
pub fn artist_max_chars(size: i32) -> i32 {
    (size / 8).max(8)
}

/// Calculate the format label's character limit for a cover size.
#[must_use]
pub fn format_max_chars(size: i32) -> i32 {
    (size / 10).max(6)
}

/// Return the card containing `overlay`.
///
/// Looks up the overlay's parent and downcasts it to the card `Box`.
/// Returns `None` for an unexpected widget tree.
fn card_container(overlay: &Overlay) -> Option<GtkBox> {
    overlay.parent()?.downcast().ok()
}

/// Return the `Label` immediately following `anchor` in its card.
///
/// Returns `None` for a missing sibling or an unexpected widget type.
fn sibling_label(anchor: &Widget) -> Option<Label> {
    anchor.next_sibling()?.downcast().ok()
}

/// Resize an album card's layout and metadata constraints to `size`.
pub fn resize_album_card(overlay: &Overlay, size: i32) {
    let Some(card) = card_container(overlay) else {
        return;
    };
    card.set_width_request(size);

    let Some(title) = sibling_label(overlay.upcast_ref()) else {
        return;
    };
    title.set_max_width_chars(title_max_chars(size));

    let Some(artist) = sibling_label(title.upcast_ref()) else {
        return;
    };
    artist.set_max_width_chars(artist_max_chars(size));

    let Some(next) = artist.next_sibling() else {
        return;
    };
    let Ok(format_row) = next.downcast::<GtkBox>() else {
        return;
    };
    format_row.set_width_request(size);
    if let Some(child) = format_row.first_child()
        && let Ok(format_label) = child.downcast::<Label>()
    {
        format_label.set_max_width_chars(format_max_chars(size));
    }
}

/// Resize an artist card's layout and metadata constraints to `size`.
///
/// Mirrors [`resize_album_card`] for the two-label artist structure
/// (overlay, name, album count). The name cap matches the album title cap
/// and the count cap matches the album format cap so both grids stay
/// visually in sync and long names cannot inflate the card beyond `size`.
pub fn resize_artist_card(overlay: &Overlay, size: i32) {
    let Some(card) = card_container(overlay) else {
        return;
    };
    card.set_width_request(size);

    let Some(name) = sibling_label(overlay.upcast_ref()) else {
        return;
    };
    name.set_max_width_chars(title_max_chars(size));

    let Some(count) = sibling_label(name.upcast_ref()) else {
        return;
    };
    count.set_max_width_chars(format_max_chars(size));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, bail, ensure},
        libadwaita::{
            gtk::{self, Box, Label, Orientation::Horizontal, Overlay, test},
            prelude::{Cast, WidgetExt},
        },
    };

    use crate::{
        app::runtime::AppState,
        storage::{
            catalog::{Album, Artist},
            formats::FormatInfo,
        },
        ui::gallery::{
            avatar::build_artist_card,
            card::build_album_card,
            label_sizing::{
                artist_max_chars, format_max_chars, resize_album_card, resize_artist_card,
                title_max_chars,
            },
        },
    };

    fn build_mock_card(size: i32) -> Result<(Box, Overlay)> {
        let album = Album {
            id: 7,
            title: "Test Album".into(),
            artist_id: 1,
            year: Some(2024),
            genre: None,
            artwork_path: None,
            track_count: 12,
            total_duration: 3600.0,
            format_summary: "FLAC".into(),
            lossless: true,
            format: "FLAC".into(),
            bit_depth: Some(24),
            sample_rate: Some(96000),
        };
        let state = Arc::new(AppState::mock()?);
        Ok(build_album_card(
            &state,
            &album,
            "Test Artist",
            &FormatInfo::default(),
            size,
        ))
    }

    fn long_named_artist() -> Artist {
        Artist {
            id: 1,
            name: "Black Country, New Road".into(),
            album_count: 2,
        }
    }

    #[test]
    fn max_chars_scale_with_cover_size() {
        let cases = [
            (0, 6, 8),
            (40, 6, 8),
            (60, 6, 8),
            (72, 7, 9),
            (100, 10, 12),
            (120, 12, 15),
            (180, 18, 22),
            (240, 24, 30),
        ];
        for (size, title, artist) in cases {
            assert_eq!(
                title_max_chars(size),
                title,
                "cover size {size} px must cap the title at {title} chars"
            );
            assert_eq!(
                format_max_chars(size),
                title,
                "cover size {size} px must cap the format at {title} chars"
            );
            assert_eq!(
                artist_max_chars(size),
                artist,
                "cover size {size} px must cap the artist at {artist} chars"
            );
        }
    }

    #[test]
    fn resize_album_card_scales_layout_and_labels() -> Result<()> {
        let (card, overlay) = build_mock_card(120)?;
        for size in [120, 200, 40] {
            resize_album_card(&overlay, size);
            ensure!(
                card.width_request() == size,
                "card width must follow the cover size"
            );
            let title = overlay.next_sibling();
            let Some(title) = title.as_ref().and_then(|w| w.downcast_ref::<Label>()) else {
                bail!("title label must follow the overlay in the card");
            };
            let cap = title_max_chars(size);
            ensure!(
                title.max_width_chars() == cap,
                "title must cap at {cap} chars for cover size {size}"
            );
            let artist = title.next_sibling();
            let Some(artist) = artist.as_ref().and_then(|w| w.downcast_ref::<Label>()) else {
                bail!("artist label must follow the title label");
            };
            let cap = artist_max_chars(size);
            ensure!(
                artist.max_width_chars() == cap,
                "artist must cap at {cap} chars for cover size {size}"
            );
            let format_row = artist.next_sibling();
            let Some(format_row) = format_row.as_ref().and_then(|w| w.downcast_ref::<Box>()) else {
                bail!("format row must follow the artist label");
            };
            ensure!(
                format_row.width_request() == size,
                "format row width must follow the cover size"
            );
            let format_label = format_row.first_child();
            let Some(format_label) = format_label
                .as_ref()
                .and_then(|w| w.downcast_ref::<Label>())
            else {
                bail!("format row must start with the format label");
            };
            let cap = format_max_chars(size);
            ensure!(
                format_label.max_width_chars() == cap,
                "format label must cap at {cap} chars for cover size {size}"
            );
        }
        Ok(())
    }

    #[test]
    fn resize_artist_card_scales_layout_and_labels() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let artist = long_named_artist();
        let (card, overlay) = build_artist_card(&state, &artist, 120);
        for size in [120, 200, 40] {
            resize_artist_card(&overlay, size);
            ensure!(
                card.width_request() == size,
                "card width must follow the avatar size"
            );
            let name = overlay.next_sibling();
            let Some(name) = name.as_ref().and_then(|w| w.downcast_ref::<Label>()) else {
                bail!("name label must follow the overlay in the card");
            };
            let cap = title_max_chars(size);
            ensure!(
                name.max_width_chars() == cap,
                "name must cap at {cap} chars for avatar size {size}"
            );
            let count = name.next_sibling();
            let Some(count) = count.as_ref().and_then(|w| w.downcast_ref::<Label>()) else {
                bail!("album count label must follow the name label");
            };
            let cap = format_max_chars(size);
            ensure!(
                count.max_width_chars() == cap,
                "album count must cap at {cap} chars for avatar size {size}"
            );
        }
        Ok(())
    }

    #[test]
    fn long_artist_name_keeps_card_width() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let artist = long_named_artist();
        for size in [120, 180, 240] {
            let (card, _) = build_artist_card(&state, &artist, size);
            let (_, natural, _, _) = card.measure(Horizontal, -1);
            ensure!(
                natural == size,
                "a long artist name must not inflate the card, got {natural} px for avatar size \
                 {size}"
            );
        }
        Ok(())
    }
}
