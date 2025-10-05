use std::{path::Path, sync::Arc};

use gtk4::{
    Align::Center,
    Button, EventControllerMotion,
    glib::{MainContext, clone::Downgrade},
    prelude::{ButtonExt, WidgetExt},
};
use sqlx::SqlitePool;

use crate::{
    data::db::crud::{fetch_album_by_id, fetch_artist_by_id, fetch_songs_by_album},
    ui::{
        components::player_bar::PlayerBar,
        pages::artist::data::artist_data::AlbumDisplayInfoWithYear,
    },
};

/// Creates a play button that appears on hover over the album cover
pub fn create_play_button() -> Button {
    let play_button = Button::builder()
        .icon_name("media-playback-start")
        .css_classes(&["play-pause-button", "album-cover-play"][..])
        .build();
    play_button.set_size_request(56, 56);
    play_button.set_halign(Center);
    play_button.set_valign(Center);
    play_button.set_visible(false);
    play_button
}

/// Sets up the click handler for the play button to queue album
pub fn setup_play_button_handler(
    play_button: &Button,
    album: &AlbumDisplayInfoWithYear,
    db_pool: Arc<SqlitePool>,
    player_bar: PlayerBar,
) {
    // Clone all necessary data to avoid lifetime issues
    let player_bar_clone = player_bar.clone();
    let album_id = album.id;
    let album_title = album.title.clone();
    let cover_art_path = album.cover_art.clone();
    let album_format_bit_depth = album.bit_depth;
    let album_format_sample_rate = album.sample_rate;
    let album_format = album.format.clone();

    // Clone db_pool before the play_button closure to avoid moving the original
    let db_pool_for_play_button = db_pool.clone();
    play_button.connect_clicked(move |_| {
        // Clone metadata for direct update
        let album_title_local = album_title.clone();
        let cover_art_path_local = cover_art_path.clone();
        let album_format_bit_depth_local = album_format_bit_depth;
        let album_format_sample_rate_local = album_format_sample_rate;
        let album_format_local = album_format.clone();
        let db_pool_clone = db_pool_for_play_button.clone();
        let album_id_local = album_id;

        // Get the playback controller from the player bar
        let player_bar_async = player_bar_clone.clone();
        if let Some(controller) = player_bar_async.get_playback_controller() {
            // Spawn async task to fetch the first song's metadata and queue the album
            MainContext::default().spawn_local(async move {
                // Fetch the first song's metadata from the database first
                let db_pool_songs = db_pool_clone.clone();
                let first_song_info =
                    if let Ok(songs) = fetch_songs_by_album(&db_pool_songs, album_id_local).await {
                        if let Some(first_song) = songs.first() {
                            // Fetch artist information for the song
                            if let Ok(artist) =
                                fetch_artist_by_id(&db_pool_songs, first_song.artist_id).await
                            {
                                Some((first_song.clone(), artist))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                // Update the player bar with the first song's metadata directly to ensure it's updated before visibility
                // This ensures the player bar shows correct metadata even if the SongChanged event is delayed
                if let Some((first_song, artist)) = first_song_info {
                    player_bar_async.update_with_metadata(
                        &album_title_local,
                        &first_song.title,
                        &artist.name,
                        cover_art_path_local.as_deref().map(Path::new),
                        first_song.bit_depth,
                        first_song.sample_rate,
                        first_song.format.as_deref(),
                        first_song.duration,
                    );
                } else {
                    // Fallback to album metadata if we can't fetch the first song
                    // In this case, we need to fetch the album information to get the artist_id
                    if let Ok(album_info) = fetch_album_by_id(&db_pool_clone, album_id_local).await
                    {
                        if let Ok(artist) =
                            fetch_artist_by_id(&db_pool_clone, album_info.artist_id).await
                        {
                            player_bar_async.update_with_metadata(
                                &album_title_local,
                                &album_title_local,
                                &artist.name,
                                cover_art_path_local.as_deref().map(Path::new),
                                album_format_bit_depth_local,
                                album_format_sample_rate_local,
                                album_format_local.as_deref(),
                                None,
                            );
                        } else {
                            // If we can't get the artist, use a generic fallback
                            player_bar_async.update_with_metadata(
                                &album_title_local,
                                &album_title_local,
                                "Unknown Artist",
                                cover_art_path_local.as_deref().map(Path::new),
                                album_format_bit_depth_local,
                                album_format_sample_rate_local,
                                album_format_local.as_deref(),
                                None,
                            );
                        }
                    } else {
                        // If we can't get album info either, use a generic fallback
                        player_bar_async.update_with_metadata(
                            &album_title_local,
                            &album_title_local,
                            "Unknown Artist",
                            cover_art_path_local.as_deref().map(Path::new),
                            album_format_bit_depth_local,
                            album_format_sample_rate_local,
                            album_format_local.as_deref(),
                            None,
                        );
                    }
                }

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
}

/// Sets up hover event controller to show/hide play button
pub fn setup_hover_controller(play_button: &Button) -> EventControllerMotion {
    let motion_controller = EventControllerMotion::new();
    let play_button_weak = play_button.downgrade();
    motion_controller.connect_enter(move |_, _, _| {
        // Show play button when mouse enters the album cover
        if let Some(btn) = play_button_weak.upgrade() {
            btn.set_visible(true);
        }
    });

    // Clone weak reference for leave handler
    let play_button_weak = play_button.downgrade();
    motion_controller.connect_leave(move |_| {
        // Hide play button when mouse leaves the album cover
        if let Some(btn) = play_button_weak.upgrade() {
            btn.set_visible(false);
        }
    });
    motion_controller
}
