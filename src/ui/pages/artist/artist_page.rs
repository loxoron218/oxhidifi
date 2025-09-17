use std::{
    cell::{Cell, RefCell},
    cmp::Ordering::{Equal, Greater, Less},
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    Align::{Center, Start},
    Box, FlowBox, Justification, Label,
    Orientation::Vertical,
    SelectionMode,
    glib::{WeakRef, clone::Downgrade},
};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{BoxExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    data::db::crud::fetch_artist_by_id,
    ui::{
        components::{player_bar::PlayerBar, view_controls::ZoomLevel},
        pages::artist::{
            components::album_grid::build_album_card,
            data::artist_data::fetch_album_display_info_by_artist,
        },
    },
    utils::screen::ScreenInfo,
};

/// Build and present the artist page for a given artist ID.
///
/// This asynchronous function constructs and displays the artist page, showing
/// all albums by the specified artist in a responsive grid layout. The function
/// handles data fetching, UI construction, and integration with the application's
/// navigation system.
///
/// # Parameters
/// - `stack`: Weak reference to the main view stack for page navigation
/// - `db_pool`: Shared database connection pool for data access
/// - `artist_id`: Unique identifier of the artist to display
/// - `header_btn_stack`: Weak reference to the header button stack for navigation controls
/// - `right_btn_box`: Weak reference to the right header button container
/// - `nav_history`: Shared reference to the navigation history tracker
/// - `sender`: Channel sender for communication with other parts of the application
/// - `show_dr_badges`: Shared flag indicating whether to display DR badges
/// - `use_original_year`: Shared flag for choosing between original and release years
/// - `player_bar`: Shared player control bar component
/// - `screen_info`: Screen information for calculating cover and tile sizes
///
/// # Behavior
/// - Fetches artist information and album data from the database
/// - Sorts albums chronologically (oldest first), with albums without year last
/// - Constructs a responsive UI with artist header and album grid
/// - Integrates with the application's view stack navigation system
/// - Removes any existing page with the same name to prevent duplicates
/// - Hides right header buttons when displaying the artist page
///
/// # Panics
/// This function returns early (without panicking) if required weak references
/// cannot be upgraded or if database queries fail.
pub async fn artist_page(
    stack: WeakRef<ViewStack>,
    db_pool: Arc<SqlitePool>,
    artist_id: i64,
    header_btn_stack: WeakRef<ViewStack>,
    right_btn_box: WeakRef<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    screen_info: Rc<RefCell<ScreenInfo>>,
    current_zoom_level: ZoomLevel,
) {
    // Generate a unique page name for this artist
    let page_name = format!("artist_{}", artist_id);

    // Upgrade weak references to ensure they're still valid
    // Return early if any references have been dropped
    let stack = match stack.upgrade() {
        Some(s) => s,
        None => return,
    };
    let header_btn_stack = match header_btn_stack.upgrade() {
        Some(s) => s,
        None => return,
    };

    // Fetch artist information from the database
    // Return early if the artist cannot be found
    let artist = match fetch_artist_by_id(&db_pool, artist_id).await {
        Ok(a) => a,
        Err(_) => return,
    };

    // Fetch all albums by this artist with display information
    // Return early if the query fails
    let albums = match fetch_album_display_info_by_artist(&db_pool, artist_id).await {
        Ok(albums) => albums,
        Err(_) => return,
    };

    // Sort albums by year (oldest first), with albums without year information last
    // This provides a chronological view of the artist's work
    let mut albums = albums;
    albums.sort_by(|a, b| match (a.year, b.year) {
        // Compare years when both are available
        (Some(ya), Some(yb)) => ya.cmp(&yb),

        // Albums with year come before those without
        (Some(_), None) => Less,

        // Albums without year come after those with
        (None, Some(_)) => Greater,

        // Albums without year are considered equal
        (None, None) => Equal,
    });

    // Build the main UI container with vertical orientation and appropriate spacing
    let vbox = Box::builder()
        .orientation(Vertical)
        .spacing(24)
        .margin_top(32)
        .margin_bottom(32)
        .margin_start(32)
        .margin_end(32)
        .build();

    // Create a centered header with the artist's name using appropriate styling
    let header = Label::builder()
        .label(&artist.name)
        .css_classes(["title-1"])
        .halign(Center)
        .justify(Justification::Center)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    vbox.append(&header);

    // Compute dynamic cover and tile sizes based on screen dimensions
    // This ensures the UI adapts to different screen sizes and resolutions
    let cover_size = screen_info.borrow().get_cover_size();
    let tile_size = screen_info.borrow().get_tile_size();

    // Create a responsive flow box for displaying albums in a grid
    // This matches the styling used in the main albums grid and album page
    let flowbox = FlowBox::builder()
        .valign(Start)
        .max_children_per_line(128)
        .selection_mode(SelectionMode::None)
        .row_spacing(1)
        .column_spacing(0)
        .build();

    // Center the grid horizontally
    flowbox.set_halign(Center);

    // Populate the flow box with album cards for each album
    for album in albums {
        // Build an album card widget for each album
        let album_card = build_album_card(
            &album,
            cover_size,
            tile_size,
            stack.downgrade(),
            db_pool.clone(),
            header_btn_stack.downgrade(),
            right_btn_box.clone(),
            nav_history.clone(),
            sender.clone(),
            page_name.clone(),
            show_dr_badges.clone(),
            use_original_year.clone(),
            player_bar.clone(),
            // Empty search text for artist page
            "",
            current_zoom_level,
        );

        // Add the album card to the flow box
        flowbox.insert(&album_card, -1);
    }

    // Wrap the flow box in a clamp for consistent horizontal padding
    // This matches the styling used in the album page for visual consistency
    let clamp = Clamp::builder().child(&flowbox).build();
    vbox.append(&clamp);

    // Add the constructed page to the view stack and make it visible
    let page_name = format!("artist_{}", artist_id);

    // Remove any existing page with the same name to avoid duplicate warnings
    // This can happen during rapid navigation or refresh operations
    if let Some(existing_child) = stack.child_by_name(&page_name) {
        stack.remove(&existing_child);
    }

    // Add the new page to the stack and make it the visible page
    stack.add_named(&vbox, Some(&page_name));
    stack.set_visible_child_name(&page_name);

    // Set the header button stack to show the back button
    header_btn_stack.set_visible_child_name("back");

    // Hide the right header buttons when displaying the artist page
    // This provides a cleaner UI focused on the artist's content
    if let Some(right_btn_box) = right_btn_box.upgrade() {
        right_btn_box.set_visible(false);
    }
}
