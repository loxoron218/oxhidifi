use std::{path::Path, sync::Arc};

use sqlx::{Row, SqlitePool, query};
use tokio::sync::mpsc::UnboundedSender;

use crate::data::{
    db::{
        cleanup::{
            remove_album_and_tracks, remove_albums_with_no_tracks, remove_artists_with_no_albums,
            remove_folder_and_albums, remove_orphaned_tracks,
        },
        dr_sync::synchronize_dr_is_best_from_store,
        query::fetch_all_folders,
    },
    scanner::scan_folder_parallel,
};

/// Initiates a full scan of all configured music folders, updates the database,
/// and performs necessary cleanup operations.
///
/// This function first fetches all known folders from the database, then recursively
/// scans each one to process audio files and update metadata. After scanning, it
/// performs several cleanup tasks:
/// 1. Removes folders from the database that no longer exist on disk.
/// 2. Removes albums from the database that have no existing tracks on disk.
/// 3. Removes orphaned tracks (files no longer existing on disk).
/// 4. Removes albums with no associated tracks.
/// 5. Removes artists with no associated albums.
/// 6. Synchronizes DR completion statuses from the persistent store.
/// Finally, it sends a signal to the UI to indicate scan completion.
///
/// # Arguments
/// * `db_pool` - An `Arc` reference to the SQLite database connection pool.
/// * `sender` - An `UnboundedSender` to send a signal to the UI upon scan completion.
pub async fn run_full_scan(db_pool: &Arc<SqlitePool>, sender: &UnboundedSender<()>) {
    // Fetch all folders from the database.
    let folders = match fetch_all_folders(db_pool).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error fetching folders for full scan: {}", e);

            // Even if fetching folders fails, we still want to attempt cleanup
            // and send a completion signal.
            return;
        }
    };

    // Iterate through each folder and scan it. Errors during individual folder
    // scans are logged within `scan_folder` and do not prevent the overall scan
    // from continuing.
    for folder in &folders {
        // Use the parallel scanning function for improved performance
        if let Err(e) = scan_folder_parallel(Arc::clone(db_pool), &folder.path, folder.id, 4).await
        {
            eprintln!("Error scanning folder {}: {}", folder.path.display(), e);
        }
    }

    // --- Cleanup Operations ---
    // Remove folders from the DB that no longer exist on disk.
    // This iterates over folders fetched *before* the scan, ensuring that if
    // a folder was added during the scan, it's not prematurely removed.
    for folder in &folders {
        if !folder.path.exists() {
            if let Err(e) = remove_folder_and_albums(db_pool, folder.id).await {
                eprintln!(
                    "Error removing folder and albums for {}: {}",
                    folder.path.display(),
                    e
                );
            }
        }
    }
    let albums_to_check = match query("SELECT id FROM albums").fetch_all(&**db_pool).await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error fetching albums for cleanup: {}", e);

            // Continue with other cleanup tasks even if this fails.
            Vec::new()
        }
    };
    for album_row in albums_to_check {
        let album_id: i64 = album_row.get("id");

        // Check if any track for this album still exists on disk.
        let tracks_exist = match query("SELECT path FROM tracks WHERE album_id = ?")
            .bind(album_id)
            .fetch_all(&**db_pool)
            .await
        {
            Ok(tracks) => tracks.into_iter().any(|r| {
                let path_str: String = r.get("path");
                Path::new(&path_str).exists()
            }),
            Err(e) => {
                eprintln!("Error checking tracks for album {}: {}", album_id, e);

                // Assume tracks don't exist if we can't query them.
                false
            }
        };
        if !tracks_exist {
            if let Err(e) = remove_album_and_tracks(db_pool, album_id).await {
                eprintln!(
                    "Error removing album and tracks for album {}: {}",
                    album_id, e
                );
            }
        }
    }

    // Perform general cleanup operations using dedicated functions.
    if let Err(e) = remove_orphaned_tracks(db_pool).await {
        eprintln!("Error removing orphaned tracks: {}", e);
    }
    if let Err(e) = remove_albums_with_no_tracks(db_pool).await {
        eprintln!("Error removing albums with no tracks: {}", e);
    }
    if let Err(e) = remove_artists_with_no_albums(db_pool).await {
        eprintln!("Error removing artists with no albums: {}", e);
    }
    if let Err(e) = synchronize_dr_is_best_from_store(db_pool).await {
        eprintln!("Error synchronizing DR best status: {}", e);
    }

    // Signal UI that scan is complete.
    if let Err(e) = sender.send(()) {
        eprintln!("Error sending scan completion signal: {}", e);
    }
}
