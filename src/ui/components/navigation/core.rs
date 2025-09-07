use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    sync::Arc,
};

use glib::{MainContext, WeakRef};
use gtk4::{Button, ColumnView, FlowBox};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{ButtonExt, CastNone, ListModelExt, ObjectExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::components::{
    player_bar::PlayerBar, view_controls::list_view::data_model::AlbumListItemObject,
};

use super::{
    VIEW_STACK_ALBUMS, VIEW_STACK_ARTISTS, VIEW_STACK_BACK_HEADER, VIEW_STACK_MAIN_HEADER,
};

/// Encapsulates common logic for navigating back to a main grid view (Albums or Artists).
///
/// This function performs several UI updates:
/// 1. Sets the `left_btn_stack` (header's left button area) back to the main header view.
/// 2. Makes the `right_btn_box` (header's right button area) visible.
/// 3. Triggers a refresh of the library UI, applying the current sort order.
///
/// This reduces code duplication in `handle_esc_navigation` and `connect_tab_navigation`.
///
/// # Arguments
/// * `stack` - The main `ViewStack` of the application. (Currently unused, but kept for consistency if needed later)
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar.
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar.
/// * `refresh_library_ui` - A closure to refresh the main library UI (albums/artists grid).
/// * `sort_ascending` - `Rc<Cell<bool>>` indicating the current sort direction for albums.
/// * `sort_ascending_artists` - `Rc<Cell<bool>>` indicating the current sort direction for artists.
pub fn navigate_back_to_main_grid(
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    refresh_library_ui: &Rc<dyn Fn(bool, bool)>,
    sort_ascending: &Rc<Cell<bool>>,
    sort_ascending_artists: &Rc<Cell<bool>>,
) {
    left_btn_stack.set_visible_child_name(VIEW_STACK_MAIN_HEADER);
    right_btn_box.set_visible(true);

    // Refresh the UI with the current sort settings for albums and artists.
    refresh_library_ui(sort_ascending.get(), sort_ascending_artists.get());
}

/// Connects a handler to the `albums_grid` to manage navigation to the album detail page.
///
/// When an album child in the `FlowBox` is activated (e.g., clicked), this function:
/// 1. Pushes the current visible page onto the `nav_history` stack for back navigation.
/// 2. Changes the header to display the back button (`VIEW_STACK_BACK_HEADER`).
/// 3. Hides the right-side header buttons.
/// 4. Spawns an asynchronous task to build and display the `album_page` for the selected album.
///
/// # Type Parameters
/// * `Fut`: The future type returned by `album_page`.
/// * `F`: The function type for `album_page`, which builds the album detail UI.
///
/// # Arguments
/// * `albums_grid` - The `FlowBox` displaying album tiles.
/// * `stack` - The main `ViewStack` managing application pages.
/// * `db_pool` - The database connection pool.
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar.
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar.
/// * `nav_history` - `Rc<RefCell<Vec<String>>>` storing the history of visited page names.
/// * `sender` - `UnboundedSender<()>` for triggering UI refreshes.
/// * `album_page` - An async function that takes `WeakRef<ViewStack>`, `Arc<SqlitePool>`, `i64` (album ID),
///   `WeakRef<ViewStack>` (header button stack), and `UnboundedSender<()>` and returns a `Future`.
pub fn connect_album_navigation<Fut, F>(
    albums_grid: &FlowBox,
    stack: &ViewStack,
    db_pool: Arc<SqlitePool>,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    album_page: F,
) where
    F: Fn(
            WeakRef<ViewStack>,
            Arc<SqlitePool>,
            i64,
            WeakRef<ViewStack>,
            WeakRef<Clamp>,
            UnboundedSender<()>,
        ) -> Fut
        + 'static,
    Fut: Future<Output = ()> + 'static,
{
    // Downgrade `Rc` references to `WeakRef` for use in closures to prevent reference cycles.
    let stack_weak = stack.downgrade();
    let db_pool_clone = db_pool.clone();
    let left_btn_stack_weak = left_btn_stack.downgrade();
    let right_btn_box_weak = right_btn_box.downgrade();
    let nav_history_clone = nav_history.clone();
    let sender_clone_for_closure = sender.clone();
    albums_grid.connect_child_activated(move |_, child| {
        // Upgrade weak references to strong references or return if they are no longer valid.
        let left_btn_stack = left_btn_stack_weak
            .upgrade()
            .expect("left_btn_stack disappeared");
        let right_btn_box = right_btn_box_weak
            .upgrade()
            .expect("right_btn_box disappeared");

        // Retrieve the `album_id` from the clicked child's data.
        let album_id = child
            .widget_name()
            .parse::<i64>()
            .expect("FlowBoxChild widget name is not a valid i64 album_id");

        // If there's a current visible page, push it onto the navigation history.
        if let Some(current_page) = stack_weak.upgrade().and_then(|s| s.visible_child_name()) {
            nav_history_clone
                .borrow_mut()
                .push(current_page.to_string());
        }

        // Update header visibility for the detail page.
        left_btn_stack.set_visible_child_name(VIEW_STACK_BACK_HEADER);
        right_btn_box.set_visible(false);

        // Spawn an async task to load and display the album detail page.
        MainContext::default().spawn_local(album_page(
            stack_weak.clone(),
            db_pool_clone.clone(),
            album_id,
            left_btn_stack_weak.clone(),
            right_btn_box_weak.clone(),
            sender_clone_for_closure.clone(),
        ));
    });
}

/// Connects a handler to the `artist_grid` to manage navigation to the artist detail page.
///
/// When an artist child in the `FlowBox` is activated (e.g., clicked), this function:
/// 1. Pushes the current visible page onto the `nav_history` stack for back navigation.
/// 2. Changes the header to display the back button (`VIEW_STACK_BACK_HEADER`).
/// 3. Hides the right-side header buttons.
/// 4. Spawns an asynchronous task to build and display the `artist_page` for the selected artist.
///
/// # Type Parameters
/// * `Fut`: The future type returned by `artist_page`.
/// * `F`: The function type for `artist_page`, which builds the artist detail UI.
///
/// # Arguments
/// * `artist_grid` - The `FlowBox` displaying artist tiles.
/// * `stack` - The main `ViewStack` managing application pages.
/// * `db_pool` - The database connection pool.
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar.
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar.
/// * `nav_history` - `Rc<RefCell<Vec<String>>>` storing the history of visited page names.
/// * `sender` - `UnboundedSender<()>` for triggering UI refreshes.
/// * `artist_page` - An async function that takes `WeakRef<ViewStack>`, `Arc<SqlitePool>`, `i64` (artist ID),
///   `WeakRef<ViewStack>`, `WeakRef<Clamp>`, `Rc<RefCell<Vec<String>>>`, `UnboundedSender<()>`, `Rc<Cell<bool>>`,
///   `Rc<Cell<bool>>`, and `PlayerBar` and returns a `Future`.
pub fn connect_artist_navigation<Fut, F>(
    artist_grid: &FlowBox,
    stack: &ViewStack,
    db_pool: Arc<SqlitePool>,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    artist_page: F,
) where
    F: Fn(
            WeakRef<ViewStack>,
            Arc<SqlitePool>,
            i64,
            WeakRef<ViewStack>,
            WeakRef<Clamp>,
            Rc<RefCell<Vec<String>>>,
            UnboundedSender<()>,
            Rc<Cell<bool>>,
            Rc<Cell<bool>>,
            PlayerBar,
        ) -> Fut
        + 'static,
    Fut: Future<Output = ()> + 'static,
{
    // Downgrade `Rc` references to `WeakRef` for use in closures to prevent reference cycles.
    let stack_weak = stack.downgrade();
    let db_pool_clone = db_pool.clone();
    let left_btn_stack_weak = left_btn_stack.downgrade();
    let right_btn_box_weak = right_btn_box.downgrade();
    let nav_history_clone = nav_history.clone();
    let sender_clone_for_closure = sender.clone();
    let show_dr_badges_clone = show_dr_badges.clone();
    let use_original_year_clone = use_original_year.clone();
    let player_bar_clone = player_bar.clone();
    artist_grid.connect_child_activated(move |_, child| {
        // Upgrade weak references to strong references or return if they are no longer valid.
        let left_btn_stack = left_btn_stack_weak
            .upgrade()
            .expect("left_btn_stack disappeared");
        let right_btn_box = right_btn_box_weak
            .upgrade()
            .expect("right_btn_box disappeared");

        // Retrieve the `artist_id` from the clicked child's data.
        let artist_id = child
            .widget_name()
            .parse::<i64>()
            .expect("FlowBoxChild widget name is not a valid i64 artist_id");

        // If there's a current visible page, push it onto the navigation history.
        if let Some(current_page) = stack_weak.upgrade().and_then(|s| s.visible_child_name()) {
            nav_history_clone
                .borrow_mut()
                .push(current_page.to_string());
        }

        // Update header visibility for the detail page.
        left_btn_stack.set_visible_child_name(VIEW_STACK_BACK_HEADER);
        right_btn_box.set_visible(false);

        // Spawn an async task to load and display the artist detail page.
        MainContext::default().spawn_local(artist_page(
            stack_weak.clone(),
            db_pool_clone.clone(),
            artist_id,
            left_btn_stack_weak.clone(),
            right_btn_box_weak.clone(),
            nav_history_clone.clone(),
            sender_clone_for_closure.clone(),
            show_dr_badges_clone.clone(),
            use_original_year_clone.clone(),
            player_bar_clone.clone(),
        ));
    });
}

/// Connects the back button's `clicked` signal to trigger back navigation.
///
/// This function reuses the `handle_back_navigation` closure, ensuring consistent
/// behavior whether the back button is clicked or the Escape key is pressed.
///
/// # Arguments
/// * `back_button` - The GTK `Button` acting as the back button.
/// * `stack` - The main `ViewStack` managing application pages.
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar.
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar.
/// * `last_tab` - `Rc<Cell<&'static str>>` storing the name of the last active main tab.
/// * `nav_history` - `Rc<RefCell<Vec<String>>>` storing the history of visited page names.
/// * `refresh_library_ui` - A closure to refresh the main library UI (albums/artists grid).
/// * `sort_ascending` - `Rc<Cell<bool>>` indicating the current sort direction for albums.
/// * `sort_ascending_artists` - `Rc<Cell<bool>>` indicating the current sort direction for artists.
pub fn connect_back_button(
    back_button: &Button,
    stack: &ViewStack,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    last_tab: Rc<Cell<&'static str>>,
    nav_history: Rc<RefCell<Vec<String>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) {
    // Create the navigation closure that will be executed when the back button is clicked.
    let back_nav_action = handle_back_navigation(
        stack.clone(),
        left_btn_stack.clone(),
        right_btn_box.clone(),
        last_tab.clone(),
        nav_history.clone(),
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
    );

    // Connect the closure to the back button's `clicked` signal.
    back_button.connect_clicked(move |_| {
        back_nav_action();
    });
}

/// Provides a reusable closure for handling back navigation logic,
/// typically triggered by the back button or Escape key.
///
/// This function determines the previous page from `nav_history`. If history is available,
/// it navigates back to the previous page. If history is empty and the current page is
/// not a main grid (albums/artists), it navigates to the `last_tab`.
/// It also handles resetting header visibility and refreshing the UI when returning to a main grid.
///
/// # Arguments
/// * `stack` - The main `ViewStack` managing application pages.
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar.
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar.
/// * `last_tab` - `Rc<Cell<&'static str>>` storing the name of the last active main tab.
/// * `nav_history` - `Rc<RefCell<Vec<String>>>` storing the history of visited page names.
/// * `refresh_library_ui` - A closure to refresh the main library UI (albums/artists grid).
/// * `sort_ascending` - `Rc<Cell<bool>>` indicating the current sort direction for albums.
/// * `sort_ascending_artists` - `Rc<Cell<bool>>` indicating the current sort direction for artists.
///
/// # Returns
/// An `impl Fn()` closure that encapsulates the back navigation logic.
pub fn handle_back_navigation(
    stack: ViewStack,
    left_btn_stack: ViewStack,
    right_btn_box: Clamp,
    last_tab: Rc<Cell<&'static str>>,
    nav_history: Rc<RefCell<Vec<String>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) -> impl Fn() {
    move || {
        // Attempt to pop the previous page from the navigation history.
        if let Some(prev_page) = nav_history.borrow_mut().pop() {
            stack.set_visible_child_name(&prev_page);
            match prev_page.as_str() {
                // The `|` operator creates a pattern that matches either constant.
                VIEW_STACK_ALBUMS | VIEW_STACK_ARTISTS => {
                    // Navigating back to a main grid view, so reset header and refresh UI
                    navigate_back_to_main_grid(
                        &left_btn_stack,
                        &right_btn_box,
                        &refresh_library_ui,
                        &sort_ascending,
                        &sort_ascending_artists,
                    );
                }

                // This arm explicitly does nothing for any other page values.
                _ => {}
            }

            // If not on a main grid, navigate to the last remembered tab and reset header.
        } else {
            // If history is empty, navigate to the last remembered tab and reset header.
            // Get the name of the last active tab.
            let tab = last_tab.get();
            stack.set_visible_child_name(tab);

            // Navigating back to a main grid view, so reset header and refresh UI
            navigate_back_to_main_grid(
                &left_btn_stack,
                &right_btn_box,
                &refresh_library_ui,
                &sort_ascending,
                &sort_ascending_artists,
            );
        }
    }
}

/// Connects album navigation for the list view.
///
/// This function sets up the navigation logic for when a user activates (clicks on) an album
/// in the list view (ColumnView). When an album is activated, it:
/// 1. Saves the current page to navigation history for back navigation
/// 2. Updates the header UI to show the back button and hide other controls
/// 3. Spawns an asynchronous task to load and display the album detail page
///
/// # Type Parameters
/// * `Fut` - The future type returned by the `album_page` function
/// * `F` - The function type for `album_page`, which builds the album detail UI
///
/// # Arguments
/// * `column_view` - The `ColumnView` widget displaying albums in list format
/// * `navigation_stack` - Weak reference to the main `ViewStack` managing application pages
/// * `db_pool` - Shared database connection pool for data access
/// * `left_btn_stack` - The `ViewStack` controlling the left side of the header bar
/// * `right_btn_box` - The `Clamp` widget containing the right side buttons of the header bar
/// * `nav_history` - `Rc<RefCell<Vec<String>>>` storing the history of visited page names
/// * `sender` - `UnboundedSender<()>` for triggering UI refreshes
/// * `show_dr_badges` - `Rc<Cell<bool>>` indicating whether to show DR badges on the album page
/// * `player_bar` - Reference to the application's player bar component
/// * `album_page` - An async function that takes navigation components, database pool,
///   album ID, and other parameters to build and display the album detail page
///
/// # Implementation Details
/// The function uses weak references for UI components to avoid circular reference issues
/// that could prevent proper cleanup. It connects to the `activate` signal of the ColumnView,
/// which is emitted when a user selects an item (single click with single_click_activate(true)).
///
/// When an album is activated:
/// 1. The function retrieves the selected album's ID from the AlbumListItemObject
/// 2. Pushes the current visible page onto the navigation history stack
/// 3. Updates the header to show the back button and hide right-side controls
/// 4. Spawns an async task to load the album detail page using the provided `album_page` function
///
/// # Example
/// ```rust
/// connect_list_view_album_navigation(
///     &column_view,
///     stack.downgrade(),
///     db_pool.clone(),
///     &left_btn_stack,
///     &right_btn_box,
///     nav_history.clone(),
///     sender.clone(),
///     show_dr_badges.clone(),
///     player_bar.clone(),
///     |stack_weak, db_pool, album_id, left_btn_stack_weak, right_btn_box_weak, sender, show_dr_badges, player_bar| {
///         async move {
///             album_page(
///                 stack_weak,
///                 db_pool,
///                 album_id,
///                 left_btn_stack_weak,
///                 right_btn_box_weak,
///                 sender,
///                 show_dr_badges,
///                 player_bar,
///             )
///             .await;
///         }
///     },
/// );
/// ```
pub fn connect_list_view_album_navigation<Fut, F>(
    column_view: &ColumnView,
    navigation_stack: WeakRef<ViewStack>,
    db_pool: Arc<SqlitePool>,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    show_dr_badges: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    album_page: F,
) where
    F: Fn(
            WeakRef<ViewStack>,
            Arc<SqlitePool>,
            i64,
            WeakRef<ViewStack>,
            WeakRef<Clamp>,
            UnboundedSender<()>,
            Rc<Cell<bool>>,
            PlayerBar,
        ) -> Fut
        + 'static,
    Fut: Future<Output = ()> + 'static,
{
    // Downgrade `Rc` references to `WeakRef` for use in closures to prevent reference cycles.
    // This is a common pattern in GTK applications to avoid circular references that
    // could prevent proper memory cleanup.
    let stack_weak = navigation_stack;
    let db_pool_clone = db_pool.clone();
    let left_btn_stack_weak = left_btn_stack.downgrade();
    let right_btn_box_weak = right_btn_box.downgrade();
    let nav_history_clone = nav_history.clone();
    let sender_clone_for_closure = sender.clone();
    let show_dr_badges_clone = show_dr_badges.clone();
    let player_bar_clone = player_bar.clone();

    // Connect to the activate signal of the ColumnView, which is emitted when a user
    // selects an item (single click with single_click_activate(true) enabled)
    column_view.connect_activate(move |column_view, position| {
        // Get the item at the activated position from the ColumnView's model
        // The model contains AlbumListItemObject instances that wrap the album data
        let list_store = column_view.model().unwrap();
        let item = list_store.item(position);

        // Try to cast the generic item to AlbumListItemObject to access album data
        // This downcasting is necessary because the model stores items as generic Objects
        if let Some(album_item) = item.and_downcast::<AlbumListItemObject>() {
            // Extract the album ID from the AlbumListItemObject's wrapped AlbumListItem
            // The album ID is used to fetch the full album details from the database
            let album_id = album_item.item().as_ref().unwrap().id;

            // If there's a current visible page, push it onto the navigation history
            // This enables the back button to navigate to the previous page
            if let Some(current_page) = stack_weak.upgrade().and_then(|s| s.visible_child_name()) {
                nav_history_clone
                    .borrow_mut()
                    .push(current_page.to_string());
            }

            // Update header visibility for the detail page
            // Show the back button and hide the right-side header buttons
            let left_btn_stack = left_btn_stack_weak
                .upgrade()
                .expect("left_btn_stack disappeared");
            let right_btn_box = right_btn_box_weak
                .upgrade()
                .expect("right_btn_box disappeared");
            left_btn_stack.set_visible_child_name(VIEW_STACK_BACK_HEADER);
            right_btn_box.set_visible(false);

            // Spawn an async task to load and display the album detail page
            // This prevents blocking the UI thread during database operations
            // The album_page function is responsible for building the album detail UI
            MainContext::default().spawn_local(album_page(
                stack_weak.clone(),
                db_pool_clone.clone(),
                album_id,
                left_btn_stack_weak.clone(),
                right_btn_box_weak.clone(),
                sender_clone_for_closure.clone(),
                show_dr_badges_clone.clone(),
                player_bar_clone.clone(),
            ));
        }
    });
}
