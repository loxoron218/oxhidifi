use std::{borrow::Cow, error::Error, future::Future, path::Path, pin::Pin, rc::Rc, sync::Arc};
use std::cell::{Cell, RefCell};

use glib::MainContext;
use gtk4::Label;
use libadwaita::ViewStack;
use libadwaita::prelude::WidgetExt;
use lofty::{probe::Probe, tag::items::Timestamp};
use lofty::prelude::{Accessor, AudioFile, TaggedFileExt};
use lofty::prelude::ItemKey::{AlbumArtist, OriginalReleaseDate};
use regex::Regex;
use sqlx::{query, Row, SqlitePool};
use tokio::fs::{File, read_dir};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::data::db::db_dr_sync::synchronize_dr_completed_from_store;
use crate::data::db::db_cleanup::{remove_album_and_tracks, remove_albums_with_no_tracks, remove_artists_with_no_albums, remove_folder_and_albums, remove_orphaned_tracks};
use crate::data::db::db_crud::{insert_track, insert_or_get_album, insert_or_get_artist};
use crate::data::db::db_query::fetch_all_folders;

/// Recursively scan a folder for supported audio files and subfolders.
/// For each audio file, extract tags and insert into the database.
/// Also scans for DR value in .txt/.log files in the folder.
pub fn scan_folder<'a>(
    pool: &'a SqlitePool,
    folder_path: &'a str,
    folder_id: i64,
) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>> + 'a>> {
    Box::pin(async move {

        // Scan for DR value in .txt/.log files in this folder
        let dr_value = scan_dr_value(folder_path).await?;
        let mut entries = match read_dir(folder_path).await {
            Ok(e) => e,
            Err(_) => {
                return Ok(());
            }
        };
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {

                // Recurse into subdirectories
                if let Some(path_str) = path.to_str() {
                    if let Err(_) = scan_folder(pool, path_str, folder_id).await {
                    }
                }
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let supported_extensions = ["mp3", "flac", "ogg", "wav", "m4a", "opus", "aiff"];
                if supported_extensions.contains(&ext.to_lowercase().as_str()) {
                    if let Err(_) = process_file(pool, &path, folder_id, dr_value).await {
                    }
                }
            }
        }
        Ok(())
    })
}

/// Create a scanning label widget for UI feedback.
pub fn create_scanning_label() -> Label {
    Label::builder()
        .label("Scanning...")
        .visible(false.into())
        .css_classes(["album-artist-label"])
        .build()
}

/// Listen for scan completion and update label/UI accordingly.
pub fn spawn_scanning_label_refresh_task(
    receiver: Rc<RefCell<UnboundedReceiver<()>>>,
    scanning_label_albums: Rc<Label>,
    scanning_label_artists: Rc<Label>,
    stack: ViewStack,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) {
    let refresh_library_ui_clone = refresh_library_ui.clone();
    let sort_ascending_for_refresh = sort_ascending.clone();
    let sort_ascending_artists = sort_ascending_artists.clone();
    let stack = stack.clone();
    MainContext::default().spawn_local(async move {
        let mut receiver = receiver.borrow_mut();
        while receiver.recv().await.is_some() {
            let page = stack.visible_child_name().unwrap_or_default();
            if page == "albums" {
                scanning_label_albums.set_visible(false);
            } else if page == "artists" {
                scanning_label_artists.set_visible(false);
            } else {
                scanning_label_albums.set_visible(false);
                scanning_label_artists.set_visible(false);
            }
            refresh_library_ui_clone(
                sort_ascending_for_refresh.get(),
                sort_ascending_artists.get(),
            );
        }
    });
}

/// Connect the rescan button to trigger scanning and update the UI.
/// Performs a full library scan, including cleanup of orphaned data.
pub async fn run_full_scan(db_pool: &Arc<SqlitePool>, sender: &UnboundedSender<()>) {
    match fetch_all_folders(db_pool).await {
        Ok(folders) => {

            // Scan all folders on record
            for folder in &folders {
                let _ = scan_folder(db_pool, &folder.path, folder.id).await;
            }

            // Get current folder paths on disk
            let mut folders_on_disk = Vec::new();
            for folder in &folders {
                if Path::new(&folder.path).exists() {
                    folders_on_disk.push(folder.id);
                }
            }

            // Remove folders from DB that no longer exist on disk
            for folder in &folders {
                if !Path::new(&folder.path).exists() {
                    remove_folder_and_albums(db_pool, folder.id).await.ok();
                }
            }

            // Remove albums whose folder is missing
            let albums = query("SELECT id FROM albums").fetch_all(&**db_pool).await.unwrap_or_default();
            for album in albums {
                let album_id: i64 = album.get("id");

                // Fetch all track paths for this album
                let tracks = query("SELECT path FROM tracks WHERE album_id = ?")
                    .bind(album_id)
                    .fetch_all(&**db_pool)
                    .await
                    .unwrap_or_default();
                let mut any_track_exists = false;
                for track in &tracks {
                    let path: String = track.get("path");
                    if Path::new(&path).exists() {
                        any_track_exists = true;
                        break;
                    }
                }
                if tracks.is_empty() || !any_track_exists {
                    remove_album_and_tracks(db_pool, album_id).await.ok();
                }
            }
        }
        Err(_) => {}
    }
    remove_orphaned_tracks(db_pool).await.ok();
    remove_albums_with_no_tracks(db_pool).await.ok();
    remove_artists_with_no_albums(db_pool).await.ok();
    synchronize_dr_completed_from_store(db_pool).await.ok();
    sender.send(()).ok();
}

