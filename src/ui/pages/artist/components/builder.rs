use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{Align::Start, Box, FlowBoxChild, Orientation::Vertical};
use libadwaita::{
    Clamp, ViewStack,
    glib::WeakRef,
    prelude::{BoxExt, FlowBoxChildExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use super::{
    cover::{
        add_overlay_to_album_overlay, create_album_cover_container, create_album_overlay,
        create_cover_fixed, create_dr_badge,
    },
    interaction::setup_click_gesture,
    metadata::{
        create_format_label, create_metadata_container, create_title_area_box, create_title_label,
        create_year_label,
    },
    play_button::{create_play_button, setup_hover_controller, setup_play_button_handler},
};
use crate::ui::{
    components::{player_bar::PlayerBar, tiles::text_utils::highlight, view_controls::ZoomLevel},
    pages::artist::data::artist_data::AlbumDisplayInfoWithYear,
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
/// - `nav_history`: Navigation history songer
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
    let title_label = create_title_label(&highlight(&album.title, search_text), cover_size);

    // Create and style the year label for displaying release year information
    let year_label = create_year_label(album, use_original_year.get(), zoom_level);

    // Create and style the format label for displaying audio format information
    let format_label = create_format_label(album, zoom_level, cover_size);

    // Main container for the album tile with vertical orientation
    let album_tile_box = Box::builder().orientation(Vertical).spacing(2).build();

    // Set size: cover size plus additional space for text elements
    album_tile_box.set_size_request(tile_size, tile_size + 80);
    album_tile_box.set_hexpand(false);
    album_tile_box.set_vexpand(false);
    album_tile_box.set_halign(Start);
    album_tile_box.set_valign(Start);

    // Create the album cover container
    let cover_container = create_album_cover_container(album.cover_art.as_deref(), cover_size);

    // Create DR badge
    let dr_label = create_dr_badge(album.dr_value, album.dr_is_best);

    // Create the album overlay with cover container and DR badge
    let overlay = create_album_overlay(cover_container, dr_label, show_dr_badges.get());

    // Create play button that appears on hover over the album cover
    let play_button = create_play_button();

    // Add play button as an overlay to the album overlay
    add_overlay_to_album_overlay(&overlay, &play_button);

    // Add click handler to play button to queue album
    setup_play_button_handler(&play_button, album, db_pool.clone(), player_bar.clone());

    // Add hover event controller to show/hide play button
    let motion_controller = setup_hover_controller(&play_button);
    overlay.add_controller(motion_controller);

    // Create fixed container for the cover area to ensure consistent sizing
    let cover_fixed = create_cover_fixed(&overlay, cover_size);
    album_tile_box.append(&cover_fixed);

    // Create title area box
    let title_area_box = create_title_area_box(&title_label);
    album_tile_box.append(&title_area_box);

    // Create metadata container
    let metadata_container = create_metadata_container(&format_label, &year_label, cover_size);
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
    setup_click_gesture(
        &flow_child,
        album,
        cover_size,
        stack,
        db_pool,
        header_btn_stack,
        right_btn_box,
        nav_history,
        sender,
        artist_page_name,
        show_dr_badges,
        player_bar,
        play_button,
    );
    flow_child
}
