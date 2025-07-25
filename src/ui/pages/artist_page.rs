use std::{cmp::Ordering, rc::Rc, sync::Arc};
use std::{cell::RefCell};

use gdk_pixbuf::{InterpType, PixbufLoader};
use gdk_pixbuf::prelude::PixbufLoaderExt;
use glib::{MainContext, WeakRef};
use gtk4::{Align, Box, FlowBox, GestureClick, Justification, Label, Orientation, Picture, SelectionMode};
use gtk4::pango::{EllipsizeMode, WrapMode};
use libadwaita::{Clamp, ViewStack};
use libadwaita::prelude::{BoxExt, ObjectExt, WidgetExt};
use sqlx::{Error, query, Row, SqlitePool};
use tokio::sync::mpsc::UnboundedSender;

use crate::data::db::fetch_artist_by_id;
use crate::ui::pages::album_page::album_page;
use crate::utils::formatting::format_freq_khz;
use crate::utils::screen::{compute_cover_and_tile_size, get_primary_screen_size};

/// Helper to create a styled label for album metadata.
fn create_album_label(text: &str, css_classes: &[&str], max_width: Option<i32>, ellipsize: Option<EllipsizeMode>, wrap: bool, wrap_mode: Option<WrapMode>, lines: Option<i32>) -> Label {
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

/// Build and present the artist page for a given artist ID.
/// Shows all albums by the artist in a grid, replacing artist name with album year.
pub async fn artist_page(
    stack: WeakRef<ViewStack>,
    db_pool: Arc<SqlitePool>,
    artist_id: i64,
    header_btn_stack: WeakRef<ViewStack>,
    right_btn_box: WeakRef<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
) {

    // Upgrade weak references
    let stack = match stack.upgrade() {
        Some(s) => s,
        None => return,
    };
    let header_btn_stack = match header_btn_stack.upgrade() {
        Some(s) => s,
        None => return,
    };

    // Fetch artist info
    let artist = match fetch_artist_by_id(&db_pool, artist_id).await {
        Ok(a) => a,
        Err(_) => return,
    };

    // Fetch all albums by this artist (custom query)
    let albums = match fetch_album_display_info_by_artist(&db_pool, artist_id).await {
        Ok(albums) => albums,
        Err(_) => return,
    };

    // Sort albums by year (oldest first), albums with no year last
    let mut albums = albums;
    albums.sort_by(|a, b| match (a.year, b.year) {
        (Some(ya), Some(yb)) => ya.cmp(&yb),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });

    // Build UI
    let vbox = Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(24)
        .margin_top(32)
        .margin_bottom(32)
        .margin_start(32)
        .margin_end(32)
        .build();

    // Header with artist name, centered and consistent
    let header = Label::builder()
        .label(&artist.name)
        .css_classes(["title-1"])
        .halign(Align::Center)
        .justify(Justification::Center)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    vbox.append(&header);

    // Compute dynamic cover and tile size
    let (screen_width, _) = get_primary_screen_size();
    let (cover_size, tile_size) = compute_cover_and_tile_size(screen_width);

    // Albums grid (match main albums grid and album_page)
    let flowbox = FlowBox::builder()
        .valign(Align::Start)
        .max_children_per_line(128)
        .selection_mode(SelectionMode::None)
        .row_spacing(1)
        .column_spacing(0)
        .build();
    flowbox.set_halign(Align::Center);
    for album in albums {
        let album_card = build_album_card(
            &album,
            cover_size,
            tile_size,
            stack.downgrade(),
            db_pool.clone(),
            header_btn_stack.downgrade(),
            nav_history.clone(),
            sender.clone(),
        );
        flowbox.insert(&album_card, -1);
    }

    // Clamp for grid horizontal padding (matches album_page)
    let clamp = Clamp::builder().child(&flowbox).build();
    vbox.append(&clamp);

    // Add to stack and show
    let page_name = format!("artist_{}", artist_id);

    // Remove any existing page with the same name to avoid duplicate warning
    if let Some(existing_child) = stack.child_by_name(&page_name) {
        stack.remove(&existing_child);
    }
    stack.add_named(&vbox, Some(&page_name));
    stack.set_visible_child_name(&page_name);
    header_btn_stack.set_visible_child_name("back");

    // Hide right header buttons
    if let Some(right_btn_box) = right_btn_box.upgrade() {
        right_btn_box.set_visible(false);
    }
}

/// Build an album card widget for the artist page, replacing artist name with album year.
fn build_album_card(
    album: &AlbumDisplayInfoWithYear,
    cover_size: i32,
    tile_size: i32,
    stack: WeakRef<ViewStack>,
    db_pool: Arc<SqlitePool>,
    header_btn_stack: WeakRef<ViewStack>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
) -> Box {

    // Cover (scaled to cover_size x cover_size)
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
        picture.add_css_class("album-cover-border");
        picture
    } else {
        let pic = Picture::new();
        pic.set_size_request(cover_size, cover_size);
        pic.add_css_class("album-cover-border");
        pic
    };

    // Album title (bold)
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

    // Album artist label
    let artist_label = create_album_label(
        &album._artist,
        &["album-artist-label"],
        Some(18),
        Some(EllipsizeMode::End),
        false,
        None,
        None,
    );

    // Year label
    let year_text = if let Some(original_release_date_str) = album.original_release_date.clone() {
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

    // Format line (small)
    let format_line = if let Some(ref format) = album.format {
        let format_caps = format.to_uppercase();
        match (album.bit_depth, album.frequency) {
            (Some(bit), Some(freq)) => format!("{} {}/{}", format_caps, bit, format_freq_khz(freq)),
            (None, Some(freq)) => format!("{} {}", format_caps, format_freq_khz(freq)),
            _ => format_caps,
        }
    } else {
        String::new()
    };
    let format_label = create_album_label(
        &format_line,
        &["album-format-label"],
        Some(((cover_size - 16) / 10).max(8)),
        Some(EllipsizeMode::End),
        false,
        None,
        None,
    );
    format_label.set_size_request(cover_size - 16, -1);

    // Album box creation
    let album_tile_box = Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(2) // Changed spacing to 2 for consistency with albums_grid
        .build();
    album_tile_box.set_size_request(tile_size, tile_size + 80);
    album_tile_box.set_hexpand(true);
    album_tile_box.set_vexpand(false);
    album_tile_box.set_halign(Align::Fill);
    album_tile_box.set_valign(Align::Start);
    album_tile_box.append(&cover);

    // Box to ensure consistent height for the title area (2 lines)
    let title_area_box = Box::builder()
        .orientation(Orientation::Vertical)
        .height_request(40) // Explicitly request height for two lines of text + extra buffer
        .margin_top(12)     // Keep the margin from the cover
        .build();
    title_label.set_valign(Align::End); // Align label to the end of its box
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
    unsafe {
        album_tile_box.set_data("album_id", album.id);
    }
    let gesture = GestureClick::builder().build();
    let stack_weak_for_closure = stack.clone();
    let db_pool_clone_for_closure = db_pool.clone();
    let header_btn_stack_weak_for_closure = header_btn_stack.clone();
    let nav_history_clone_for_closure = nav_history.clone();
    let album_id = album.id;
    gesture.connect_pressed(move |_, _, _, _| {
        if let (Some(stack), Some(header_btn_stack)) = (stack_weak_for_closure.upgrade(), header_btn_stack_weak_for_closure.upgrade()) {
            if let Some(current_page) = stack.visible_child_name() {
                nav_history_clone_for_closure.borrow_mut().push(current_page.to_string());
            }
            MainContext::default().spawn_local(
                album_page(
                    stack.downgrade(),
                    db_pool_clone_for_closure.clone(),
                    album_id,
                    header_btn_stack.downgrade(),
                    sender.clone(),
                )
            );
        }
    });
    album_tile_box.add_controller(gesture);
    album_tile_box
}

/// Fetch all albums by a given artist, with display info and year.
pub async fn fetch_album_display_info_by_artist(
    pool: &SqlitePool,
    artist_id: i64,
) -> Result<Vec<AlbumDisplayInfoWithYear>, Error> {
    let rows = query(
        r#"SELECT albums.id, albums.title, albums.year, artists.name as artist, albums.cover_art,
                     tracks.format, tracks.bit_depth, tracks.frequency, albums.dr_value, albums.original_release_date
            FROM albums
            JOIN artists ON albums.artist_id = artists.id
            LEFT JOIN tracks ON tracks.album_id = albums.id
            WHERE albums.artist_id = ?
            GROUP BY albums.id
            ORDER BY albums.year DESC, albums.title COLLATE NOCASE"#,
    )
    .bind(artist_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| AlbumDisplayInfoWithYear {
            id: row.get("id"),
            title: row.get("title"),
            year: row.get("year"),
            _artist: row.get("artist"),
            cover_art: row.get("cover_art"),
            format: row.get("format"),
            bit_depth: row.get("bit_depth"),
            frequency: row.get("frequency"),
            _dr_value: row.get("dr_value"),
            original_release_date: row.get("original_release_date"),
        })
        .collect())
}

/// Album display info with year for artist page.
#[derive(Clone)]
pub struct AlbumDisplayInfoWithYear {
    pub id: i64,
    pub title: String,
    pub year: Option<i32>,
    pub _artist: String,
    pub cover_art: Option<Vec<u8>>,
    pub format: Option<String>,
    pub bit_depth: Option<u32>,
    pub frequency: Option<u32>,
    pub _dr_value: Option<u8>,
    pub original_release_date: Option<String>,
}
