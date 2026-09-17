//! Artist avatar/card builders.

use std::sync::Arc;

use libadwaita::{
    glib::spawn_future_local,
    gtk::{
        Align::Start, Box, EventControllerMotion, Image, Label, Overlay, Widget,
        accessible::Property::Label as PropertyLabel, pango::EllipsizeMode::End,
    },
    prelude::{AccessibleExtManual, BoxExt, ButtonExt, Cast, WidgetExt},
};

use crate::{
    app::runtime::{AppState, NavigationEvent::ArtistDetail},
    storage::catalog::Artist,
    ui::{
        gallery::{build_navigation_gesture, card::build_card_box, play_action::play_artist},
        osd_button::build_album_play_button,
    },
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
    let tooltip = format!("View albums by {}", artist.name);
    let card = build_card_box(size, &tooltip);

    let avatar = build_artist_avatar(size);

    let overlay = Overlay::builder()
        .width_request(size)
        .height_request(size)
        .halign(Start)
        .build();
    overlay.set_child(Some(&avatar));
    overlay.set_css_classes(&["cover-overlay"]);

    let play_button = build_album_play_button();
    play_button.set_icon_name("media-playback-start-symbolic");
    play_button.set_tooltip_text(Some(&format!("Play all albums by {}", artist.name)));
    play_button.update_property(&[PropertyLabel(&format!(
        "Play all albums by {}",
        artist.name
    ))]);
    play_button.set_visible(false);
    overlay.add_overlay(&play_button);

    let motion = EventControllerMotion::new();
    let btn_show = play_button.clone();
    state
        .handles
        .lock()
        .retain_signal(motion.connect_enter(move |_, _, _| {
            btn_show.set_visible(true);
        }));
    let btn_hide = play_button.clone();
    state
        .handles
        .lock()
        .retain_signal(motion.connect_leave(move |_| {
            btn_hide.set_visible(false);
        }));
    overlay.add_controller(motion);

    let artist_id = artist.id;
    let state_clone = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(play_button.connect_clicked(move |_| {
            let state_cb = Arc::clone(&state_clone);
            state_clone
                .handles
                .lock()
                .retain_task(spawn_future_local(async move {
                    play_artist(&state_cb, artist_id).await;
                }));
        }));

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

    let gesture = build_navigation_gesture(state, ArtistDetail(artist.id));
    card.add_controller(gesture);

    (card, overlay)
}
