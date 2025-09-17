use std::{cell::Cell, rc::Rc, sync::Arc};

use gtk4::{Box, Orientation::Vertical, PolicyType::Never, ScrolledWindow};
use libadwaita::{
    Clamp, ViewStack,
    glib::WeakRef,
    prelude::{BoxExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    components::player_bar::PlayerBar,
    pages::album::{
        components::{header::build_album_header, track_list::build_track_list},
        data::album_data::fetch_album_page_data,
    },
};

/// Build and present the album detail page for a given album ID.
///
/// This function is the main entry point for constructing the album detail page.
/// It fetches all necessary data for the album asynchronously and then builds
/// the UI components to display that information.
///
/// The function performs these key operations:
/// 1. Upgrades weak references to UI components
/// 2. Fetches album, artist, folder, and track data from the database
/// 3. Constructs the main page layout container
/// 4. Builds the album header and track list components
/// 5. Manages the view stack to display the new page
///
/// # Arguments
///
/// * `stack` - Weak reference to the main view stack for page navigation
/// * `db_pool` - Shared database connection pool for data access
/// * `album_id` - Unique identifier of the album to display
/// * `header_btn_stack` - Weak reference to the header button stack for navigation controls
/// * `header_right_btn_box` - Weak reference to the right header button container
/// * `sender` - Channel sender for UI update notifications
/// * `show_dr_badges` - Shared flag controlling display of dynamic range badges
/// * `player_bar` - Reference to the application's player bar component
///
/// # Implementation Details
///
/// The function uses weak references for UI components to avoid circular
/// reference issues that could prevent proper cleanup. If any weak reference
/// cannot be upgraded, the function returns early.
///
/// Data fetching is performed asynchronously to prevent blocking the UI thread.
/// If data fetching fails, the function returns early without displaying the page.
///
/// UI construction follows a modular approach, delegating to specialized
/// functions for the header and track list components. This promotes code
/// reuse and maintainability.
pub async fn album_page(
    stack: WeakRef<ViewStack>,
    db_pool: Arc<SqlitePool>,
    album_id: i64,
    header_btn_stack: WeakRef<ViewStack>,
    header_right_btn_box: WeakRef<Clamp>,
    sender: UnboundedSender<()>,
    show_dr_badges: Rc<Cell<bool>>,
    player_bar: PlayerBar,
) {
    // Attempt to upgrade weak references to strong references
    // If any upgrade fails, we cannot proceed with page construction
    let stack = match stack.upgrade() {
        Some(s) => s,
        None => return,
    };
    let header_btn_stack = match header_btn_stack.upgrade() {
        Some(s) => s,
        None => return,
    };
    let header_right_btn_box = match header_right_btn_box.upgrade() {
        Some(s) => s,
        None => return,
    };

    // Fetch all required data for the album page asynchronously
    // This includes album metadata, artist information, folder details,
    // track listing, and artist names for tracks (needed for various artists albums)
    let (album, artist, folder, tracks, track_artists, is_various_artists_album) =
        match fetch_album_page_data(&db_pool, album_id).await {
            Ok(data) => data,
            Err(_) => return,
        };

    // Create the main container for the album page with appropriate styling
    // Horizontal margins provide visual breathing room on the sides
    let horizontal_margin = 32;
    let page = Box::builder()
        .orientation(Vertical)
        .spacing(12)
        .margin_start(horizontal_margin)
        .margin_end(horizontal_margin)
        .build();
    page.add_css_class("album-detail-page");

    // Build the album header section containing cover art, title, artist,
    // metadata, and technical information
    let header = build_album_header(
        &album,
        &artist,
        &folder,
        &tracks,
        show_dr_badges.clone(),
        db_pool.clone(),
        sender.clone(),
        player_bar.clone(),
    );
    page.append(&header);

    // Build the track list section showing all tracks in the album
    // The track list handles various artists albums by displaying track-specific artists
    let track_list = build_track_list(
        &tracks,
        &album,
        &artist,
        &track_artists,
        is_various_artists_album,
        &player_bar,
    );
    page.append(&track_list);

    // Add bottom margin for visual spacing
    page.set_margin_bottom(32);

    // Manage the view stack to ensure only one album detail page exists
    // If a previous album detail page exists, remove it before adding the new one
    if let Some(existing) = stack.child_by_name("album_detail") {
        stack.remove(&existing);
    }

    // Wrap the page in a scrolled window to handle content that exceeds
    // the available vertical space, with vertical expansion enabled
    let album_scrolled_window = ScrolledWindow::builder()
        .child(&page)
        .vexpand(true)
        .hscrollbar_policy(Never)
        .build();

    // Add the album page to the main view stack and make it visible
    stack.add_titled(&album_scrolled_window, Some("album_detail"), "Album");
    stack.set_visible_child_name("album_detail");

    // Update header navigation controls to show the back button
    // and hide the right-side header buttons
    header_btn_stack.set_visible_child_name("back");
    header_right_btn_box.set_visible(false);
}
