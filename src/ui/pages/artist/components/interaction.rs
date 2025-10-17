use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    Button, FlowBoxChild, GestureClick,
    glib::{MainContext, WeakRef, clone::Downgrade},
    graphene::Point,
};
use libadwaita::{Clamp, ViewStack, prelude::WidgetExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::ui::{
    components::player_bar::PlayerBar,
    pages::{album::album_page::album_page, artist::data::artist_data::AlbumDisplayInfoWithYear},
};

/// Sets up the click gesture for navigation to album page
pub fn setup_click_gesture(
    flow_child: &FlowBoxChild,
    album: &AlbumDisplayInfoWithYear,
    _cover_size: i32,
    stack: WeakRef<ViewStack>,
    db_pool: Arc<SqlitePool>,
    header_btn_stack: WeakRef<ViewStack>,
    right_btn_box: WeakRef<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    artist_page_name: String,
    show_dr_badges: Rc<Cell<bool>>,
    player_bar: PlayerBar,
    play_button: Button,
) {
    let stack_weak = stack.clone();
    let header_btn_stack_weak = header_btn_stack.clone();
    let right_btn_box_weak = right_btn_box.clone();
    let sender_clone = sender.clone();
    let album_id = album.id;
    let play_button_weak = play_button.downgrade();
    let flow_child_weak = flow_child.downgrade();
    let gesture = GestureClick::builder().build();
    gesture.connect_pressed(move |_gesture, _, x, y| {
        // Check if the click was on the play button by checking if the play button is visible
        // and if the coordinates fall within the play button area
        let is_play_button_click = if let Some(play_btn) = play_button_weak.upgrade() {
            if play_btn.is_visible() {
                // The click coordinates (x, y) are relative to the gesture widget (flow_child)
                // We need to get the play button's position in the same coordinate system
                if let Some(flow_child_strong) = flow_child_weak.upgrade() {
                    let btn_point = Point::new(0.0, 0.0);
                    if let Some(btn_pos_in_flow) =
                        play_btn.compute_point(&flow_child_strong, &btn_point)
                    {
                        let play_btn_x = btn_pos_in_flow.x() as f64;
                        let play_btn_y = btn_pos_in_flow.y() as f64;
                        let play_btn_width = play_btn.width() as f64;
                        let play_btn_height = play_btn.height() as f64;

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
                }
            } else {
                false
            }
        } else {
            false
        };

        // Only navigate to album page if click was not on the play button
        if !is_play_button_click {
            // Navigate to album page when clicked
            if let (Some(stack), Some(header_btn_stack)) =
                (stack_weak.upgrade(), header_btn_stack_weak.upgrade())
            {
                // Add current page to navigation history
                nav_history.borrow_mut().push(artist_page_name.clone());

                // Spawn async task to load album page
                let db_pool_for_navigation = Arc::clone(&db_pool);
                MainContext::default().spawn_local(album_page(
                    stack.downgrade(),
                    db_pool_for_navigation,
                    album_id,
                    header_btn_stack.downgrade(),
                    right_btn_box_weak.clone(),
                    sender_clone.clone(),
                    show_dr_badges.clone(),
                    player_bar.clone(),
                ));
            }
        }
    });
    flow_child.add_controller(gesture);
}
