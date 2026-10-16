//! Widgets for the album detail content area.

use libadwaita::{
    gtk::{
        Align::Start,
        Box, Button,
        ContentFit::Cover,
        Label, ListBox,
        Orientation::{Horizontal, Vertical},
        Overlay, Picture, ScrolledWindow,
        accessible::Property::Label as PropertyLabel,
        pango::EllipsizeMode::End,
    },
    prelude::{AccessibleExtManual, BoxExt},
};

use crate::ui::{detail::page::build_scroll_content, osd_button::build_album_play_button};

/// Size of the album cover artwork on the detail page in pixels.
pub const DETAIL_COVER_SIZE: i32 = 320;

/// Owned widgets for the album detail content area.
#[derive(Debug)]
pub struct AlbumDetailContent {
    /// The scroll window wrapping content.
    pub scroll: ScrolledWindow,
    /// Album artwork display.
    pub artwork: Picture,
    /// Play/pause button overlaid on the artwork.
    pub play_button: Button,
    /// Album title label.
    pub title_label: Label,
    /// Artist name label.
    pub artist_label: Label,
    /// Release year label.
    pub year_label: Label,
    /// Genre label.
    pub genre_label: Label,
    /// Track count label.
    pub tracks_label: Label,
    /// Format summary label.
    pub format_label: Label,
    /// Track listing container.
    pub track_list: ListBox,
}

/// Widget references for the album detail page.
#[derive(Debug)]
pub struct AlbumDetailWidgets<'a> {
    /// Album artwork display.
    pub artwork: &'a Picture,
    /// Album title label.
    pub title_label: &'a Label,
    /// Artist name label.
    pub artist_label: &'a Label,
    /// Release year label.
    pub year_label: &'a Label,
    /// Genre label.
    pub genre_label: &'a Label,
    /// Track count label.
    pub tracks_label: &'a Label,
    /// Format summary label.
    pub format_label: &'a Label,
    /// Track listing container.
    pub track_list: &'a ListBox,
}

/// Build the title and artist labels for the album content area.
///
/// # Returns
///
/// * `(Label, Label)` - Title label and artist name label.
fn build_title_labels() -> (Label, Label) {
    let title_label = Label::builder()
        .css_classes(["title-2", "heading"])
        .ellipsize(End)
        .halign(Start)
        .tooltip_text("Album title")
        .build();
    title_label.update_property(&[PropertyLabel("Album title")]);
    let artist_label = Label::builder()
        .css_classes(["title-4", "accent"])
        .ellipsize(End)
        .halign(Start)
        .tooltip_text("Artist name")
        .build();
    artist_label.update_property(&[PropertyLabel("Artist name")]);
    (title_label, artist_label)
}

/// Build one dimmed metadata label.
///
/// # Arguments
///
/// * `tooltip` - Tooltip and accessible label text.
///
/// # Returns
///
/// * `Label` - Ellipsized metadata label.
fn build_meta_label(tooltip: &str) -> Label {
    let label = Label::builder()
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .tooltip_text(tooltip)
        .build();
    label.update_property(&[PropertyLabel(tooltip)]);
    label
}

/// Build the metadata box with year, genre, track-count, and format labels.
///
/// # Returns
///
/// * `(Box, Label, Label, Label, Label)` - Metadata box with year, genre, track-count, and format
///   labels.
fn build_meta_box() -> (Box, Label, Label, Label, Label) {
    let meta_box = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .halign(Start)
        .css_classes(["detail-meta"])
        .tooltip_text("Album metadata")
        .build();
    meta_box.update_property(&[PropertyLabel("Album metadata")]);
    let year_label = build_meta_label("Release year");
    meta_box.append(&year_label);
    let genre_label = build_meta_label("Genre");
    meta_box.append(&genre_label);
    let tracks_label = build_meta_label("Track count");
    meta_box.append(&tracks_label);
    let format_label = build_meta_label("Audio format");
    meta_box.append(&format_label);
    (
        meta_box,
        year_label,
        genre_label,
        tracks_label,
        format_label,
    )
}

/// Build the scrollable content area with album widgets.
#[must_use]
pub fn build_album_content() -> AlbumDetailContent {
    let (scroll, content) = build_scroll_content();

    let artwork = Picture::builder()
        .content_fit(Cover)
        .can_shrink(true)
        .css_classes(["album-cover"])
        .tooltip_text("Album artwork")
        .build();
    artwork.update_property(&[PropertyLabel("Album artwork")]);

    let artwork_wrapper = Box::builder()
        .orientation(Horizontal)
        .width_request(DETAIL_COVER_SIZE)
        .height_request(DETAIL_COVER_SIZE)
        .halign(Start)
        .css_classes(["artwork-wrapper"])
        .tooltip_text("Album artwork container")
        .build();
    artwork_wrapper.append(&artwork);

    let overlay = Overlay::builder()
        .child(&artwork_wrapper)
        .css_classes(["cover-overlay"])
        .tooltip_text("Album cover overlay")
        .halign(Start)
        .build();

    let play_button = build_album_play_button();
    overlay.add_overlay(&play_button);
    content.append(&overlay);

    let (title_label, artist_label) = build_title_labels();
    content.append(&title_label);
    content.append(&artist_label);

    let (meta_box, year_label, genre_label, tracks_label, format_label) = build_meta_box();
    content.append(&meta_box);

    let tracks_header = Label::builder()
        .label("Tracks")
        .css_classes(["title-4", "heading"])
        .halign(Start)
        .tooltip_text("Track list")
        .build();
    tracks_header.update_property(&[PropertyLabel("Track list")]);
    content.append(&tracks_header);

    let track_list = ListBox::builder()
        .css_classes(["boxed-list"])
        .tooltip_text("Album track list")
        .build();
    track_list.update_property(&[PropertyLabel("Album track list")]);
    content.append(&track_list);

    scroll.set_child(Some(&content));

    AlbumDetailContent {
        scroll,
        artwork,
        play_button,
        title_label,
        artist_label,
        year_label,
        genre_label,
        tracks_label,
        format_label,
        track_list,
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, test},
            prelude::WidgetExt,
        },
    };

    use crate::ui::detail::body_compose::{AlbumDetailContent, build_album_content};

    fn assert_content_focusable(content: &AlbumDetailContent) -> Result<()> {
        ensure!(
            content.scroll.can_focus(),
            "detail scroll must stay focusable so keyboard focus (and Escape-to-back) reaches \
             detail content"
        );
        ensure!(
            content.track_list.can_focus(),
            "detail track list must stay focusable so track rows (and Escape-to-back) keep working"
        );
        Ok(())
    }

    #[test]
    fn album_content_widgets_stay_in_focus_chain() -> Result<()> {
        let content = build_album_content();
        assert_content_focusable(&content)
    }
}
