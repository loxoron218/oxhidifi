//! Album and artist card label sizing for in-place zoom.
//!
//! Provides character-limit helpers and card resize logic
//! used to scale album and artist card layouts with cover size.

use {
    libadwaita::{
        gtk::{Box as GtkBox, Label, Overlay, Widget},
        prelude::{Cast, WidgetExt},
    },
    thiserror::Error,
};

/// Errors from album and artist card widget-tree lookups.
#[derive(Debug, Clone, Copy, Error)]
pub enum LabelSizingError {
    /// Overlay has no parent card container.
    #[error("card widget tree mismatch at {context}: missing parent container")]
    MissingParent {
        /// Lookup site (e.g. `"album card"`) for log context.
        context: &'static str,
    },
    /// Expected sibling widget is absent.
    #[error("card widget tree mismatch at {context}: missing sibling widget")]
    MissingSibling {
        /// Lookup site (e.g. `"album title"`) for log context.
        context: &'static str,
    },
    /// Widget exists but has an unexpected type.
    #[error("card widget tree mismatch at {context}: unexpected widget type")]
    UnexpectedType {
        /// Lookup site (e.g. `"album title"`) for log context.
        context: &'static str,
    },
}

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
/// # Arguments
///
/// * `overlay` - Cover/avatar overlay whose parent card is needed.
/// * `context` - Lookup site for error context.
fn card_container(overlay: &Overlay, context: &'static str) -> Result<GtkBox, LabelSizingError> {
    let parent = overlay
        .parent()
        .ok_or(LabelSizingError::MissingParent { context })?;
    let Ok(card) = parent.downcast::<GtkBox>() else {
        return Err(LabelSizingError::UnexpectedType { context });
    };
    Ok(card)
}

/// Return the `Label` immediately following `anchor` in its card.
///
/// # Arguments
///
/// * `anchor` - Widget preceding the wanted label.
/// * `context` - Lookup site for error context.
fn sibling_label(anchor: &Widget, context: &'static str) -> Result<Label, LabelSizingError> {
    let sibling = anchor
        .next_sibling()
        .ok_or(LabelSizingError::MissingSibling { context })?;
    let Ok(label) = sibling.downcast::<Label>() else {
        return Err(LabelSizingError::UnexpectedType { context });
    };
    Ok(label)
}

/// Resize an album card's layout and metadata constraints to `size`.
///
/// # Arguments
///
/// * `overlay` - Cover overlay anchoring the card's widget tree.
/// * `size` - Cover size in pixels.
///
/// # Errors
///
/// Returns [`LabelSizingError`] naming the failing lookup when the widget
/// tree is missing a widget or has an unexpected type.
pub fn resize_album_card(overlay: &Overlay, size: i32) -> Result<(), LabelSizingError> {
    let card = card_container(overlay, "album card")?;
    card.set_width_request(size);

    let title = sibling_label(overlay.upcast_ref(), "album title")?;
    title.set_max_width_chars(title_max_chars(size));

    let artist = sibling_label(title.upcast_ref(), "album artist")?;
    artist.set_max_width_chars(artist_max_chars(size));

    let next = artist
        .next_sibling()
        .ok_or(LabelSizingError::MissingSibling {
            context: "format row",
        })?;
    let Ok(format_row) = next.downcast::<GtkBox>() else {
        return Err(LabelSizingError::UnexpectedType {
            context: "format row",
        });
    };
    format_row.set_width_request(size);
    if let Some(child) = format_row.first_child() {
        let Ok(format_label) = child.downcast::<Label>() else {
            return Err(LabelSizingError::UnexpectedType {
                context: "format label",
            });
        };
        format_label.set_max_width_chars(format_max_chars(size));
    }
    Ok(())
}

/// Resize an artist card's layout and metadata constraints to `size`.
///
/// Mirrors [`resize_album_card`] for the two-label artist structure.
///
/// # Arguments
///
/// * `overlay` - Avatar overlay anchoring the card's widget tree.
/// * `size` - Avatar size in pixels.
///
/// # Errors
///
/// Returns [`LabelSizingError`] naming the failing lookup when the widget
/// tree is missing a widget or has an unexpected type.
pub fn resize_artist_card(overlay: &Overlay, size: i32) -> Result<(), LabelSizingError> {
    let card = card_container(overlay, "artist card")?;
    card.set_width_request(size);

    let name = sibling_label(overlay.upcast_ref(), "artist name")?;
    name.set_max_width_chars(title_max_chars(size));

    let count = sibling_label(name.upcast_ref(), "artist count")?;
    count.set_max_width_chars(format_max_chars(size));
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Context, Result, bail, ensure},
        libadwaita::{
            gtk::{
                self, Box, Button, Label,
                Orientation::{Horizontal, Vertical},
                Overlay, test,
            },
            prelude::{BoxExt, Cast, WidgetExt},
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
                LabelSizingError, artist_max_chars, format_max_chars, resize_album_card,
                resize_artist_card, title_max_chars,
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
            resize_album_card(&overlay, size)
                .with_context(|| format!("resize album card to {size} px"))?;
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
            resize_artist_card(&overlay, size)
                .with_context(|| format!("resize artist card to {size} px"))?;
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
    fn resize_orphan_overlay_reports_missing_parent() -> Result<()> {
        let overlay = Overlay::new();
        let album_err = resize_album_card(&overlay, 120);
        ensure!(
            matches!(album_err, Err(LabelSizingError::MissingParent { .. })),
            "orphan album overlay must report a missing parent"
        );
        let artist_err = resize_artist_card(&overlay, 120);
        ensure!(
            matches!(artist_err, Err(LabelSizingError::MissingParent { .. })),
            "orphan artist overlay must report a missing parent"
        );
        Ok(())
    }

    #[test]
    fn resize_wrong_sibling_type_reports_unexpected_type() -> Result<()> {
        let card = Box::new(Vertical, 0);
        let overlay = Overlay::new();
        card.append(&overlay);
        card.append(&Button::new());
        let err = resize_artist_card(&overlay, 120);
        ensure!(
            matches!(err, Err(LabelSizingError::UnexpectedType { .. })),
            "a non-label sibling must report an unexpected widget type"
        );
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
