use std::{cmp::Ordering, rc::Rc, sync::Arc};
use std::cell::{Cell, RefCell};

use gdk_pixbuf::{InterpType, PixbufLoader};
use glib::markup_escape_text;
use gtk4::{Align, Box, Button, Fixed, FlowBox, FlowBoxChild, Label, Orientation, Overlay, Picture, PolicyType, ScrolledWindow, SelectionMode, Widget};
use gtk4::pango::{EllipsizeMode, WrapMode};
use libadwaita::{ApplicationWindow, StatusPage, ViewStack};
use libadwaita::prelude::{BoxExt, FixedExt, FlowBoxChildExt, IsA, ObjectExt, PixbufLoaderExt, WidgetExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::data::db::fetch_album_display_info;
use crate::data::search::clear_grid;
use crate::ui::components::dialogs::connect_add_folder_dialog;
use crate::ui::components::sorting::SortOrder;
use crate::utils::formatting::format_freq_khz;

/// Helper to create a styled label for album metadata.
fn create_album_label(text: &str, css_classes: &[&str], max_width: Option<i32>, ellipsize: Option<EllipsizeMode>, wrap: bool, wrap_mode: Option<WrapMode>, lines: Option<i32>) -> Label {
    let builder = Label::builder().label(&*markup_escape_text(text)).halign(Align::Start);
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
        let scaled = cropped.scale_simple(cover_size, cover_size, InterpType::Bilinear).unwrap();
        let picture = Picture::for_pixbuf(&scaled);
        picture.set_size_request(cover_size, cover_size);
        picture.set_halign(Align::Start);
        picture.set_valign(Align::Start);
        picture
    } else {
        let pic = Picture::new();
        pic.set_size_request(cover_size, cover_size);
        pic.set_halign(Align::Start);
        pic.set_valign(Align::Start);
        pic
    }
}

/// Helper to create the DR badge overlay if present.
fn create_dr_overlay(dr_value: Option<u8>) -> Option<Label> {
    dr_value.map(|dr| {
        let dr_str = format!("{:02}", dr);
        let dr_label = Label::builder().label(&dr_str).build();
        dr_label.add_css_class("dr-badge-label");
        dr_label.add_css_class("dr-badge-label-grid");
        dr_label.set_size_request(28, 28);
        let dr_value_class = format!("dr-{:02}", dr);
        dr_label.add_css_class(&dr_value_class);
        dr_label.set_tooltip_text(Some("Official Dynamic Range Value"));
        dr_label.set_halign(Align::End);
        dr_label.set_valign(Align::End);
        dr_label
    })
}

/// Rebuild the albums grid in the main window.
/// Removes the existing 'albums' child, resets the albums_grid_cell, builds a new grid, and adds it to the stack.
pub fn rebuild_albums_grid_for_window(
    stack: &ViewStack,
    scanning_label_albums: &impl IsA<Widget>,
    cover_size_rc: &Rc<Cell<i32>>,
    tile_size_rc: &Rc<Cell<i32>>,
    albums_grid_cell: &Rc<RefCell<Option<FlowBox>>>,
) {

    // Remove old grid widget if present
    if let Some(child) = stack.child_by_name("albums") {
        stack.remove(&child);
    }

    // Drop old FlowBox
    *albums_grid_cell.borrow_mut() = None;

    // Create new grid
    let (albums_stack, albums_grid) = build_albums_grid(
        scanning_label_albums,
        cover_size_rc.get(),
        tile_size_rc.get(),
    );
    stack.add_titled(&albums_stack, Some("albums"), "Albums");
    *albums_grid_cell.borrow_mut() = Some(albums_grid.clone());
}

/// Build the albums grid and its containing stack.
/// Returns (albums_stack, albums_grid).
pub fn build_albums_grid<W: IsA<Widget>>(
    scanning_label: &W,
    _cover_size: i32,
    _tile_size: i32,
) -> (Box, FlowBox) {

    // Albums grid
    let albums_grid = FlowBox::builder()
        .valign(Align::Start)
        .max_children_per_line(128)
        .row_spacing(1)
        .column_spacing(0)
        .selection_mode(SelectionMode::None)
        .build();
    albums_grid.set_halign(Align::Center);
    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Automatic)
        .child(&albums_grid)
        .min_content_height(400)
        .min_content_width(400)
        .vexpand(true)
        .build();
    scrolled.set_hexpand(true);
    scrolled.set_halign(Align::Fill);
    let albums_stack = Box::builder()
        .orientation(Orientation::Vertical)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    albums_stack.append(scanning_label);
    albums_stack.append(&scrolled);
    (albums_stack, albums_grid)
}

