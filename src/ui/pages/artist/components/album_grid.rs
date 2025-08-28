use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use glib::{MainContext, WeakRef};
use gtk4::{
    Align::{Center, End, Start},
    Box, Button, EventControllerMotion, Fixed, FlowBoxChild, GestureClick,
    Orientation::{Horizontal, Vertical},
    Overlay,
    pango::{EllipsizeMode, WrapMode::WordChar},
};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{BoxExt, FixedExt, FlowBoxChildExt, ObjectExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    ui::{
        components::{
            player_bar::PlayerBar,
            tiles::{create_album_cover, create_album_label, create_dr_overlay},
        },
        pages::album::album_page::album_page,
        pages::artist::data::artist_data::AlbumDisplayInfoWithYear,
    },
    utils::formatting::format_freq_khz,
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
) -> FlowBoxChild {
    let title_label = create_album_label(
        &album.title,
        &["album-title-label"],
        Some(((cover_size - 16) / 10).max(8)),
        Some(EllipsizeMode::End),
        true,
        Some(WordChar),
        Some(2),
        // use_markup: false for plain text
        false,
    );
    title_label.set_size_request(cover_size - 16, -1);
    title_label.set_halign(Start);
    title_label.set_xalign(0.0);
    let artist_label = create_album_label(
        &album.artist,
        &["album-artist-label"],
        Some(18),
        Some(EllipsizeMode::End),
        false,
        None,
        None,
        // Explicitly set use_markup to false
        false,
    );

    // Ensure this class is applied
    artist_label.add_css_class("album-artist-label");

    // Format the audio quality line (e.g., "FLAC 24/96")
    let format_line = album
        .format
        .as_ref()
        .map(|format_str| {
            // Convert format to uppercase for consistent display
            let format_caps = format_str.to_uppercase();

            // Add technical details if available (bit depth and frequency)
            let tech_details = match (album.bit_depth, album.frequency) {
                (Some(bit), Some(freq)) => format!(" {}/{}", bit, format_freq_khz(freq)),
                (None, Some(freq)) => format!(" {}", format_freq_khz(freq)),
                _ => String::new(),
            };

            // Combine format and technical details
            format!("{}{}", format_caps, tech_details)
        })
        // Provide empty string if format is not available
        .unwrap_or_default();
    let format_label = create_album_label(
        &format_line,
        &["album-format-label"],
        None,
        None,
        false,
        None,
        None,
        // use_markup: false for plain text
        false,
    );
    format_label.set_halign(Start);
    format_label.set_hexpand(true);

    // Extract year from original release date string (e.g., "2023-05-15" -> "2023")
    // `as_deref` converts Option<String> to Option<&str> safely.
    let year_from_date = album
        .original_release_date
        .as_deref()
        .and_then(|s| s.split('-').next());

    // Get the year from the integer field, if available.
    let year_from_num = album.year;

    // Determine the final year string based on user preference for original vs. release year.
    // Convert both potential year sources to strings for consistent handling.
    let date_str_opt = year_from_date.map(str::to_string);
    let num_str_opt = year_from_num.map(|y| y.to_string());

    // Select year based on preference: original year first if enabled, otherwise release year
    let year_text = if use_original_year.get() {
        // Prefer original release date year, fallback to album year
        date_str_opt.or(num_str_opt)
    } else {
        // Prefer album year, fallback to original release date year
        num_str_opt.or(date_str_opt)
    }
    .unwrap_or_default(); // Default to empty string if neither is available

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
        let dr_label = create_dr_overlay(album.dr_value, album.dr_completed).unwrap();
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

    // Create container for title with fixed height to ensure consistent layout
    // Height of 40px allows for exactly two lines of text with proper spacing
    let title_area_box = Box::builder()
        .orientation(Vertical)
        .height_request(40)
        .margin_top(12)
        .build();
    title_label.set_valign(End);
    title_area_box.append(&title_label);
    album_tile_box.append(&title_area_box);
    album_tile_box.append(&artist_label);

    // Create year label with the determined year text
    let year_label = create_album_label(
        &year_text,
        &["album-format-label"],
        None,
        None,
        false,
        None,
        None,
        // Explicitly set use_markup to false for plain text
        false,
    );
    year_label.set_halign(End);
    year_label.set_hexpand(false);

    // Create horizontal container for format and year information
    let metadata_box = Box::builder()
        .orientation(Horizontal)
        .spacing(0)
        .hexpand(true)
        .build();
    metadata_box.append(&format_label);
    metadata_box.append(&year_label);
    album_tile_box.append(&metadata_box);
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
    let gesture = GestureClick::builder().build();
    gesture.connect_pressed(move |_, _, _, _| {
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
    });
    flow_child.add_controller(gesture);
    flow_child
}
