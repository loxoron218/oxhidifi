use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use glib::{MainContext, prelude::ObjectExt};
use gtk4::{
    Align::{Center, End, Fill, Start},
    Box, FlowBoxChild, GestureClick, Image,
    Orientation::Vertical,
    pango::{EllipsizeMode, WrapMode::WordChar},
};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{BoxExt, FlowBoxChildExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    components::{
        player_bar::PlayerBar, tiles::helpers::create_album_label, tiles::text_utils::highlight,
    },
    pages::artist::artist_page::artist_page,
};

/// Creates a `FlowBoxChild` containing the UI representation of an artist.
///
/// This function constructs a visual tile for a given artist, including their
/// avatar and name. It also attaches a click gesture to navigate to the
/// artist's dedicated page.
///
/// # Parameters
/// - `artist_id`: Unique identifier of the artist in the database
/// - `artist_name`: Display name of the artist
/// - `cover_size`: Size in pixels for the artist avatar
/// - `_tile_size`: Reserved parameter for tile size (currently unused)
/// - `search_text`: Text to highlight in the artist name (for search results)
/// - `stack`: Reference to the main view stack for navigation
/// - `db_pool`: Database connection pool for data access
/// - `left_btn_stack`: Reference to the left button stack in the header
/// - `right_btn_box`: Reference to the right button container in the header
/// - `nav_history`: Navigation history tracker
/// - `sender`: Channel sender for application communication
/// - `show_dr_badges`: Flag indicating whether to display DR badges
/// - `use_original_year`: Flag for choosing between original and release years
/// - `player_bar`: Shared player control bar component
///
/// # Returns
/// A `FlowBoxChild` widget containing the complete artist tile UI
pub fn create_artist_tile(
    artist_id: i64,
    artist_name: &str,
    cover_size: i32,
    _tile_size: i32,
    search_text: &str,
    stack: Rc<ViewStack>,
    db_pool: Arc<SqlitePool>,
    left_btn_stack: Rc<ViewStack>,
    right_btn_box: Rc<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
) -> FlowBoxChild {
    // Create the default avatar icon for the artist
    let icon = Image::from_icon_name("avatar-default-symbolic");
    icon.set_pixel_size(cover_size);

    // Create the artist name label with highlighting for search matches
    let label = create_album_label(
        &highlight(artist_name, search_text),
        &[],
        // Font size calculation based on cover size
        Some(((cover_size - 16) / 10).max(8)),
        Some(EllipsizeMode::End),
        true,
        Some(WordChar),
        // Limit to 2 lines of text
        Some(2),
        // Enable markup for search highlighting
        true,
    );
    label.set_size_request(cover_size - 16, -1);

    // Create the main container box for the tile
    let tile = Box::builder().orientation(Vertical).spacing(2).build();

    // Set the overall size of the tile (cover size + room for text)
    tile.set_size_request(cover_size, cover_size + 80);
    tile.set_hexpand(false);
    tile.set_vexpand(false);
    tile.set_halign(Start);
    tile.set_valign(Start);

    // Create a fixed-size container for the artist avatar
    // This ensures consistent sizing regardless of the actual image dimensions
    let icon_container = Box::new(Vertical, 0);
    icon_container.set_size_request(cover_size, cover_size);
    icon_container.set_halign(Start);
    icon_container.set_valign(Start);
    icon_container.add_css_class("album-cover-border");
    icon_container.append(&icon);
    tile.append(&icon_container);

    // Create a container for the label area with a fixed height
    // This ensures consistent spacing for the artist name across all tiles
    let label_area_box = Box::builder()
        .orientation(Vertical)
        .height_request(40)
        .margin_top(12)
        .build();
    label_area_box.set_halign(Center);
    label.set_valign(End);
    label_area_box.append(&label);
    tile.append(&label_area_box);

    // Apply CSS styling to the tile
    tile.set_css_classes(&["album-tile"]);

    // Create the FlowBoxChild container that will hold the tile
    let flow_child = FlowBoxChild::builder().build();
    flow_child.set_child(Some(&tile));
    flow_child.set_hexpand(false);
    flow_child.set_vexpand(false);
    flow_child.set_halign(Fill);
    flow_child.set_valign(Start);

    // Create weak references for use in the click handler closure
    // This prevents circular reference issues that could cause memory leaks
    let stack_weak = stack.downgrade();
    let left_btn_stack_weak = left_btn_stack.downgrade();
    let right_btn_box_weak = right_btn_box.downgrade();

    // Create and configure the click gesture handler
    let gesture = GestureClick::builder().build();
    gesture.connect_pressed(move |_, _, _, _| {
        // Upgrade weak references to ensure they're still valid
        if let (Some(stack), Some(left_btn_stack)) =
            (stack_weak.upgrade(), left_btn_stack_weak.upgrade())
        {
            // Add current page to navigation history before navigating
            if let Some(current_page) = stack.visible_child_name() {
                nav_history.borrow_mut().push(current_page.to_string());
            }

            // Spawn the async artist page creation task
            MainContext::default().spawn_local(artist_page(
                stack.downgrade(),
                db_pool.clone(),
                artist_id,
                left_btn_stack.downgrade(),
                right_btn_box_weak.clone(),
                nav_history.clone(),
                sender.clone(),
                show_dr_badges.clone(),
                use_original_year.clone(),
                player_bar.clone(),
            ));
        }
    });

    // Attach the click gesture controller to the flow child
    flow_child.add_controller(gesture);
    
    // Set the widget name to the artist ID for navigation purposes
    flow_child.set_widget_name(&artist_id.to_string());
    flow_child
}
