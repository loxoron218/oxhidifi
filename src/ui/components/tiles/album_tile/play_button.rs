use std::{path::Path, sync::Arc};

use gtk4::{
    Align::Center,
    Button, EventControllerMotion, Fixed, Overlay,
    glib::{MainContext, prelude::ObjectExt},
};
use libadwaita::prelude::{ButtonExt, FixedExt, WidgetExt};
use sqlx::SqlitePool;

use crate::{
    data::db::crud::fetch_songs_by_album,
    ui::{components::player_bar::PlayerBar, grids::album_grid_state::AlbumGridItem},
};

/// Creates the play button overlay that appears on hover
pub fn create_play_button(
    album: &AlbumGridItem,
    player_bar: PlayerBar,
    db_pool: Arc<SqlitePool>,
    cover_size: i32,
) -> Button {
    let play_button = Button::builder()
        .icon_name("media-playback-start")
        .css_classes(&["play-pause-button", "album-cover-play"][..])
        .build();
    play_button.set_size_request(56, 56);
    play_button.set_halign(Center);
    play_button.set_valign(Center);
    play_button.set_visible(false);

    // Clone all necessary data to avoid lifetime issues
    let player_bar_clone = player_bar.clone();
    let album_id = album.id;
    let album_title = album.title.clone();
    let artist_name = album.artist.clone();
    let cover_art_path = album.cover_art.clone();
    let album_format_bit_depth = album.bit_depth;
    let album_format_sample_rate = album.sample_rate;
    let album_format = album.format.clone();
    let db_pool_for_play_button = db_pool.clone();
    play_button.connect_clicked(move |_| {
        // Clone metadata for direct update
        let album_title_local = album_title.clone();
        let artist_name_local = artist_name.clone();
        let cover_art_path_local = cover_art_path.clone();
        let album_format_bit_depth_local = album_format_bit_depth;
        let album_format_sample_rate_local = album_format_sample_rate;
        let album_format_local = album_format.clone();
        let db_pool_for_songs = db_pool_for_play_button.clone();
        let album_id_local = album_id;

        // Get the playback controller from the player bar
        let player_bar_async = player_bar_clone.clone();
        if let Some(controller) = player_bar_async.get_playback_controller() {
            // Spawn async task to fetch the first song's metadata and queue the album
            MainContext::default().spawn_local(async move {
                // Fetch the first song's metadata from the database first
                let db_pool_songs = db_pool_for_songs.clone();
                let (duration, first_song_title) =
                    if let Ok(songs) = fetch_songs_by_album(&db_pool_songs, album_id_local).await {
                        if let Some(first_song) = songs.first() {
                            (first_song.duration, first_song.title.clone())
                        } else {
                            (None, album_title_local.clone())
                        }
                    } else {
                        (None, album_title_local.clone())
                    };

                // Update the player bar with album metadata directly to ensure it's updated before visibility
                // This ensures the player bar shows correct metadata even if the SongChanged event is delayed
                player_bar_async.update_with_metadata(
                    &album_title_local,
                    &first_song_title,
                    &artist_name_local,
                    cover_art_path_local.as_deref().map(Path::new),
                    album_format_bit_depth_local.map(|d| d as u32),
                    album_format_sample_rate_local.map(|d| d as u32),
                    album_format_local.as_deref(),
                    duration,
                );

                // Queue the album for playback
                let mut controller = controller.lock().await;
                if let Err(e) = controller.queue_album(album_id_local).await {
                    eprintln!("Error queuing album {}: {}", album_id_local, e);
                }

                // Update navigation button states after queue initialization
                player_bar_async.update_navigation_button_states();

                // Ensure the player bar is visible when playback starts
                player_bar_async.ensure_visible();
            });
        } else {
            eprintln!("No playback controller available");
        }
    });
    play_button
}

/// Creates the motion controller for hover effects on the album cover
pub fn create_motion_controller(play_button: &Button) -> EventControllerMotion {
    let motion_controller = EventControllerMotion::new();
    let play_button_weak = play_button.downgrade();
    motion_controller.connect_enter(move |_, _, _| {
        if let Some(btn) = play_button_weak.upgrade() {
            btn.set_visible(true);
        }
    });

    // Re-clone for the leave handler
    let play_button_weak = play_button.downgrade();
    motion_controller.connect_leave(move |_| {
        if let Some(btn) = play_button_weak.upgrade() {
            btn.set_visible(false);
        }
    });
    motion_controller
}

/// Creates the fixed container for the overlay, ensuring correct positioning
pub fn create_cover_fixed(overlay: &Overlay, cover_size: i32) -> Fixed {
    let cover_fixed = Fixed::new();
    cover_fixed.set_size_request(-1, cover_size);
    cover_fixed.put(overlay, 0.0, 0.0);
    cover_fixed
}
