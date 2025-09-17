use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gtk4::{
    Entry, Stack,
    gio::ListStore,
    glib::{MainContext, SourceId},
};
use libadwaita::{Clamp, ViewStack, prelude::EditableExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    data::db::query::{search_album_display_info, search_artists},
    ui::{
        components::{
            player_bar::PlayerBar,
            view_controls::list_view::data_model::{AlbumListItem, AlbumListItemObject},
        },
        search::search_utils::{debounce_search, sort_albums_by_relevance},
    },
};

/// Connects live search logic to the given search entry, updating albums and artists lists as the user types.
///
/// This function sets up a `changed` signal handler for the `search_entry`.
/// When the search query changes, it clears the existing lists, performs
/// asynchronous database searches for albums and artists, and then populates
/// the respective `ListStore` models with new data based on the search results.
/// It also handles the display of empty states if no results are found.
///
/// # Arguments
/// * `search_entry` - The GTK Entry widget for user input
/// * `albums_model` - The ListStore model for album data in ListView mode
/// * `albums_stack` - The Stack widget for managing album view states
/// * `artists_model` - The ListStore model for artist data in ListView mode
/// * `artists_stack` - The Stack widget for managing artist view states
/// * `db_pool` - Database connection pool
/// * `sort_ascending` - Cell for album sort direction
/// * `sort_ascending_artists` - Cell for artist sort direction
/// * `refresh_library_ui` - Closure for refreshing the library UI
/// * `stack` - The main ViewStack for navigation
/// * `left_btn_stack` - The left button ViewStack
/// * `right_btn_box` - The right button container
/// * `nav_history` - Navigation history
/// * `sender` - Channel sender for communication
/// * `show_dr_badges` - Cell for DR badge visibility
/// * `use_original_year` - Cell for original year usage
/// * `player_bar` - The player bar component
pub fn connect_live_search_list_view(
    search_entry: &Entry,
    albums_model: ListStore,
    albums_stack: &Stack,
    artists_model: ListStore,
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
    // Clone shared resources for the closure to avoid moving them into the closure
    // and allow them to be used across multiple async operations.
    let db_pool_cloned = db_pool.clone();
    let albums_model_cloned = albums_model.clone();
    let albums_stack_cloned = albums_stack.clone();
    let artists_model_cloned = artists_model.clone();
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
        let albums_model_cloned = albums_model_cloned.clone();
        let albums_stack_cloned = albums_stack_cloned.clone();
        let artists_model_cloned = artists_model_cloned.clone();
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
        let _player_bar_clone_inner = player_bar.clone();
        debounce_search(
            &search_timer_cloned,
            Duration::from_millis(300),
            move || {
                // Clear models before performing a new search
                albums_model_cloned.remove_all();
                artists_model_cloned.remove_all();

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
                let albums_model = albums_model_cloned.clone();
                let albums_stack = albums_stack_cloned.clone();
                let _artists_model = artists_model_cloned.clone();
                let artists_stack = artists_stack_cloned.clone();
                let sort_ascending = sort_ascending_cloned.clone();
                let _stack_for_closure = stack_cloned.clone();
                let _left_btn_stack_for_closure = left_btn_stack_cloned.clone();
                let _right_btn_box_clone = right_btn_box_cloned.clone();
                let _nav_history = nav_history_cloned.clone();
                let _sender = sender_cloned.clone();
                let _show_dr_badges = show_dr_badges_cloned.clone();
                let _use_original_year = use_original_year_cloned.clone();

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

                            // Populate the ListStore with search results
                            if !albums.is_empty() {
                                for album in &albums {
                                    // Create the album list item
                                    let album_list_item = AlbumListItem::new(
                                        album.id,
                                        album.title.clone(),
                                        album.artist.clone(),
                                        album.cover_art.clone(),
                                        album.year,
                                        album.original_release_date.clone(),
                                        album.dr_value,
                                        album.dr_is_best,
                                        album.format.clone(),
                                        album.bit_depth,
                                        album.sample_rate,
                                        album.folder_path.clone(),
                                    );

                                    // Create an AlbumListItemObject wrapper for the album data and append it to the column view model.
                                    let album_object = AlbumListItemObject::new(album_list_item);
                                    albums_model.append(&album_object);
                                }
                            }
                        }
                    }

                    // Perform artist search
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

                            // For artists, we don't populate the ListStore directly as it's handled differently
                            // The artists search in ListView mode would need a different approach
                            // For now, we'll just set the stack state
                        }
                    }
                });
            },
        );
    });
}
