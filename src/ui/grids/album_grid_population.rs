use std::{
    cell::{Cell, RefCell},
    cmp::Ordering::Equal,
    rc::Rc,
    sync::Arc,
    time,
};

use glib::{ControlFlow::Continue, timeout_add_local};
use gtk4::{
    Align, Box, Button, EventControllerMotion, Fixed, FlowBox, FlowBoxChild, Label, Orientation,
    Overlay, Stack,
    pango::{EllipsizeMode::End, WrapMode::WordChar},
};
use libadwaita::prelude::{BoxExt, FixedExt, ObjectExt, WidgetExt};
use sqlx::SqlitePool;

use crate::{
    data::db::query::fetch_album_display_info,
    ui::{
        components::sorting::sorting_types::SortOrder,
        grids::album_grid_state::AlbumGridState,
        grids::album_grid_utils::{
            create_album_cover_picture, create_dr_badge_label, create_styled_label,
        },
    },
    utils::{
        best_dr_persistence::{AlbumKey, DrValueStore},
        formatting::format_freq_khz,
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
    let fetch_result = fetch_album_display_info(&db_pool).await;
    let dr_store = DrValueStore::load(); // Load the DR store once for efficiency
    match fetch_result {
        Err(e) => {
            // Log the error for debugging purposes.
            eprintln!("Error fetching album display info: {:?}", e);
            // On error, revert busy state and show an empty state.
            IS_BUSY.with(|cell| cell.set(false));
            albums_inner_stack.set_visible_child_name(AlbumGridState::Empty.as_str());
            album_count_label.set_text("0 Albums"); // Update count on error
        }
        Ok(mut albums) => {
            // Determine the appropriate state to show if no albums are found.
            if albums.is_empty() {
                let state_to_show = if scanning_label.is_visible() {
                    AlbumGridState::Scanning
                } else {
                    AlbumGridState::Empty
                };
                albums_inner_stack.set_visible_child_name(state_to_show.as_str());
                album_count_label.set_text("0 Albums"); // Update count if no albums
                IS_BUSY.with(|cell| cell.set(false));
                return;
            }

            // Albums fetched: {}
            album_count_label.set_text(&format!("{} Albums", albums.len())); // Update count with actual number

            // If albums are found, transition to the populated grid state.
            albums_inner_stack.set_visible_child_name(AlbumGridState::Populated.as_str());

            // Multi-level sort albums according to user-defined sort orders.
            let current_sort_orders = sort_orders.borrow();
            albums.sort_by(|a, b| {
                for order in &*current_sort_orders {
                    let cmp = match order {
                        SortOrder::Artist => a.artist.to_lowercase().cmp(&b.artist.to_lowercase()),
                        SortOrder::Album => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                        SortOrder::Year => {
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
                        SortOrder::DrValue => a._dr_value.cmp(&b._dr_value),
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
            for album_info in albums {
                // Create album title label.
                let title_label = create_styled_label(
                    &album_info.title,
                    &["album-title-label"],
                    Some(((cover_size - 16) / 10).max(8)), // Max width chars for title
                    Some(End),
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
                    Some(End),
                    false, // No wrapping
                    None,
                    None,
                );

                // Format audio details (format, bit depth, frequency).
                let mut format_fields = Vec::new();
                if let Some(format_str) = album_info.format.as_ref() {
                    let format_caps = format_str.to_uppercase();
                    match (album_info.bit_depth, album_info.frequency) {
                        (Some(bit), Some(freq)) => {
                            format_fields.push(format!(
                                "{} {}/{}",
                                format_caps,
                                bit,
                                format_freq_khz(freq)
                            ));
                        }
                        (None, Some(freq)) => {
                            format_fields.push(format!("{} {}", format_caps, format_freq_khz(freq)))
                        }
                        _ => format_fields.push(format_caps),
                    }
                }
                let format_line = format_fields.join(" · ");
                let format_label = create_styled_label(
                    &format_line,
                    &["album-format-label"],
                    None,
                    None,
                    false,
                    None,
                    None,
                );
                format_label.set_halign(Align::Start);
                format_label.set_hexpand(true); // Allow format label to expand

                // Extract and format year based on setting.
                let year_text = if use_original_year_clone_for_loop.get() {
                    if let Some(original_release_date_str) = album_info.original_release_date {
                        original_release_date_str
                            .split('-')
                            .next()
                            .unwrap_or("N/A")
                            .to_string()
                    } else if let Some(year) = album_info.year {
                        format!("{}", year)
                    } else {
                        String::new()
                    }
                } else {
                    if let Some(year) = album_info.year {
                        format!("{}", year)
                    } else if let Some(original_release_date_str) = album_info.original_release_date
                    {
                        original_release_date_str
                            .split('-')
                            .next()
                            .unwrap_or("N/A")
                            .to_string()
                    } else {
                        String::new()
                    }
                };

                let year_label = create_styled_label(
                    &year_text,
                    &["album-format-label"],
                    None,
                    None,
                    false,
                    None,
                    None,
                );
                year_label.set_halign(Align::End);
                year_label.set_hexpand(false); // Do not allow year label to expand

                // --- Album Tile Construction ---
                let album_tile_box = Box::builder()
                    .orientation(Orientation::Vertical)
                    .spacing(2)
                    .hexpand(false)
                    .vexpand(false)
                    .halign(Align::Start)
                    .valign(Align::Start)
                    .css_classes(&["album-tile"] as &[&str]) // Apply tile specific CSS
                    .build();
                album_tile_box.set_size_request(tile_size, tile_size + 80); // Fixed size for the whole tile

                // Cover container and overlay for DR badge and play button.
                let cover_container = Box::new(Orientation::Vertical, 0);
                cover_container.set_size_request(cover_size, cover_size);
                cover_container.set_halign(Align::Start);
                cover_container.set_valign(Align::Start);
                let cover_picture =
                    create_album_cover_picture(album_info.cover_art.as_ref(), cover_size);
                cover_container.append(&cover_picture);
                let overlay = Overlay::new();
                overlay.set_size_request(cover_size, cover_size);
                overlay.set_child(Some(&cover_container));
                overlay.set_halign(Align::Start);
                overlay.set_valign(Align::Start);

                // Add DR badge to overlay, if enabled in settings.
                if show_dr_badges.get() {
                    let album_key = AlbumKey {
                        title: album_info.title.clone(),
                        artist: album_info.artist.clone(),
                        folder_path: album_info.folder_path.clone(),
                    };
                    let is_dr_completed_from_store = dr_store.contains(&album_key);
                    let dr_label =
                        create_dr_badge_label(album_info._dr_value, is_dr_completed_from_store);
                    overlay.add_overlay(&dr_label);
                }

                // Add play button to overlay with hover effect.
                let play_button = Button::builder()
                    .icon_name("media-playback-start")
                    .css_classes(&["play-pause-button", "album-cover-play"] as &[&str])
                    .halign(Align::Center)
                    .valign(Align::Center)
                    .visible(false) // Initially hidden
                    .build();
                play_button.set_size_request(56, 56);
                overlay.add_overlay(&play_button);

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
                    .orientation(Orientation::Vertical)
                    .height_request(40) // Explicitly request height for two lines of text + extra buffer
                    .margin_top(12) // Margin from the cover
                    .build();
                title_label.set_valign(Align::End); // Align title to the bottom of its allocated space
                title_area_box.append(&title_label);
                album_tile_box.append(&title_area_box);
                album_tile_box.append(&artist_label);
                let metadata_box = Box::builder()
                    .orientation(Orientation::Horizontal)
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
                    .halign(Align::Start)
                    .valign(Align::Start)
                    .build();
                flow_child.set_widget_name(&album_info.id.to_string());

                // Insert the new album tile into the FlowBox.
                albums_grid.insert(&flow_child, -1); // -1 appends to the end

                processed_count += 1;
                // Yield control to the GTK main thread periodically to keep the UI responsive.
                if processed_count % BATCH_SIZE == 0 {
                    timeout_add_local(time::Duration::from_millis(1), || Continue);
                }
            }
            // Reset busy flag after all albums have been processed.
            IS_BUSY.with(|cell| cell.set(false));
        }
    }
}
