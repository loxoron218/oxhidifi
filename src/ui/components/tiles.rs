use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    sync::Arc,
};

use gdk_pixbuf::Pixbuf;
use glib::{MainContext, markup_escape_text, prelude::ObjectExt};
use gtk4::{
    Align::{Center, End, Fill, Start},
    Box, Button, EventControllerMotion, Fixed, FlowBoxChild, GestureClick, Image, Label,
    Orientation::{Horizontal, Vertical},
    Overlay, Picture,
    pango::{
        EllipsizeMode,
        WrapMode::{self, WordChar},
    },
};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{BoxExt, FixedExt, FlowBoxChildExt, WidgetExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    data::db::query::AlbumDisplayInfo,
    ui::pages::{album_page::album_page, artist_page::artist_page},
    utils::formatting::format_freq_khz,
};

/// Helper to create the album cover as a Picture widget.
///
/// This function takes an optional path to a cached image file and a desired size,
/// then creates a `Picture` widget displaying the scaled album cover.
/// If no path is provided or the file doesn't exist, an empty `Picture` is returned.
pub fn create_album_cover(cover_art_path: Option<&Path>, cover_size: i32) -> Picture {
    let pic = Picture::new();
    pic.set_size_request(cover_size, cover_size);
    pic.set_halign(Start);
    pic.set_valign(Start);
    pic.add_css_class("album-cover-border");
    if let Some(path) = cover_art_path {
        if let Ok(pixbuf) = Pixbuf::from_file_at_scale(path, cover_size, cover_size, true) {
            pic.set_pixbuf(Some(&pixbuf));
        }
    }
    pic
}

/// Helper to create the DR badge overlay if present.
///
/// This function generates a `Label` widget to display the Dynamic Range (DR) value
/// of an album. It applies specific CSS classes based on the DR value and completion status,
/// and provides a tooltip for additional information.
pub fn create_dr_overlay(dr_value: Option<u8>, dr_completed: bool) -> Option<Label> {
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
    dr_label.set_halign(End);
    dr_label.set_valign(End);
    Some(dr_label)
}