/// Populate the given albums grid with album tiles, clearing and sorting as needed.
pub async fn populate_albums_grid(
    albums_grid: &FlowBox,
    db_pool: Arc<SqlitePool>,
    sort_ascending: bool,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    cover_size: i32,
    tile_size: i32,
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

    // Clear all children before repopulating to avoid duplicates
    clear_grid(albums_grid);
    let fetch_result = fetch_album_display_info(&db_pool).await;
    match fetch_result {
        Err(_) => {
            BUSY.with(|b| b.set(false));
        }
        Ok(mut albums) => {
            if albums.is_empty() {
                let status_page = StatusPage::builder()
                    .icon_name("folder-music-symbolic")
                    .title("No Music Found")
                    .description("Add music to your library to get started.")
                    .vexpand(true)
                    .hexpand(true)
                    .build();
                let add_music_button = Button::with_label("Add Music");
                add_music_button.add_css_class("suggested-action");
                connect_add_folder_dialog(
                    &add_music_button,
                    window.clone(),
                    scanning_label.clone(),
                    db_pool.clone(),
                    sender.clone(),
                );
                status_page.set_child(Some(&add_music_button));
                albums_grid.insert(&status_page, -1);
                BUSY.with(|b| b.set(false));
                return;
            }

            // Multi-level sort albums according to sort_orders
            let sort_orders = sort_orders.borrow();

            // ... rest of population logic ...
            BUSY.with(|b| b.set(false));
            albums.sort_by(|a, b| {
                for order in &*sort_orders {
                    let cmp = match order {
                        SortOrder::Artist => a.artist.to_lowercase().cmp(&b.artist.to_lowercase()),
                        SortOrder::Album => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                        SortOrder::Year => a.year.cmp(&b.year),
                        SortOrder::Format => a.format.cmp(&b.format),
                    };
                    if cmp != Ordering::Equal {
                        return if sort_ascending { cmp } else { cmp.reverse() };
                    }
                }
                Ordering::Equal
            });
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
                let format_line = if let Some(ref format) = album.format {
                    let format_caps = format.to_uppercase();
                    match (album.bit_depth, album.frequency) {
                        (Some(bit), Some(freq)) => {
                            format!("{} {}/{}", format_caps, bit, format_freq_khz(freq))
                        }
                        (None, Some(freq)) => format!("{} {}", format_caps, format_freq_khz(freq)),
                        _ => format_caps,
                    }
                } else {
                    String::new()
                };
                let format_label = create_album_label(
                    &format_line,
                    &["album-format-label"],
                    None,
                    None,
                    false,
                    None,
                    None,
                );

                // Album box creation
                let box_ = Box::builder()
                    .orientation(Orientation::Vertical)
                    .spacing(2)
                    .build();

                // tile_size + room for text
                box_.set_size_request(tile_size, tile_size + 80);
                box_.set_hexpand(false);
                box_.set_vexpand(false);
                box_.set_halign(Align::Start);
                box_.set_valign(Align::Start);

                // Fixed-size container for cover (new instance per tile)
                let cover_container = Box::new(Orientation::Vertical, 0);
                cover_container.set_size_request(cover_size, cover_size);
                cover_container.set_halign(Align::Start);
                cover_container.set_valign(Align::Start);
                let cover = create_album_cover(album.cover_art.as_ref(), cover_size);
                cover_container.append(&cover);

                // Overlay for DR badge (new instance per tile)
                let overlay = Overlay::new();
                overlay.set_size_request(cover_size, cover_size);
                overlay.set_child(Some(&cover_container));
                overlay.set_halign(Align::Start);
                overlay.set_valign(Align::Start);
                if let Some(dr_label) = create_dr_overlay(album._dr_value) {
                    overlay.add_overlay(&dr_label);
                }

                // Overlay (cover) at the top
                // Use GtkFixed for a pixel-perfect 192px cover area
                let cover_fixed = Fixed::new();
                cover_fixed.set_size_request(-1, cover_size);
                cover_fixed.put(&overlay, 0.0, 0.0);
                box_.append(&cover_fixed);
                let title_box = Box::new(Orientation::Vertical, 0);
                title_box.set_size_request(-1, 36);
                title_box.set_valign(Align::Start);
                title_box.set_margin_top(12);
                title_label.set_valign(Align::Start);
                title_box.append(&title_label);
                box_.append(&title_box);
                box_.append(&artist_label);
                box_.append(&format_label);
                box_.set_css_classes(&["album-tile"]);

                // Set album_id as widget data for double-click navigation
                let flow_child = FlowBoxChild::builder().build();
                flow_child.set_child(Some(&box_));
                flow_child.set_hexpand(false);
                flow_child.set_vexpand(false);
                flow_child.set_halign(Align::Fill);
                flow_child.set_valign(Align::Start);
                unsafe {
                    flow_child.set_data::<i64>("album_id", album.id);
                }
                box_.set_hexpand(true);
                box_.set_halign(Align::Fill);
                albums_grid.insert(&flow_child, -1);
            }
        }
    }
}