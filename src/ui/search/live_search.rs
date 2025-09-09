use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use glib::{MainContext, SourceId};
use gtk4::{Entry, FlowBox, Stack};
use libadwaita::{Clamp, ViewStack, prelude::EditableExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    data::db::query::{search_album_display_info, search_artists},
    ui::{
        components::{
            player_bar::PlayerBar,
            tiles::{album_tile::create_album_tile, artist_tile::create_artist_tile},
        },
        search::search_utils::{clear_grid, debounce_search, sort_albums_by_relevance},
    },
    utils::screen::ScreenInfo,
};

/// Connects live search logic to the given search entry, updating albums and artists grids as the user types.
///
/// This function sets up a `changed` signal handler for the `search_entry`.
/// When the search query changes, it clears the existing grids, performs
/// asynchronous database searches for albums and artists, and then populates
/// the respective `FlowBox` widgets with new UI tiles based on the search results.
/// It also handles the display of empty states if no results are found.
pub fn connect_live_search(
    search_entry: &Entry,
    albums_grid: &FlowBox,
    albums_stack: &Stack,
    artist_grid: &FlowBox,
    artists_stack: &Stack,
    db_pool: Arc<SqlitePool>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    stack: Rc<ViewStack>,
    left_btn_stack: Rc<ViewStack>,
    right_btn_box: Rc<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
) {
    // Compute dynamic sizes based on screen dimensions
    let screen_info = ScreenInfo::new();
    let cover_size = screen_info.get_cover_size();
    let tile_size = screen_info.get_tile_size();

    // Clone shared resources for the closure to avoid moving them into the closure
    // and allow them to be used across multiple async operations.
    let db_pool_cloned = db_pool.clone();
    let albums_grid_cloned = albums_grid.clone();
    let albums_stack_cloned = albums_stack.clone();
    let artist_grid_cloned = artist_grid.clone();
    let artists_stack_cloned = artists_stack.clone();
    let sort_ascending_cloned = sort_ascending.clone();
    let refresh_library_ui_cloned = refresh_library_ui.clone();
    let sort_ascending_artists_cloned = sort_ascending_artists.clone();
    let stack_cloned = stack.clone();
    let left_btn_stack_cloned = left_btn_stack.clone();
    let right_btn_box_cloned = right_btn_box.clone();
    let nav_history_cloned = nav_history.clone();
    let sender_cloned = sender.clone();
    let show_dr_badges_cloned = show_dr_badges.clone();
    let use_original_year_cloned = use_original_year.clone();
    let search_timer: Rc<RefCell<Option<SourceId>>> = Rc::new(RefCell::new(None));

    // Connect search entry changed signal
    search_entry.connect_changed(move |entry| {
        let text = entry.text().to_string();
        let sort_ascending_cloned = sort_ascending_cloned.clone();
        let sort_ascending_artists_cloned = sort_ascending_artists_cloned.clone();
        let refresh_library_ui_cloned = refresh_library_ui_cloned.clone();
        let db_pool_cloned = db_pool_cloned.clone();
        let albums_grid_cloned = albums_grid_cloned.clone();
        let albums_stack_cloned = albums_stack_cloned.clone();
        let artist_grid_cloned = artist_grid_cloned.clone();
        let artists_stack_cloned = artists_stack_cloned.clone();
        let stack_cloned = stack_cloned.clone();
        let left_btn_stack_cloned = left_btn_stack_cloned.clone();
        let right_btn_box_cloned = right_btn_box_cloned.clone();
        let nav_history_cloned = nav_history_cloned.clone();
        let sender_cloned = sender_cloned.clone();
        let show_dr_badges_cloned = show_dr_badges_cloned.clone();
        let use_original_year_cloned = use_original_year_cloned.clone();
        let search_timer_cloned = search_timer.clone();

        // Schedule a new timer.
        let player_bar_clone_inner = player_bar.clone();
        debounce_search(
            &search_timer_cloned,
            Duration::from_millis(300),
            move || {
                // Clear grids before performing a new search
                clear_grid(&albums_grid_cloned);
                clear_grid(&artist_grid_cloned);

                // If the search query is empty, refresh the library UI to show all albums/artists
                if text.trim().is_empty() {
                    refresh_library_ui_cloned(
                        sort_ascending_cloned.get(),
                        sort_ascending_artists_cloned.get(),
                    );
                    return;
                }

                // Trim the search query to remove leading/trailing whitespace
                let text = text.trim().to_string();

                // Clone variables for the async `spawn_local` closure
                let db_pool = db_pool_cloned.clone();
                let albums_grid = albums_grid_cloned.clone();
                let albums_stack = albums_stack_cloned.clone();
                let artist_grid = artist_grid_cloned.clone();
                let artists_stack = artists_stack_cloned.clone();
                let sort_ascending = sort_ascending_cloned.clone();
                let stack_for_closure = stack_cloned.clone();
                let left_btn_stack_for_closure = left_btn_stack_cloned.clone();
                let right_btn_box_clone = right_btn_box_cloned.clone();
                let nav_history = nav_history_cloned.clone();
                let sender = sender_cloned.clone();
                let show_dr_badges = show_dr_badges_cloned.clone();
                let use_original_year = use_original_year_cloned.clone();

                // Spawn an asynchronous task to perform the search and update the UI
                MainContext::default().spawn_local(async move {
                    // Perform album search
                    match search_album_display_info(&db_pool, &text).await {
                        Err(e) => {
                            // Log the error for debugging purposes
                            eprintln!("Error searching albums: {:?}", e);
                        }
                        Ok(mut albums) => {
                            // Sort albums based on relevance to the search query and then by artist/title
                            sort_albums_by_relevance(&mut albums, &text, &sort_ascending);

                            // Update the album UI based on search results
                            let child_name = if albums.is_empty() {
                                "no_results_state"
                            } else {
                                "populated_grid"
                            };
                            albums_stack.set_visible_child_name(child_name);
                            if !albums.is_empty() {
                                for album in &albums {
                                    let flow_child = Rc::new(create_album_tile(
                                        album,
                                        cover_size,
                                        tile_size,
                                        &text,
                                        stack_for_closure.clone(),
                                        db_pool.clone(),
                                        left_btn_stack_for_closure.clone(),
                                        right_btn_box_clone.clone(),
                                        nav_history.clone(),
                                        sender.clone(),
                                        show_dr_badges.clone(),
                                        use_original_year.clone(),
                                        player_bar_clone_inner.clone(),
                                    ));
                                    albums_grid.insert(&*flow_child, -1);
                                }
                            }
                        }
                    }

                    // Perform artist search
                    clear_grid(&artist_grid);
                    match search_artists(&db_pool, &text).await {
                        Err(e) => {
                            // Log the error for debugging purposes
                            eprintln!("Error searching artists: {:?}", e);
                        }
                        Ok(artists) => {
                            // Update the artist UI based on search results
                            let child_name = if artists.is_empty() {
                                "no_results_state"
                            } else {
                                "populated_grid"
                            };
                            artists_stack.set_visible_child_name(child_name);
                            if !artists.is_empty() {
                                for artist in &artists {
                                    let flow_child = Rc::new(create_artist_tile(
                                        artist.id,
                                        &artist.name,
                                        cover_size,
                                        tile_size,
                                        &text,
                                        stack_for_closure.clone(),
                                        db_pool.clone(),
                                        left_btn_stack_for_closure.clone(),
                                        right_btn_box_clone.clone(),
                                        nav_history.clone(),
                                        sender.clone(),
                                        show_dr_badges.clone(),
                                        use_original_year.clone(),
                                        player_bar_clone_inner.clone(),
                                    ));
                                    artist_grid.insert(&*flow_child, -1);
                                }
                            }
                        }
                    }
                });
            },
        );
    });
}
