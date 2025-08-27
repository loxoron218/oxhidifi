use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    sync::Arc,
};

use glib::{MainContext, WeakRef};
use gtk4::{Button, FlowBox};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{ButtonExt, ObjectExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

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
    let sender_clone_for_closure = sender.clone(); // Clone sender for the closure

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

            // If navigating back to a main grid, reset the header and refresh the UI.
            if prev_page.as_str() == VIEW_STACK_ALBUMS || prev_page.as_str() == VIEW_STACK_ARTISTS {
                navigate_back_to_main_grid(
                    &left_btn_stack,
                    &right_btn_box,
                    &refresh_library_ui,
                    &sort_ascending,
                    &sort_ascending_artists,
                );
            }
        } else {
            // If history is empty, check if the current page is already a main grid.
            let current_page = stack.visible_child_name().unwrap_or_default();

            // If not on a main grid, navigate to the last remembered tab and reset header.
            if current_page != VIEW_STACK_ALBUMS && current_page != VIEW_STACK_ARTISTS {
                let tab = last_tab.get(); // Get the name of the last active tab.
                stack.set_visible_child_name(tab);
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
}
