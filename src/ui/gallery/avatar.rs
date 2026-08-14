//! Artist avatar/card builders.

use std::sync::Arc;

use libadwaita::{
    glib::{prelude::Cast, spawn_future_local},
    gtk::{
        Align::Start, Box, GestureClick, Image, Label, Orientation::Vertical, Overlay, Widget,
        accessible::Property::Label as PropertyLabel, pango::EllipsizeMode::End,
    },
    prelude::{AccessibleExtManual, BoxExt, WidgetExt},
};

use crate::{
    app::runtime::{AppState, NavigationEvent::ArtistDetail},
    storage::catalog::Artist,
};

/// Build the avatar widget for an artist.
///
/// Returns an `Image` with a generic artist icon.
fn build_artist_avatar(size: i32) -> Widget {
    let avatar = Image::builder()
        .icon_name("avatar-default-symbolic")
        .pixel_size(size / 2)
        .width_request(size)
        .height_request(size)
        .css_classes(["artist-avatar", "dim-label"])
        .build();
    avatar.update_property(&[PropertyLabel("Artist icon")]);
    avatar.upcast()
}

/// Build a single artist card widget.
///
/// Returns a `Box` containing a vertical layout with avatar,
/// name, and album count labels. Matches the album card structural
/// pattern (Overlay wrapper) for consistent card sizing.
///
/// Also returns the `Overlay` wrapping the avatar so zoom can resize it
/// in place without rebuilding the card.
pub fn build_artist_card(state: &Arc<AppState>, artist: &Artist, size: i32) -> (Box, Overlay) {
    let card = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .css_classes(["card"])
        .can_focus(true)
        .tooltip_text(format!("View albums by {}", artist.name))
        .build();
    card.update_property(&[PropertyLabel(&format!("View albums by {}", artist.name))]);

    let avatar = build_artist_avatar(size);

    let overlay = Overlay::new();
    overlay.set_child(Some(&avatar));
    overlay.set_css_classes(&["cover-overlay"]);

    card.append(&overlay.clone().upcast::<Widget>());

    let name_label = Label::builder()
        .label(&artist.name)
        .ellipsize(End)
        .max_width_chars(20)
        .css_classes(["heading", "title"])
        .halign(Start)
        .build();
    name_label.update_property(&[PropertyLabel(&format!("Artist: {}", artist.name))]);

    let album_count_label = Label::builder()
        .label(format!("{} albums", artist.album_count))
        .ellipsize(End)
        .max_width_chars(20)
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    album_count_label.update_property(&[PropertyLabel(&format!(
        "{} albums by {}",
        artist.album_count, artist.name
    ))]);

    card.append(&name_label);
    card.append(&album_count_label);

    let gesture = GestureClick::new();
    let state_clone = Arc::clone(state);
    let artist_id = artist.id;
    gesture.connect_released(move |_, _, _, _| {
        let state = Arc::clone(&state_clone);
        spawn_future_local(async move {
            state.send_navigation_event(ArtistDetail(artist_id)).await;
        });
    });
    card.add_controller(gesture);

    (card, overlay)
}
