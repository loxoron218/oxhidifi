use std::{
    cell::{Cell, RefCell},
    cmp::Ordering,
    rc::Rc,
    sync::Arc,
};

use gdk_pixbuf::{InterpType, PixbufLoader};
use glib::{ControlFlow, MainContext, timeout_add_local};
use gtk4::{
    Align, Box, Button, EventControllerMotion, Fixed, FlowBox, FlowBoxChild, GestureClick, Label,
    Orientation, Overlay, Picture, PolicyType, ScrolledWindow, SelectionMode, Spinner, Stack,
    StackTransitionType,
    pango::{EllipsizeMode, WrapMode},
};
use libadwaita::{
    ApplicationWindow, StatusPage, ViewStack,
    prelude::{BoxExt, FixedExt, FlowBoxChildExt, ObjectExt, PixbufLoaderExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    data::db::query::fetch_album_display_info,
    ui::{
        components::{scan_feedback::create_scanning_label, sorting::sorting_types::SortOrder},
        pages::album_page::album_page,
    },
    utils::{
        best_dr_persistence::{AlbumKey, DrValueStore},
        formatting::format_freq_khz,
        screen::ScreenInfo,
    },
};

/// Helper to create a styled label for album metadata.
fn create_album_label(
    text: &str,
    css_classes: &[&str],
    max_width: Option<i32>,
    ellipsize: Option<EllipsizeMode>,
    wrap: bool,
    wrap_mode: Option<WrapMode>,
    lines: Option<i32>,
) -> Label {
    let builder = Label::builder().label(text).halign(Align::Start);
    let label = builder.build();
    label.set_xalign(0.0);
    if let Some(width) = max_width {
        label.set_max_width_chars(width);
    }
    if let Some(mode) = ellipsize {
        label.set_ellipsize(mode);
    }
    if wrap {
        label.set_wrap(true);
    }
    if let Some(mode) = wrap_mode {
        label.set_wrap_mode(mode);
    }
    if let Some(l) = lines {
        label.set_lines(l);
    }
    for class in css_classes {
        label.add_css_class(class);
    }
    label
}

/// Helper to create the album cover as a Picture widget.
fn create_album_cover(cover_art: Option<&Vec<u8>>, cover_size: i32) -> Picture {
    if let Some(art) = cover_art {
        let loader = PixbufLoader::new();
        loader.write(art).expect("Failed to load cover art");
        loader.close().expect("Failed to close loader");
        let pixbuf = loader.pixbuf().expect("No pixbuf loaded");
        let (w, h) = (pixbuf.width(), pixbuf.height());
        let side = w.min(h);
        let cropped = pixbuf.new_subpixbuf((w - side) / 2, (h - side) / 2, side, side);
        let scaled = cropped
            .scale_simple(cover_size, cover_size, InterpType::Bilinear)
            .unwrap();
        let picture = Picture::for_pixbuf(&scaled);
        picture.set_size_request(cover_size, cover_size);
        picture.set_halign(Align::Start);
        picture.set_valign(Align::Start);
        picture.add_css_class("album-cover-border");
        picture
    } else {
        let pic = Picture::new();
        pic.set_size_request(cover_size, cover_size);
        pic.set_halign(Align::Start);
        pic.set_valign(Align::Start);
        pic.add_css_class("album-cover-border");
        pic
    }
}

/// Helper to create the DR badge overlay if present.
fn create_dr_overlay(dr_value: Option<u8>, dr_completed: bool) -> Option<Label> {
    let (dr_str, tooltip_text, mut css_classes) = match dr_value {
        Some(value) => (
            format!("{:02}", value),
            Some("Official Dynamic Range Value"),
            vec![format!("dr-{:02}", value)],
        ),
        None => (
            "N/A".to_string(),
            Some("Dynamic Range Value not available"),
            vec!["dr-na".to_string()],
        ),
    };
    let dr_label = Label::builder().label(&dr_str).build();
    dr_label.add_css_class("dr-badge-label");
    dr_label.add_css_class("dr-badge-label-grid");
    dr_label.set_size_request(28, 28);
    if dr_completed {
        css_classes.push("dr-completed".to_string());
    }
    for class in css_classes {
        dr_label.add_css_class(&class);
    }
    dr_label.set_tooltip_text(tooltip_text);
    dr_label.set_halign(Align::End);
    dr_label.set_valign(Align::End);
    Some(dr_label)
}

/// Rebuild the albums grid in the main window.
/// Removes the existing 'albums' child, resets the albums_grid_cell, builds a new grid, and adds it to the stack.
pub fn rebuild_albums_grid_for_window(
    stack: &ViewStack,
    scanning_label_albums: &Label,
    screen_info: &Rc<RefCell<ScreenInfo>>,
    albums_grid_cell: &Rc<RefCell<Option<FlowBox>>>,
    albums_stack_cell: &Rc<RefCell<Option<Stack>>>,
    _add_music_button: &Button, // Prefix with _ to mark as intentionally unused
) {
    // Remove old grid widget if present
    if let Some(child) = stack.child_by_name("albums") {
        stack.remove(&child);
    }

    // Drop old FlowBox and Stack
    *albums_grid_cell.borrow_mut() = None;
    *albums_stack_cell.borrow_mut() = None;

    // Create new grid and stack
    let (albums_stack, albums_grid) = build_albums_grid(
        scanning_label_albums,
        screen_info.borrow().cover_size,
        screen_info.borrow().tile_size,
        _add_music_button, // Use the passed button
    );
    stack.add_titled(&albums_stack, Some("albums"), "Albums");
    *albums_grid_cell.borrow_mut() = Some(albums_grid.clone());
    *albums_stack_cell.borrow_mut() = Some(albums_stack.clone());
}

/// Build the albums grid and its containing stack.
/// Returns (albums_stack, albums_grid).
pub fn build_albums_grid(
    scanning_label: &Label,
    _cover_size: i32,
    _tile_size: i32,
    add_music_button: &Button, // Add this parameter
) -> (Stack, FlowBox) {
    // Empty state
    let empty_state_status_page = StatusPage::builder()
        .icon_name("folder-music-symbolic")
        .title("No Music Found")
        .description("Add music to your library to get started.")
        .vexpand(true)
        .hexpand(true)
        .build();

    // The add_music_button is now passed in, not created here
    add_music_button.add_css_class("suggested-action");
    empty_state_status_page.set_child(Some(add_music_button)); // Use the passed button
    let empty_state_container = Box::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Center)
        .valign(Align::Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    empty_state_container.append(&empty_state_status_page);

    // No results state
    let no_results_status_page = StatusPage::builder()
        .icon_name("system-search-symbolic")
        .title("No Albums Found")
        .description("Try a different search query.")
        .vexpand(true)
        .hexpand(true)
        .build();
    let no_results_container = Box::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Center)
        .valign(Align::Center)
        .vexpand(true)
        .hexpand(true)
        .build();
    no_results_container.append(&no_results_status_page);

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

    // Albums grid
    let albums_grid = FlowBox::builder()
        .valign(Align::Start)
        .max_children_per_line(128)
        .row_spacing(8)
        .column_spacing(8)
        .selection_mode(SelectionMode::None)
        .homogeneous(true)
        .build();
    albums_grid.set_hexpand(true);
    albums_grid.set_halign(Align::Fill);
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Automatic)
        .child(&albums_grid)
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
    let albums_stack = Stack::builder()
        .transition_type(StackTransitionType::None)
        .build();

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
    albums_stack.add_named(&loading_state_container, Some("loading_state"));
    albums_stack.add_named(&empty_state_container, Some("empty_state"));
    albums_stack.add_named(&no_results_container, Some("no_results_state"));
    albums_stack.add_named(&scanning_state_container, Some("scanning_state"));
    let albums_content_box = Box::builder().orientation(Orientation::Vertical).build();
    albums_content_box.append(&scrolled);
    albums_stack.add_named(&albums_content_box, Some("populated_grid"));
    albums_stack.set_visible_child_name("loading_state"); // Set initial state to loading
    (albums_stack, albums_grid)
}

