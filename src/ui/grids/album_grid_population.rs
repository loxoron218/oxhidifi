use std::{
    cell::{Cell, RefCell},
    cmp::Ordering::Equal,
    path::Path,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use glib::{ControlFlow::Continue, MainContext, timeout_add_local};
use gtk4::{
    Align::{Center, End, Start},
    Box, Button, EventControllerMotion, Fixed, FlowBox, FlowBoxChild, Label,
    Orientation::{Horizontal, Vertical},
    Overlay, Stack,
    pango::{EllipsizeMode, WrapMode::WordChar},
};
use libadwaita::prelude::{BoxExt, ButtonExt, FixedExt, ObjectExt, WidgetExt};
use sqlx::SqlitePool;

use crate::{
    data::db::{
        crud::fetch_tracks_by_album, dr_sync::synchronize_dr_completed_background,
        query::fetch_album_display_info,
    },
    ui::{
        components::{
            player_bar::PlayerBar,
            sorting::sorting_types::SortOrder::{self, Album, Artist, DrValue, Year},
        },
        grids::{
            album_grid_state::AlbumGridState::{Empty, Populated, Scanning},
            album_grid_utils::{
                create_album_cover_picture, create_dr_badge_label, create_styled_label,
            },
        },
    },
    utils::{
        best_dr_persistence::{AlbumKey, DrValueStore},
        formatting::{format_freq_khz, format_year_info},
        image_loader_async::AsyncImageLoader,
        screen::ScreenInfo,
    },
};

/// Populates the given `albums_grid` with album tiles, handling data fetching, sorting, and UI updates.
///
/// This asynchronous function fetches album display information from the database,
/// sorts the albums based on user-defined criteria, and then iteratively creates
/// and adds `FlowBoxChild` widgets (album tiles) to the `FlowBox`.
/// It incorporates batch processing to maintain UI responsiveness during large data loads.
///
/// # Arguments
/// * `albums_grid` - The `gtk4::FlowBox` to populate with album tiles.
/// * `db_pool` - An `Arc<SqlitePool>` for database access.
/// * `sort_ascending` - A boolean indicating the overall sort direction (ascending/descending).
/// * `sort_orders` - A `Rc<RefCell<Vec<SortOrder>>>` defining the multi-level sorting criteria.
/// * `screen_info` - A `Rc<RefCell<ScreenInfo>>` providing screen dimensions for UI sizing.
/// * `scanning_label` - A `gtk4::Label` used for scanning feedback.
/// * `sender` - An `UnboundedSender<()>` to send signals (e.g., for UI refresh).
/// * `stack` - The main `libadwaita::ViewStack` for navigation.
/// * `header_btn_stack` - A `libadwaita::ViewStack` for managing header buttons.
/// * `albums_inner_stack` - The `gtk4::Stack` managing the different states of the album grid.
pub async fn populate_albums_grid(
    albums_grid: &FlowBox,
    db_pool: Arc<SqlitePool>,
    sort_ascending: bool,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    screen_info: &Rc<RefCell<ScreenInfo>>,
    scanning_label: &Label,
    albums_inner_stack: &Stack,
    album_count_label: &Label,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    _view_mode: Rc<RefCell<String>>,
    player_bar: PlayerBar,
) {
    // A thread-local static to prevent multiple simultaneous population calls,
    // ensuring data consistency and preventing redundant work.
    thread_local! {
        static IS_BUSY: Cell<bool> = Cell::new(false);
    }

    // Check and set the busy flag. If already busy, return immediately.
    let already_busy = IS_BUSY.with(|cell| {
        if cell.get() {
            true
        } else {
            cell.set(true);
            false
        }
    });
    if already_busy {
        return;
    }

    // Clear existing children from the grid to prepare for new population.
    while let Some(child) = albums_grid.first_child() {
        albums_grid.remove(&child);
    }

    // Synchronize DR completed status from the persistence store in the background.
    // This ensures that any manual changes to best_dr_values.json or updates from other
    // parts of the application are reflected in the database without blocking the UI.
    let db_pool_clone = Arc::clone(&db_pool);
    MainContext::default().spawn_local(async move {
        if let Err(e) = synchronize_dr_completed_background(db_pool_clone, None).await {
            eprintln!(
                "Error synchronizing DR completed status in background: {}",
                e
            );
        }
    });

    // For immediate population, we'll use the existing DR status in the database
    let dr_store = DrValueStore::load(); // Load the DR store once for efficiency
    match fetch_album_display_info(&db_pool).await {
        Err(e) => {
            // Log the error for debugging purposes.
            eprintln!("Error fetching album display info: {:?}", e);

            // On error, revert busy state and show an empty state.
            IS_BUSY.with(|cell| cell.set(false));
            albums_inner_stack.set_visible_child_name(Empty.as_str());
            album_count_label.set_text("0 Albums"); // Update count on error
        }
        Ok(mut albums) => {
            // Determine the appropriate state to show if no albums are found.
            if albums.is_empty() {
                let state_to_show = if scanning_label.is_visible() {
                    Scanning.as_str()
                } else {
                    Empty.as_str()
                };
                albums_inner_stack.set_visible_child_name(state_to_show);

                // Update count if no albums
                album_count_label.set_text("0 Albums");
                IS_BUSY.with(|cell| cell.set(false));
                return;
            }

            // Albums fetched: {}
            album_count_label.set_text(&format!("{} Albums", albums.len())); // Update count with actual number

            // If albums are found, transition to the populated grid state.
            albums_inner_stack.set_visible_child_name(Populated.as_str());

            // Multi-level sort albums according to user-defined sort orders.
            let current_sort_orders = sort_orders.borrow();
            albums.sort_by(|a, b| {
                for order in &*current_sort_orders {
                    let cmp = match order {
                        Artist => a.artist.to_lowercase().cmp(&b.artist.to_lowercase()),
                        Album => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                        Year => {
                            // Extract year from original_release_date or year field.
                            let a_year = a
                                .original_release_date
                                .as_ref()
                                .and_then(|s| s.split('-').next())
                                .and_then(|y| y.parse::<i32>().ok())
                                .or(a.year); // Fallback to `year` field if original_release_date parsing fails
                            let b_year = b
                                .original_release_date
                                .as_ref()
                                .and_then(|s| s.split('-').next())
                                .and_then(|y| y.parse::<i32>().ok())
                                .or(b.year); // Fallback to `year` field
                            a_year.cmp(&b_year)
                        }
                        DrValue => a.dr_value.cmp(&b.dr_value),
                    };
                    // If comparison is not equal, return the result, applying ascending/descending.
                    if cmp != Equal {
                        return if sort_ascending { cmp } else { cmp.reverse() };
                    }
                }
                // If all sort criteria are equal, maintain original order (or arbitrary).
                Equal
            });

            // BATCH_SIZE: The number of album tiles to process before yielding control
            // back to the GTK main thread. This helps prevent UI freezes during large
            // grid population operations. A larger batch size means fewer yields but
            // potentially longer individual UI blocking.
            const BATCH_SIZE: usize = 50;
            let mut processed_count = 0;
            let cover_size = screen_info.borrow().cover_size;
            let tile_size = screen_info.borrow().tile_size;
            let use_original_year_clone_for_loop = use_original_year.clone();

            // Create async image loader
            let async_image_loader = match AsyncImageLoader::new() {
                Ok(loader) => loader,
                Err(e) => {
                    eprintln!("Failed to create async image loader: {:?}", e);
                    return;
                }
            };
            for album_info in &albums {
                // Create album title label.
                let title_label = create_styled_label(
                    &album_info.title,
                    &["album-title-label"],
                    Some(((cover_size - 16) / 10).max(8)), // Max width chars for title
                    Some(EllipsizeMode::End),
                    true, // Wrap text
                    Some(WordChar),
                    Some(2), // Max 2 lines
                );
                title_label.set_size_request(cover_size - 16, -1); // Set explicit width

                // Create artist label.
                let artist_label = create_styled_label(
                    &album_info.artist,
                    &["album-artist-label"],
                    Some(18), // Max width chars for artist
                    Some(EllipsizeMode::End),
                    false, // No wrapping
                    None,
                    None,
                );

                // Format audio details (format, bit depth, frequency).
                let format_line = if let Some(format_str) = album_info.format.as_ref() {
                    let format_caps = format_str.to_uppercase();
                    match (album_info.bit_depth, album_info.frequency) {
                        (Some(bit), Some(freq)) => {
                            format!("{} {}/{}", format_caps, bit, format_freq_khz(freq as u32))
                        }
                        (None, Some(freq)) => {
                            format!("{} {}", format_caps, format_freq_khz(freq as u32))
                        }
                        _ => format_caps,
                    }
                } else {
                    String::new()
                };
                let format_label = create_styled_label(
                    &format_line,
                    &["album-format-label"],
                    None,
                    None,
                    false,
                    None,
                    None,
                );
                format_label.set_halign(Start);
                format_label.set_hexpand(true); // Allow format label to expand

                // Extract and format year based on setting.
                let year_text = format_year_info(
                    album_info.year,
                    album_info.original_release_date.as_deref(),
                    use_original_year_clone_for_loop.get(),
                );
                let year_label = create_styled_label(
                    &year_text,
                    &["album-format-label"],
                    None,
                    None,
                    false,
                    None,
                    None,
                );
                year_label.set_halign(End);
                year_label.set_hexpand(false); // Do not allow year label to expand

                // --- Album Tile Construction ---
                let album_tile_box = Box::builder()
                    .orientation(Vertical)
                    .spacing(2)
                    .hexpand(false)
                    .vexpand(false)
                    .halign(Start)
                    .valign(Start)
                    // Apply tile specific CSS
                    .css_classes(&["album-tile"] as &[&str])
                    .build();

                // Fixed size for the whole tile
                album_tile_box.set_size_request(tile_size, tile_size + 80);

                // Cover container and overlay for DR badge and play button.
                let cover_container = Box::new(Vertical, 0);
                cover_container.set_size_request(cover_size, cover_size);
                cover_container.set_halign(Start);
                cover_container.set_valign(Start);
                let cover_picture = create_album_cover_picture(
                    album_info.cover_art.as_deref().map(Path::new),
                    cover_size,
                );
                cover_container.append(&cover_picture);

                // Load the image asynchronously
                async_image_loader.load_image_async(
                    cover_picture.clone(),
                    album_info.cover_art.as_deref().map(Path::new),
                    cover_size,
                );

                let overlay = Overlay::new();
                overlay.set_size_request(cover_size, cover_size);
                overlay.set_child(Some(&cover_container));
                overlay.set_halign(Start);
                overlay.set_valign(Start);

                // Add DR badge to overlay, if enabled in settings.
                if show_dr_badges.get() {
                    let album_key = AlbumKey {
                        title: album_info.title.clone(),
                        artist: album_info.artist.clone(),
                        folder_path: album_info.folder_path.clone(),
                    };
                    let is_dr_completed_from_store = dr_store.contains(&album_key);
                    let dr_label = create_dr_badge_label(
                        album_info.dr_value.map(|dr| dr as u8),
                        is_dr_completed_from_store,
                    );
                    overlay.add_overlay(&dr_label);
                }

                // Add play button to overlay with hover effect.
                let play_button = Button::builder()
                    .icon_name("media-playback-start")
                    .css_classes(&["play-pause-button", "album-cover-play"] as &[&str])
                    .halign(Center)
                    .valign(Center)
                    .visible(false) // Initially hidden
                    .build();
                play_button.set_size_request(56, 56);
                overlay.add_overlay(&play_button);

                // Make player bar visible on click
                let player_bar_clone = player_bar.clone();
                let db_pool_clone = db_pool.clone();
                let album_info_clone = album_info.clone();
                play_button.connect_clicked(move |_| {
                    // Fetch the first track of the album to get track title
                    let db_pool_clone = db_pool_clone.clone();
                    let player_bar_clone = player_bar_clone.clone();
                    let album_info_clone = album_info_clone.clone();
                    MainContext::default().spawn_local(async move {
                        match fetch_tracks_by_album(&*db_pool_clone, album_info_clone.id).await {
                            Ok(tracks) => {
                                if let Some(first_track) = tracks.first() {
                                    // Clone track values to move into closure
                                    let track_bit_depth = first_track.bit_depth;
                                    let track_frequency = first_track.frequency;
                                    let track_format = first_track.format.clone();
                                    let track_duration = first_track.duration;
                                    let track_title = first_track.title.clone();
                                    player_bar_clone.update_with_metadata(
                                        &album_info_clone.title,
                                        &track_title,
                                        &album_info_clone.artist,
                                        album_info_clone.cover_art.as_deref().map(Path::new),
                                        track_bit_depth,
                                        track_frequency,
                                        track_format.as_deref(),
                                        track_duration,
                                    );
                                } else {
                                    // Fallback to album title if no tracks found
                                    player_bar_clone.update_with_metadata(
                                        &album_info_clone.title,
                                        &album_info_clone.title,
                                        &album_info_clone.artist,
                                        album_info_clone.cover_art.as_deref().map(Path::new),
                                        None,
                                        None,
                                        None,
                                        None,
                                    );
                                }
                            }
                            Err(_) => {
                                // Fallback to album title if track fetch fails
                                player_bar_clone.update_with_metadata(
                                    &album_info_clone.title,
                                    &album_info_clone.title,
                                    &album_info_clone.artist,
                                    album_info_clone.cover_art.as_deref().map(Path::new),
                                    None,
                                    None,
                                    None,
                                    None,
                                );
                            }
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
                let play_button_weak = play_button.downgrade(); // Re-clone for the leave handler
                motion_controller.connect_leave(move |_| {
                    if let Some(btn) = play_button_weak.upgrade() {
                        btn.set_visible(false);
                    }
                });
                overlay.add_controller(motion_controller);

                // Add cover area to the tile box using Fixed container for precise positioning.
                let cover_fixed = Fixed::new();
                cover_fixed.set_size_request(-1, cover_size); // Height is fixed to cover_size
                cover_fixed.put(&overlay, 0.0, 0.0);
                album_tile_box.append(&cover_fixed);

                // Add title, artist, and metadata to the tile box.
                let title_area_box = Box::builder()
                    .orientation(Vertical)
                    .height_request(40) // Explicitly request height for two lines of text + extra buffer
                    .margin_top(12) // Margin from the cover
                    .build();
                title_label.set_valign(End); // Align title to the bottom of its allocated space
                title_area_box.append(&title_label);
                album_tile_box.append(&title_area_box);
                album_tile_box.append(&artist_label);
                let metadata_box = Box::builder()
                    .orientation(Horizontal)
                    .spacing(0) // No spacing between the two labels
                    .hexpand(true)
                    .build();
                metadata_box.append(&format_label);
                metadata_box.append(&year_label);
                album_tile_box.append(&metadata_box);

                // --- FlowBoxChild and Navigation ---
                let flow_child = FlowBoxChild::builder()
                    .child(&album_tile_box)
                    .hexpand(false)
                    .vexpand(false)
                    .halign(Start)
                    .valign(Start)
                    .build();
                flow_child.set_widget_name(&album_info.id.to_string());

                // Insert the new album tile into the FlowBox.
                albums_grid.insert(&flow_child, -1); // -1 appends to the end

                processed_count += 1;
                // Yield control to the GTK main thread periodically to keep the UI responsive.
                if processed_count % BATCH_SIZE == 0 {
                    timeout_add_local(Duration::from_millis(1), || Continue);
                }
            }
            // Reset busy flag after all albums have been processed.
            IS_BUSY.with(|cell| cell.set(false));
        }
    }
}
