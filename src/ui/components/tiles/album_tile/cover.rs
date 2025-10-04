use std::{cell::Cell, path::Path, rc::Rc};

use gtk4::{Align::Start, Box, Orientation::Vertical, Overlay, Picture};
use libadwaita::prelude::{BoxExt, WidgetExt};

use crate::ui::{
    components::tiles::helpers::{create_album_cover, create_dr_overlay},
    grids::album_grid_state::AlbumGridItem,
};

/// Creates the album cover container with overlay for DR badge
pub fn create_album_cover_section(
    album: &AlbumGridItem,
    cover_size: i32,
    show_dr_badges: Rc<Cell<bool>>,
) -> (Overlay, Picture) {
    // Create album cover picture from cached image file
    let cover = create_album_cover(album.cover_art.as_deref().map(Path::new), cover_size);

    // Container for the cover, to ensure fixed size
    let cover_container = Box::new(Vertical, 0);
    cover_container.set_size_request(cover_size, cover_size);
    cover_container.set_halign(Start);
    cover_container.set_valign(Start);
    cover_container.append(&cover);

    // Overlay for DR badge on the cover
    let overlay = Overlay::new();
    overlay.set_size_request(cover_size, cover_size);
    overlay.set_child(Some(&cover_container));
    overlay.set_halign(Start);
    overlay.set_valign(Start);
    if show_dr_badges.get()
        && let Some(dr_label) =
            create_dr_overlay(album.dr_value.map(|dr| dr as u8), album.dr_is_best)
    {
        overlay.add_overlay(&dr_label);
    }
    (overlay, cover)
}
