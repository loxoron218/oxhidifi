use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    sync::Arc,
};

use glib::{MainContext, WeakRef, clone::Downgrade};
use gtk4::{FlowBox, prelude::WidgetExt};
use libadwaita::{Clamp, ViewStack};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::components::player_bar::PlayerBar;

use super::VIEW_STACK_BACK_HEADER;

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
///   `WeakRef<ViewStack>`, `WeakRef<Clamp>`, and `UnboundedSender<()>` and returns a `Future`.
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
    let stack_weak = Downgrade::downgrade(stack);
    let db_pool_clone = db_pool.clone();
    let left_btn_stack_weak = Downgrade::downgrade(left_btn_stack);
    let right_btn_box_weak = Downgrade::downgrade(right_btn_box);
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
    let stack_weak = Downgrade::downgrade(stack);
    let db_pool_clone = db_pool.clone();
    let left_btn_stack_weak = Downgrade::downgrade(left_btn_stack);
    let right_btn_box_weak = Downgrade::downgrade(right_btn_box);
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
