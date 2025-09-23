use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    Align::{Center, End, Start},
    Box, Button, EventControllerMotion, Fixed, FlowBoxChild, GestureClick,
    Orientation::{Horizontal, Vertical},
    Overlay,
    glib::{MainContext, prelude::ObjectExt},
    pango::{EllipsizeMode, WrapMode::WordChar},
};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{BoxExt, ButtonExt, FixedExt, FlowBoxChildExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    ui::{
        components::{
            player_bar::PlayerBar,
            tiles::helpers::{create_album_cover, create_album_label, create_dr_overlay},
            tiles::text_utils::highlight,
            view_controls::ZoomLevel::{self, ExtraSmall, Small},
        },
        grids::album_grid_state::AlbumGridItem,
        pages::album::album_page::album_page,
    },
    utils::formatting::{format_sample_rate_khz, format_sample_rate_value, format_year_info},
};

/// Creates a `FlowBoxChild` containing the UI representation of an album.
///
/// This function constructs a visual tile for a given album, including its cover,
/// title, artist, format, year, and DR badge. It also attaches a click gesture
/// to navigate to the album's dedicated page.
///
/// # Arguments
///
/// * `album` - The album data to display in the tile
/// * `cover_size` - Size in pixels for the album cover art
/// * `tile_size` - Overall size of the tile
/// * `search_text` - Text to highlight in the album title and artist name
/// * `stack_for_closure` - Reference to the main view stack for navigation
/// * `db_pool` - Database connection pool for data access
/// * `left_btn_stack_for_closure` - Reference to the header button stack
/// * `right_btn_box_for_closure` - Reference to the right header button container
/// * `nav_history` - Navigation history stack for back navigation
/// * `sender` - Channel sender for UI update notifications
/// * `show_dr_badges` - Flag controlling display of dynamic range badges
/// * `use_original_year` - Flag controlling whether to show original release year
/// * `player_bar` - Reference to the application's player bar component
///
/// # Returns
///
/// A `FlowBoxChild` widget containing the complete album tile UI
pub fn create_album_tile(
    album: &AlbumGridItem,
    cover_size: i32,
    tile_size: i32,
    search_text: &str,
    stack_for_closure: Rc<ViewStack>,
    db_pool: Arc<SqlitePool>,
    left_btn_stack_for_closure: Rc<ViewStack>,
    right_btn_box_for_closure: Rc<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    zoom_level: ZoomLevel,
) -> FlowBoxChild {
    // Create and style the album title label with search highlighting
    let title_label = {
        let label = create_album_label(
            &highlight(&album.title, search_text),
            &["album-title-label"],
            Some(((cover_size - 16) / 10).max(8)),
            Some(EllipsizeMode::End),
            true,
            Some(WordChar),
            Some(2),
            // use_markup: true because highlight is used
            true,
        );
        label.set_size_request(cover_size - 16, -1);

        // Align to the bottom of its allocated space
        label.set_valign(End);
        label
    };

    // Create and style the artist label with search highlighting
    let artist_label = {
        let label = create_album_label(
            &highlight(&album.artist, search_text),
            &["album-artist-label"],
            Some(((cover_size - 16) / 10).max(8)),
            Some(EllipsizeMode::End),
            false,
            None,
            None,
            // use_markup: true because highlight is used
            true,
        );
        label.set_size_request(cover_size - 16, -1);
        label
    };

    // Format the audio quality line (e.g., "FLAC 24/96")
    let format_line = album
        .format
        .as_ref()
        .map(|format_str| {
            // Convert format to uppercase for consistent display
            let format_caps = format_str.to_uppercase();

            // For ExtraSmall zoom level, only show the format without bit depth/sample rate
            if zoom_level == ExtraSmall {
                format_caps
            } else {
                // First, determine only the part of the string that changes.
                let tech_details = match (album.bit_depth, album.sample_rate) {
                    (Some(bit), Some(freq)) => {
                        // For Small zoom level, don't show "kHz" suffix
                        match zoom_level {
                            Small => {
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
                            Small => {
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
        // If `album.format` was None, this provides an empty String.
        .unwrap_or_default();

    // Create and style the format label for displaying audio format information
    let format_label = create_album_label(
        &format_line,
        &["album-format-label"],
        Some(((cover_size - 16) / 10).max(8)),
        // Only ellipsize at ExtraSmall zoom level, not at Small or larger
        match zoom_level {
            ExtraSmall => Some(EllipsizeMode::End),
            _ => None,
        },
        false,
        None,
        None,
        // use_markup: false for plain text
        false,
    );
    format_label.set_halign(Start);
    format_label.set_hexpand(true);

    // Extract and format the release year based on user preference for original vs. release year
    let year_text = format_year_info(
        album.year,
        album.original_release_date.as_deref(),
        use_original_year.get(),
    );

    // Create and style the year label for displaying release year information
    let year_label = create_album_label(
        &year_text,
        &["album-format-label"],
        Some(8),
        Some(EllipsizeMode::End),
        false,
        None,
        None,
        // use_markup: false for plain text
        false,
    );
    year_label.set_halign(End);
    year_label.set_hexpand(false);
    year_label.set_visible(match zoom_level {
        ExtraSmall | Small => false,
        _ => true,
    });

    // Create album cover picture from cached image file
    let cover = create_album_cover(album.cover_art.as_deref().map(Path::new), cover_size);

    // Main vertical box for the album tile layout
    let album_tile_box = Box::builder().orientation(Vertical).spacing(2).build();
    album_tile_box.set_size_request(cover_size, tile_size + 80);
    album_tile_box.set_hexpand(false);
    album_tile_box.set_vexpand(false);
    album_tile_box.set_halign(Start);
    album_tile_box.set_valign(Start);

    // Container for the cover, to ensure fixed size
    let cover_container = Box::new(Vertical, 0);
    cover_container.set_size_request(cover_size, cover_size);
    cover_container.set_halign(Start);
    cover_container.set_valign(Start);
    cover_container.append(&cover);

    // Overlay for DR badge on the cover
    let overlay = Overlay::new();
    overlay.set_size_request(cover_size, cover_size);
    overlay.set_child(Some(&cover_container));
    overlay.set_halign(Start);
    overlay.set_valign(Start);
    if show_dr_badges.get()
        && let Some(dr_label) =
            create_dr_overlay(album.dr_value.map(|dr| dr as u8), album.dr_is_best)
    {
        overlay.add_overlay(&dr_label);
    }

    // Play button overlay that appears on hover
    let play_button = Button::builder()
        .icon_name("media-playback-start")
        .css_classes(&["play-pause-button", "album-cover-play"][..])
        .build();
    play_button.set_size_request(56, 56);
    play_button.set_halign(Center);
    play_button.set_valign(Center);
    play_button.set_visible(false);
    overlay.add_overlay(&play_button);

    // Add click handler to play button to update player bar
    let player_bar_clone = player_bar.clone();
    let album_id = album.id;
    play_button.connect_clicked(move |_| {
        // Get the playback controller from the player bar
        if let Some(controller) = player_bar_clone.get_playback_controller() {
            // Spawn async task to queue the album
            let player_bar_async = player_bar_clone.clone();
            MainContext::default().spawn_local(async move {
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

    // Event controller for hover effects on the album cover
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

    // Fixed container for the overlay, ensuring correct positioning
    let cover_fixed = Fixed::new();
    cover_fixed.set_size_request(-1, cover_size);
    cover_fixed.put(&overlay, 0.0, 0.0);
    album_tile_box.append(&cover_fixed);

    // Box for title, with explicit height for two lines of text
    let title_area_box = Box::builder()
        .orientation(Vertical)
        .height_request(40)
        .margin_top(12)
        .build();
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
    album_tile_box.set_css_classes(&["album-tile"]);

    // Create the FlowBoxChild and set its properties
    let flow_child = FlowBoxChild::new();
    flow_child.set_child(Some(&album_tile_box));
    flow_child.set_hexpand(false);
    flow_child.set_vexpand(false);
    flow_child.set_halign(Start);
    flow_child.set_valign(Start);
    flow_child.set_size_request(cover_size, -1);

    // Add click gesture for navigation to album page
    let stack_weak = stack_for_closure.downgrade();
    let gesture = GestureClick::builder().build();

    // The `move` keyword captures the needed variables safely.
    let album_id = album.id;
    let play_button_weak = play_button.downgrade();
    gesture.connect_pressed(move |_gesture, _, x, y| {
        // Check if the click was on the play button by checking if the play button is visible
        // and if the coordinates fall within the play button area
        let is_play_button_click = if let Some(play_btn) = play_button_weak.upgrade() {
            if play_btn.is_visible() {
                // Get the play button allocation to check coordinates
                let allocation = play_btn.allocation();
                let play_btn_x = allocation.x() as f64;
                let play_btn_y = allocation.y() as f64;
                let play_btn_width = allocation.width() as f64;
                let play_btn_height = allocation.height() as f64;

                // Check if click coordinates are within play button bounds
                x >= play_btn_x
                    && x <= play_btn_x + play_btn_width
                    && y >= play_btn_y
                    && y <= play_btn_y + play_btn_height
            } else {
                false
            }
        } else {
            false
        };

        // Only navigate to album page if click was not on the play button
        if !is_play_button_click {
            // The album ID is now owned by the closure.
            if let (Some(stack), Some(header_btn_stack)) = (
                stack_weak.upgrade(),
                left_btn_stack_for_closure.downgrade().upgrade(),
            ) {
                // Save current page to navigation history for back navigation
                if let Some(current_page) = stack.visible_child_name() {
                    nav_history.borrow_mut().push(current_page.to_string());
                }
                // Navigate to the album detail page asynchronously
                MainContext::default().spawn_local(album_page(
                    stack.downgrade(),
                    db_pool.clone(),
                    album_id,
                    header_btn_stack.downgrade(),
                    right_btn_box_for_closure.downgrade(),
                    sender.clone(),
                    show_dr_badges.clone(),
                    player_bar.clone(),
                ));
            }
        }
    });
    flow_child.add_controller(gesture);
    flow_child
}
