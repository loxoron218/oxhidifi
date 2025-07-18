use std::sync::Arc;

use gdk_pixbuf::{InterpType, Pixbuf, PixbufLoader};
use gdk_pixbuf::prelude::PixbufLoaderExt;
use glib::{markup_escape_text, WeakRef};
use gtk4::{Align, Box, Button, Image, Label, Orientation, Overlay, Picture, ScrolledWindow};
use gtk4::pango::{EllipsizeMode, WrapMode};
use libadwaita::{ActionRow, Clamp, PreferencesGroup, ViewStack};
use libadwaita::prelude::{ActionRowExt, BoxExt, PreferencesGroupExt, WidgetExt};
use sqlx::SqlitePool;

use crate::data::db::{fetch_album_by_id, fetch_artist_by_id, fetch_tracks_by_album};
use crate::utils::formatting::{format_bit_freq, format_duration_hms, format_duration_mmss, format_freq_khz};

/// Build and present the album detail page for a given album ID.
/// Fetches album, artist, and track data asynchronously and constructs the UI.
pub async fn album_page(
    stack: WeakRef<ViewStack>,
    db_pool: Arc<SqlitePool>,
    album_id: i64,
    header_btn_stack: WeakRef<ViewStack>,
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
    let album = fetch_album_by_id(&*db_pool, album_id).await.unwrap();
    let artist = fetch_artist_by_id(&*db_pool, album.artist_id).await.unwrap();
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
fn build_dr_badge(dr: Option<u8>) -> Box {
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
        dr_label.set_tooltip_text(tooltip_text);
        dr_box.append(&dr_label);
        let dr_text_label = build_info_label("Official DR Value", Some("album-technical-label"));
        dr_box.append(&dr_text_label);
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
            .title(&*markup_escape_text(&t.title))
            .subtitle(&*markup_escape_text(&subtitle))
            .build();
        let disc = t.disc_no.unwrap_or(1);
        let track = t.track_no.unwrap_or(0);
        let number_label = Label::builder()
            .label(&format!("{}-{:02}", disc, track))
            .css_classes(["dim-label"])
            .xalign(1.0)
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
        .label(&*markup_escape_text(&album.title))
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
        .label(&*markup_escape_text(&artist.name))
        .halign(Align::Start)
        .wrap(true)
        .wrap_mode(WrapMode::Word)
        .ellipsize(EllipsizeMode::None)
        .hexpand(true)
        .build();
    artist_label.set_xalign(0.0);
    artist_label.set_css_classes(&["album-artist-label"]);
    info_box.append(&artist_label);

    // Year (no label)
    if let Some(year) = album.year {
        let year_label = Label::builder()
            .label(&format!("{}", year))
            .halign(Align::Start)
            .build();
        info_box.append(&year_label);
    }

    // Duration as HH:MM:SS
    let total_length: u32 = tracks.iter().filter_map(|t| t.duration).sum();
    info_box.append(&build_info_label(&format_duration_hms(total_length), Some("album-meta-label")));
    let (bit_depth, freq, format_opt) = tracks
        .iter()
        .find_map(|t| Some((t.bit_depth, t.frequency, t.format.as_ref())))
        .unwrap_or((None, None, None));

    // Bit depth / Freq and Format, with Hi-Res icon aligned to both lines
    let bit_freq_str = format_bit_freq(bit_depth, freq);
    let show_hires = matches!((bit_depth, freq), (Some(bd), Some(fq)) if bd >= 24 && fq >= 88_200);

    // Only build this row if any content
    if show_hires || !bit_freq_str.is_empty() || format_opt.is_some() {
        let outer_row = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .halign(Align::Start)
            .build();

        // Hi-Res or CD icon (tall, left)
        if show_hires {
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale("assets/hires.png", -1, 44, true) {
                let hires_pic = Picture::for_pixbuf(&pixbuf);
                hires_pic.set_halign(Align::Start);
                outer_row.append(&hires_pic);
            }
        } else {

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
            .build();
        if !bit_freq_str.is_empty() {
            lines_box.append(&build_info_label(&bit_freq_str, Some("album-technical-label")));
        }
        if let Some(format) = format_opt {
            if !format.is_empty() {
                lines_box.append(&build_info_label(&format.to_uppercase(), Some("album-technical-label")));
            }
        }
        outer_row.append(&lines_box);
        info_box.append(&outer_row);
    }
    info_box.append(&build_dr_badge(album.dr_value));
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
    let scrolled = ScrolledWindow::builder()
        .child(&group)
        .min_content_height(200)
        .max_content_height(500)
        .vexpand(true)
        .hexpand(true)
        .build();
    scrolled.set_css_classes(&["track-list-container"]);
    page.append(&scrolled);

    // Stack Management
    if let Some(existing) = stack.child_by_name("album_detail") {
        stack.remove(&existing);
    }
    stack.add_titled(&page, Some("album_detail"), "Album");
    stack.set_visible_child_name("album_detail");
    header_btn_stack.set_visible_child_name("back");
}
