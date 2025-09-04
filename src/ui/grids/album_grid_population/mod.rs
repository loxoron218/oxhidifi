use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use glib::{ControlFlow::Continue, MainContext, timeout_add_local};
use gtk4::{FlowBox, Label, Stack};
use libadwaita::prelude::WidgetExt;
use sqlx::SqlitePool;
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};

use crate::{
    data::db::dr_sync::synchronize_dr_completed_background,
    ui::{
        components::{player_bar::PlayerBar, sorting_types::SortOrder},
        grids::{
            album_grid_population::{sorting::sort_albums, ui_builder::create_album_tile},
            album_grid_state::{
                AlbumGridItem,
                AlbumGridState::{Empty, Populated},
            },
            async_data_loader::{
                DataLoaderMessage::{AlbumData, ArtistData, Completed, Error, Progress},
                spawn_album_loader,
            },
        },
    },
    utils::screen::ScreenInfo,
};

mod sorting;
mod ui_builder;

/// Populates the given `albums_grid` with album tiles, handling data fetching, sorting, and UI updates.
///
/// This function orchestrates the population of the album grid by coordinating between different
/// modules that handle data fetching, sorting, UI creation, and state management.
///
/// # Arguments
/// * `albums_grid` - The `gtk4::FlowBox` to populate with album tiles.
/// * `db_pool` - An `Arc<SqlitePool>` for database access.
/// * `sort_ascending` - A boolean indicating the overall sort direction (ascending/descending).
/// * `sort_orders` - A `Rc<RefCell<Vec<SortOrder>>>` defining the multi-level sorting criteria.
/// * `screen_info` - A `Rc<RefCell<ScreenInfo>>` providing screen dimensions for UI sizing.
/// * `scanning_label` - A `gtk4::Label` used for scanning feedback.
/// * `albums_inner_stack` - The `gtk4::Stack` managing the different states of the album grid.
/// * `album_count_label` - A `gtk4::Label` to display the number of albums.
/// * `show_dr_badges` - A `Rc<Cell<bool>>` indicating whether to show DR badges.
/// * `use_original_year` - A `Rc<Cell<bool>>` indicating whether to use original release year.
/// * `view_mode` - A `Rc<RefCell<String>>` representing the current view mode.
/// * `player_bar` - A `PlayerBar` instance for playback functionality.
pub async fn populate_albums_grid(
    albums_grid: &FlowBox,
    db_pool: Arc<SqlitePool>,
    sort_ascending: bool,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    screen_info: &Rc<RefCell<ScreenInfo>>,
    albums_inner_stack: &Stack,
    album_count_label: &Label,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    _view_mode: Rc<RefCell<String>>,
    player_bar: PlayerBar,
) {
    // A thread-local static to prevent multiple simultaneous population calls,
    // ensuring data consistency and preventing redundant work.
    thread_local! {
        static IS_BUSY: Cell<bool> = Cell::new(false);
    }

    // Check and set the busy flag. If already busy, return immediately.
    let already_busy = IS_BUSY.with(|cell| cell.replace(true));
    if already_busy {
        return;
    }

    // Clear existing children from the grid to prepare for new population.
    while let Some(child) = albums_grid.first_child() {
        albums_grid.remove(&child);
    }

    // Synchronize DR completed status from the persistence store in the background.
    // This ensures that any manual changes to best_dr_values.json or updates from other
    // parts of the application are reflected in the database without blocking the UI.
    let db_pool_clone = Arc::clone(&db_pool);
    MainContext::default().spawn_local(async move {
        if let Err(e) = synchronize_dr_completed_background(db_pool_clone, None).await {
            eprintln!(
                "Error synchronizing DR completed status in background: {}",
                e
            );
        }
    });

    // Spawn the async album loader
    let (receiver, _handle) = spawn_album_loader(db_pool.clone());
    let mut stream = UnboundedReceiverStream::new(receiver);

    // Variables to track state
    let mut all_albums: Vec<AlbumGridItem> = Vec::new();

    // Process messages from the async loader
    while let Some(message) = stream.next().await {
        match message {
            AlbumData(albums) => {
                // Collect albums for sorting
                all_albums.extend(albums);

                // Update UI with new albums without blocking
                process_albums_in_batches(
                    all_albums.clone(),
                    albums_grid,
                    screen_info,
                    show_dr_badges.clone(),
                    use_original_year.clone(),
                    player_bar.clone(),
                    db_pool.clone(),
                )
                .await;
            }
            ArtistData(_) => {
                // This message type is not relevant for album grid population
                // We can safely ignore it
            }
            Progress(processed, total) => {
                // Update progress in UI if needed
                if total > 0 {
                    album_count_label
                        .set_text(&format!("Loading... {} of {} Albums", processed, total));
                }
            }
            Completed => {
                // Final update with sorted albums
                if !all_albums.is_empty() {
                    // Update album count
                    album_count_label.set_text(&format!("{} Albums", all_albums.len()));

                    // If albums are found, transition to the populated grid state.
                    albums_inner_stack.set_visible_child_name(Populated.as_str());

                    // Multi-level sort albums according to user-defined sort orders.
                    sort_albums(&mut all_albums, &sort_orders, sort_ascending);

                    // Process all albums in batches to maintain UI responsiveness
                    process_albums_in_batches(
                        all_albums.clone(),
                        albums_grid,
                        screen_info,
                        show_dr_badges.clone(),
                        use_original_year.clone(),
                        player_bar.clone(),
                        db_pool.clone(),
                    )
                    .await;
                } else {
                    // Determine the appropriate state to show if no albums are found.
                    // After scan completion, we should show the empty state with "Add Music" button
                    // rather than remaining in loading or scanning state
                    albums_inner_stack.set_visible_child_name(Empty.as_str());

                    // Update count if no albums
                    album_count_label.set_text("0 Albums");
                }

                // Reset busy flag after all albums have been processed.
                IS_BUSY.with(|cell| cell.set(false));
                break;
            }
            Error(e) => {
                // Handle error case
                eprintln!("Error loading album data: {}", e);

                // On error, revert busy state and show an empty state.
                IS_BUSY.with(|cell| cell.set(false));
                albums_inner_stack.set_visible_child_name(Empty.as_str());

                // Update count on error
                album_count_label.set_text("0 Albums");
                break;
            }
        }
    }
}

