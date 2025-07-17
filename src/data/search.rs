use std::{rc::Rc, sync::Arc};
use std::cell::{Cell, RefCell};

use gdk_pixbuf::{InterpType, PixbufLoader};
use gdk_pixbuf::prelude::PixbufLoaderExt;
use glib::{MainContext, markup_escape_text};
use glib::prelude::ObjectExt;
use gtk4::{Align, Box, Entry, FlowBox, FlowBoxChild, GestureClick, Image, Label, Orientation, Picture};
use gtk4::pango::EllipsizeMode;
use libadwaita::{Clamp, ViewStack};
use libadwaita::prelude::{BoxExt, EditableExt, FlowBoxChildExt, WidgetExt};
use sqlx::SqlitePool;

use crate::data::db::{search_album_display_info, search_artists};
use crate::ui::pages::album_page::album_page;
use crate::ui::pages::artist_page::artist_page;
use crate::utils::formatting::format_freq_khz;
use crate::utils::screen::{compute_cover_and_tile_size, get_primary_screen_size};

/// Connects live search logic to the given search entry, updating albums and artists grids as the user types.
pub fn connect_live_search(
    search_entry: &Entry,
    albums_grid: &FlowBox,
    artists_grid: &FlowBox,
    db_pool: Arc<SqlitePool>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    stack: Rc<ViewStack>,
    left_btn_stack: Rc<ViewStack>,
    right_btn_box: Rc<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
) {

    // Compute dynamic sizes based on screen dimensions
    let (screen_width, _) = get_primary_screen_size();
    let (cover_size, tile_size) = compute_cover_and_tile_size(screen_width);

    // Clone dependencies for async closure
    let db_pool = db_pool.clone();
    let albums_grid = albums_grid.clone();
    let artists_grid = artists_grid.clone();
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
            return;
        }

        // Trim search query
        let text = text.trim().to_string();

        // Clone variables for async move
        let db_pool = db_pool.clone();
        let albums_grid = albums_grid.clone();
        let artists_grid = artists_grid.clone();
        let sort_ascending = sort_ascending.clone();
        let stack_for_closure = stack_clone.clone();
        let left_btn_stack_for_closure = left_btn_stack_clone.clone();
        let right_btn_box_clone = right_btn_box_clone.clone();
        let nav_history = nav_history.clone();

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
                        let label = Label::builder()
                            .label("No albums found")
                            .css_classes(["dim-label"])
                            .build();
                        albums_grid.insert(&label, -1);
                    } else {

                        // Create album tiles
                        for album in albums {
                            let title_label = Label::builder()
                                .use_markup(true)
                                .label(&highlight(&album.title, &text))
                                .halign(Align::Start)
                                .build();
                            title_label.set_xalign(0.0);
                            title_label.set_max_width_chars(18);
                            title_label.set_ellipsize(EllipsizeMode::End);
                            title_label.set_css_classes(&["album-title-label"]);
                            let artist_label = Label::builder()
                                .use_markup(true)
                                .label(&highlight(&album.artist, &text))
                                .halign(Align::Start)
                                .build();
                            artist_label.set_xalign(0.0);
                            artist_label.set_max_width_chars(18);
                            artist_label.set_ellipsize(EllipsizeMode::End);
                            artist_label.set_css_classes(&["album-artist-label"]);
                            let format_line = if let Some(ref format) = album.format {
                                let format_caps = format.to_uppercase();
                                match (album.bit_depth, album.frequency) {
                                    (Some(bit), Some(freq)) => format!(
                                        "{} {}/{}",
                                        format_caps,
                                        bit,
                                        format_freq_khz(freq)
                                    ),
                                    (None, Some(freq)) => format!(
                                        "{} {}",
                                        format_caps,
                                        format_freq_khz(freq)
                                    ),
                                    _ => format_caps,
                                }
                            } else {
                                String::new()
                            };
                            let format_label = Label::builder()
                                .label(&format_line)
                                .halign(Align::Start)
                                .build();
                            format_label.set_xalign(0.0);
                            format_label.set_css_classes(&["album-format-label"]);
                            let cover = if let Some(ref art) = album.cover_art {
                                let pixbuf_loader = PixbufLoader::new();
                                pixbuf_loader.write(art).expect("Failed to load cover art");
                                pixbuf_loader.close().expect("Failed to close loader");
                                let pixbuf = pixbuf_loader.pixbuf().expect("No pixbuf loaded");
                                let scaled = pixbuf
                                    .scale_simple(cover_size, cover_size, InterpType::Bilinear)
                                    .unwrap();
                                let picture = Picture::for_pixbuf(&scaled);
                                picture.set_size_request(cover_size, cover_size);
                                picture
                            } else {
                                let pic = Picture::new();
                                pic.set_size_request(cover_size, cover_size);
                                pic
                            };
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
                            box_.append(&cover);
                            box_.append(&title_label);
                            box_.append(&artist_label);
                            box_.append(&format_label);
                            box_.set_css_classes(&["album-tile"]);
                            let flow_child = Rc::new(FlowBoxChild::new());
                            flow_child.set_child(Some(&box_));
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
                            let flow_child_clone = flow_child.clone(); // Clone Rc for the closure
                            let nav_history_clone = nav_history.clone();
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
                        let label = Label::builder()
                            .label("No artists found")
                            .css_classes(["dim-label"])
                            .build();
                        artists_grid.insert(&label, -1);
                    } else {
                        let right_btn_box_weak = right_btn_box_clone.downgrade();

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
                            let gesture = GestureClick::builder().build();
                            let flow_child_clone = flow_child.clone();
                            let right_btn_box_weak_clone = right_btn_box_weak.clone();
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
                                            right_btn_box_weak_clone.clone(),
                                            nav_history_clone.clone(),
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
