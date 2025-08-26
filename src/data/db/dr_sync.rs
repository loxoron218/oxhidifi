use std::{collections::HashMap, path::PathBuf, sync::Arc};

use sqlx::{Result, Row, SqlitePool, query};

use crate::utils::best_dr_persistence::{AlbumKey, DrValueStore};

/// Synchronizes the `dr_completed` status in the database with the JSON store.
/// This function optimizes database queries by fetching all artists and folders once,
/// then using HashMaps for quick lookups to construct `AlbumKey`s.
pub async fn synchronize_dr_completed_from_store(pool: &SqlitePool) -> Result<()> {
    let dr_store = DrValueStore::load();

    // Fetch all artists and folders once for efficient lookups
    let artists_map: HashMap<i64, String> = query("SELECT id, name FROM artists")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| (row.get("id"), row.get("name")))
        .collect();
    let folders_map: HashMap<i64, PathBuf> = query("SELECT id, path FROM folders")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| (row.get("id"), PathBuf::from(row.get::<String, _>("path"))))
        .collect();

    // Fetch all albums from the database to ensure we can update all their dr_completed statuses
    let all_albums = query("SELECT id, title, artist_id, folder_id, dr_completed FROM albums")
        .fetch_all(pool)
        .await?;
        
    // Process each album to synchronize dr_completed status
    for album_row in all_albums {
        let album_id: i64 = album_row.get("id");
        let title: String = album_row.get("title");
        let artist_id: i64 = album_row.get("artist_id");
        let folder_id: i64 = album_row.get("folder_id");
        let current_dr_completed: bool = album_row.get("dr_completed");

        // Use cached maps to get artist name and folder path
        let artist_name = artists_map.get(&artist_id).cloned().unwrap_or_default();
        let folder_path = folders_map.get(&folder_id).cloned().unwrap_or_default();
        let album_key = AlbumKey {
            title,
            artist: artist_name,
            folder_path: folder_path.clone(),
        };

        // Determine if the album should be marked as DR completed based on the DrValueStore
        let should_be_completed = dr_store.contains(&album_key);

        // Skip to next album if status is already correct
        if should_be_completed == current_dr_completed {
            continue;
        }

        // Update the database since the status needs to change
        // This function is still in db.rs, will be moved to crud.rs later.
        // For now, it's called directly from here.
        query("UPDATE albums SET dr_completed = ? WHERE id = ?")
            .bind(should_be_completed)
            .bind(album_id)
            .execute(pool)
            .await?;
    }
    
    Ok(())
}
/// Synchronizes the `dr_completed` status in the database with the JSON store in the background.
/// This function runs asynchronously and can optionally report progress through a callback.
pub async fn synchronize_dr_completed_background(
    pool: Arc<SqlitePool>,
    update_callback: Option<Box<dyn Fn(String) + Send>>,
) -> Result<()> {
    let dr_store = DrValueStore::load();

    // Fetch all artists and folders once for efficient lookups
    let artists_map: HashMap<i64, String> = query("SELECT id, name FROM artists")
        .fetch_all(&*pool)
        .await?
        .into_iter()
        .map(|row| (row.get("id"), row.get("name")))
        .collect();
    let folders_map: HashMap<i64, PathBuf> = query("SELECT id, path FROM folders")
        .fetch_all(&*pool)
        .await?
        .into_iter()
        .map(|row| (row.get("id"), PathBuf::from(row.get::<String, _>("path"))))
        .collect();

    // Fetch all albums from the database
    let all_albums = query("SELECT id, title, artist_id, folder_id, dr_completed FROM albums")
        .fetch_all(&*pool)
        .await?;
    let total_albums = all_albums.len();
    
    // Process each album to synchronize dr_completed status
    for (index, album_row) in all_albums.into_iter().enumerate() {
        let album_id: i64 = album_row.get("id");
        let title: String = album_row.get("title");
        let artist_id: i64 = album_row.get("artist_id");
        let folder_id: i64 = album_row.get("folder_id");
        let current_dr_completed: bool = album_row.get("dr_completed");

        // Use cached maps to get artist name and folder path
        let artist_name = artists_map.get(&artist_id).cloned().unwrap_or_default();
        let folder_path = folders_map.get(&folder_id).cloned().unwrap_or_default();
        let album_key = AlbumKey {
            title,
            artist: artist_name,
            folder_path: folder_path.clone(),
        };

        // Determine if the album should be marked as DR completed
        let should_be_completed = dr_store.contains(&album_key);

        // Update the database if status needs to change
        if should_be_completed != current_dr_completed {
            query("UPDATE albums SET dr_completed = ? WHERE id = ?")
                .bind(should_be_completed)
                .bind(album_id)
                .execute(&*pool)
                .await?;
        }

        // Report progress if callback is provided
        if update_callback.is_none() {
            continue;
        }
        let callback = update_callback.as_ref().unwrap();
        let progress = format!("DR Sync: {}/{} albums processed", index + 1, total_albums);
        callback(progress);
    }
    Ok(())
}
