use std::{
    cell::{Cell, RefCell},
    cmp::Ordering::{Equal, Greater, Less},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

use glib::{MainContext, WeakRef};
use gtk4::{
    Align::{Center, End, Start},
    Box, Button, EventControllerMotion, Fixed, FlowBox, FlowBoxChild, GestureClick, Justification,
    Label,
    Orientation::{Horizontal, Vertical},
    Overlay, SelectionMode,
    pango::{EllipsizeMode, WrapMode::WordChar},
};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{BoxExt, FixedExt, FlowBoxChildExt, ObjectExt, WidgetExt},
};
use sqlx::{Error, Row, SqlitePool, query};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    data::db::crud::fetch_artist_by_id,
    ui::{
        components::{
            player_bar::PlayerBar,
            tiles::{create_album_cover, create_album_label, create_dr_overlay},
        },
        pages::album_page::album_page,
    },
    utils::{formatting::format_freq_khz, screen::ScreenInfo},
};

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
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
) {
    let page_name = format!("artist_{}", artist_id);

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
        (Some(_), None) => Less,
        (None, Some(_)) => Greater,
        (None, None) => Equal,
    });

    // Build UI
    let vbox = Box::builder()
        .orientation(Vertical)
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
        .halign(Center)
        .justify(Justification::Center)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    vbox.append(&header);

    // Compute dynamic cover and tile size
    let screen_info = ScreenInfo::new();
    let cover_size = screen_info.get_cover_size();
    let tile_size = screen_info.get_tile_size();

    // Albums grid (match main albums grid and album_page)
    let flowbox = FlowBox::builder()
        .valign(Start)
        .max_children_per_line(128)
        .selection_mode(SelectionMode::None)
        .row_spacing(1)
        .column_spacing(0)
        .build();
    flowbox.set_halign(Center);
    for album in albums {
        let album_card = build_album_card(
            &album,
            cover_size,
            tile_size,
            stack.downgrade(),
            db_pool.clone(),
            header_btn_stack.downgrade(),
            right_btn_box.clone(),
            nav_history.clone(),
            sender.clone(),
            page_name.clone(),
            show_dr_badges.clone(),
            use_original_year.clone(),
            player_bar.clone(),
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
    right_btn_box: WeakRef<Clamp>,
    nav_history: Rc<RefCell<Vec<String>>>,
    sender: UnboundedSender<()>,
    artist_page_name: String,
    show_dr_badges: Rc<Cell<bool>>,
    use_original_year: Rc<Cell<bool>>,
    player_bar: PlayerBar,
) -> FlowBoxChild {
    let title_label = create_album_label(
        &album.title,
        &["album-title-label"],
        Some(((cover_size - 16) / 10).max(8)),
        Some(EllipsizeMode::End),
        true,
        Some(WordChar),
        Some(2),
        false, // use_markup: false for plain text
    );
    title_label.set_size_request(cover_size - 16, -1);
    title_label.set_halign(Start);
    title_label.set_xalign(0.0);
    let artist_label = create_album_label(
        &album.artist,
        &["album-artist-label"],
        Some(18),
        Some(EllipsizeMode::End),
        false,
        None,
        None,
        false, // Explicitly set use_markup to false
    );
    artist_label.add_css_class("album-artist-label"); // Ensure this class is applied
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
        false, // use_markup: false for plain text
    );
    format_label.set_halign(Start);
    format_label.set_hexpand(true);
    let year_text = if use_original_year.get() {
        if let Some(original_release_date_str) = album.original_release_date.clone() {
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
        } else if let Some(original_release_date_str) = album.original_release_date.clone() {
            original_release_date_str
                .split('-')
                .next()
                .unwrap_or("N/A")
                .to_string()
        } else {
            String::new()
        }
    };
    let year_label = create_album_label(
        &year_text,
        &["album-format-label"],
        None,
        None,
        false,
        None,
        None,
        false, // Explicitly set use_markup to false
    );
    year_label.set_halign(End);
    year_label.set_hexpand(false);

    // Album box creation
    let album_tile_box = Box::builder().orientation(Vertical).spacing(2).build();

    // tile_size + room for text
    album_tile_box.set_size_request(tile_size, tile_size + 80);
    album_tile_box.set_hexpand(false);
    album_tile_box.set_vexpand(false);
    album_tile_box.set_halign(Start);
    album_tile_box.set_valign(Start);

    // Fixed-size container for cover (new instance per tile)
    let cover_container = Box::new(Vertical, 0);
    cover_container.set_size_request(cover_size, cover_size);
    cover_container.set_halign(Start);
    cover_container.set_valign(Start);
    let cover = create_album_cover(album.cover_art.as_deref(), cover_size);
    cover_container.append(&cover);

    // Overlay for DR badge
    let overlay = Overlay::new();
    overlay.set_size_request(cover_size, cover_size);
    overlay.set_child(Some(&cover_container));
    overlay.set_halign(Start);
    overlay.set_valign(Start);

    // Conditionally add DR badge to overlay, if enabled in settings.
    if show_dr_badges.get() {
        let dr_label = create_dr_overlay(album.dr_value, album.dr_completed).unwrap();
        overlay.add_overlay(&dr_label);
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

    // Fixed-size container for the cover area to ensure consistent sizing
    let cover_fixed = Fixed::new();
    cover_fixed.set_size_request(-1, cover_size);
    cover_fixed.put(&overlay, 0.0, 0.0);
    album_tile_box.append(&cover_fixed);

    // Box to ensure consistent height for the title area (2 lines)
    let title_area_box = Box::builder()
        .orientation(Vertical)
        .height_request(40) // Explicitly request height for two lines of text + extra buffer
        .margin_top(12) // Keep the margin from the cover
        .build();
    title_label.set_valign(End);
    title_area_box.append(&title_label);
    album_tile_box.append(&title_area_box);
    album_tile_box.append(&artist_label);

    // Create a horizontal box to hold format and year labels
    let metadata_box = Box::builder()
        .orientation(Horizontal)
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
    flow_child.set_halign(Start);
    flow_child.set_valign(Start);

    // Add click gesture for navigation
    let stack_weak = stack.clone();
    let db_pool_clone = Arc::clone(&db_pool);
    let header_btn_stack_weak = header_btn_stack.clone();
    let right_btn_box_weak = right_btn_box.clone();
    let sender_clone = sender.clone();
    let album_id = album.id;
    let gesture = GestureClick::builder().build();
    gesture.connect_pressed(move |_, _, _, _| {
        if let (Some(stack), Some(header_btn_stack)) =
            (stack_weak.upgrade(), header_btn_stack_weak.upgrade())
        {
            nav_history.borrow_mut().push(artist_page_name.clone());
            MainContext::default().spawn_local(album_page(
                stack.downgrade(),
                db_pool_clone.clone(),
                album_id,
                header_btn_stack.downgrade(),
                right_btn_box_weak.clone(),
                sender_clone.clone(),
                show_dr_badges.clone(),
                player_bar.clone(),
            ));
        }
    });
    flow_child.add_controller(gesture);
    flow_child
}

/// Fetch all albums by a given artist, with display info and year.
pub async fn fetch_album_display_info_by_artist(
    pool: &SqlitePool,
    artist_id: i64,
) -> Result<Vec<AlbumDisplayInfoWithYear>, Error> {
    let rows = query(
        r#"SELECT albums.id, albums.title, albums.year, artists.name as artist, albums.cover_art,
                     tracks.format, tracks.bit_depth, tracks.frequency, albums.dr_value, albums.dr_completed, albums.original_release_date
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
            artist: row.get("artist"),
            cover_art: row.get::<Option<String>, _>("cover_art").map(PathBuf::from),
            format: row.get("format"),
            bit_depth: row.get("bit_depth"),
            frequency: row.get("frequency"),
            dr_value: row.get("dr_value"),
            dr_completed: row.get("dr_completed"),
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
    pub artist: String,
    pub cover_art: Option<PathBuf>,
    pub format: Option<String>,
    pub bit_depth: Option<u32>,
    pub frequency: Option<u32>,
    pub dr_value: Option<u8>,
    pub dr_completed: bool,
    pub original_release_date: Option<String>,
}
