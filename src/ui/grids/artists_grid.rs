use std::cell::{Cell, RefCell};
use std::{rc::Rc, sync::Arc};

use glib::{MainContext, WeakRef};
use gtk4::pango::{EllipsizeMode, WrapMode};
use gtk4::{
    Align, Box, Button, FlowBox, FlowBoxChild, GestureClick, Image, Label, Orientation, PolicyType,
    ScrolledWindow, SelectionMode, Spinner, Stack, StackTransitionType,
};
use libadwaita::prelude::{BoxExt, FlowBoxChildExt, ObjectExt, WidgetExt};
use libadwaita::{ApplicationWindow, Clamp, StatusPage, ViewStack};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::data::db::db_query::fetch_all_artists;
use crate::ui::components::scan_feedback::create_scanning_label;
use crate::ui::pages::artist_page::artist_page;
use crate::utils::screen::{compute_cover_and_tile_size, get_primary_screen_size};

/// Rebuild the artists grid in the main window.
/// Removes the existing 'artists' child, resets the artists_grid_cell, builds a new grid, and adds it to the stack.
pub fn rebuild_artists_grid_for_window(
    stack: &ViewStack,
    scanning_label_artists: &Label,
    artists_grid_cell: &Rc<RefCell<Option<FlowBox>>>,
    artists_stack_cell: &Rc<RefCell<Option<Stack>>>,
    _sender: UnboundedSender<()>,
    _add_music_button: &Button,
) {
    // Always remove existing "artists" child before adding a new one
    if let Some(child) = stack.child_by_name("artists") {
        stack.remove(&child);
    }
    *artists_grid_cell.borrow_mut() = None;
    *artists_stack_cell.borrow_mut() = None;
    let (artists_stack, artists_grid) =
        build_artists_grid(scanning_label_artists, _add_music_button);
    stack.add_titled(&artists_stack, Some("artists"), "Artists");
    *artists_grid_cell.borrow_mut() = Some(artists_grid.clone());
    *artists_stack_cell.borrow_mut() = Some(artists_stack.clone());
}

/// Build the artists grid and its containing stack.
/// Returns (artists_stack, artists_grid).
pub fn build_artists_grid(scanning_label: &Label, add_music_button: &Button) -> (Stack, FlowBox) {
    // Empty state
    let empty_state_status_page = StatusPage::builder()
        .icon_name("avatar-default-symbolic")
        .title("No Artists Found")
        .description("Add music to your library to get started.")
        .vexpand(true)
        .hexpand(true)
        .build();

    // The add_music_button is now passed in, not created here
    add_music_button.add_css_class("suggested-action");
    empty_state_status_page.set_child(Some(add_music_button));
    let empty_state_container = Box::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Center)
        .valign(Align::Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    empty_state_container.append(&empty_state_status_page);

    // Artists grid
    let artists_grid = FlowBox::builder()
        .valign(Align::Start)
        .max_children_per_line(128)
        .row_spacing(8)
        .column_spacing(8)
        .selection_mode(SelectionMode::None)
        .homogeneous(true)
        .build();
    artists_grid.set_halign(Align::Center);
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Automatic)
        .child(&artists_grid)
        .min_content_height(400)
        .min_content_width(400)
        .vexpand(true)
        .margin_start(24)
        .margin_end(24)
        .margin_top(24)
        .margin_bottom(24)
        .build();
    scrolled.set_hexpand(true);
    scrolled.set_halign(Align::Fill);
    let artists_stack = Stack::builder()
        .transition_type(StackTransitionType::None)
        .build();

    // Loading state
    let loading_spinner = Spinner::builder().spinning(true).build();
    loading_spinner.set_size_request(48, 48);
    let loading_state_container = Box::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Center)
        .valign(Align::Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    loading_state_container.append(&loading_spinner);
    let scanning_label_widget = create_scanning_label();
    loading_state_container.append(&scanning_label_widget);
    scanning_label_widget.set_visible(true);

    // Scanning state
    let scanning_spinner = Spinner::builder().spinning(true).build();
    scanning_spinner.set_size_request(48, 48);
    let scanning_state_container = Box::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Center)
        .valign(Align::Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    scanning_state_container.append(&scanning_spinner);
    scanning_state_container.append(scanning_label);
    artists_stack.add_named(&loading_state_container, Some("loading_state"));
    artists_stack.add_named(&empty_state_container, Some("empty_state"));
    artists_stack.add_named(&scanning_state_container, Some("scanning_state"));
    let artists_content_box = Box::builder().orientation(Orientation::Vertical).build();
    artists_content_box.append(&scrolled);
    artists_stack.add_named(&artists_content_box, Some("populated_grid"));
    artists_stack.set_visible_child_name("loading_state"); // Set initial state to loading
    (artists_stack, artists_grid)
}

