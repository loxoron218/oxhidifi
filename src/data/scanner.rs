use std::{borrow::Cow, error::Error, future::Future, path::Path, pin::Pin, rc::Rc, sync::Arc, thread::spawn};
use std::cell::{Cell, RefCell};

use glib::MainContext;
use gtk4::{Button, Label};
use libadwaita::ViewStack;
use libadwaita::prelude::{ButtonExt, WidgetExt};
use lofty::{probe::Probe, tag::Tag};
use lofty::prelude::{Accessor, AudioFile, TaggedFileExt};
use regex::Regex;
use sqlx::{query, Row, SqlitePool};
use tokio::{fs::File, fs::read_dir, io::AsyncBufReadExt, io::BufReader, runtime::Runtime};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::data::album_artists::get_album_artist;
use crate::data::db::{clear_all_dr_values, fetch_all_folders, insert_or_get_album, insert_or_get_artist, insert_track, remove_album_and_tracks, remove_artists_with_no_albums, remove_folder_and_albums, remove_albums_with_no_tracks, remove_orphaned_tracks};

/// Recursively scan a folder for supported audio files and subfolders.
/// For each audio file, extract tags and insert into the database.
/// Also scans for DR value in .txt/.log files in the folder.
pub fn scan_folder<'a>(
    pool: &'a SqlitePool,
    folder_path: &'a str,
    folder_id: i64,
) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>> + 'a>>
{
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
                if let Err(_) =
                    scan_folder(pool, path.to_str().unwrap_or("INVALID UTF-8"), folder_id).await
                {
                }
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let supported_extensions = ["mp3", "flac", "ogg", "wav", "m4a", "opus", "aiff"];
                if supported_extensions.contains(&ext.to_lowercase().as_str()) {
                }
            }
        }

        // After scanning all entries in a folder, process it as an album
        if let Err(_e) = process_album_folder(pool, Path::new(folder_path), folder_id, dr_value).await {
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

/// Connect the rescan button to update scanning labels based on the visible tab.
pub fn connect_scanning_label_visibility(
    rescan_button: &Button,
    stack: &ViewStack,
    scanning_label_albums: &Label,
    scanning_label_artists: &Label,
) {
    let scanning_label_albums = scanning_label_albums.clone();
    let scanning_label_artists = scanning_label_artists.clone();
    let stack = stack.clone();
    rescan_button.connect_clicked(move |_| {
        let page = stack.visible_child_name().unwrap_or_default();
        if page == "albums" {
            scanning_label_albums.set_visible(true);
            scanning_label_artists.set_visible(false);
        } else if page == "artists" {
            scanning_label_albums.set_visible(false);
            scanning_label_artists.set_visible(true);
        } else {

            // Hide both by default
            scanning_label_albums.set_visible(false);
            scanning_label_artists.set_visible(false);
        }
    });
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
pub fn connect_rescan_button(
    rescan_button: &Button,
    scanning_label: Label,
    sender: UnboundedSender<()>,
    db_pool: Arc<SqlitePool>,
) {
    let scanning_label_rescan = scanning_label.clone();
    let db_pool_rescan = db_pool.clone();
    let sender_rescan = sender.clone();
    rescan_button.connect_clicked(move |_| {
        scanning_label_rescan.set_visible(true);
        let db_pool = db_pool_rescan.clone();
        let sender = sender_rescan.clone();
        spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {

                // Clear all DR values before re-scanning
                if let Err(_) = clear_all_dr_values(&db_pool).await {}
                match fetch_all_folders(&db_pool).await {
                    Ok(folders) => {

                        // Scan all folders on record
                        for folder in &folders {
                            let _ = scan_folder(&db_pool, &folder.path, folder.id).await;
                        }

                        // Get current folder paths on disk
                        let mut folders_on_disk = Vec::new();
                        for folder in &folders {
                            let exists = Path::new(&folder.path).exists();
                            if exists {
                                folders_on_disk.push(folder.id);
                            }
                        }

                        // Remove folders from DB that no longer exist on disk
                        for folder in &folders {
                            if !Path::new(&folder.path).exists() {
                                remove_folder_and_albums(&db_pool, folder.id).await.ok();
                            }
                        }

                        // Remove albums whose folder is missing
                        let albums = query("SELECT id FROM albums").fetch_all(&*db_pool).await.unwrap_or_default();
                        for album in albums {
                            let album_id: i64 = album.get("id");

                            // Fetch all track paths for this album
                            let tracks = query("SELECT path FROM tracks WHERE album_id = ?")
                                .bind(album_id)
                                .fetch_all(&*db_pool)
                                .await
                                .unwrap_or_default();
                            let mut any_track_exists = false;
                            for track in &tracks {
                                let path: String = track.get("path");
                                let exists = Path::new(&path).exists();
                                if exists {
                                    any_track_exists = true;
                                    break;
                                }
                            }
                            if tracks.is_empty() || !any_track_exists {
                                remove_album_and_tracks(&*db_pool, album_id).await.ok();
                            }
                        }
                    }
                    Err(_) => {}
                }
                remove_orphaned_tracks(&db_pool).await.ok();
                remove_albums_with_no_tracks(&db_pool).await.ok();
                remove_artists_with_no_albums(&db_pool).await.ok();
                sender.send(()).ok();

                // UI refresh happens on main thread after receiving signal
            });
        });
    });
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

