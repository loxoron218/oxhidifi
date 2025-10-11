use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{Align::Start, Box, FlowBoxChild, Orientation::Vertical};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{BoxExt, FlowBoxChildExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    components::{
        player_bar::PlayerBar,
        tiles::album_tile::{
            cover::create_album_cover_section,
            labels::{
                create_artist_label, create_format_label, create_metadata_container,
                create_title_label, create_year_label,
            },
            navigation::create_navigation_gesture,
            play_button::{create_cover_fixed, create_motion_controller, create_play_button},
        },
        view_controls::ZoomLevel,
    },
    grids::album_grid_state::AlbumGridItem,
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
/// * `show_album_metadata` - Flag controlling whether to show album metadata (title, artist, format, year)
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
    show_album_metadata: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    zoom_level: ZoomLevel,
) -> FlowBoxChild {
    // Create title and artist labels
    let title_label = create_title_label(album, search_text, cover_size);
    let artist_label = create_artist_label(album, search_text, cover_size);

    // Create format and year labels
    let format_label = create_format_label(album, cover_size, zoom_level);
    let year_label = create_year_label(album, zoom_level, use_original_year.clone());

    // Create album cover section with DR badge
    let (overlay, _cover) = create_album_cover_section(album, cover_size, show_dr_badges.clone());

    // Create play button and add it to the overlay
    let play_button = create_play_button(album, player_bar.clone(), db_pool.clone());
    overlay.add_overlay(&play_button);

    // Add motion controller for hover effects
    let motion_controller = create_motion_controller(&play_button);
    overlay.add_controller(motion_controller);

    // Create the fixed container for the cover overlay
    let cover_fixed = create_cover_fixed(&overlay, cover_size);

    // Main vertical box for the album tile layout
    let album_tile_box = Box::builder().orientation(Vertical).spacing(2).build();

    // Set tile size based on whether metadata is shown
    if show_album_metadata.get() {
        // Show metadata - use the current size with space for text
        album_tile_box.set_size_request(cover_size, tile_size + 80);
    } else {
        // Hide metadata - only use space for cover
        album_tile_box.set_size_request(cover_size, cover_size);
    }

    album_tile_box.set_hexpand(false);
    album_tile_box.set_vexpand(false);
    album_tile_box.set_halign(Start);
    album_tile_box.set_valign(Start);

    // Add the cover section first
    album_tile_box.append(&cover_fixed);

    // Conditionally add metadata based on setting
    if show_album_metadata.get() {
        // Box for title, with explicit height for two lines of text
        let title_area_box = Box::builder()
            .orientation(Vertical)
            .height_request(40)
            .margin_top(12)
            .build();
        title_area_box.append(&title_label);
        album_tile_box.append(&title_area_box);
        album_tile_box.append(&artist_label);

        // Create and add the metadata container
        let metadata_container = create_metadata_container(&format_label, &year_label, cover_size);
        album_tile_box.append(&metadata_container);
    }
    album_tile_box.set_css_classes(&["album-tile"]);

    // Create the FlowBoxChild and set its properties
    let flow_child = FlowBoxChild::new();
    flow_child.set_child(Some(&album_tile_box));
    flow_child.set_hexpand(false);
    flow_child.set_vexpand(false);
    flow_child.set_halign(Start);
    flow_child.set_valign(Start);
    flow_child.set_size_request(cover_size, -1);

    // Add navigation gesture to the flow child
    create_navigation_gesture(
        &flow_child,
        album,
        stack_for_closure,
        db_pool,
        left_btn_stack_for_closure,
        right_btn_box_for_closure,
        nav_history,
        sender,
        show_dr_badges,
        player_bar,
        &play_button,
    );
    flow_child
}
