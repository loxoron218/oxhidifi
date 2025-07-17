use std::{rc::Rc, sync::Arc};
use std::cell::{Cell, RefCell};

use glib::{markup_escape_text, MainContext, WeakRef};
use gtk4::{Align, Box, Button, FlowBox, FlowBoxChild, GestureClick, Image, Label, Orientation, PolicyType, ScrolledWindow, SelectionMode, Widget};
use gtk4::pango::{EllipsizeMode, WrapMode};
use libadwaita::{ApplicationWindow, Clamp, StatusPage, ViewStack};
use libadwaita::prelude::{BoxExt, FlowBoxChildExt, IsA, ObjectExt, WidgetExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::data::db::fetch_all_artists;
use crate::data::search::clear_grid;
use crate::ui::components::dialogs::connect_add_folder_dialog;
use crate::ui::pages::artist_page::artist_page;
use crate::utils::screen::{compute_cover_and_tile_size, get_primary_screen_size};

/// Rebuild the artists grid in the main window.
/// Removes the existing 'artists' child, resets the artists_grid_cell, builds a new grid, and adds it to the stack.
pub fn rebuild_artists_grid_for_window(
    stack: &ViewStack,
    scanning_label_artists: &impl IsA<Widget>,
    artists_grid_cell: &Rc<RefCell<Option<FlowBox>>>,
) {

    // Always remove existing "artists" child before adding a new one
    if let Some(child) = stack.child_by_name("artists") {
        stack.remove(&child);
    }
    *artists_grid_cell.borrow_mut() = None;
    let (artists_stack, artists_grid) = build_artists_grid(scanning_label_artists);
    stack.add_titled(&artists_stack, Some("artists"), "Artists");
    *artists_grid_cell.borrow_mut() = Some(artists_grid.clone());
}

/// Build the artists grid and its containing stack.
/// Returns (artists_stack, artists_grid).
pub fn build_artists_grid<W: IsA<Widget>>(scanning_label: &W) -> (Box, FlowBox) {
    use {ScrolledWindow, PolicyType};
    let artists_grid = FlowBox::builder()
        .valign(Align::Start)
        .max_children_per_line(128)
        .row_spacing(1)
        .column_spacing(0)
        .selection_mode(SelectionMode::None)
        .build();
        artists_grid.set_halign(Align::Center);
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Automatic)
        .child(&artists_grid)
        .min_content_height(400)
        .min_content_width(400)
        .vexpand(true)
        .build();
    scrolled.set_hexpand(true);
    scrolled.set_halign(Align::Fill);
    let artists_stack = Box::builder()
        .orientation(Orientation::Vertical)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    artists_stack.append(scanning_label);
    artists_stack.append(&scrolled);
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
) -> FlowBoxChild {
    let (screen_width, _) = get_primary_screen_size();
    let (cover_size, _) = compute_cover_and_tile_size(screen_width);
    let icon = Image::from_icon_name("avatar-default-symbolic");
    icon.set_pixel_size(cover_size);
    let label = Label::builder().label(&*markup_escape_text(artist_name)).build();
    label.set_halign(Align::Start);
    label.set_xalign(0.0);
    label.set_ellipsize(EllipsizeMode::End);
    label.set_wrap(true);
    label.set_wrap_mode(WrapMode::WordChar);
    label.set_lines(2);
    label.set_size_request(cover_size - 16, -1);
    let tile = Box::builder()
        .orientation(Orientation::Vertical)
        .build();
    tile.set_hexpand(true);
    tile.set_halign(Align::Fill);
    tile.append(&icon);
    tile.append(&label);
    let flow_child = FlowBoxChild::builder().build();
    flow_child.set_child(Some(&tile));
    flow_child.set_hexpand(false);
    flow_child.set_vexpand(false);
    flow_child.set_halign(Align::Fill);
    flow_child.set_valign(Align::Start);
    unsafe {
        flow_child.set_data::<i64>("artist_id", artist_id);
    }
    let stack_weak = stack.downgrade();
    let db_pool = Arc::clone(db_pool);
    let left_btn_stack_weak = left_btn_stack.downgrade();
    let right_btn_box_weak_inner = right_btn_box_weak.clone();
    let gesture = GestureClick::builder().build();
    let flow_child_clone = flow_child.clone();
    gesture.connect_pressed(move |_, _, _, _| {
        if let (Some(stack), Some(left_btn_stack)) =
            (stack_weak.upgrade(), left_btn_stack_weak.upgrade())
        {
            let artist_id = unsafe { flow_child_clone.data::<i64>("artist_id").map(|ptr| *ptr.as_ref()).unwrap_or_default() };
            MainContext::default().spawn_local(
                artist_page(
                    stack.downgrade(),
                    db_pool.clone(),
                    artist_id,
                    {
                        let left_btn_stack_weak = WeakRef::<ViewStack>::new();
                        left_btn_stack_weak.set(Some(&left_btn_stack));
                        left_btn_stack_weak
                    },
                    right_btn_box_weak_inner.clone(),
                ),
            );
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
    window: &ApplicationWindow,
    scanning_label: &Label,
    sender: &UnboundedSender<()>,
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
    let db_pool = Arc::clone(&db_pool);
    let window = window.clone();
    let scanning_label = scanning_label.clone();
    let sender = sender.clone();

    // Clear all children before repopulating to avoid duplicates
    clear_grid(&artists_grid);
    MainContext::default().spawn_local(async move {
        let fetch_result = fetch_all_artists(&db_pool).await;
        match fetch_result {
            Err(_) => BUSY.with(|b| b.set(false)),
            Ok(mut artists) => {
                if artists.is_empty() {
                    let status_page = StatusPage::builder()
                        .icon_name("avatar-default-symbolic")
                        .title("No Artists Found")
                        .description("Add music to your library to get started.")
                        .vexpand(true)
                        .hexpand(true)
                        .build();
                    let add_music_button = Button::with_label("Add Music");
                    add_music_button.add_css_class("suggested-action");
                    connect_add_folder_dialog(
                        &add_music_button,
                        window,
                        scanning_label,
                        db_pool.clone(),
                        sender,
                    );
                    status_page.set_child(Some(&add_music_button));
                    artists_grid.insert(&status_page, -1);
                    BUSY.with(|b| b.set(false));
                    return;
                }
                artists.sort_by(|a, b| {
                    let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                    if sort_ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                });
                BUSY.with(|b| b.set(false));
                for artist in artists {
                    let tile = create_artist_tile(
                        artist.id,
                        &artist.name,
                        &stack,
                        &db_pool,
                        &left_btn_stack,
                        &right_btn_box_weak,
                    );
                    artists_grid.insert(&tile, -1);
                }
            }
        }
    });
}
