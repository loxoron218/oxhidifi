use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use glib::MainContext;
use gtk4::{Entry, FlowBox, Stack};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{EditableExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::data::db::db_query::{search_album_display_info, search_artists};
use crate::ui::components::tiles::{create_album_tile, create_artist_tile};
use crate::utils::screen::{compute_cover_and_tile_size, get_primary_screen_size};

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
    artists_grid: &FlowBox,
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
) {
    // Compute dynamic sizes based on screen dimensions
    let (screen_width, _) = get_primary_screen_size();
    let (cover_size, tile_size) = compute_cover_and_tile_size(screen_width);

    // Clone shared resources for the closure to avoid moving them into the closure
    // and allow them to be used across multiple async operations.
    let db_pool_cloned = db_pool.clone();
    let albums_grid_cloned = albums_grid.clone();
    let albums_stack_cloned = albums_stack.clone();
    let artists_grid_cloned = artists_grid.clone();
    let artists_stack_cloned = artists_stack.clone();
    let sort_ascending_cloned = sort_ascending.clone();
    let refresh_library_ui_cloned = refresh_library_ui.clone();
    let sort_ascending_artists_cloned = sort_ascending_artists.clone();
    let stack_cloned = stack.clone();
    let left_btn_stack_cloned = left_btn_stack.clone();
    let right_btn_box_cloned = right_btn_box.clone();
    let nav_history_cloned = nav_history.clone();
    let sender_cloned = sender.clone();

    // Connect search entry changed signal
    search_entry.connect_changed(move |entry| {
        let text = entry.text().to_string();

        // Clear grids before performing a new search
        clear_grid(&albums_grid_cloned);
        clear_grid(&artists_grid_cloned);

        // If the search query is empty, refresh the library UI to show all albums/artists
        if text.trim().is_empty() {
            refresh_library_ui_cloned(
                sort_ascending_cloned.get(),
                sort_ascending_artists_cloned.get(),
            );

            // Ensure stacks are set to populated_grid, as refresh_library_ui will handle empty state if needed
            albums_stack_cloned.set_visible_child_name("populated_grid");
            artists_stack_cloned.set_visible_child_name("populated_grid");
            return;
        }

        // Trim the search query to remove leading/trailing whitespace
        let text = text.trim().to_string();

        // Clone variables for the async `spawn_local` closure
        let db_pool = db_pool_cloned.clone();
        let albums_grid = albums_grid_cloned.clone();
        let albums_stack = albums_stack_cloned.clone();
        let artists_grid = artists_grid_cloned.clone();
        let artists_stack = artists_stack_cloned.clone();
        let sort_ascending = sort_ascending_cloned.clone();
        let stack_for_closure = stack_cloned.clone();
        let left_btn_stack_for_closure = left_btn_stack_cloned.clone();
        let right_btn_box_clone = right_btn_box_cloned.clone();
        let nav_history = nav_history_cloned.clone();
        let sender = sender_cloned.clone();

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
                    albums.sort_by(|a, b| {
                        let a_title = a.title.to_lowercase();
                        let b_title = b.title.to_lowercase();
                        let a_artist = a.artist.to_lowercase();
                        let b_artist = b.artist.to_lowercase();
                        let query = text.to_lowercase();

                        // Scoring function: exact match (0), starts with (1), contains (2), no match (3)
                        let score = |s: &str| {
                            if s == query {
                                0
                            } else if s.starts_with(&query) {
                                1
                            } else if s.contains(&query) {
                                2
                            } else {
                                3
                            }
                        };
                        let a_score = score(&a_title).min(score(&a_artist));
                        let b_score = score(&b_title).min(score(&b_artist));

                        // Primary sort by score, then by artist name (ascending/descending)
                        a_score.cmp(&b_score).then_with(|| {
                            let cmp = a_artist.cmp(&b_artist);
                            if sort_ascending.get() {
                                cmp
                            } else {
                                cmp.reverse()
                            }
                        })
                    });

                    // Update the album UI based on search results
                    if albums.is_empty() {
                        albums_stack.set_visible_child_name("empty_state");
                    } else {
                        albums_stack.set_visible_child_name("populated_grid");
                        for album in albums {
                            let flow_child = create_album_tile(
                                album,
                                cover_size,
                                tile_size,
                                &text,
                                stack_for_closure.clone(),
                                db_pool.clone(),
                                left_btn_stack_for_closure.clone(),
                                nav_history.clone(),
                                sender.clone(),
                            );
                            albums_grid.insert(&*flow_child, -1);
                        }
                    }
                }
            }

            // Perform artist search
            clear_grid(&artists_grid); // Clear artist grid before populating
            match search_artists(&db_pool, &text).await {
                Err(e) => {
                    // Log the error for debugging purposes
                    eprintln!("Error searching artists: {:?}", e);
                }
                Ok(artists) => {
                    // Update the artist UI based on search results
                    if artists.is_empty() {
                        artists_stack.set_visible_child_name("empty_state");
                    } else {
                        artists_stack.set_visible_child_name("populated_grid");
                        for artist in artists {
                            let flow_child = create_artist_tile(
                                artist,
                                cover_size,
                                tile_size,
                                &text,
                                stack_for_closure.clone(),
                                db_pool.clone(),
                                left_btn_stack_for_closure.clone(),
                                right_btn_box_clone.clone(),
                                nav_history.clone(),
                                sender.clone(),
                            );
                            artists_grid.insert(&*flow_child, -1);
                        }
                    }
                }
            }
        });
    });
}

/// Clears all children from a given `FlowBox`.
///
/// This is a utility function to remove all visible items from a GTK `FlowBox`
/// widget, typically used before repopulating it with new content.
///
/// # Arguments
/// * `grid` - A reference to the `FlowBox` to be cleared.
pub fn clear_grid(grid: &FlowBox) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
}
