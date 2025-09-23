use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    Align::{Center, End, Start},
    Box, Button, EventControllerMotion, Fixed, FlowBoxChild,
    Orientation::{Horizontal, Vertical},
    Overlay,
    glib::MainContext,
    pango::{EllipsizeMode, WrapMode::WordChar},
};
use libadwaita::prelude::{BoxExt, ButtonExt, FixedExt, ObjectExt, WidgetExt};
use sqlx::SqlitePool;

use crate::{
    ui::{
        components::{player_bar::PlayerBar, view_controls::ZoomLevel},
        grids::{
            album_grid_state::AlbumGridItem,
            album_grid_utils::{
                create_album_cover_picture, create_dr_badge_label, create_styled_label,
            },
        },
    },
    utils::{
        best_dr_persistence::{AlbumKey, DrValueStore},
        formatting::{format_sample_rate_khz, format_sample_rate_value, format_year_info},
        image::AsyncImageLoader,
        screen::ScreenInfo,
    },
};

/// Creates an album tile for display in the grid.
///
/// This function constructs a visual tile for a given album, including its cover,
/// title, artist, format, year, and DR badge.
///
/// # Arguments
/// * `album_info` - The album data to display in the tile
/// * `screen_info` - A `Rc<RefCell<ScreenInfo>>` providing screen dimensions for UI sizing
/// * `show_dr_badges` - A `Rc<Cell<bool>>` indicating whether to show DR badges
/// * `use_original_year` - A `Rc<Cell<bool>>` indicating whether to use original release year
/// * `player_bar` - A `PlayerBar` instance for playback functionality
/// * `zoom_level` - The current zoom level to determine display density
///
/// # Returns
/// A `FlowBoxChild` widget with the complete album tile UI
pub fn create_album_tile(
    album_info: &AlbumGridItem,
    screen_info: &Rc<RefCell<ScreenInfo>>,
    show_dr_badges: &Rc<Cell<bool>>,
    use_original_year: &Rc<Cell<bool>>,
    player_bar: &PlayerBar,
    _db_pool: Arc<SqlitePool>,
    zoom_level: ZoomLevel,
) -> FlowBoxChild {
    let cover_size = screen_info.borrow().cover_size;
    let tile_size = screen_info.borrow().tile_size;

    // Create album title label.
    let title_label = create_styled_label(
        &album_info.title,
        &["album-title-label"],
        // Max width chars for title
        Some(((cover_size - 16) / 10).max(8)),
        Some(EllipsizeMode::End),
        // Wrap text
        true,
        Some(WordChar),
        // Max 2 lines
        Some(2),
    );

    // Set explicit width
    title_label.set_size_request(cover_size - 16, -1);

    // Create artist label.
    let artist_label = create_styled_label(
        &album_info.artist,
        &["album-artist-label"],
        // Max width chars for artist
        Some(((cover_size - 16) / 10).max(8)),
        Some(EllipsizeMode::End),
        // No wrapping
        false,
        None,
        None,
    );

    // Set explicit width
    artist_label.set_size_request(cover_size - 16, -1);

    // Format audio details (format, bit depth, sample rate).
    let format_line = album_info
        .format
        .as_ref()
        .map(|format_str| {
            // This closure only runs if `album_info.format` is Some.
            let format_caps = format_str.to_uppercase();

            // For ExtraSmall zoom level, only show the format without bit depth/sample rate
            if zoom_level == ZoomLevel::ExtraSmall {
                format_caps
            } else {
                // First, determine only the part of the string that changes.
                let tech_details = match (album_info.bit_depth, album_info.sample_rate) {
                    (Some(bit), Some(freq)) => {
                        // For Small zoom level, don't show "kHz" suffix
                        match zoom_level {
                            ZoomLevel::Small => {
                                format!(" {}/{}", bit, format_sample_rate_value(freq as u32))
                            }
                            _ => {
                                format!(" {}/{}", bit, format_sample_rate_khz(freq as u32))
                            }
                        }
                    }
                    (None, Some(freq)) => {
                        // For Small zoom level, don't show "kHz" suffix
                        match zoom_level {
                            ZoomLevel::Small => {
                                format!(" {}", format_sample_rate_value(freq as u32))
                            }
                            _ => {
                                format!(" {}", format_sample_rate_khz(freq as u32))
                            }
                        }
                    }
                    _ => String::new(),
                };

                // Combine the static and dynamic parts in one place.
                format!("{}{}", format_caps, tech_details)
            }
        })
        .unwrap_or_default();
    let format_label = create_styled_label(
        &format_line,
        &["album-format-label"],
        Some(((cover_size - 16) / 10).max(8)),
        // Only ellipsize at ExtraSmall zoom level, not at Small or larger
        match zoom_level {
            ZoomLevel::ExtraSmall => Some(EllipsizeMode::End),
            _ => None,
        },
        false,
        None,
        None,
    );
    format_label.set_halign(Start);

    // Allow format label to expand
    format_label.set_hexpand(true);

    // Extract and format year based on setting.
    let year_text = format_year_info(
        album_info.year,
        album_info.original_release_date.as_deref(),
        use_original_year.get(),
    );

    // Create year label but only show it for Medium and larger zoom levels
    // At ExtraSmall and Small zoom levels, hiding the year helps maintain visual consistency
    let year_label = create_styled_label(
        &year_text,
        &["album-format-label"],
        Some(8),
        Some(EllipsizeMode::End),
        false,
        None,
        None,
    );
    year_label.set_halign(End);
    year_label.set_visible(match zoom_level {
        ZoomLevel::ExtraSmall | ZoomLevel::Small => false,
        _ => true,
    });

    // Do not allow year label to expand
    year_label.set_hexpand(false);

    // --- Album Tile Construction ---
    let album_tile_box = Box::builder()
        .orientation(Vertical)
        .spacing(2)
        .hexpand(false)
        .vexpand(false)
        .halign(Start)
        .valign(Start)
        .css_classes(&["album-tile"] as &[&str])
        .build();

    // Fixed size for the whole tile
    album_tile_box.set_size_request(cover_size, tile_size + 80);

    // Cover container and overlay for DR badge and play button.
    let cover_container = Box::new(Vertical, 0);
    cover_container.set_size_request(cover_size, cover_size);
    cover_container.set_halign(Start);
    cover_container.set_valign(Start);
    let cover_picture =
        create_album_cover_picture(album_info.cover_art.as_deref().map(Path::new), cover_size);
    cover_container.append(&cover_picture);

    // Load the image asynchronously
    if let Ok(async_image_loader) = AsyncImageLoader::new() {
        async_image_loader.load_image_async(
            cover_picture.clone(),
            album_info.cover_art.as_deref().map(Path::new),
            cover_size,
        );
    }

    let overlay = Overlay::new();
    overlay.set_size_request(cover_size, cover_size);
    overlay.set_child(Some(&cover_container));
    overlay.set_halign(Start);
    overlay.set_valign(Start);

    // Add DR badge to overlay, if enabled in settings.
    if show_dr_badges.get() {
        let dr_store = DrValueStore::load();
        let album_key = AlbumKey {
            title: album_info.title.clone(),
            artist: album_info.artist.clone(),
            folder_path: album_info.folder_path.clone(),
        };
        let is_dr_best_from_store = dr_store.contains(&album_key);
        let dr_label = create_dr_badge_label(
            album_info.dr_value.map(|dr| dr as u8),
            is_dr_best_from_store,
        );
        overlay.add_overlay(&dr_label);
    }

    // Add play button to overlay with hover effect.
    let play_button = Button::builder()
        .icon_name("media-playback-start")
        .css_classes(&["play-pause-button", "album-cover-play"] as &[&str])
        .halign(Center)
        .valign(Center)
        .visible(false)
        .build();
    play_button.set_size_request(56, 56);
    overlay.add_overlay(&play_button);

    // Add click handler to play button to update player bar
    let player_bar_clone = player_bar.clone();
    let album_id = album_info.id;
    play_button.connect_clicked(move |_| {
        // Clone values for the async block
        let player_bar_clone = player_bar_clone.clone();
        let album_id = album_id; // Clone album_id for the async block

        // Spawn async task to initialize the queue and start playback
        MainContext::default().spawn_local(async move {
            // Get the playback controller from the player bar
            if let Some(controller) = player_bar_clone.get_playback_controller() {
                // Lock the controller and queue the album
                let mut controller = controller.lock().await;
                if let Err(e) = controller.queue_album(album_id).await {
                    eprintln!("Error queuing album {}: {}", album_id, e);
                    return;
                }

                // Update navigation button states after queue initialization
                player_bar_clone.update_navigation_button_states();
            } else {
                eprintln!("No playback controller available");
            }
        });
    });

    // Connect hover events for the play button visibility.
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
    overlay.add_controller(motion_controller);

    // Add cover area to the tile box using Fixed container for precise positioning.
    let cover_fixed = Fixed::new();

    // Height is fixed to cover_size
    cover_fixed.set_size_request(-1, cover_size);
    cover_fixed.put(&overlay, 0.0, 0.0);
    album_tile_box.append(&cover_fixed);

    // Add title, artist, and metadata to the tile box.
    let title_area_box = Box::builder()
        .orientation(Vertical)
        .height_request(40)
        .margin_top(12)
        .build();

    // Align title to the bottom of its allocated space
    title_label.set_valign(End);
    title_area_box.append(&title_label);
    album_tile_box.append(&title_area_box);
    album_tile_box.append(&artist_label);

    // Container to constrain metadata box width
    let metadata_container = Box::builder().orientation(Vertical).hexpand(false).build();
    metadata_container.set_size_request(cover_size - 16, -1);

    let metadata_box = Box::builder()
        .orientation(Horizontal)
        .spacing(0)
        .hexpand(false)
        .build();
    metadata_box.append(&format_label);
    metadata_box.append(&year_label);
    metadata_container.append(&metadata_box);
    album_tile_box.append(&metadata_container);

    // --- FlowBoxChild and Navigation ---
    let flow_child = FlowBoxChild::builder()
        .child(&album_tile_box)
        .hexpand(false)
        .vexpand(false)
        .halign(Start)
        .valign(Start)
        .width_request(cover_size)
        .build();
    flow_child.set_widget_name(&album_info.id.to_string());
    flow_child
}
