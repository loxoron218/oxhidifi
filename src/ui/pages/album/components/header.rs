use std::{cell::Cell, rc::Rc, sync::Arc};

use gtk4::{
    Align::{End, Start},
    Box, Button, Label,
    Orientation::{Horizontal, Vertical},
    Overlay,
    glib::MainContext,
    pango::{EllipsizeMode, WrapMode::Word},
};
use libadwaita::prelude::{BoxExt, ButtonExt, WidgetExt};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    data::models::{Album, Artist, Folder, Track},
    ui::{
        components::player_bar::PlayerBar,
        pages::album::components::{
            dr_badge::build_dr_badge,
            technical_info::{build_album_cover, build_album_metadata, build_technical_info},
        },
    },
};

/// Build the header section of the album page
///
/// This function creates the top section of the album page which includes:
/// - Album cover art with an overlay play button
/// - Album title and artist name
/// - Album metadata (year, track count, duration)
/// - Technical information (bit depth, sample rate, format)
/// - Dynamic range (DR) badge if enabled
///
/// # Arguments
///
/// * `album` - The album to display
/// * `artist` - The album's artist
/// * `folder` - The folder containing the album
/// * `tracks` - The tracks in the album
/// * `show_dr_badges` - Flag to control DR badge visibility
/// * `db_pool` - Database connection pool for DR data
/// * `sender` - Channel sender for UI updates
/// * `player_bar` - Reference to the application's player bar
///
/// # Returns
///
/// A GTK Box widget containing all the album header elements
pub fn build_album_header(
    album: &Album,
    artist: &Artist,
    folder: &Folder,
    tracks: &[Track],
    show_dr_badges: Rc<Cell<bool>>,
    db_pool: Arc<sqlx::SqlitePool>,
    sender: UnboundedSender<()>,
    player_bar: PlayerBar,
) -> Box {
    // Create the main header container with horizontal orientation
    let header = Box::builder()
        .orientation(Horizontal)
        .spacing(32)
        .css_classes(["album-header"])
        .hexpand(true)
        .build();

    // Add top margin if album has cover art
    if album.cover_art.is_some() {
        header.set_margin_top(32);
    }

    // Build the album cover widget
    let cover = build_album_cover(&album.cover_art);

    // Create an overlay to position the play button on top of the cover
    let overlay = Overlay::new();
    overlay.set_halign(Start);
    overlay.set_valign(Start);
    overlay.set_child(Some(&cover));

    // Create the play/pause button that overlays on the album cover
    let play_pause_button = Button::builder()
        .icon_name("media-playback-start")
        .css_classes(["play-pause-button", "album-cover-play"])
        .build();
    play_pause_button.set_size_request(56, 56);
    play_pause_button.set_valign(End);
    play_pause_button.set_halign(End);
    play_pause_button.set_margin_bottom(12);
    play_pause_button.set_margin_end(12);

    // Connect the play button click handler to queue the album
    let player_bar_clone = player_bar.clone();
    let album_id = album.id;
    play_pause_button.connect_clicked(move |_| {
        // Get the playback controller from the player bar
        if let Some(controller) = player_bar_clone.get_playback_controller() {
            // Spawn async task to queue the album
            let player_bar_async = player_bar_clone.clone();
            let context = MainContext::default();
            context.spawn_local(async move {
                // Queue the album for playback
                let mut controller = controller.lock().await;
                if let Err(e) = controller.queue_album(album_id).await {
                    eprintln!("Error queuing album {}: {}", album_id, e);
                }

                // Update navigation button states after queue initialization
                player_bar_async.update_navigation_button_states();
            });
        } else {
            eprintln!("No playback controller available");
        }
    });

    // Add the play button as an overlay on the album cover
    overlay.add_overlay(&play_pause_button);
    header.append(&overlay);

    // Create a vertical box to hold all the album information
    let info_box = Box::builder()
        .orientation(Vertical)
        .spacing(12)
        .halign(Start)
        .valign(Start)
        .hexpand(true)
        .css_classes(["album-info-box"])
        .build();

    // Create and configure the album title label
    let title_label = Label::builder()
        .label(&album.title)
        .halign(Start)
        .wrap(true)
        .wrap_mode(Word)
        .ellipsize(EllipsizeMode::End)
        .lines(3)
        .hexpand(true)
        .build();
    title_label.set_xalign(0.0);
    title_label.set_css_classes(&["album-title-label"]);
    info_box.append(&title_label);

    // Create and configure the artist name label
    let artist_label = Label::builder()
        .label(&artist.name)
        .halign(Start)
        .wrap(true)
        .wrap_mode(Word)
        .ellipsize(EllipsizeMode::End)
        .lines(1)
        .hexpand(true)
        .build();
    artist_label.set_xalign(0.0);
    artist_label.set_css_classes(&["album-artist-label"]);
    info_box.append(&artist_label);

    // Add album metadata (year, track count, duration) if available
    if let Some(meta_box) = build_album_metadata(tracks, album) {
        info_box.append(&meta_box);
    }

    // Add technical information (bit depth, sample rate, format, Hi-Res/Lossy icon) if available
    if let Some(tech_box) = build_technical_info(tracks, album) {
        info_box.append(&tech_box);
    }

    // Add DR badge if show_dr_badges is enabled
    if show_dr_badges.get() {
        info_box.append(&build_dr_badge(
            album.id,
            album.dr_value,
            album.dr_is_best,
            db_pool.clone(),
            sender.clone(),
            Rc::new(album.clone()),
            Rc::new(artist.clone()),
            Rc::new(folder.clone()),
        ));
    }

    // Add the information box to the main header
    header.append(&info_box);
    header
}
