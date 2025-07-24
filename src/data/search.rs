use std::{rc::Rc, sync::Arc};
use std::cell::{Cell, RefCell};

use gdk_pixbuf::{InterpType, PixbufLoader};
use gdk_pixbuf::prelude::PixbufLoaderExt;
use glib::{MainContext, markup_escape_text};
use glib::prelude::ObjectExt;
use gtk4::{Align, Box, Entry, Fixed, FlowBox, FlowBoxChild, GestureClick, Image, Label, Orientation, Overlay, Picture, Stack};
use gtk4::pango::{EllipsizeMode, WrapMode};
use libadwaita::{Clamp, ViewStack};
use libadwaita::prelude::{BoxExt, EditableExt, FixedExt, FlowBoxChildExt, WidgetExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::data::db::{search_album_display_info, search_artists};
use crate::ui::pages::album_page::album_page;
use crate::ui::pages::artist_page::artist_page;
use crate::utils::formatting::format_freq_khz;
use crate::utils::screen::{compute_cover_and_tile_size, get_primary_screen_size};

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

/// Helper to create a styled label for album metadata.
fn create_album_label(text: &str, css_classes: &[&str], max_width: Option<i32>, ellipsize: Option<EllipsizeMode>, wrap: bool, wrap_mode: Option<WrapMode>, lines: Option<i32>) -> Label {
    let builder = Label::builder().label(text).halign(Align::Start).use_markup(true);
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

/// Connects live search logic to the given search entry, updating albums and artists grids as the user types.
pub fn connect_live_search(
    search_entry: &Entry,
    albums_grid: &FlowBox,
    albums_stack: &Stack,
    artists_grid: &FlowBox,
    artists_stack: &Stack,
    db_pool: Arc<SqlitePool>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    stack: Rc<ViewStack>,
    left_btn_stack: Rc<ViewStack>,
    right_btn_box: Rc<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
) {

    // Compute dynamic sizes based on screen dimensions
    let (screen_width, _) = get_primary_screen_size();
    let (cover_size, tile_size) = compute_cover_and_tile_size(screen_width);
    let db_pool = db_pool.clone();
    let albums_grid = albums_grid.clone();
    let albums_stack = albums_stack.clone();
    let artists_grid = artists_grid.clone();
    let artists_stack = artists_stack.clone();
    let sort_ascending = sort_ascending.clone();
    let refresh_library_ui = refresh_library_ui.clone();
    let sort_ascending_artists = sort_ascending_artists.clone();
    let stack_clone = stack.clone();
    let left_btn_stack_clone = left_btn_stack.clone();
    let right_btn_box_clone = right_btn_box.clone();
    let nav_history = nav_history.clone();

    // Connect search entry changed signal
    search_entry.connect_changed(move |entry| {
        let text = entry.text().to_string();

        // Always clear grids before search
        clear_grid(&albums_grid);
        clear_grid(&artists_grid);

        // Handle empty search query
        if text.trim().is_empty() {
            refresh_library_ui(sort_ascending.get(), sort_ascending_artists.get());

            // Ensure stacks are set to populated_grid, as refresh_library_ui will handle empty state if needed
            albums_stack.set_visible_child_name("populated_grid");
            artists_stack.set_visible_child_name("populated_grid");
            return;
        }

        // Trim search query
        let text = text.trim().to_string();

        // Clone variables for async move
        let db_pool = db_pool.clone();
        let albums_grid = albums_grid.clone();
        let albums_stack = albums_stack.clone();
        let artists_grid = artists_grid.clone();
        let artists_stack = artists_stack.clone();
        let sort_ascending = sort_ascending.clone();
        let stack_for_closure = stack_clone.clone();
        let left_btn_stack_for_closure = left_btn_stack_clone.clone();
        let right_btn_box_clone = right_btn_box_clone.clone();
        let nav_history = nav_history.clone();
        let sender = sender.clone();

        // Spawn async task for search
        MainContext::default().spawn_local(async move {

            // Albums search
            match search_album_display_info(&db_pool, &text).await {
                Err(_) => {}
                Ok(mut albums) => {

                    // Sort albums by score and title
                    albums.sort_by(|a, b| {
                        let a_title = a.title.to_lowercase();
                        let b_title = b.title.to_lowercase();
                        let a_artist = a.artist.to_lowercase();
                        let b_artist = b.artist.to_lowercase();
                        let query = text.to_lowercase();
                        let score = |s: &str| {
                            if s == query {
                                0
                            } else if s.starts_with(&query) {
                                1
                            } else if s.contains(&query) {
                                2
                            } else {
                                3
                            }
                        };
                        let a_score = score(&a_title).min(score(&a_artist));
                        let b_score = score(&b_title).min(score(&b_artist));
                        a_score.cmp(&b_score).then_with(|| {
                            let cmp = a_artist.cmp(&b_artist);
                            if sort_ascending.get() {
                                cmp
                            } else {
                                cmp.reverse()
                            }
                        })
                    });

                    // Handle empty search results
                    if albums.is_empty() {
                        albums_stack.set_visible_child_name("empty_state");
                    } else {
                        albums_stack.set_visible_child_name("populated_grid");

                        // Create album tiles
                        for album in albums {
                            let title_label = {
                                let label = create_album_label(
                                    &highlight(&markup_escape_text(&album.title).to_string(), &text),
                                    &["album-title-label"],
                                    Some(((cover_size - 16) / 10).max(8)),
                                    Some(EllipsizeMode::End),
                                    true,
                                    Some(WrapMode::WordChar),
                                    Some(2),
                                );
                                label.set_markup(&highlight(&markup_escape_text(&album.title).to_string(), &text));
                                label.set_size_request(cover_size - 16, -1);
                                label
                            };
                            let artist_label = {
                                let label = create_album_label(
                                    &highlight(&markup_escape_text(&album.artist).to_string(), &text),
                                    &["album-artist-label"],
                                    Some(18),
                                    Some(EllipsizeMode::End),
                                    false,
                                    None,
                                    None,
                                );
                                label.set_markup(&highlight(&markup_escape_text(&album.artist).to_string(), &text));
                                label
                            };
                            let format_line = if let Some(format_str) = album.format.as_ref() {
                                let format_caps = format_str.to_uppercase();
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
                            format_label.set_halign(Align::Start);
                            format_label.set_hexpand(true);
                            let year_text = if let Some(original_release_date_str) = album.original_release_date {
                                original_release_date_str.split('-').next().unwrap_or("N/A").to_string()
                            } else if let Some(year) = album.year {
                                format!("{}", year)
                            } else {
                                String::new()
                            };
                            let year_label = create_album_label(
                                &year_text,
                                &["album-year-label"],
                                None,
                                None,
                                false,
                                None,
                                None,
                            );
                            year_label.set_halign(Align::End);
                            year_label.set_hexpand(false);
                            let cover = create_album_cover(album.cover_art.as_ref(), cover_size);
                            let album_tile_box = Box::builder()
                                .orientation(Orientation::Vertical)
                                .spacing(2)
                                .build();
                            album_tile_box.set_size_request(tile_size, tile_size + 80);
                            album_tile_box.set_hexpand(false);
                            album_tile_box.set_vexpand(false);
                            album_tile_box.set_halign(Align::Start);
                            album_tile_box.set_valign(Align::Start);
                            let cover_container = Box::new(Orientation::Vertical, 0);
                            cover_container.set_size_request(cover_size, cover_size);
                            cover_container.set_halign(Align::Start);
                            cover_container.set_valign(Align::Start);
                            cover_container.append(&cover);
                            let overlay = Overlay::new();
                            overlay.set_size_request(cover_size, cover_size);
                            overlay.set_child(Some(&cover_container));
                            overlay.set_halign(Align::Start);
                            overlay.set_valign(Align::Start);
                            let dr_label = create_dr_overlay(album._dr_value, album.dr_completed).unwrap();
                            overlay.add_overlay(&dr_label);
                            let cover_fixed = Fixed::new();
                            cover_fixed.set_size_request(-1, cover_size);
                            cover_fixed.put(&overlay, 0.0, 0.0);
                            album_tile_box.append(&cover_fixed);
                            let title_area_box = Box::builder()
                                .orientation(Orientation::Vertical)
                                .height_request(40) // Explicitly request height for two lines of text + extra buffer
                                .margin_top(12)     // Keep the margin from the cover
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
                            let flow_child = Rc::new(FlowBoxChild::new());
                            flow_child.set_child(Some(&album_tile_box));
                            flow_child.set_hexpand(false);
                            flow_child.set_vexpand(false);
                            flow_child.set_halign(Align::Fill);
                            flow_child.set_valign(Align::Start);
                            unsafe {
                                flow_child.set_data::<i64>("album_id", album.id);
                            }

                            // Add click gesture for navigation
                            let stack_weak = stack_for_closure.downgrade();
                            let db_pool_clone = Arc::clone(&db_pool);
                            let header_btn_stack_weak = left_btn_stack_for_closure.downgrade();
                            let flow_child_clone = flow_child.clone();
                            let nav_history_clone = nav_history.clone();
                            let sender_clone = sender.clone();
                            let gesture = GestureClick::builder().build();
                            gesture.connect_pressed(move |_, _, _, _| {
                                if let (Some(stack), Some(header_btn_stack)) = (stack_weak.upgrade(), header_btn_stack_weak.upgrade()) {
                                    let album_id = unsafe { flow_child_clone.data::<i64>("album_id").map(|ptr| *ptr.as_ref()).unwrap_or_default() };
                                    if let Some(current_page) = stack.visible_child_name() {
                                        nav_history_clone.borrow_mut().push(current_page.to_string());
                                    }
                                    MainContext::default().spawn_local(
                                        album_page(
                                            stack.downgrade(),
                                            db_pool_clone.clone(),
                                            album_id,
                                            header_btn_stack.downgrade(),
                                            sender_clone.clone(),
                                        )
                                    );
                                }
                            });
                            flow_child.add_controller(gesture);

                            albums_grid.insert(flow_child.as_ref(), -1);
                        }
                    }
                }
            }

            // Artists search
            clear_grid(&artists_grid);
            match search_artists(&db_pool, &text).await {
                Err(_) => {}
                Ok(artists) => {

                    // Handle empty search results
                    if artists.is_empty() {
                        artists_stack.set_visible_child_name("empty_state");
                    } else {
                        artists_stack.set_visible_child_name("populated_grid");

                        // Create artist tiles
                        for artist in artists {
                            let picture = Image::from_icon_name("avatar-default-symbolic");
                            picture.set_pixel_size(cover_size);
                            let label = Label::builder()
                                .use_markup(true)
                                .label(&highlight(&artist.name, &text))
                                .build();
                            let box_ = Box::builder()
                                .orientation(Orientation::Vertical)
                                .spacing(4)
                                .build();
                            box_.set_size_request(tile_size, (tile_size as f32 * 1.44) as i32);
                            box_.set_hexpand(false);
                            box_.set_vexpand(false);
                            box_.set_halign(Align::Fill);
                            box_.set_valign(Align::Start);
                            box_.set_margin_top(8);
                            box_.set_margin_bottom(8);
                            box_.set_margin_start(8);
                            box_.set_margin_end(8);
                            box_.append(&picture);
                            box_.append(&label);
                            box_.set_css_classes(&["artist-tile"]);
                            let flow_child = FlowBoxChild::builder().build();
                            flow_child.set_child(Some(&box_));
                            flow_child.set_hexpand(false);
                            flow_child.set_vexpand(false);
                            flow_child.set_halign(Align::Fill);
                            flow_child.set_valign(Align::Start);
                            unsafe {
                                flow_child.set_data::<i64>("artist_id", artist.id);
                            }
                            let stack_weak = stack_for_closure.downgrade();
                            let db_pool_clone = Arc::clone(&db_pool);
                            let left_btn_stack_weak = left_btn_stack_for_closure.downgrade();
                            let nav_history_clone = nav_history.clone();
                            let sender_clone = sender.clone();
                            let gesture = GestureClick::builder().build();
                            let flow_child_clone = flow_child.clone();
                            let right_btn_box_weak_clone = right_btn_box_clone.clone();
                            gesture.connect_pressed(move |_, _, _, _| {
                                if let (Some(stack), Some(left_btn_stack)) =
                                    (stack_weak.upgrade(), left_btn_stack_weak.upgrade())
                                {
                                    let artist_id = unsafe { flow_child_clone.data::<i64>("artist_id").map(|ptr| *ptr.as_ref()).unwrap_or_default() };
                                    if let Some(current_page) = stack.visible_child_name() {
                                        nav_history_clone.borrow_mut().push(current_page.to_string());
                                    }
                                    MainContext::default().spawn_local(
                                        artist_page(
                                            stack.downgrade(),
                                            db_pool_clone.clone(),
                                            artist_id,
                                            left_btn_stack.downgrade(),
                                            right_btn_box_weak_clone.clone().downgrade(),
                                            nav_history_clone.clone(),
                                            sender_clone.clone(),
                                        ),
                                    );
                                }
                            });
                            flow_child.add_controller(gesture);
                            artists_grid.insert(&flow_child, -1);
                        }
                    }
                }
            }
        });
    });
}

/// Helper function to highlight matching text
fn highlight(s: &str, query: &str) -> String {
    if query.is_empty() {
        return markup_escape_text(s).to_string();
    }
    let mut result = String::new();
    let mut last = 0;
    let s_lower = s.to_lowercase();
    let q = query.to_lowercase();
    let q_len = q.len();
    let mut i = 0;
    while let Some(pos) = s_lower[i..].find(&q) {
        let start = i + pos;
        let end = start + q_len;
        result.push_str(&markup_escape_text(&s[last..start]));
        result.push_str(&format!(
            "<span background='yellow'>{}</span>",
            markup_escape_text(&s[start..end])
        ));
        last = end;
        i = end;
    }
    result.push_str(&markup_escape_text(&s[last..]));
    result
}

/// Helper function to clear grid
pub fn clear_grid(grid: &FlowBox) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
}
