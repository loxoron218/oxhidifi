//! Album cover cards with async artwork loading.

use std::sync::Arc;

use libadwaita::{
    glib::spawn_future_local,
    gtk::{
        Align::{End, Start},
        Box, EventControllerMotion, Image, Label,
        Orientation::{Horizontal, Vertical},
        Overlay, Widget,
        accessible::Property::Label as PropertyLabel,
        pango::EllipsizeMode::End as EllipsizeEnd,
    },
    prelude::{AccessibleExtManual, BoxExt, ButtonExt, Cast, WidgetExt},
};

use crate::{
    app::runtime::{AppState, NavigationEvent::AlbumDetail},
    storage::{catalog::Album, formats::FormatInfo},
    ui::{
        gallery::{
            build_navigation_gesture,
            label_sizing::{artist_max_chars, format_max_chars, title_max_chars},
            play_action::{album_play_icon, toggle_or_play_album},
        },
        osd_button::build_album_play_button,
    },
};

/// Build a card container `Box` with standard layout and tooltip.
///
/// Shared by album and artist cards to avoid duplicating the builder
/// pattern. Sets both `tooltip_text` and the accessible `Label` from
/// `tooltip`.
#[must_use]
pub fn build_card_box(size: i32, tooltip: &str) -> Box {
    let card = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .css_classes(["card"])
        .can_focus(true)
        .width_request(size)
        .tooltip_text(tooltip)
        .build();
    card.update_property(&[PropertyLabel(tooltip)]);
    card
}

/// Build a placeholder cover art widget.
///
/// Returns an `Image` with a generic audio icon. Used as the initial
/// state before async cover art loading completes.
#[must_use]
pub fn build_placeholder(size: i32) -> Widget {
    let placeholder = Image::builder()
        .icon_name("audio-x-generic-symbolic")
        .pixel_size(size / 2)
        .width_request(size)
        .height_request(size)
        .css_classes(["album-cover", "dim-label"])
        .build();
    placeholder.update_property(&[PropertyLabel("Album cover placeholder")]);
    placeholder.upcast()
}

/// Build the cover art overlay with hover play button for an album card.
fn build_card_overlay(state: &Arc<AppState>, album_id: i64, size: i32) -> Overlay {
    let cover_art = build_placeholder(size);

    let overlay = Overlay::builder()
        .width_request(size)
        .height_request(size)
        .halign(Start)
        .build();
    overlay.set_child(Some(&cover_art));
    overlay.set_css_classes(&["cover-overlay"]);

    let play_button = build_album_play_button();
    play_button.set_visible(false);

    overlay.add_overlay(&play_button);

    let motion_ctrl = EventControllerMotion::new();
    let btn_show = play_button.clone();
    let state_enter = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(motion_ctrl.connect_enter(move |_, _, _| {
            btn_show.set_icon_name(album_play_icon(&state_enter, album_id));
            btn_show.set_visible(true);
        }));
    let btn_hide = play_button.clone();
    state
        .handles
        .lock()
        .retain_signal(motion_ctrl.connect_leave(move |_| {
            btn_hide.set_visible(false);
        }));
    overlay.add_controller(motion_ctrl);

    let state_clone = Arc::clone(state);
    let btn_click = play_button.clone();
    state
        .handles
        .lock()
        .retain_signal(play_button.connect_clicked(move |_| {
            let icon = album_play_icon(&state_clone, album_id);
            btn_click.set_icon_name(if icon == "media-playback-pause-symbolic" {
                "media-playback-start-symbolic"
            } else {
                "media-playback-pause-symbolic"
            });

            let state_cb = Arc::clone(&state_clone);
            state_clone
                .handles
                .lock()
                .retain_task(spawn_future_local(async move {
                    toggle_or_play_album(&state_cb, album_id).await;
                }));
        }));

    overlay
}

/// Build a single album card widget.
///
/// Returns a `Box` containing a vertical layout with cover art,
/// title, artist, format summary, and year labels. Uses
/// `GestureClick` for click handling instead of `Button` to avoid
/// theme-inflated natural sizing from the `card` CSS class.
///
/// Also returns the `Overlay` wrapping the cover art so it can be
/// resized in place on zoom changes and updated asynchronously after
/// the card is added to the container.
///
/// # Arguments
///
/// * `state` - Application state
/// * `album` - Album data
/// * `artist_name` - Display name of the album artist
/// * `format_info` - Format summary for the album
/// * `size` - Cover art size in pixels (derived from the grid zoom level)
pub fn build_album_card(
    state: &Arc<AppState>,
    album: &Album,
    artist_name: &str,
    format_info: &FormatInfo,
    size: i32,
) -> (Box, Overlay) {
    let tooltip = format!("Play \u{201c}{}\u{201d} by album artist", album.title);
    let card = build_card_box(size, &tooltip);

    let album_id = album.id;

    let overlay = build_card_overlay(state, album_id, size);

    card.append(&overlay.clone().upcast::<Widget>());

    let title_label = Label::builder()
        .label(&album.title)
        .ellipsize(EllipsizeEnd)
        .max_width_chars(title_max_chars(size))
        .css_classes(["heading", "title"])
        .halign(Start)
        .build();
    title_label.update_property(&[PropertyLabel(&format!("Album: {}", album.title))]);

    let artist_label = Label::builder()
        .label(artist_name)
        .ellipsize(EllipsizeEnd)
        .max_width_chars(artist_max_chars(size))
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();
    artist_label.update_property(&[PropertyLabel(&format!("Artist: {artist_name}"))]);

    let format_row = Box::builder()
        .orientation(Horizontal)
        .spacing(6)
        .width_request(size)
        .build();

    let (format_text, format_tooltip) = if format_info.is_mixed() {
        ("Mixed".to_string(), format_info.summary())
    } else {
        let s = format_info.summary();
        (s.clone(), s)
    };

    let format_label = Label::builder()
        .label(&format_text)
        .tooltip_text(&format_tooltip)
        .ellipsize(EllipsizeEnd)
        .max_width_chars(format_max_chars(size))
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .hexpand(true)
        .build();
    format_label.update_property(&[PropertyLabel(&format!("Format: {format_tooltip}"))]);

    let year_label = Label::builder()
        .label(album.year.map_or(String::new(), |y| y.to_string()))
        .css_classes(["dim-label", "caption"])
        .halign(End)
        .build();
    year_label.update_property(&[PropertyLabel("Release year")]);

    format_row.append(&format_label);
    format_row.append(&year_label);

    card.append(&title_label);
    card.append(&artist_label);
    card.append(&format_row);

    if !state.storage.get_show_album_labels() {
        title_label.set_visible(false);
        artist_label.set_visible(false);
        format_row.set_visible(false);
    }

    let gesture = build_navigation_gesture(state, AlbumDetail(album_id));
    card.add_controller(gesture);

    (card, overlay)
}
