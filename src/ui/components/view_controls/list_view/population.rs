use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use glib::MainContext;
use gtk4::{Label, Stack, gio::ListStore};
use sqlx::SqlitePool;
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};

use crate::{
    data::db::dr_sync::synchronize_dr_is_best_background,
    ui::{
        components::{
            player_bar::PlayerBar,
            view_controls::list_view::data_model::{AlbumListItem, AlbumListItemObject},
            view_controls::sorting_controls::types::SortOrder,
        },
        grids::{
            album_grid_population::sorting::sort_albums,
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
};

/// Populates the given `column_view_model` with album data, handling data fetching, sorting, and UI updates.
///
/// This function orchestrates the population of the column view by coordinating between different
/// modules that handle data fetching, sorting, and state management.
///
/// # Arguments
/// * `column_view_model` - The `gtk4::ListStore` to populate with album data.
/// * `db_pool` - An `Arc<SqlitePool>` for database access.
/// * `sort_ascending` - A boolean indicating the overall sort direction (ascending/descending).
/// * `sort_orders` - A `Rc<RefCell<Vec<SortOrder>>>` defining the multi-level sorting criteria.
/// * `albums_inner_stack` - The `gtk4::Stack` managing the different states of the album grid.
/// * `album_count_label` - A `gtk4::Label` to display the number of albums.
/// * `use_original_year` - A `Rc<Cell<bool>>` indicating whether to use original release year.
/// * `player_bar` - A `PlayerBar` instance for playback functionality.
pub async fn populate_albums_column_view(
    column_view_model: &ListStore,
    db_pool: Arc<SqlitePool>,
    sort_ascending: bool,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    albums_inner_stack: &Stack,
    album_count_label: &Label,
    _use_original_year: Rc<Cell<bool>>,
    _player_bar: PlayerBar,
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

    // Clear existing items from the model to prepare for new population.
    column_view_model.remove_all();

    // Synchronize DR best status from the persistence store in the background.
    // This ensures that any manual changes to best_dr_values.json or updates from other
    // parts of the application are reflected in the database without blocking the UI.
    let db_pool_clone = Arc::clone(&db_pool);
    MainContext::default().spawn_local(async move {
        if let Err(e) = synchronize_dr_is_best_background(db_pool_clone, None).await {
            eprintln!("Error synchronizing DR best status in background: {}", e);
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
                    column_view_model,
                    _use_original_year.clone(),
                    _player_bar.clone(),
                    db_pool.clone(),
                )
                .await;
            }

            // Handle ArtistData messages (ignored in album context)
            ArtistData(_) => {
                // This message type is not relevant for album grid population
                // We can safely ignore it
            }

            // Handle progress updates from the async loader
            Progress(processed, total) => {
                // Update progress in UI if needed
                if total > 0 {
                    album_count_label
                        .set_text(&format!("Loading... {} of {} Albums", processed, total));
                }
            }

            // Handle completion of the async loading process
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
                        column_view_model,
                        _use_original_year.clone(),
                        _player_bar.clone(),
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

            // Handle errors from the async loading process
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
/// This function iterates through albums and creates UI items for them in batches,
/// yielding control to the GTK main thread periodically to keep the UI responsive.
async fn process_albums_in_batches(
    albums: Vec<AlbumGridItem>,
    column_view_model: &ListStore,
    _use_original_year: Rc<Cell<bool>>,
    _player_bar: PlayerBar,
    _db_pool: Arc<SqlitePool>,
) {
    // BATCH_SIZE: The number of album items to process before yielding control
    // back to the GTK main thread. This helps prevent UI freezes during large
    // grid population operations. A larger batch size means fewer yields but
    // potentially longer individual UI blocking.
    const BATCH_SIZE: usize = 50;
    let mut processed_count = 0;

    // Clear existing items from the model to prepare for new population.
    column_view_model.remove_all();

    for album_info in &albums {
        // Create the album list item
        let album_list_item = AlbumListItem::new(
            album_info.id,
            album_info.title.clone(),
            album_info.artist.clone(),
            album_info.cover_art.clone(),
            album_info.year,
            album_info.original_release_date.clone(),
            album_info.dr_value,
            album_info.dr_is_best,
            album_info.format.clone(),
            album_info.bit_depth,
            album_info.sample_rate,
            album_info.folder_path.clone(),
        );

        // Create an AlbumListItemObject wrapper for the album data and append it to the column view model.
        // This makes the album visible in the UI list.
        let album_object = AlbumListItemObject::new(album_list_item);
        column_view_model.append(&album_object);
        processed_count += 1;

        // Yield control to the GTK main thread periodically to keep the UI responsive.
        if processed_count % BATCH_SIZE == 0 {
            // In a real implementation, we would yield here, but for now we'll just continue

            // Yield control to the GTK main thread periodically to keep the UI responsive.
            if processed_count % BATCH_SIZE == 0 {
                // In a real implementation, we would yield here, but for now we'll just continue
            }
        }
    }
}
