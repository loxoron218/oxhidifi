//! Artist detail rendering logic.

use libadwaita::{
    gtk::{
        Align::Start,
        Box, Label,
        Orientation::{Horizontal, Vertical},
        Widget,
        pango::EllipsizeMode::End,
    },
    prelude::{BoxExt, Cast},
};

use crate::{library::models::Artist, ui::components::cover_art::CoverArt};

/// Renderer for artist detail views.
pub struct ArtistDetailRenderer;

impl ArtistDetailRenderer {
    /// Renders artist detail to a container.
    ///
    /// # Arguments
    ///
    /// * `container` - Container widget to append content to
    /// * `artist` - Artist to display
    ///
    /// # Returns
    ///
    /// Nothing, renders content directly to the container.
    pub fn render(container: &Box, artist: &Artist) {
        let header = Self::create_artist_header(artist);
        container.append(&header);

        let album_list = Self::create_album_list_placeholder();
        container.append(&album_list);
    }

    /// Creates the artist header section with image and metadata.
    ///
    /// # Arguments
    ///
    /// * `artist` - The artist to create header for
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the artist header.
    fn create_artist_header(artist: &Artist) -> Widget {
        let header_container = Box::builder().orientation(Horizontal).spacing(24).build();

        let cover_art = CoverArt::builder()
            .artwork_path("")
            .show_dr_badge(false)
            .dimensions(300, 300)
            .build();

        let cover_container = Box::builder()
            .orientation(Vertical)
            .halign(Start)
            .valign(Start)
            .build();

        cover_container.append(&cover_art.widget);

        let metadata_container = Box::builder()
            .orientation(Vertical)
            .hexpand(true)
            .spacing(6)
            .build();

        let name_label = Label::builder()
            .label(&artist.name)
            .halign(Start)
            .xalign(0.0)
            .css_classes(["title-1"])
            .ellipsize(End)
            .tooltip_text(&artist.name)
            .build();
        metadata_container.append(name_label.upcast_ref::<Widget>());

        let bio_label = Label::builder()
            .label("Artist biography would appear here when available.")
            .halign(Start)
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(80)
            .css_classes(["dim-label"])
            .build();
        metadata_container.append(bio_label.upcast_ref::<Widget>());

        header_container.append(cover_container.upcast_ref::<Widget>());
        header_container.append(metadata_container.upcast_ref::<Widget>());

        header_container.upcast_ref::<Widget>().clone()
    }

    /// Creates a placeholder for the album listing section.
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the album list placeholder.
    fn create_album_list_placeholder() -> Widget {
        let list_container = Box::builder().orientation(Vertical).spacing(6).build();

        let title_label = Label::builder()
            .label("Albums")
            .halign(Start)
            .xalign(0.0)
            .css_classes(["title-2"])
            .build();
        list_container.append(title_label.upcast_ref::<Widget>());

        let placeholder_label = Label::builder()
            .label("Album listing would appear here.")
            .halign(Start)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        list_container.append(placeholder_label.upcast_ref::<Widget>());

        list_container.upcast_ref::<Widget>().clone()
    }
}