/// Populate the given artists grid with artist tiles, clearing and sorting as needed.
/// Helper to create an artist tile widget with gesture and signal connection.
fn create_artist_tile(
    artist_id: i64,
    artist_name: &str,
    stack: &ViewStack,
    db_pool: &Arc<SqlitePool>,
    left_btn_stack: &ViewStack,
    right_btn_box_weak: &WeakRef<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
) -> FlowBoxChild {
    let (screen_width, _) = get_primary_screen_size();
    let (cover_size, _) = compute_cover_and_tile_size(screen_width);
    let icon = Image::from_icon_name("avatar-default-symbolic");
    icon.set_pixel_size(cover_size);
    let label = Label::builder().label(artist_name).build();
    label.set_halign(Align::Start);
    label.set_xalign(0.0);
    label.set_ellipsize(EllipsizeMode::End);
    label.set_wrap(true);
    label.set_wrap_mode(WrapMode::WordChar);
    label.set_lines(2);
    label.set_size_request(cover_size - 16, -1);
    let tile = Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .build();

    // tile_size + room for text
    tile.set_size_request(cover_size, cover_size + 80);
    tile.set_hexpand(false);
    tile.set_vexpand(false);
    tile.set_halign(Align::Start);
    tile.set_valign(Align::Start);

    // Fixed-size container for icon (new instance per tile)
    let icon_container = Box::new(Orientation::Vertical, 0);
    icon_container.set_size_request(cover_size, cover_size);
    icon_container.set_halign(Align::Start);
    icon_container.set_valign(Align::Start);
    icon_container.append(&icon);
    tile.append(&icon_container);

    // Box to ensure consistent height for the label area (2 lines)
    let label_area_box = Box::builder()
        .orientation(Orientation::Vertical)
        .height_request(40)
        .margin_top(12)
        .build();
    label.set_valign(Align::End);
    label_area_box.append(&label);
    tile.append(&label_area_box);
    tile.set_css_classes(&["artist-tile"]);
    let flow_child = FlowBoxChild::builder().build();
    flow_child.set_child(Some(&tile));
    flow_child.set_hexpand(false);
    flow_child.set_vexpand(false);
    flow_child.set_halign(Align::Center);
    flow_child.set_valign(Align::Start);
    unsafe {
        flow_child.set_data::<i64>("artist_id", artist_id);
    }
    let stack_weak = stack.downgrade();
    let db_pool = Arc::clone(db_pool);
    let left_btn_stack_weak = left_btn_stack.downgrade();
    let right_btn_box_weak_inner = right_btn_box_weak.clone();
    let nav_history = nav_history.clone();
    let gesture = GestureClick::builder().build();
    let gesture_for_closure = gesture.clone();
    let flow_child_clone = flow_child.clone();
    gesture_for_closure.connect_pressed(move |_, _, _, _| {
        if let (Some(stack), Some(left_btn_stack)) =
            (stack_weak.upgrade(), left_btn_stack_weak.upgrade())
        {
            let artist_id = unsafe {
                flow_child_clone
                    .data::<i64>("artist_id")
                    .map(|ptr| *ptr.as_ref())
                    .unwrap_or_default()
            };
            if let Some(current_page) = stack.visible_child_name() {
                nav_history.borrow_mut().push(current_page.to_string());
            }
            MainContext::default().spawn_local(artist_page(
                stack.downgrade(),
                db_pool.clone(),
                artist_id,
                {
                    let left_btn_stack_weak = WeakRef::<ViewStack>::new();
                    left_btn_stack_weak.set(Some(&left_btn_stack));
                    left_btn_stack_weak
                },
                right_btn_box_weak_inner.clone(),
                nav_history.clone(),
                sender.clone(),
            ));
        }
    });
    flow_child.add_controller(gesture);
    flow_child
}

/// Populate the given artists grid with artist tiles, clearing and sorting as needed.
pub fn populate_artists_grid(
    artists_grid: &FlowBox,
    db_pool: Arc<SqlitePool>,
    sort_ascending: bool,
    stack: &ViewStack,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    _window: &ApplicationWindow,
    scanning_label: &Label,
    sender: UnboundedSender<()>,
    nav_history: Rc<RefCell<Vec<String>>>,
    artists_inner_stack: &Stack,
) {
    thread_local! {
        static BUSY: Cell<bool> = Cell::new(false);
    }
    let already_busy = BUSY.with(|b| {
        if b.get() {
            true
        } else {
            b.set(true);
            false
        }
    });
    if already_busy {
        return;
    }
    let stack = stack.clone();
    let left_btn_stack = left_btn_stack.clone();
    let right_btn_box_weak = right_btn_box.downgrade();
    let artists_grid = artists_grid.clone();
    let artists_inner_stack = artists_inner_stack.clone();
    let db_pool = Arc::clone(&db_pool);
    let scanning_label = scanning_label.clone();
    let sender = sender.clone();
    MainContext::default().spawn_local(async move {
        let fetch_result = fetch_all_artists(&db_pool).await;
        match fetch_result {
            Err(_) => {
                BUSY.with(|b| b.set(false));

                // In case of error, show empty state or a specific error state
                artists_inner_stack.set_visible_child_name("empty_state");
            }
            Ok(mut artists) => {
                if artists.is_empty() {
                    if scanning_label.is_visible() {
                        artists_inner_stack.set_visible_child_name("scanning_state");
                    } else {
                        artists_inner_stack.set_visible_child_name("empty_state");
                    }
                    BUSY.with(|b| b.set(false));
                    return;
                }
                artists_inner_stack.set_visible_child_name("populated_grid");

                // Filter out "Various Artists"
                artists.retain(|artist| artist.name != "Various Artists");
                artists.sort_by(|a, b| {
                    let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                    if sort_ascending { cmp } else { cmp.reverse() }
                });
                for artist in artists {
                    let tile = create_artist_tile(
                        artist.id,
                        &artist.name,
                        &stack,
                        &db_pool,
                        &left_btn_stack,
                        &right_btn_box_weak,
                        nav_history.clone(),
                        sender.clone(),
                    );
                    artists_grid.insert(&tile, -1);
                }
                artists_inner_stack.set_visible_child_name("populated_grid");
            }
        }
        BUSY.with(|b| b.set(false));
    });
}