/// Process a single audio file: extract tags and return relevant data.
async fn process_audio_file(
    path: &Path,
) -> Result<Option<(Tag, String, Option<u32>, Option<u32>, Option<String>, u32, Option<u32>, Option<u32>)>, Box<dyn Error>> {
    let tagged_file = Probe::open(path)?.read()?;
    let tag = tagged_file.primary_tag().cloned();
    if let Some(tag) = tag {
        let title = tag
            .title()
            .map(Cow::into_owned)
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "Unknown Title".to_string())
            });
        let track_no = tag.track();
        let disc_no = tag.disk();
        let duration = tagged_file.properties().duration().as_secs() as u32;
        let format = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase());
        let bit_depth = tagged_file.properties().bit_depth();
        let frequency = tagged_file.properties().sample_rate();
        Ok(Some((tag, title, track_no, disc_no, format, duration, bit_depth.map(|b| b as u32), frequency)))
    } else {
        Ok(None)
    }
}

/// Process an album folder: extract tags from all files, determine album artist, and insert/update DB.
async fn process_album_folder(
    pool: &SqlitePool,
    folder_path: &Path,
    folder_id: i64,
    dr_value: Option<u8>,
) -> Result<(), Box<dyn Error>> {
    let mut audio_files_in_folder = Vec::new();
    let mut dir_entries = read_dir(folder_path).await?;
    while let Some(entry) = dir_entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let supported_extensions = ["mp3", "flac", "ogg", "wav", "m4a", "opus", "aiff"];
                if supported_extensions.contains(&ext.to_lowercase().as_str()) {
                    audio_files_in_folder.push(path);
                }
            }
        }
    }
    if audio_files_in_folder.is_empty() {
        return Ok(());
    }
    let mut tags_in_album = Vec::new();
    let mut album_title: Option<String> = None;
    let mut cover_art: Option<Vec<u8>> = None;
    for path in &audio_files_in_folder {
        if let Ok(tagged_file) = Probe::open(path)?.read() {
            if let Some(tag) = tagged_file.primary_tag() {
                tags_in_album.push(tag.clone());
                if album_title.is_none() {
                    album_title = tag.album().map(Cow::into_owned);
                }
                if cover_art.is_none() {
                    cover_art = tag.pictures().first().map(|pic| pic.data().to_vec());
                }
            }
        }
    }
    let album_title = album_title.unwrap_or_else(|| {
        folder_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Unknown Album".to_string())
    });
    let album_artist_name = get_album_artist(&tags_in_album.iter().collect::<Vec<_>>());
    let album_artist_id = insert_or_get_artist(pool, &album_artist_name).await?;
    let album_id = insert_or_get_album(
        pool,
        &album_title,
        album_artist_id,
        tags_in_album.first().and_then(|t| t.year()).map(|y| y as i32),
        cover_art,
        folder_id,
        dr_value,
    )
    .await?;
    for path in &audio_files_in_folder {
        if let Some((tag, title, track_no, disc_no, format, duration, bit_depth, frequency)) =
            process_audio_file(path).await?
        {
            let artist_name = tag
                .artist()
                .map(Cow::into_owned)
                .unwrap_or_else(|| "Unknown Artist".to_string());
            let artist_id = insert_or_get_artist(pool, &artist_name).await?;
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
        }
    }

    Ok(())
}
