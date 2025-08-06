use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use glib::MainContext;
use gtk4::{FlowBox, Label, Stack};
use libadwaita::{ApplicationWindow, Clamp, ViewStack, prelude::WidgetExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    data::db::query::fetch_all_artists, ui::components::tiles::create_artist_tile,
    utils::screen::ScreenInfo,
};

/// Populates the given artists grid with artist tiles, clearing and sorting as needed.
///
/// This asynchronous function fetches all artists from the database, filters out
/// "Various Artists", sorts them based on the `sort_ascending` flag, and then
/// creates and inserts `FlowBoxChild` tiles for each artist into the `artist_grid`.
/// It also manages the visibility of the `artists_inner_stack` to show appropriate
/// states (loading, empty, scanning, populated).
///
/// # Arguments
///
/// * `artist_grid` - The `gtk4::FlowBox` where artist tiles will be displayed.
/// * `db_pool` - An `Arc<SqlitePool>` for database operations.
/// * `sort_ascending` - A `bool` indicating whether artists should be sorted in ascending order.
/// * `stack` - The main `ViewStack` of the application (used for navigating to artist pages).
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar.
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar.
/// * `_window` - The `libadwaita::ApplicationWindow` (currently unused, but potentially useful
///   for future interactions with the main window).
/// * `scanning_label` - The `gtk4::Label` indicating scanning progress.
/// * `sender` - An `UnboundedSender<()>` for sending signals (e.g., for UI refreshes).
/// * `nav_history` - A `Rc<RefCell<Vec<String>>>` to manage navigation history.
/// * `artists_inner_stack` - The `gtk4::Stack` that manages the states of the artists view.
pub fn populate_artist_grid(
    artist_grid: &FlowBox,
    db_pool: Arc<SqlitePool>,
    sort_ascending: bool,
    stack: &ViewStack,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    _window: &ApplicationWindow,
    scanning_label: &Label,
    sender: UnboundedSender<()>,
    nav_history: Rc<RefCell<Vec<String>>>,
    artists_inner_stack: &Stack,
) {
    // `thread_local!` is used to prevent multiple concurrent calls to this function,
    // which could lead to race conditions or unnecessary re-population of the grid.
    thread_local! {
        static BUSY: Cell<bool> = Cell::new(false);
    }
    let already_busy = BUSY.with(|b| {
        if b.get() {
            true // If already busy, return true and do not proceed.
        } else {
            b.set(true); // If not busy, set busy to true and proceed.
            false
        }
    });
    if already_busy {
        return; // Exit if a population task is already in progress.
    }

    // Clone necessary `Rc` and `Arc` references for use within the `spawn_local` closure.
    let stack_rc = Rc::new(stack.clone());
    let left_btn_stack_rc = Rc::new(left_btn_stack.clone());
    let right_btn_box_rc = Rc::new(right_btn_box.clone()); // Clone as Rc directly
    let artist_grid = artist_grid.clone();
    let artists_inner_stack = artists_inner_stack.clone();
    let db_pool = Arc::clone(&db_pool);
    let scanning_label = scanning_label.clone();
    let sender = sender.clone();

    // Spawn a local asynchronous task on the GLib main context.
    // This allows UI updates to happen on the main thread after data fetching.
    MainContext::default().spawn_local(async move {
        let fetch_result = fetch_all_artists(&db_pool).await; // Fetch artists from the database.
        match fetch_result {
            Err(_) => {
                // On error, set busy to false and show the empty state.
                BUSY.with(|b| b.set(false));
                artists_inner_stack.set_visible_child_name("empty_state");
            }
            Ok(mut artists) => {
                // If no artists are found after fetching:
                if artists.is_empty() {
                    // Check if scanning label is visible, if so, show scanning state, else empty state.
                    if scanning_label.is_visible() {
                        artists_inner_stack.set_visible_child_name("scanning_state");
                    } else {
                        artists_inner_stack.set_visible_child_name("empty_state");
                    }
                    BUSY.with(|b| b.set(false)); // Set busy to false.
                    return; // Exit the function.
                }

                // If artists are found, set the stack to show the populated grid.
                artists_inner_stack.set_visible_child_name("populated_grid");

                // Filter out "Various Artists" as per application logic.
                artists.retain(|artist| artist.name != "Various Artists");

                // Sort artists based on the `sort_ascending` flag.
                artists.sort_by(|a, b| {
                    let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                    if sort_ascending { cmp } else { cmp.reverse() }
                });

                // Get screen info for tile sizing.
                let screen_info = ScreenInfo::new();
                let cover_size = screen_info.get_cover_size();
                let tile_size = screen_info.get_tile_size();

                // Clear existing children from the grid before adding new ones.
                while let Some(child) = artist_grid.first_child() {
                    artist_grid.remove(&child);
                }

                // Create and insert a tile for each artist.
                for artist in artists {
                    let tile = Rc::new(create_artist_tile(
                        artist.id,
                        &artist.name,
                        cover_size,
                        tile_size,
                        stack_rc.clone(),
                        db_pool.clone(),
                        left_btn_stack_rc.clone(),
                        right_btn_box_rc.clone(),
                        nav_history.clone(),
                        sender.clone(),
                    ));
                    artist_grid.insert(&*tile, -1); // Insert at the end.
                }
                // Ensure the populated grid is visible after adding tiles.
                artists_inner_stack.set_visible_child_name("populated_grid");
            }
        }
        BUSY.with(|b| b.set(false)); // Finally, set busy to false.
    });
}
