//! Album card label sizing for in-place zoom.
//!
//! Provides character-limit helpers and card resize logic
//! used to scale album card layouts with cover size.

use libadwaita::{
    glib::prelude::Cast,
    gtk::{Box as GtkBox, Label, Overlay},
    prelude::WidgetExt,
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

/// Resize an album card's layout and metadata constraints to `size`.
pub fn resize_album_card(overlay: &Overlay, size: i32) {
    let Some(parent) = overlay.parent() else {
        return;
    };
    let Ok(card) = parent.downcast::<GtkBox>() else {
        return;
    };
    card.set_width_request(size);

    let Some(next) = overlay.next_sibling() else {
        return;
    };
    let Ok(title) = next.downcast::<Label>() else {
        return;
    };
    title.set_max_width_chars(title_max_chars(size));

    let Some(next) = title.next_sibling() else {
        return;
    };
    let Ok(artist) = next.downcast::<Label>() else {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, bail, ensure},
        libadwaita::{
            glib::prelude::Cast,
            gtk::{self, Box, Label, Overlay, test},
            prelude::WidgetExt,
        },
    };

    use crate::{
        app::runtime::AppState,
        storage::{catalog::Album, formats::FormatInfo},
        ui::gallery::{
            card::build_album_card,
            label_sizing::{
                artist_max_chars, format_max_chars, resize_album_card, title_max_chars,
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
}
