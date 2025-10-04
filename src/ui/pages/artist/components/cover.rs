use std::path::Path;

use gtk4::{Align::Start, Box, Fixed, Label, Orientation::Vertical, Overlay, Widget};
use libadwaita::prelude::{BoxExt, FixedExt, IsA, WidgetExt};

use crate::ui::components::tiles::helpers::{create_album_cover, create_dr_overlay};

/// Creates the album cover container with proper sizing and alignment
pub fn create_album_cover_container(cover_art_path: Option<&Path>, cover_size: i32) -> Box {
    // Create fixed-size container for album cover to ensure consistent sizing
    let cover_container = Box::new(Vertical, 0);
    cover_container.set_size_request(cover_size, cover_size);
    cover_container.set_halign(Start);
    cover_container.set_valign(Start);
    let cover = create_album_cover(cover_art_path, cover_size);
    cover_container.append(&cover);
    cover_container
}

/// Creates a DR overlay label
pub fn create_dr_badge(dr_value: Option<u8>, dr_is_best: bool) -> Option<Label> {
    create_dr_overlay(dr_value, dr_is_best)
}

/// Creates an overlay to stack DR badge and play button on top of album cover
pub fn create_album_overlay(
    cover_container: Box,
    dr_badge: Option<Label>,
    show_dr_badges: bool,
) -> Overlay {
    let overlay = Overlay::new();
    let (width, height) = cover_container.size_request();
    overlay.set_size_request(width, height);
    overlay.set_child(Some(&cover_container));
    overlay.set_halign(Start);
    overlay.set_valign(Start);

    // Conditionally add DR badge to overlay based on user settings
    if show_dr_badges
        && let Some(dr_label) = dr_badge {
            overlay.add_overlay(&dr_label);
        }
    overlay
}

/// Adds a widget as an overlay to the main overlay
pub fn add_overlay_to_album_overlay(overlay: &Overlay, widget: &impl IsA<Widget>) {
    overlay.add_overlay(widget);
}

/// Creates the fixed container for the cover area
pub fn create_cover_fixed(overlay: &Overlay, cover_size: i32) -> Fixed {
    let cover_fixed = Fixed::new();
    cover_fixed.set_size_request(-1, cover_size);
    cover_fixed.put(overlay, 0.0, 0.0);
    cover_fixed
}