/// Populate the given albums grid with album tiles, clearing and sorting as needed.
pub async fn populate_albums_grid(
    albums_grid: &FlowBox,
    db_pool: Arc<SqlitePool>,
    sort_ascending: bool,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    screen_info: &Rc<RefCell<ScreenInfo>>,
    _window: &ApplicationWindow, // Prefix with _ to mark as intentionally unused
    scanning_label: &Label,
    sender: UnboundedSender<()>,
    stack: &ViewStack,
    header_btn_stack: &ViewStack,
    albums_inner_stack: &Stack,
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
    let fetch_result = fetch_album_display_info(&db_pool).await;
    let dr_store = DrValueStore::load(); // Load the DR store once
    match fetch_result {
        Err(_) => {
            BUSY.with(|b| b.set(false));

            // In case of error, show empty state or a specific error state
            albums_inner_stack.set_visible_child_name("empty_state");
        }
        Ok(mut albums) => {
            if albums.is_empty() {
                if scanning_label.is_visible() {
                    albums_inner_stack.set_visible_child_name("scanning_state");
                } else {
                    albums_inner_stack.set_visible_child_name("empty_state");
                }
                if scanning_label.is_visible() {
                    albums_inner_stack.set_visible_child_name("scanning_state");
                } else {
                    albums_inner_stack.set_visible_child_name("empty_state");
                }
                BUSY.with(|b| b.set(false));
                return;
            }
            albums_inner_stack.set_visible_child_name("populated_grid");

            // Multi-level sort albums according to sort_orders
            let current_sort_orders = sort_orders.borrow();

            // ... rest of population logic ...
            BUSY.with(|b| b.set(false));
            albums.sort_by(|a, b| {
                for order in &*current_sort_orders {
                    let cmp = match order {
                        SortOrder::Artist => a.artist.to_lowercase().cmp(&b.artist.to_lowercase()),
                        SortOrder::Album => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                        SortOrder::Year => {
                            let a_year = a.original_release_date.as_ref().and_then(|s| {
                                s.split('-').next().and_then(|y| y.parse::<i32>().ok())
                            });
                            let b_year = b.original_release_date.as_ref().and_then(|s| {
                                s.split('-').next().and_then(|y| y.parse::<i32>().ok())
                            });
                            a_year.cmp(&b_year)
                        }
                        SortOrder::Format => a.format.cmp(&b.format),
                    };
                    if cmp != Ordering::Equal {
                        return if sort_ascending { cmp } else { cmp.reverse() };
                    }
                }
                Ordering::Equal
            });

            // BATCH_SIZE: The number of album tiles to process before yielding control
            // back to the GTK main thread. This helps prevent UI freezes during large
            // grid population operations. A larger batch size means fewer yields but
            // potentially longer individual UI blocking.
            const BATCH_SIZE: usize = 50;
            let mut processed_count = 0;
            let cover_size = screen_info.borrow().cover_size;
            let tile_size = screen_info.borrow().tile_size;
            for album in albums {
                let title_label = create_album_label(
                    &album.title,
                    &["album-title-label"],
                    Some(((cover_size - 16) / 10).max(8)),
                    Some(EllipsizeMode::End),
                    true,
                    Some(WrapMode::WordChar),
                    Some(2),
                );
                title_label.set_size_request(cover_size - 16, -1);
                title_label.set_halign(Align::Start);
                title_label.set_xalign(0.0);
                let artist_label = create_album_label(
                    &album.artist,
                    &["album-artist-label"],
                    Some(18),
                    Some(EllipsizeMode::End),
                    false,
                    None,
                    None,
                );
                artist_label.add_css_class("album-artist-label"); // Ensure this class is applied
                let mut format_fields = Vec::new();
                if let Some(format_str) = album.format.as_ref() {
                    let format_caps = format_str.to_uppercase();
                    match (album.bit_depth, album.frequency) {
                        (Some(bit), Some(freq)) => {
                            format_fields.push(format!(
                                "{} {}/{}",
                                format_caps,
                                bit,
                                format_freq_khz(freq)
                            ));
                        }
                        (None, Some(freq)) => {
                            format_fields.push(format!("{} {}", format_caps, format_freq_khz(freq)))
                        }
                        _ => format_fields.push(format_caps),
                    }
                }
                let format_line = format_fields.join(" · ");
                let format_label = create_album_label(
                    &format_line,
                    &["album-format-label"],
                    None,
                    None,
                    false,
                    None,
                    None,
                );
                format_label.set_halign(Align::Start);
                format_label.set_hexpand(true); // Allow format label to expand
                let year_text = if let Some(original_release_date_str) = album.original_release_date
                {
                    original_release_date_str
                        .split('-')
                        .next()
                        .unwrap_or("N/A")
                        .to_string()
                } else if let Some(year) = album.year {
                    format!("{}", year)
                } else {
                    String::new()
                };
                let year_label = create_album_label(
                    &year_text,
                    &["album-format-label"],
                    None,
                    None,
                    false,
                    None,
                    None,
                );
                year_label.set_halign(Align::End);
                year_label.set_hexpand(false); // Do not allow year label to expand

                // Album box creation
                let album_tile_box = Box::builder()
                    .orientation(Orientation::Vertical)
                    .spacing(2)
                    .build();

                // tile_size + room for text
                album_tile_box.set_size_request(tile_size, tile_size + 80);
                album_tile_box.set_hexpand(false);
                album_tile_box.set_vexpand(false);
                album_tile_box.set_halign(Align::Start);
                album_tile_box.set_valign(Align::Start);

                // Fixed-size container for cover (new instance per tile)
                let cover_container = Box::new(Orientation::Vertical, 0);
                cover_container.set_size_request(cover_size, cover_size);
                cover_container.set_halign(Align::Start);
                cover_container.set_valign(Align::Start);
                let cover = create_album_cover(album.cover_art.as_ref(), cover_size);
                cover_container.append(&cover);

                // Overlay for DR badge
                let overlay = Overlay::new();
                overlay.set_size_request(cover_size, cover_size);
                overlay.set_child(Some(&cover_container));
                overlay.set_halign(Align::Start);
                overlay.set_valign(Align::Start);

                // Construct AlbumKey for lookup in DrValueStore
                let album_key = AlbumKey {
                    title: album.title.clone(),
                    artist: album.artist.clone(),
                    folder_path: album.folder_path.clone(),
                };
                let is_dr_completed_from_store = dr_store.contains(&album_key);
                let dr_label =
                    create_dr_overlay(album._dr_value, is_dr_completed_from_store).unwrap();
                overlay.add_overlay(&dr_label);

                // Play button overlay
                let play_button = Button::builder()
                    .icon_name("media-playback-start")
                    .css_classes(&["play-pause-button", "album-cover-play"][..])
                    .build();
                play_button.set_size_request(56, 56);
                play_button.set_halign(Align::Center);
                play_button.set_valign(Align::Center);
                play_button.set_visible(false);
                overlay.add_overlay(&play_button);

                // Event controller for hover
                let motion_controller = EventControllerMotion::new();
                let play_button_weak = play_button.downgrade();
                motion_controller.connect_enter(move |_, _, _| {
                    if let Some(btn) = play_button_weak.upgrade() {
                        btn.set_visible(true);
                    }
                });
                let play_button_weak = play_button.downgrade(); // Re-clone for the leave handler
                motion_controller.connect_leave(move |_| {
                    if let Some(btn) = play_button_weak.upgrade() {
                        btn.set_visible(false);
                    }
                });
                overlay.add_controller(motion_controller);

                // Fixed-size container for the cover area to ensure consistent sizing
                let cover_fixed = Fixed::new();
                cover_fixed.set_size_request(-1, cover_size);
                cover_fixed.put(&overlay, 0.0, 0.0);
                album_tile_box.append(&cover_fixed);

                // Box to ensure consistent height for the title area (2 lines)
                let title_area_box = Box::builder()
                    .orientation(Orientation::Vertical)
                    .height_request(40) // Explicitly request height for two lines of text + extra buffer
                    .margin_top(12) // Keep the margin from the cover
                    .build();
                title_label.set_valign(Align::End);
                title_area_box.append(&title_label);
                album_tile_box.append(&title_area_box);
                album_tile_box.append(&artist_label);

                // Create a horizontal box to hold format and year labels
                let metadata_box = Box::builder()
                    .orientation(Orientation::Horizontal)
                    .spacing(0) // No spacing between the two labels
                    .hexpand(true)
                    .build();
                metadata_box.append(&format_label);
                metadata_box.append(&year_label);
                album_tile_box.append(&metadata_box);
                album_tile_box.set_css_classes(&["album-tile"]);

                // Set album_id as widget data for double-click navigation
                let flow_child = FlowBoxChild::builder().build();
                flow_child.set_child(Some(&album_tile_box));
                flow_child.set_hexpand(false);
                flow_child.set_vexpand(false);
                flow_child.set_halign(Align::Start);
                flow_child.set_valign(Align::Start);
                unsafe {
                    flow_child.set_data::<i64>("album_id", album.id);
                }
                album_tile_box.set_hexpand(false);
                album_tile_box.set_halign(Align::Center);

                // Add click gesture for navigation
                let stack_weak = stack.downgrade();
                let db_pool_clone = Arc::clone(&db_pool);
                let header_btn_stack_weak = header_btn_stack.downgrade();
                let flow_child_clone = flow_child.clone();
                let sender_clone = sender.clone();
                let gesture = GestureClick::builder().build();
                let gesture_for_closure = gesture.clone();
                gesture_for_closure.connect_pressed(move |_, _, _, _| {
                    if let (Some(stack), Some(header_btn_stack)) =
                        (stack_weak.upgrade(), header_btn_stack_weak.upgrade())
                    {
                        let album_id = unsafe {
                            flow_child_clone
                                .data::<i64>("album_id")
                                .map(|ptr| *ptr.as_ref())
                                .unwrap_or_default()
                        };
                        MainContext::default().spawn_local(album_page(
                            stack.downgrade(),
                            db_pool_clone.clone(),
                            album_id,
                            header_btn_stack.downgrade(),
                            sender_clone.clone(),
                        ));
                    }
                });
                flow_child.add_controller(gesture); // Move original into add_controller

                albums_grid.insert(&flow_child, -1);

                processed_count += 1;
                // Yield control to the GTK main thread periodically.
                // This allows the UI to update and remain responsive during long-running
                // grid population tasks. `ControlFlow::Continue` ensures the timer
                // does not repeat, as we only need a single yield.
                if processed_count % BATCH_SIZE == 0 {
                    timeout_add_local(std::time::Duration::from_millis(1), || {
                        ControlFlow::Continue
                    });
                }
            }
        }
    }
}
