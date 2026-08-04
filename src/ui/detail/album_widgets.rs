//! Widgets for the album detail content area.

use libadwaita::gtk::{
    Align::Start,
    Box, Button,
    ContentFit::Cover,
    Label, ListBox,
    Orientation::{Horizontal, Vertical},
    Overlay, Picture, ScrolledWindow,
    accessible::Property::Label as PropertyLabel,
    pango::EllipsizeMode::End,
    prelude::{AccessibleExtManual, BoxExt, WidgetExt},
};

use crate::ui::{build_album_play_button, detail::common::build_scroll_content};

/// Size of the album cover artwork on the detail page in pixels.
pub const DETAIL_COVER_SIZE: i32 = 320;

/// Owned widgets for the album detail content area.
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

/// Build the scrollable content area with album widgets.
#[must_use]
pub fn build_album_content() -> AlbumDetailContent {
    let (scroll, content) = build_scroll_content();

    let artwork = Picture::builder()
        .content_fit(Cover)
        .can_shrink(true)
        .css_classes(["album-cover"])
        .build();
    artwork.update_property(&[PropertyLabel("Album artwork")]);

    let artwork_wrapper = Box::builder()
        .orientation(Horizontal)
        .width_request(DETAIL_COVER_SIZE)
        .height_request(DETAIL_COVER_SIZE)
        .halign(Start)
        .build();
    artwork_wrapper.append(&artwork);

    let overlay = Overlay::new();
    overlay.set_child(Some(&artwork_wrapper));
    overlay.set_css_classes(&["cover-overlay"]);
    overlay.set_halign(Start);

    let play_button = build_album_play_button();
    overlay.add_overlay(&play_button);
    content.append(&overlay);

    let title_label = Label::builder()
        .css_classes(["title-2", "heading"])
        .ellipsize(End)
        .halign(Start)
        .build();
    title_label.update_property(&[PropertyLabel("Album title")]);
    content.append(&title_label);

    let artist_label = Label::builder()
        .css_classes(["title-4", "accent"])
        .ellipsize(End)
        .halign(Start)
        .build();
    artist_label.update_property(&[PropertyLabel("Artist name")]);
    content.append(&artist_label);

    let meta_box = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .halign(Start)
        .build();

    let year_label = Label::builder()
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    year_label.update_property(&[PropertyLabel("Release year")]);
    meta_box.append(&year_label);

    let genre_label = Label::builder()
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    genre_label.update_property(&[PropertyLabel("Genre")]);
    meta_box.append(&genre_label);

    let tracks_label = Label::builder()
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    tracks_label.update_property(&[PropertyLabel("Track count")]);
    meta_box.append(&tracks_label);

    let format_label = Label::builder()
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    format_label.update_property(&[PropertyLabel("Audio format")]);
    meta_box.append(&format_label);

    content.append(&meta_box);

    let tracks_header = Label::builder()
        .label("Tracks")
        .css_classes(["title-4", "heading"])
        .halign(Start)
        .build();
    tracks_header.update_property(&[PropertyLabel("Track list")]);
    content.append(&tracks_header);

    let track_list = ListBox::builder().css_classes(["boxed-list"]).build();
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
