use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    Button, FlowBoxChild, GestureClick,
    glib::{MainContext, prelude::ObjectExt},
};
use libadwaita::{Clamp, ViewStack, prelude::WidgetExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    components::player_bar::PlayerBar, grids::album_grid_state::AlbumGridItem,
    pages::album::album_page::album_page,
};

/// Creates the click gesture for navigating to album page
pub fn create_navigation_gesture(
    flow_child: &FlowBoxChild,
    album: &AlbumGridItem,
    stack_for_closure: Rc<ViewStack>,
    db_pool: Arc<SqlitePool>,
    left_btn_stack_for_closure: Rc<ViewStack>,
    right_btn_box_for_closure: Rc<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    show_dr_badges: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    play_button: &Button,
) {
    let stack_weak = stack_for_closure.downgrade();
    let gesture = GestureClick::builder().build();

    // The `move` keyword captures the needed variables safely.
    let album_id = album.id;
    let play_button_weak = play_button.downgrade();
    gesture.connect_pressed(move |_gesture, _, x, y| {
        // Check if the click was on the play button by checking if the play button is visible
        // and if the coordinates fall within the play button area
        let is_play_button_click = if let Some(play_btn) = play_button_weak.upgrade() {
            if play_btn.is_visible() {
                // Get the play button allocation to check coordinates
                let allocation = play_btn.allocation();
                let play_btn_x = allocation.x() as f64;
                let play_btn_y = allocation.y() as f64;
                let play_btn_width = allocation.width() as f64;
                let play_btn_height = allocation.height() as f64;

                // Check if click coordinates are within play button bounds
                x >= play_btn_x
                    && x <= play_btn_x + play_btn_width
                    && y >= play_btn_y
                    && y <= play_btn_y + play_btn_height
            } else {
                false
            }
        } else {
            false
        };

        // Only navigate to album page if click was not on the play button
        if !is_play_button_click {
            // The album ID is now owned by the closure.
            if let (Some(stack), Some(header_btn_stack)) = (
                stack_weak.upgrade(),
                left_btn_stack_for_closure.downgrade().upgrade(),
            ) {
                // Save current page to navigation history for back navigation
                if let Some(current_page) = stack.visible_child_name() {
                    nav_history.borrow_mut().push(current_page.to_string());
                }

                // Navigate to the album detail page asynchronously
                let db_pool_for_navigation = Arc::clone(&db_pool);
                MainContext::default().spawn_local(album_page(
                    stack.downgrade(),
                    db_pool_for_navigation,
                    album_id,
                    header_btn_stack.downgrade(),
                    right_btn_box_for_closure.downgrade(),
                    sender.clone(),
                    show_dr_badges.clone(),
                    player_bar.clone(),
                ));
            }
        }
    });
    flow_child.add_controller(gesture);
}
