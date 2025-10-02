use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{FlowBox, Label, Stack, glib::MainContext};
use libadwaita::{Clamp, ViewStack, prelude::WidgetExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};

use crate::{
    data::{db::dr_sync::synchronize_dr_is_best_from_store, models::Artist},
    ui::{
        components::{
            player_bar::PlayerBar, tiles::artist_tile::create_artist_tile, view_controls::ZoomLevel,
        },
        grids::async_data_loader::{DataLoaderMessage, spawn_artist_loader},
    },
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
/// * `current_zoom_level` - The current zoom level for consistent sizing across views.
pub fn populate_artist_grid(
    artist_grid: &FlowBox,
    db_pool: Arc<SqlitePool>,
    sort_ascending: bool,
    stack: &ViewStack,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    screen_info: &Rc<RefCell<ScreenInfo>>,
    sender: UnboundedSender<()>,
    nav_history: Rc<RefCell<Vec<String>>>,
    artists_inner_stack: &Stack,
    artist_count_label: Rc<Label>,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    current_zoom_level: ZoomLevel,
) {
    // `thread_local!` is used to prevent multiple concurrent calls to this function,
    // which could lead to race conditions or unnecessary re-population of the grid.
    thread_local! {
        static BUSY: Cell<bool> = const { Cell::new(false) };
    }
    let already_busy = BUSY.with(|b| b.replace(true));
    if already_busy {
        return;
    }

    // Clone necessary `Rc` and `Arc` references for use within the `spawn_local` closure.
    let stack_rc = Rc::new(stack.clone());
    let left_btn_stack_rc = Rc::new(left_btn_stack.clone());
    let right_btn_box_rc = Rc::new(right_btn_box.clone());
    let artist_grid = artist_grid.clone();
    let artists_inner_stack = artists_inner_stack.clone();
    let db_pool = Arc::clone(&db_pool);
    let screen_info = Rc::clone(screen_info);
    let sender = sender.clone();
    let artist_count_label = artist_count_label.clone();
    let show_dr_badges = show_dr_badges.clone();
    let use_original_year = use_original_year.clone();

    // Spawn a local asynchronous task on the GLib main context.
    // This allows UI updates to happen on the main thread after data fetching.
    MainContext::default().spawn_local(async move {
        // Synchronize DR best status from the persistence store before fetching artist info.
        if let Err(e) = synchronize_dr_is_best_from_store(&db_pool).await {
            eprintln!(
                "Error synchronizing DR best status before artist grid population: {}",
                e
            );
        }

        // Spawn the async artist loader
        let (receiver, _handle) = spawn_artist_loader(db_pool.clone());
        let mut stream = UnboundedReceiverStream::new(receiver);

        // Variables to song state
        let mut all_artists: Vec<Artist> = Vec::new();

        // Process messages from the async loader
        while let Some(message) = stream.next().await {
            match message {
                DataLoaderMessage::ArtistData(artists) => {
                    // Collect artists
                    all_artists.extend(artists);

                    // Update UI with new artists without blocking
                    update_artist_grid_ui(
                        &artist_grid,
                        &all_artists,
                        &stack_rc,
                        &left_btn_stack_rc,
                        &right_btn_box_rc,
                        &screen_info,
                        &nav_history,
                        &sender,
                        &show_dr_badges,
                        &use_original_year,
                        &player_bar,
                        db_pool.clone(),
                        current_zoom_level,
                    )
                    .await;
                }
                DataLoaderMessage::AlbumData(_) => {
                    // This message type is not relevant for artist grid population
                    // We can safely ignore it
                }
                DataLoaderMessage::Progress(processed, total) => {
                    // Update progress in UI if needed
                    if total > 0 {
                        artist_count_label
                            .set_text(&format!("Loading... {} of {} Artists", processed, total));
                    }
                }
                DataLoaderMessage::Completed => {
                    // Final update with sorted artists
                    if !all_artists.is_empty() {
                        // Filter out "Various Artists" as per application logic.
                        all_artists.retain(|artist| artist.name != "Various Artists");

                        // Artists fetched: {}
                        artist_count_label.set_text(&format!("{} Artists", all_artists.len()));

                        // If artists are found, set the stack to show the populated grid.
                        artists_inner_stack.set_visible_child_name("populated_grid");

                        // Sort artists based on the `sort_ascending` flag.
                        all_artists.sort_by(|a, b| {
                            let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                            if sort_ascending { cmp } else { cmp.reverse() }
                        });

                        // Update UI with sorted artists
                        update_artist_grid_ui(
                            &artist_grid,
                            &all_artists,
                            &stack_rc,
                            &left_btn_stack_rc,
                            &right_btn_box_rc,
                            &screen_info,
                            &nav_history,
                            &sender,
                            &show_dr_badges,
                            &use_original_year,
                            &player_bar,
                            db_pool.clone(),
                            current_zoom_level,
                        )
                        .await;

                        // Ensure the populated grid is visible after adding tiles.
                        artists_inner_stack.set_visible_child_name("populated_grid");
                    } else {
                        // If no artists are found after fetching:
                        // After scan completion, we should show the empty state with "Add Music" button
                        // rather than remaining in loading or scanning state
                        artists_inner_stack.set_visible_child_name("empty_state");

                        // Update count if no artists
                        artist_count_label.set_text("0 Artists");
                    }

                    BUSY.with(|b| b.set(false));
                    break;
                }
                DataLoaderMessage::Error(e) => {
                    eprintln!("Error loading artist data: {}", e);
                    BUSY.with(|b| b.set(false));
                    artists_inner_stack.set_visible_child_name("empty_state");
                    artist_count_label.set_text("0 Artists");
                    break;
                }
            }
        }
    });
}

/// Updates the artist grid UI with the provided artists
async fn update_artist_grid_ui(
    artist_grid: &FlowBox,
    artists: &[Artist],
    stack_rc: &Rc<ViewStack>,
    left_btn_stack_rc: &Rc<ViewStack>,
    right_btn_box_rc: &Rc<Clamp>,
    screen_info: &Rc<RefCell<ScreenInfo>>,
    nav_history: &Rc<RefCell<Vec<String>>>,
    sender: &UnboundedSender<()>,
    show_dr_badges: &Rc<Cell<bool>>,
    use_original_year: &Rc<Cell<bool>>,
    player_bar: &PlayerBar,
    db_pool: Arc<SqlitePool>,
    current_zoom_level: ZoomLevel,
) {
    // Get screen info for tile sizing.
    let cover_size = screen_info.borrow().cover_size;
    let tile_size = screen_info.borrow().tile_size;

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
            "",
            stack_rc.clone(),
            db_pool.clone(),
            left_btn_stack_rc.clone(),
            right_btn_box_rc.clone(),
            nav_history.clone(),
            sender.clone(),
            show_dr_badges.clone(),
            use_original_year.clone(),
            player_bar.clone(),
            screen_info.clone(),
            current_zoom_level,
        ));
        artist_grid.insert(&*tile, -1);
    }
}
