use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use gdk_pixbuf::{InterpType, Pixbuf, PixbufLoader};
use gdk_pixbuf::prelude::PixbufLoaderExt;
use glib::{MainContext, WeakRef};
use gtk4::{Align, Box, Button, CheckButton, EventControllerMotion, Image, Label, Orientation, Overlay, Picture, PolicyType::Never, ScrolledWindow, Stack, StackTransitionType};
use gtk4::pango::{EllipsizeMode, WrapMode};
use libadwaita::{ActionRow, Clamp, PreferencesGroup, ViewStack};
use libadwaita::prelude::{ActionRowExt, BoxExt, CheckButtonExt, PreferencesGroupExt, WidgetExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::data::db::{fetch_album_by_id, fetch_artist_by_id, fetch_folder_by_id, fetch_tracks_by_album, update_album_dr_completed};
use crate::data::models::Track;
use crate::utils::best_dr_persistence::{AlbumKey, DrValueStore};
use crate::utils::formatting::{format_bit_freq, format_duration_hms, format_duration_mmss, format_freq_khz};

/// Build and present the album detail page for a given album ID.
/// Fetches album, artist, and track data asynchronously and constructs the UI.
pub async fn album_page(
    stack: WeakRef<ViewStack>,
    db_pool: Arc<SqlitePool>,
    album_id: i64,
    header_btn_stack: WeakRef<ViewStack>,
    sender: UnboundedSender<()>,
) {
    let stack = match stack.upgrade() {
        Some(s) => s,
        None => return,
    };
    let header_btn_stack = match header_btn_stack.upgrade() {
        Some(s) => s,
        None => return,
    };

    // Fetch album, artist, and tracks asynchronously
    let album: Rc<crate::data::models::Album> = Rc::new(fetch_album_by_id(&*db_pool, album_id).await.unwrap());
    let artist: Rc<crate::data::models::Artist> = Rc::new(fetch_artist_by_id(&*db_pool, album.artist_id).await.unwrap());
    let folder: Rc<crate::data::models::Folder> = Rc::new(fetch_folder_by_id(&*db_pool, album.folder_id).await.unwrap()); // Fetch folder
    let tracks = fetch_tracks_by_album(&*db_pool, album_id).await.unwrap();
    let horizontal_margin = 32;
    let page = Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_start(horizontal_margin)
        .margin_end(horizontal_margin)
        .build();
    page.add_css_class("album-detail-page");

/// Build the album cover widget, scaling and falling back if needed.
fn build_album_cover(art: &Option<Vec<u8>>) -> Picture {
        if let Some(art) = art {
            let loader = PixbufLoader::new();
            if loader.write(art).is_ok() {
                loader.close().ok();
                if let Some(pixbuf) = loader.pixbuf() {
                    let (width, height) = (pixbuf.width(), pixbuf.height());
                    let scale = f64::min(300.0 / width as f64, 300.0 / height as f64);
                    let new_width = (width as f64 * scale).round() as i32;
                    let new_height = (height as f64 * scale).round() as i32;
                    if let Some(scaled) = pixbuf.scale_simple(new_width, new_height, InterpType::Bilinear) {
                        let pic = Picture::for_pixbuf(&scaled);
                        pic.set_size_request(300, 300);
                        return pic;
                    }
                }
            }
        }
        let pic = Picture::new();
        pic.set_size_request(300, 300);
        pic
    }

/// Build a GTK label with optional CSS class.
fn build_info_label(label: &str, css_class: Option<&str>) -> Label {
        let l = Label::builder()
            .label(label)
            .halign(Align::Start)
            .build();
        if let Some(class) = css_class {
            l.add_css_class(class);
        }
        l
    }

/// Build the DR badge widget for dynamic range value.
fn build_dr_badge(
    album_id: i64,
    dr: Option<u8>,
    dr_completed: bool,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    album: Rc<crate::data::models::Album>,
    artist: Rc<crate::data::models::Artist>,
    folder: Rc<crate::data::models::Folder>,
) -> Box {
    let dr_box = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(4)
        .build();
    let (dr_str, tooltip_text, css_class) = match dr {
        Some(value) => (
            format!("{:02}", value),
            Some("Official Dynamic Range Value"),
            format!("dr-{:02}", value),
        ),
        None => (
            "N/A".to_string(),
            Some("Dynamic Range Value not available"),
            "dr-na".to_string(),
        ),
    };
    let dr_label = Label::builder()
        .label(&dr_str)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();
    dr_label.set_size_request(44, 44);
    dr_label.add_css_class("dr-badge-label");
    dr_label.add_css_class(&css_class);
    let dr_text_label = build_info_label("Official DR Value", Some("album-technical-label"));
    dr_text_label.set_width_chars(18); // Set a fixed width to prevent UI movement
    let checkbox = CheckButton::builder()
        .active(dr_completed)
        .halign(Align::Center)
        .valign(Align::Center)
        .css_classes(vec!["dr-completion-checkbox"])
        .build();
    let stack = Stack::builder()
        .transition_type(StackTransitionType::None)
        .build();
    stack.add_named(&dr_label, Some("dr_label"));
    stack.add_named(&checkbox, Some("checkbox"));
    stack.set_visible_child_name("dr_label"); // Initially show the label
    let overlay = Overlay::new();
    overlay.set_child(Some(&stack)); // Set the stack as the child of the overlay
    overlay.set_tooltip_text(tooltip_text);
    dr_box.append(&overlay);
    dr_box.append(&dr_text_label);
    let checkbox_weak = Rc::new(RefCell::new(checkbox));
    let dr_text_label_weak = Rc::new(RefCell::new(dr_text_label));
    let stack_weak = Rc::new(RefCell::new(stack));

    // Event controller for hover
    let motion_controller = EventControllerMotion::new();
    motion_controller.connect_enter({
        let dr_text_label_weak = dr_text_label_weak.clone();
        let stack_weak = stack_weak.clone();
        move |_, _, _| {
            stack_weak.borrow().set_visible_child_name("checkbox");
            dr_text_label_weak.borrow().set_label("Best DR Value");
        }
    });
    motion_controller.connect_leave({
        let dr_text_label_weak = dr_text_label_weak.clone();
        let stack_weak = stack_weak.clone();
        move |_| {
            stack_weak.borrow().set_visible_child_name("dr_label");
            dr_text_label_weak.borrow().set_label("Official DR Value"); // Revert to original on leave
        }
    });
    overlay.add_controller(motion_controller);

    // Connect checkbox toggled signal
    checkbox_weak.borrow().connect_toggled(move |btn| {
        let db_pool = db_pool.clone();
        let sender = sender.clone();
        let is_completed = btn.is_active();
        let current_db_pool = db_pool.clone();
        let sender = sender.clone();
        let album_rc = album.clone();
        let artist_rc = artist.clone();
        let folder_rc = folder.clone();
        MainContext::default().spawn_local(async move {
            if let Err(_e) = update_album_dr_completed(&*current_db_pool, album_id, is_completed).await {
            }
            if let Err(_e) = sender.send(()) {

                // Handle error if sending fails, e.g., receiver dropped
            }

            // Update DrValueStore for persistence
            let mut dr_store = DrValueStore::load();
            let album_key = AlbumKey {
                title: album_rc.title.clone(),
                artist: artist_rc.name.clone(),
                folder_path: folder_rc.path.clone(),
            };
            if is_completed {
                dr_store.add_dr_value(album_key, dr.unwrap_or(0)); // Store DR value if completed
            } else {
                dr_store.remove_dr_value(&album_key);
            }
            if let Err(_e) = dr_store.save() {
            }
        });
    });

    dr_box
}

/// Build a track row for the album tracklist.
fn build_track_row(t: &crate::data::models::Track) -> ActionRow {
        let mut subtitle_fields = Vec::new();
        if let Some(fmt) = &t.format {
            subtitle_fields.push(fmt.to_uppercase());
        }
        if let Some(bit) = t.bit_depth {
            subtitle_fields.push(format!("{}-Bit", bit));
        }
        if let Some(freq) = t.frequency {
            subtitle_fields.push(format!("{} kHz", format_freq_khz(freq)));
        }
        let subtitle = subtitle_fields.join(" · ");
        let row = ActionRow::builder()
            .title(glib::markup_escape_text(&t.title))
            .subtitle(glib::markup_escape_text(&subtitle))
            .build();
        let disc = t.disc_no.unwrap_or(1);
        let track = t.track_no.unwrap_or(0);
        let number_label = Label::builder()
            .label(&format!("{}-{:02}", disc, track))
            .css_classes(["dim-label"])
            .xalign(0.0)
            .width_chars(5)
            .build();
        number_label.set_margin_end(16);
        row.add_prefix(&number_label);
        if let Some(length) = t.duration.map(format_duration_mmss) {
            let length_label = Label::builder()
                .label(&length)
                .css_classes(["dim-label"])
                .xalign(1.0)
                .build();
            length_label.set_margin_end(8);
            row.add_suffix(&length_label);
        }
        let play_pause_button = Button::builder()
            .icon_name("media-playback-start")
            .css_classes(["flat"])
            .halign(Align::End)
            .build();
        row.add_suffix(&play_pause_button);
        row
    }

    // Header
    let header = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(32)
        .margin_start(horizontal_margin)
        .margin_end(horizontal_margin)
        .css_classes(["album-header"])
        .build();
    if album.cover_art.is_some() {
        header.set_margin_top(32);
    }
    let cover = build_album_cover(&album.cover_art);
    let overlay = Overlay::new();
    overlay.set_halign(Align::Start);
    overlay.set_valign(Align::Start);
    overlay.set_child(Some(&cover));
    let play_pause_button = Button::builder()
        .icon_name("media-playback-start")
        .css_classes(["play-pause-button", "album-cover-play"])
        .build();
    play_pause_button.set_size_request(56, 56);
    play_pause_button.set_valign(Align::End);
    play_pause_button.set_halign(Align::End);
    play_pause_button.set_margin_bottom(12);
    play_pause_button.set_margin_end(12);
    overlay.add_overlay(&play_pause_button);
    header.append(&overlay);
    let info_box = Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .halign(Align::Start)
        .valign(Align::End)
        .hexpand(true)
        .css_classes(["album-info-box"])
        .build();
    let title_label = Label::builder()
        .label(&album.title)
        .halign(Align::Start)
        .wrap(true)
        .wrap_mode(WrapMode::Word)
        .ellipsize(EllipsizeMode::None)
        .hexpand(true)
        .build();
    title_label.set_xalign(0.0);
    title_label.set_css_classes(&["album-title-label"]);
    info_box.append(&title_label);

    // Artist name (regular)
    let artist_label = Label::builder()
        .label(&artist.name)
        .halign(Align::Start)
        .wrap(true)
        .wrap_mode(WrapMode::Word)
        .ellipsize(EllipsizeMode::None)
        .hexpand(true)
        .build();
    artist_label.set_xalign(0.0);
    artist_label.set_css_classes(&["album-artist-label"]);
    info_box.append(&artist_label);

    // Year
    let mut year_display_text = String::new();
    let release_year = album.year;
    let original_release_year = album.original_release_date.as_ref().and_then(|s| s.split('-').next().map(|y| y.to_string()));
    match (release_year, original_release_year) {
        (Some(r_year), Some(o_year)) => {
            if r_year.to_string() == o_year {
                year_display_text = format!("{}", r_year);
            } else {
                year_display_text = format!("{} / {}", o_year, r_year);
            }
        },
        (Some(r_year), None) => {
            year_display_text = format!("{}", r_year);
        },
        (None, Some(o_year)) => {
            year_display_text = format!("{}", o_year);
        },
        (None, None) => {}
    }
    if !year_display_text.is_empty() {
        let year_label = Label::builder()
            .label(&year_display_text)
            .halign(Align::Start)
            .build();
        info_box.append(&year_label);
    }

    // Number of songs in the album
    let total_songs_count = tracks.len();
    if total_songs_count > 0 {
        info_box.append(&build_info_label(&format!("{} Songs", total_songs_count), Some("album-meta-label")));
    }

    // Duration as HH:MM:SS
    let total_length: u32 = tracks.iter().filter_map(|t| t.duration).sum();
    info_box.append(&build_info_label(&format_duration_hms(total_length), Some("album-meta-label")));
    let (most_common_bit_depth, most_common_freq, most_common_format_opt) =
        get_most_common_track_properties(&tracks);

    // Helper to determine if a format is considered "lossy"
    fn is_lossy_format(format: &Option<String>) -> bool {
        matches!(format.as_deref(), Some("mp3") | Some("aac") | Some("ogg") | Some("wma"))
    }

    // Calculate if the album is mainly in a lossy format
    let total_tracks = tracks.len();
    let lossy_tracks_count = tracks.iter()
        .filter(|t| is_lossy_format(&t.format))
        .count();
    let is_lossy_album = total_tracks > 0 && (lossy_tracks_count as f64 / total_tracks as f64) > 0.5;

    // Calculate if the album is mainly Hi-Res
    let hires_tracks_count = tracks.iter()
        .filter(|t| matches!((t.bit_depth, t.frequency), (Some(bd), Some(fq)) if bd >= 24 && fq >= 88_200))
        .count();
    let show_hires = total_tracks > 0 && (hires_tracks_count as f64 / total_tracks as f64) > 0.5;

    // Bit depth / Freq and Format, with Hi-Res icon aligned to both lines
    let bit_freq_str = format_bit_freq(most_common_bit_depth, most_common_freq);

    // Only build this row if any content
    if show_hires || is_lossy_album || !bit_freq_str.is_empty() || most_common_format_opt.is_some() {
        let outer_row = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .halign(Align::Start)
            .margin_start(3)
            .build();

        // Hi-Res, Lossy, or CD icon (tall, left)
        if show_hires {
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale("assets/hires.png", -1, 40, true) {
                let hires_pic = Picture::for_pixbuf(&pixbuf);
                hires_pic.set_size_request(40, 40);
                hires_pic.set_halign(Align::Start);
                outer_row.append(&hires_pic);
            }
        } else if is_lossy_album {

            // Use musical note icon for lossy albums
            let lossy_icon = Image::from_icon_name("audio-x-generic-symbolic");
            lossy_icon.set_pixel_size(44);
            lossy_icon.set_halign(Align::Start);
            outer_row.append(&lossy_icon);
        }
        else {

            // Use symbolic CD icon from system theme
            let cd_icon = Image::from_icon_name("media-optical-symbolic");
            cd_icon.set_pixel_size(44);
            cd_icon.set_halign(Align::Start);
            outer_row.append(&cd_icon);
        }

        // Right: vertical box with bit/freq and format
        let lines_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .halign(Align::Start)
            .valign(Align::Center)
            .margin_start(12)
            .build();
        if !bit_freq_str.is_empty() {
            lines_box.append(&build_info_label(&bit_freq_str, Some("album-technical-label")));
        }
        if let Some(format) = most_common_format_opt {
            if !format.is_empty() {
                lines_box.append(&build_info_label(&format.to_uppercase(), Some("album-technical-label")));
            }
        }
        outer_row.append(&lines_box);
        info_box.append(&outer_row);
    }
    let dr_store = DrValueStore::load();
    let album_key = AlbumKey {
        title: album.title.clone(),
        artist: artist.name.clone(),
        folder_path: folder.path.clone(),
    };
    let is_completed = dr_store.contains(&album_key);
    info_box.append(&build_dr_badge(
        album.id,
        album.dr_value,
        is_completed,
        db_pool.clone(),
        sender.clone(),
        Rc::clone(&album),
        Rc::clone(&artist),
        Rc::clone(&folder),
    ));
    header.append(&info_box);

    // Track List
    let group = PreferencesGroup::builder()
        .margin_start(horizontal_margin)
        .margin_end(horizontal_margin)
        .build();
    for t in &tracks {
        group.add(&build_track_row(t));
    }

    // Layout
    let clamp = Clamp::builder()
        .maximum_size(1000)
        .child(&header)
        .build();
    page.append(&clamp);
    page.append(&group);
    page.set_margin_bottom(32); // Add margin to the bottom of the page

    // Create a ScrolledWindow for the entire page
    let page_scrolled_window = ScrolledWindow::builder()
        .child(&page)
        .vexpand(true)
        .hexpand(true)
        .hscrollbar_policy(Never) // Disable horizontal scrollbar
        .build();

    // Stack Management
    if let Some(existing) = stack.child_by_name("album_detail") {
        stack.remove(&existing);
    }
    stack.add_titled(&page_scrolled_window, Some("album_detail"), "Album"); // Add the new scrolled window to stack
    stack.set_visible_child_name("album_detail");
    header_btn_stack.set_visible_child_name("back");
}

/// Helper function to get the most common bit depth, frequency, and format from a list of tracks.
fn get_most_common_track_properties(
    tracks: &[Track],
) -> (Option<u32>, Option<u32>, Option<String>) {
    let mut bit_depth_counts: HashMap<u32, usize> = HashMap::new();
    let mut freq_counts: HashMap<u32, usize> = HashMap::new();
    let mut format_counts: HashMap<String, usize> = HashMap::new();
    for track in tracks {
        if let Some(bd) = track.bit_depth {
            *bit_depth_counts.entry(bd).or_insert(0) += 1;
        }
        if let Some(fq) = track.frequency {
            *freq_counts.entry(fq).or_insert(0) += 1;
        }
        if let Some(fmt) = &track.format {
            *format_counts.entry(fmt.clone()).or_insert(0) += 1;
        }
    }
    let most_common_bit_depth = bit_depth_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(bd, _)| bd);
    let most_common_freq = freq_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(fq, _)| fq);
    let most_common_format = format_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(fmt, _)| fmt);
    (most_common_bit_depth, most_common_freq, most_common_format)
}