/// Helper to create a styled label for album metadata.
///
/// This function creates a `Label` widget with common styling properties
/// for displaying album-related text, such as title, artist, format, and year.
/// It supports markup, text wrapping, ellipsizing, and custom CSS classes.
pub fn create_album_label(
    text: &str,
    css_classes: &[&str],
    max_width: Option<i32>,
    ellipsize: Option<EllipsizeMode>,
    wrap: bool,
    wrap_mode: Option<WrapMode>,
    lines: Option<i32>,
    use_markup: bool,
) -> Label {
    let builder = Label::builder()
        .label(text)
        .halign(Start)
        .use_markup(use_markup);
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

/// Helper function to highlight matching text.
///
/// This function takes a string and a query, then wraps occurrences
/// of the query within the string with `<span background='yellow'>`
/// markup for highlighting. It is case-insensitive.
///
/// # Arguments
/// * `s` - The original string to search within.
/// * `query` - The substring to highlight.
///
/// # Returns
/// A new string with the `query` parts highlighted using Pango markup.
pub fn highlight(s: &str, query: &str) -> String {
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

/// Creates a `FlowBoxChild` containing the UI representation of an album.
///
/// This function constructs a visual tile for a given album, including its cover,
/// title, artist, format, year, and DR badge. It also attaches a click gesture
/// to navigate to the album's dedicated page.
pub fn create_album_tile(
    album: AlbumDisplayInfo,
    cover_size: i32,
    tile_size: i32,
    search_text: &str,
    stack_for_closure: Rc<ViewStack>,
    db_pool: Arc<SqlitePool>,
    left_btn_stack_for_closure: Rc<ViewStack>,
    right_btn_box_for_closure: Rc<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
) -> FlowBoxChild {
    // Create and style the album title label
    let title_label = {
        let label = create_album_label(
            &highlight(&album.title, search_text),
            &["album-title-label"],
            Some(((cover_size - 16) / 10).max(8)),
            Some(EllipsizeMode::End),
            true,
            Some(WordChar),
            Some(2),
            true, // use_markup: true because highlight is used
        );
        label.set_size_request(cover_size - 16, -1);
        label.set_valign(End); // Align to the bottom of its allocated space
        label
    };

    // Create and style the artist label
    let artist_label = {
        let label = create_album_label(
            &highlight(&album.artist, search_text),
            &["album-artist-label"],
            Some(18),
            Some(EllipsizeMode::End),
            false,
            None,
            None,
            true, // use_markup: true because highlight is used
        );
        label
    };

    // Format the audio quality line (e.g., "FLAC 24/96")
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

    // Create and style the format label
    let format_label = create_album_label(
        &format_line,
        &["album-format-label"],
        None,
        None,
        false,
        None,
        None,
        false, // use_markup: false for plain text
    );
    format_label.set_halign(Start);
    format_label.set_hexpand(true);

    // Extract and format the release year based on setting
    let year_text = if use_original_year.get() {
        if let Some(original_release_date_str) = album.original_release_date {
            original_release_date_str
                .split('-')
                .next()
                .unwrap_or("N/A")
                .to_string()
        } else if let Some(year) = album.year {
            format!("{}", year)
        } else {
            String::new()
        }
    } else {
        if let Some(year) = album.year {
            format!("{}", year)
        } else if let Some(original_release_date_str) = album.original_release_date {
            original_release_date_str
                .split('-')
                .next()
                .unwrap_or("N/A")
                .to_string()
        } else {
            String::new()
        }
    };

    // Create and style the year label
    let year_label = create_album_label(
        &year_text,
        &["album-format-label"],
        None,
        None,
        false,
        None,
        None,
        false, // use_markup: false for plain text
    );
    year_label.set_halign(End);
    year_label.set_hexpand(false);

    // Create album cover picture
    let cover = create_album_cover(album.cover_art.as_deref(), cover_size);

    // Main vertical box for the album tile
    let album_tile_box = Box::builder().orientation(Vertical).spacing(2).build();
    album_tile_box.set_size_request(tile_size, tile_size + 80);
    album_tile_box.set_hexpand(false);
    album_tile_box.set_vexpand(false);
    album_tile_box.set_halign(Start);
    album_tile_box.set_valign(Start);

    // Container for the cover, to ensure fixed size
    let cover_container = Box::new(Vertical, 0);
    cover_container.set_size_request(cover_size, cover_size);
    cover_container.set_halign(Start);
    cover_container.set_valign(Start);
    cover_container.append(&cover);

    // Overlay for DR badge on the cover
    let overlay = Overlay::new();
    overlay.set_size_request(cover_size, cover_size);
    overlay.set_child(Some(&cover_container));
    overlay.set_halign(Start);
    overlay.set_valign(Start);
    if show_dr_badges.get() {
        if let Some(dr_label) = create_dr_overlay(album.dr_value, album.dr_completed) {
            overlay.add_overlay(&dr_label);
        }
    }

    // Play button overlay
    let play_button = Button::builder()
        .icon_name("media-playback-start")
        .css_classes(&["play-pause-button", "album-cover-play"][..])
        .build();
    play_button.set_size_request(56, 56);
    play_button.set_halign(Center);
    play_button.set_valign(Center);
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

    // Fixed container for the overlay, ensuring correct positioning
    let cover_fixed = Fixed::new();
    cover_fixed.set_size_request(-1, cover_size);
    cover_fixed.put(&overlay, 0.0, 0.0);
    album_tile_box.append(&cover_fixed);

    // Box for title, with explicit height for two lines of text
    let title_area_box = Box::builder()
        .orientation(Vertical)
        .height_request(40) // Explicitly request height for two lines of text + extra buffer
        .margin_top(12) // Keep the margin from the cover
        .build();
    title_area_box.append(&title_label);
    album_tile_box.append(&title_area_box);
    album_tile_box.append(&artist_label);

    // Horizontal box to hold format and year labels
    let metadata_box = Box::builder()
        .orientation(Horizontal)
        .spacing(0) // No spacing between the two labels
        .hexpand(true)
        .build();
    metadata_box.append(&format_label);
    metadata_box.append(&year_label);
    album_tile_box.append(&metadata_box);
    album_tile_box.set_css_classes(&["album-tile"]);

    // Create the FlowBoxChild and set its properties
    let flow_child = FlowBoxChild::new();
    flow_child.set_child(Some(&album_tile_box));
    flow_child.set_hexpand(false);
    flow_child.set_vexpand(false);
    flow_child.set_halign(Fill);
    flow_child.set_valign(Start);

    // Add click gesture for navigation to album page
    let stack_weak = stack_for_closure.downgrade();
    let gesture = GestureClick::builder().build();

    // The `move` keyword captures the needed variables safely.
    gesture.connect_pressed(move |_, _, _, _| {
        // The album ID is now owned by the closure.
        let album_id = album.id;
        if let (Some(stack), Some(header_btn_stack)) = (
            stack_weak.upgrade(),
            left_btn_stack_for_closure.downgrade().upgrade(),
        ) {
            if let Some(current_page) = stack.visible_child_name() {
                nav_history.borrow_mut().push(current_page.to_string());
            }
            MainContext::default().spawn_local(album_page(
                stack.downgrade(),
                db_pool.clone(),
                album_id,
                header_btn_stack.downgrade(),
                right_btn_box_for_closure.downgrade(),
                sender.clone(),
                show_dr_badges.clone(),
            ));
        }
    });
    flow_child.add_controller(gesture);
    flow_child
}

/// Creates a `FlowBoxChild` containing the UI representation of an artist.
///
/// This function constructs a visual tile for a given artist, including their
/// avatar and name. It also attaches a click gesture to navigate to the
/// artist's dedicated page.
pub fn create_artist_tile(
    artist_id: i64,
    artist_name: &str,
    cover_size: i32,
    _tile_size: i32,
    search_text: &str,
    stack: Rc<ViewStack>,
    db_pool: Arc<SqlitePool>,
    left_btn_stack: Rc<ViewStack>,
    right_btn_box: Rc<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
) -> FlowBoxChild {
    let icon = Image::from_icon_name("avatar-default-symbolic");
    icon.set_pixel_size(cover_size);
    let label = create_album_label(
        &highlight(artist_name, search_text),
        &[],
        Some(((cover_size - 16) / 10).max(8)),
        Some(EllipsizeMode::End),
        true,
        Some(WordChar),
        Some(2),
        true,
    );
    label.set_size_request(cover_size - 16, -1);
    let tile = Box::builder().orientation(Vertical).spacing(2).build();

    // tile_size + room for text
    tile.set_size_request(cover_size, cover_size + 80);
    tile.set_hexpand(false);
    tile.set_vexpand(false);
    tile.set_halign(Start);
    tile.set_valign(Start);

    // Fixed-size container for icon (new instance per tile)
    let icon_container = Box::new(Vertical, 0);
    icon_container.set_size_request(cover_size, cover_size);
    icon_container.set_halign(Start);
    icon_container.set_valign(Start);
    icon_container.add_css_class("album-cover-border");
    icon_container.append(&icon);
    tile.append(&icon_container);

    // Box to ensure consistent height for the label area (2 lines)
    let label_area_box = Box::builder()
        .orientation(Vertical)
        .height_request(40)
        .margin_top(12)
        .build();
    label_area_box.set_halign(Center);
    label.set_valign(End);
    label_area_box.append(&label);
    tile.append(&label_area_box);
    tile.set_css_classes(&["album-tile"]);
    let flow_child = FlowBoxChild::builder().build();
    flow_child.set_child(Some(&tile));
    flow_child.set_hexpand(false);
    flow_child.set_vexpand(false);
    flow_child.set_halign(Fill);
    flow_child.set_valign(Start);
    let stack_weak = stack.downgrade();
    let left_btn_stack_weak = left_btn_stack.downgrade();
    let right_btn_box_weak = right_btn_box.downgrade();
    let gesture = GestureClick::builder().build();
    gesture.connect_pressed(move |_, _, _, _| {
        if let (Some(stack), Some(left_btn_stack)) =
            (stack_weak.upgrade(), left_btn_stack_weak.upgrade())
        {
            if let Some(current_page) = stack.visible_child_name() {
                nav_history.borrow_mut().push(current_page.to_string());
            }
            MainContext::default().spawn_local(artist_page(
                stack.downgrade(),
                db_pool.clone(),
                artist_id,
                left_btn_stack.downgrade(),
                right_btn_box_weak.clone(),
                nav_history.clone(),
                sender.clone(),
                show_dr_badges.clone(),
                use_original_year.clone(),
            ));
        }
    });
    flow_child.add_controller(gesture);
    flow_child
}
