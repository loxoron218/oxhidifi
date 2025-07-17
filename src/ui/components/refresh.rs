use std::{rc::Rc, sync::Arc, time::Duration};
use std::cell::{Cell, RefCell};
use glib::{ControlFlow::Continue, MainContext};
use gtk4::{FlowBox, Label, Stack};
use gtk4::glib::source::timeout_add_local;
use libadwaita::{ApplicationWindow, Clamp, ViewStack};
use sqlx::SqlitePool;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::ui::components::sorting::SortOrder;
use crate::ui::grids::albums_grid::populate_albums_grid;
use crate::ui::grids::artists_grid::populate_artists_grid;
use crate::utils::screen::{compute_cover_and_tile_size, get_primary_screen_size};

/// Returns a tuple with (sender, receiver, refresh_library_ui) for library refresh logic.
pub fn setup_library_refresh_channel(
    db_pool: Arc<SqlitePool>,
    albums_grid_cell: Rc<RefCell<Option<FlowBox>>>,
    albums_stack_cell: Rc<RefCell<Option<Stack>>>,
    artists_grid_cell: Rc<RefCell<Option<FlowBox>>>,
    artists_stack_cell: Rc<RefCell<Option<Stack>>>,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    stack_rc: Rc<ViewStack>,
    left_btn_stack_rc: Rc<ViewStack>,
    right_btn_box: Clamp,
    cover_size_rc: Rc<Cell<i32>>,
    tile_size_rc: Rc<Cell<i32>>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    window: ApplicationWindow,
    scanning_label_albums: Label,
    scanning_label_artists: Label,
    stack: Rc<ViewStack>,
    header_btn_stack: Rc<ViewStack>,
    nav_history: Rc<RefCell<Vec<String>>>,
) -> (
    UnboundedSender<()>, 
    UnboundedReceiver<()>, 
    Rc<dyn Fn(bool, bool)>
) {
    let (sender, receiver) = unbounded_channel::<()>();
    let sort_orders_for_refresh = sort_orders.clone();
    let refresh_library_ui: Rc<dyn Fn(bool, bool)> = {
        let db_pool = db_pool.clone();
        let sort_orders = sort_orders_for_refresh.clone();
        let stack_rc = stack_rc.clone();
        let left_btn_stack_rc = left_btn_stack_rc.clone();
        let right_btn_box_clone = right_btn_box.clone();
        let cover_size_rc = cover_size_rc.clone();
        let tile_size_rc = tile_size_rc.clone();
        let albums_grid_cell = albums_grid_cell.clone();
        let albums_stack_cell = albums_stack_cell.clone();
        let artists_grid_cell = artists_grid_cell.clone();
        let artists_stack_cell = artists_stack_cell.clone();
        let window = window.clone();
        let scanning_label_albums = scanning_label_albums.clone();
        let scanning_label_artists = scanning_label_artists.clone();
        let sender_clone = sender.clone();
            let stack_clone = stack.clone();
            let header_btn_stack_clone = header_btn_stack.clone();
            let nav_history_clone = nav_history.clone();
            Rc::new(move |sort_ascending: bool, sort_ascending_artists: bool| {
                let db_pool = db_pool.clone();
                let sort_orders = sort_orders.clone();
                let stack_rc = stack_rc.clone();
                let left_btn_stack_rc = left_btn_stack_rc.clone();
                let right_btn_box_clone = right_btn_box_clone.clone();
                let albums_grid_cell = albums_grid_cell.clone();
                let albums_stack_cell = albums_stack_cell.clone();
                let artists_grid_cell = artists_grid_cell.clone();
                let artists_stack_cell = artists_stack_cell.clone();
                let cover_size_rc = cover_size_rc.clone();
                let tile_size_rc = tile_size_rc.clone();
                let window = window.clone();
                let scanning_label_albums = scanning_label_albums.clone();
                let scanning_label_artists = scanning_label_artists.clone();
                let sender = sender_clone.clone();
                let stack = stack_clone.clone();
                let header_btn_stack = header_btn_stack_clone.clone();
                let nav_history = nav_history_clone.clone();
                MainContext::default().spawn_local(async move {
                    let current_tab = stack_rc.visible_child_name().unwrap_or_else(|| "albums".into());
                    if current_tab == "albums" {
                        if let (Some(albums_grid), Some(albums_inner_stack)) = (albums_grid_cell.borrow().as_ref(), albums_stack_cell.borrow().as_ref()) {
                            populate_albums_grid(
                                albums_grid,
                                db_pool.clone(),
                                sort_ascending,
                                sort_orders.clone(),
                                cover_size_rc.get(),
                                tile_size_rc.get(),
                                &window,
                                &scanning_label_albums,
                                &sender,
                                &stack,
                                &header_btn_stack,
                                albums_inner_stack,
                            )
                            .await;
                        }
                    } else if current_tab == "artists" {
                    if let (Some(artists_grid), Some(artists_inner_stack)) = (artists_grid_cell.borrow().as_ref(), artists_stack_cell.borrow().as_ref()) {
                        populate_artists_grid(
                            artists_grid,
                            db_pool.clone(),
                            sort_ascending_artists,
                            &stack_rc,
                            &left_btn_stack_rc,
                            &right_btn_box_clone,
                            &window,
                            &scanning_label_artists,
                            &sender,
                            nav_history.clone(),
                            artists_inner_stack,
                        );
                    }
                }
            });
        })
    };
    refresh_library_ui(sort_ascending.get(), sort_ascending_artists.get());
    (sender, receiver, refresh_library_ui)
}

/// Sets up a periodic refresh of the library UI when the monitor geometry changes.
pub fn setup_live_monitor_refresh(
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    cover_size_rc: Rc<Cell<i32>>,
    tile_size_rc: Rc<Cell<i32>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    screen_width: i32,
) {
    let last_width = Rc::new(Cell::new(screen_width));
    timeout_add_local(Duration::from_secs(1), move || {
        let (cur_width, _) = get_primary_screen_size();
        if cur_width != last_width.get() {
            let (new_cover_size, new_tile_size) = compute_cover_and_tile_size(cur_width);
            cover_size_rc.set(new_cover_size);
            tile_size_rc.set(new_tile_size);
            last_width.set(cur_width);
            refresh_library_ui(sort_ascending.get(), sort_ascending_artists.get());
        }
        Continue
    });
}