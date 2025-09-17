use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    ColumnView,
    glib::{MainContext, WeakRef, clone::Downgrade},
    prelude::{CastNone, ListModelExt, WidgetExt},
};
use libadwaita::{Clamp, ViewStack};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::components::{
    player_bar::PlayerBar, view_controls::list_view::data_model::AlbumListItemObject,
};

use super::VIEW_STACK_BACK_HEADER;

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
    let left_btn_stack_weak = Downgrade::downgrade(left_btn_stack);
    let right_btn_box_weak = Downgrade::downgrade(right_btn_box);
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
