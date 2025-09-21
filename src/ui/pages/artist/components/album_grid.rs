use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    Align::{Center, End, Start},
    Box, Button, EventControllerMotion, Fixed, FlowBoxChild, GestureClick,
    Orientation::{Horizontal, Vertical},
    Overlay,
    glib::{MainContext, WeakRef},
    pango::{EllipsizeMode, WrapMode::WordChar},
};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{BoxExt, ButtonExt, FixedExt, FlowBoxChildExt, ObjectExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    ui::{
        components::{
            player_bar::PlayerBar,
            tiles::{
                helpers::{create_album_cover, create_album_label, create_dr_overlay},
                text_utils::highlight,
            },
            view_controls::ZoomLevel::{self, ExtraSmall, Small},
        },
        pages::{
            album::album_page::album_page, artist::data::artist_data::AlbumDisplayInfoWithYear,
        },
    },
    utils::formatting::{format_sample_rate_khz, format_sample_rate_value, format_year_info},
};

/// Build an album card widget for the artist page.
///
/// This function creates a visual representation of an album for display on an artist's page.
/// The album card includes the cover art, title, artist name (replaced with album year),
/// format information, and dynamic range (DR) badge when enabled.
///
/// # Parameters
/// - `album`: Album information to display
/// - `cover_size`: Size for the album cover image in pixels
/// - `tile_size`: Overall size for the album tile
/// - `stack`: Weak reference to the view stack for navigation
/// - `db_pool`: Database connection pool for fetching additional data
/// - `header_btn_stack`: Weak reference to the header button stack
/// - `right_btn_box`: Weak reference to the right button container
/// - `nav_history`: Navigation history tracker
/// - `sender`: Channel sender for communication
/// - `artist_page_name`: Name of the current artist page
/// - `show_dr_badges`: Flag indicating whether to show DR badges
/// - `use_original_year`: Flag to determine which year to display
/// - `player_bar`: Player control bar
///
/// # Returns
/// A `FlowBoxChild` containing the complete album card UI
pub fn build_album_card(
    album: &AlbumDisplayInfoWithYear,
    cover_size: i32,
    tile_size: i32,
    stack: WeakRef<ViewStack>,
    db_pool: Arc<SqlitePool>,
    header_btn_stack: WeakRef<ViewStack>,
    right_btn_box: WeakRef<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    artist_page_name: String,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    search_text: &str,
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
                                format!(" {}/{}", bit, format_sample_rate_value(freq))
                            }
                            _ => {
                                format!(" {}/{}", bit, format_sample_rate_khz(freq))
                            }
                        }
                    }
                    (None, Some(freq)) => {
                        // For Small zoom level, don't show "kHz" suffix
                        match zoom_level {
                            Small => {
                                format!(" {}", format_sample_rate_value(freq))
                            }
                            _ => {
                                format!(" {}", format_sample_rate_khz(freq))
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

    // Main container for the album tile with vertical orientation
    let album_tile_box = Box::builder().orientation(Vertical).spacing(2).build();

    // Set size: cover size plus additional space for text elements
    album_tile_box.set_size_request(tile_size, tile_size + 80);
    album_tile_box.set_hexpand(false);
    album_tile_box.set_vexpand(false);
    album_tile_box.set_halign(Start);
    album_tile_box.set_valign(Start);

    // Create fixed-size container for album cover to ensure consistent sizing
    let cover_container = Box::new(Vertical, 0);
    cover_container.set_size_request(cover_size, cover_size);
    cover_container.set_halign(Start);
    cover_container.set_valign(Start);
    let cover = create_album_cover(album.cover_art.as_deref(), cover_size);
    cover_container.append(&cover);

    // Create overlay to stack DR badge and play button on top of album cover
    let overlay = Overlay::new();
    overlay.set_size_request(cover_size, cover_size);
    overlay.set_child(Some(&cover_container));
    overlay.set_halign(Start);
    overlay.set_valign(Start);

    // Conditionally add DR badge to overlay based on user settings
    if show_dr_badges.get() {
        let dr_label = create_dr_overlay(album.dr_value, album.dr_is_best).unwrap();
        overlay.add_overlay(&dr_label);
    }

    // Create play button that appears on hover over the album cover
    let play_button = Button::builder()
        .icon_name("media-playback-start")
        .css_classes(&["play-pause-button", "album-cover-play"][..])
        .build();
    play_button.set_size_request(56, 56);
    play_button.set_halign(Center);
    play_button.set_valign(Center);
    play_button.set_visible(false);
    overlay.add_overlay(&play_button);

    // Add click handler to play button to queue album
    let player_bar_clone = player_bar.clone();
    let album_id = album.id;
    play_button.connect_clicked(move |_| {
        // Get the playback controller from the player bar
        if let Some(controller) = player_bar_clone.get_playback_controller() {
            // Spawn async task to queue the album
            MainContext::default().spawn_local(async move {
                // Queue the album for playback
                match controller.lock() {
                    Ok(mut controller) => {
                        if let Err(e) = controller.queue_album(album_id).await {
                            eprintln!("Error queuing album {}: {}", album_id, e);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to acquire lock on playback controller: {}", e);
                    }
                }
            });
        } else {
            eprintln!("No playback controller available");
        }
    });

    // Add hover event controller to show/hide play button
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
    overlay.add_controller(motion_controller);

    // Create fixed container for the cover area to ensure consistent sizing
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

    // Create FlowBoxChild container and set the album tile as its child
    let flow_child = FlowBoxChild::builder().build();
    flow_child.set_child(Some(&album_tile_box));
    flow_child.set_hexpand(false);
    flow_child.set_vexpand(false);
    flow_child.set_halign(Start);
    flow_child.set_valign(Start);

    // Add click gesture for navigation to album page
    let stack_weak = stack.clone();
    let db_pool_clone = Arc::clone(&db_pool);
    let header_btn_stack_weak = header_btn_stack.clone();
    let right_btn_box_weak = right_btn_box.clone();
    let sender_clone = sender.clone();
    let album_id = album.id;
    let play_button_weak = play_button.downgrade();
    let gesture = GestureClick::builder().build();
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
            // Navigate to album page when clicked
            if let (Some(stack), Some(header_btn_stack)) =
                (stack_weak.upgrade(), header_btn_stack_weak.upgrade())
            {
                // Add current page to navigation history
                nav_history.borrow_mut().push(artist_page_name.clone());

                // Spawn async task to load album page
                MainContext::default().spawn_local(album_page(
                    stack.downgrade(),
                    db_pool_clone.clone(),
                    album_id,
                    header_btn_stack.downgrade(),
                    right_btn_box_weak.clone(),
                    sender_clone.clone(),
                    show_dr_badges.clone(),
                    player_bar.clone(),
                ));
            }
        }
    });
    flow_child.add_controller(gesture);
    flow_child
}