/// Processes albums in batches to maintain UI responsiveness.
///
/// This function iterates through albums and creates UI tiles for them in batches,
/// yielding control to the GTK main thread periodically to keep the UI responsive.
async fn process_albums_in_batches(
    albums: Vec<AlbumGridItem>,
    albums_grid: &FlowBox,
    screen_info: &Rc<RefCell<ScreenInfo>>,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    db_pool: Arc<SqlitePool>,
) {
    // BATCH_SIZE: The number of album tiles to process before yielding control
    // back to the GTK main thread. This helps prevent UI freezes during large
    // grid population operations. A larger batch size means fewer yields but
    // potentially longer individual UI blocking.
    const BATCH_SIZE: usize = 50;
    let mut processed_count = 0;
    let use_original_year_clone_for_loop = use_original_year.clone();

    // Clear existing children from the grid to prepare for new population.
    while let Some(child) = albums_grid.first_child() {
        albums_grid.remove(&child);
    }

    for album_info in &albums {
        // Create the album tile
        let flow_child = create_album_tile(
            album_info,
            screen_info,
            &show_dr_badges,
            &use_original_year_clone_for_loop,
            &player_bar,
            db_pool.clone(),
        );

        // Insert the new album tile into the FlowBox.
        // -1 appends to the end
        albums_grid.insert(&flow_child, -1);
        processed_count += 1;

        // Yield control to the GTK main thread periodically to keep the UI responsive.
        if processed_count % BATCH_SIZE == 0 {
            timeout_add_local(Duration::from_millis(1), || Continue);
        }
    }
}