/// Scan for DR value in .txt/.log files in a folder.
/// Returns the highest valid DR value found, or None if not found.
async fn scan_dr_value(folder_path: &str) -> Result<Option<u8>, Box<dyn Error>> {
    let mut entries = read_dir(folder_path).await?;
    let dr_regex = Regex::new(r"Official DR value:\s*DR(\d+|ERR)|Реальные значения DR:\s*DR(\d+|ERR)|Official EP/Album DR:\s*(\d+|ERR)").unwrap();
    let mut highest_dr: Option<u8> = None;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext = ext.to_lowercase();
            if ext == "txt" || ext == "log" {
                let file = File::open(&path).await;
                if let Ok(file) = file {
                    let reader = BufReader::new(file);
                    let mut lines = reader.lines();
                    loop {
                        match lines.next_line().await {
                            Ok(Some(line)) => {
                                if let Some(caps) = dr_regex.captures(&line) {
                                    for i in 1..=3 { // Check all three potential capture groups
                                        if let Some(dr_str_match) = caps.get(i) {
                                            let dr_str = dr_str_match.as_str();
                                            if dr_str != "ERR" { // Only parse if not "ERR"
                                                if let Ok(dr) = dr_str.parse::<u8>() {
                                                    if (1..=20).contains(&dr) {
                                                        
                                                        // Update highest DR value found
                                                        match highest_dr {
                                                            Some(current_max) if dr > current_max => {
                                                                highest_dr = Some(dr);
                                                            }
                                                            None => {
                                                                highest_dr = Some(dr);
                                                            }
                                                            _ => {} // Keep current max
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            //EOF
                            Ok(None) => break,
                            Err(_) => {
                                // skip this file, but do not abort scan
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(highest_dr)
}

/// Process a single audio file: extract tags, handle missing metadata, and insert into the database.
async fn process_file(
    pool: &SqlitePool,
    path: &Path,
    folder_id: i64,
    dr_value: Option<u8>,
) -> Result<(), Box<dyn Error>> {

    // Probe the file to read metadata. If it fails, we can't process it.
    let tagged_file = match Probe::open(path) {
        Ok(probe) => match probe.read() {
            Ok(tf) => tf,
            Err(_) => {
                return Ok(());
            }
        },
        Err(_) => {
            return Ok(());
        }
    };
    let tag = tagged_file.primary_tag(); // This is an Option<&Tag>
    let properties = tagged_file.properties();

    // --- Extract metadata with fallbacks ---
    // Title: use filename if tag is missing.
    let title = tag
        .and_then(|t| t.title())
        .map(Cow::into_owned)
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(String::from)
                .unwrap_or_else(|| "Unknown Title".to_string())
        });

    // Artist: use "Unknown Artist" if tag is missing.
    let artist_name = tag
        .and_then(|t| t.artist())
        .map(Cow::into_owned)
        .unwrap_or_else(|| "Unknown Artist".to_string());

    // Album: use "Unknown Album" if tag is missing.
    let album_title = tag
        .and_then(|t| t.album())
        .map(Cow::into_owned)
        .unwrap_or_else(|| "Unknown Album".to_string());

    // Album Artist: use the dedicated tag, fallback to track artist.
    let album_artist_name = tag
        .and_then(|t| t.get_string(&AlbumArtist))
        .map(|s| s.to_string())
        .unwrap_or_else(|| artist_name.clone());

    // Other metadata fields with fallbacks to None.
    let year = tag.and_then(|t| t.year()).map(|y| y as i32);
    let track_no = tag.and_then(|t| t.track());
    let disc_no = tag.and_then(|t| t.disk());
    let cover_art = tag.and_then(|t| t.pictures().first().map(|pic| pic.data().to_vec()));
    let original_release_date = tag.and_then(|t| {
        t.get_string(&OriginalReleaseDate)
            .and_then(|date_str| date_str.parse::<Timestamp>().ok())
            .map(|ts| ts.to_string())
    });

    // Extract properties from the file.
    let duration = properties.duration().as_secs() as u32;
    let format = path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_lowercase);
    let bit_depth = properties.bit_depth().map(|b| b as u32);
    let frequency = properties.sample_rate();

    // --- Database Operations ---
    // Insert artists and get their IDs.
    let artist_id = insert_or_get_artist(pool, &artist_name).await?;
    let album_artist_id = insert_or_get_artist(pool, &album_artist_name).await?;

    // Insert album and get its ID.
    let album_id = insert_or_get_album(
        pool,
        &album_title,
        album_artist_id,
        year,
        cover_art,
        folder_id,
        dr_value,
        original_release_date,
    )
    .await?;

    // Insert the track, linking it to the album and artist.
    insert_track(
        pool,
        &title,
        album_id,
        artist_id,
        path.to_string_lossy().as_ref(),
        duration,
        track_no,
        disc_no,
        format,
        bit_depth,
        frequency,
    )
    .await?;

    Ok(())
}
