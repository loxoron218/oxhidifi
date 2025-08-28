use std::sync::Arc;

use sqlx::{Row, SqlitePool};
use tokio::sync::mpsc;

use crate::{data::models::Artist, ui::grids::album_grid_state::AlbumGridItem};

/// Chunk size for data loading - number of items to load in each batch
const DATA_CHUNK_SIZE: usize = 100;

/// Message type for communication between async loader and UI
pub enum DataLoaderMessage {
    AlbumData(Vec<AlbumGridItem>),
    ArtistData(Vec<Artist>),
    Progress(usize, usize), // processed, total
    Completed,
    Error(String),
}

/// Asynchronously loads album data in chunks to prevent UI blocking
///
/// This function performs database queries in background threads and sends
/// data to the UI in chunks, allowing the UI to remain responsive.
pub async fn load_albums_async(
    db_pool: Arc<SqlitePool>,
    sender: mpsc::UnboundedSender<DataLoaderMessage>,
) {
    // First, get the total count of albums
    let total_count: Result<i64, sqlx::Error> = sqlx::query(
        "SELECT COUNT(*) as count FROM (SELECT DISTINCT albums.id FROM albums 
         JOIN artists ON albums.artist_id = artists.id 
         JOIN folders ON albums.folder_id = folders.id)",
    )
    .fetch_one(&*db_pool)
    .await
    .map(|row| row.get("count"));

    let total_count = match total_count {
        Ok(count) => count as usize,
        Err(e) => {
            let _ = sender.send(DataLoaderMessage::Error(format!(
                "Failed to get album count: {}",
                e
            )));
            return;
        }
    };

    // Send initial progress
    let _ = sender.send(DataLoaderMessage::Progress(0, total_count));

    // Load albums in chunks
    let mut offset = 0;
    let mut processed = 0;

    loop {
        // Execute the query to fetch album display information in chunks
        let query_result = sqlx::query(
            r#"
            SELECT
                albums.id,
                albums.title,
                artists.name AS artist,
                albums.year,
                albums.cover_art,
                tracks.format,
                tracks.bit_depth,
                tracks.frequency,
                albums.dr_value,
                albums.dr_completed,
                albums.original_release_date,
                folders.path AS folder_path
            FROM albums
            JOIN artists ON albums.artist_id = artists.id
            LEFT JOIN tracks ON tracks.album_id = albums.id
            JOIN folders ON albums.folder_id = folders.id
            GROUP BY albums.id
            ORDER BY artists.name COLLATE NOCASE, albums.title COLLATE NOCASE
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(DATA_CHUNK_SIZE as i64)
        .bind(offset as i64)
        .fetch_all(&*db_pool)
        .await;

        match query_result {
            Ok(rows) => {
                let rows_len = rows.len();
                if rows_len == 0 {
                    // No more data
                    break;
                }

                // Convert rows to AlbumGridItem structs
                let albums: Vec<AlbumGridItem> = rows
                    .into_iter()
                    .map(|row| AlbumGridItem {
                        id: row.get("id"),
                        title: row.get("title"),
                        artist: row.get("artist"),
                        year: row.get("year"),
                        cover_art: row.get("cover_art"),
                        format: row.get("format"),
                        bit_depth: row.get::<Option<u32>, _>("bit_depth").map(|bd| bd as i32),
                        frequency: row.get::<Option<u32>, _>("frequency").map(|f| f as i32),
                        dr_value: row.get::<Option<u8>, _>("dr_value").map(|dr| dr as i32),
                        dr_completed: row.get("dr_completed"),
                        original_release_date: row.get("original_release_date"),
                        folder_path: std::path::PathBuf::from(row.get::<String, _>("folder_path")),
                    })
                    .collect();

                processed += albums.len();

                // Send progress update
                let _ = sender.send(DataLoaderMessage::Progress(processed, total_count));

                // Send album data chunk
                if !albums.is_empty() {
                    let _ = sender.send(DataLoaderMessage::AlbumData(albums));
                }

                // If we got less than a full chunk, we're done
                if rows_len < DATA_CHUNK_SIZE {
                    break;
                }

                offset += DATA_CHUNK_SIZE;
            }
            Err(e) => {
                let _ = sender.send(DataLoaderMessage::Error(format!(
                    "Failed to load albums: {}",
                    e
                )));
                return;
            }
        }
    }

    // Send completion message
    let _ = sender.send(DataLoaderMessage::Completed);
}

/// Asynchronously loads artist data in chunks to prevent UI blocking
pub async fn load_artists_async(
    db_pool: Arc<SqlitePool>,
    sender: mpsc::UnboundedSender<DataLoaderMessage>,
) {
    // First, get the total count of artists
    let total_count: Result<i64, sqlx::Error> = sqlx::query(
        "SELECT COUNT(*) as count FROM (SELECT DISTINCT artists.id FROM artists 
         WHERE artists.id IN (SELECT DISTINCT artist_id FROM albums))",
    )
    .fetch_one(&*db_pool)
    .await
    .map(|row| row.get("count"));

    let total_count = match total_count {
        Ok(count) => count as usize,
        Err(e) => {
            let _ = sender.send(DataLoaderMessage::Error(format!(
                "Failed to get artist count: {}",
                e
            )));
            return;
        }
    };

    // Send initial progress
    let _ = sender.send(DataLoaderMessage::Progress(0, total_count));

    // Load artists in chunks
    let mut offset = 0;
    let mut processed = 0;

    loop {
        let query_result = sqlx::query(
            "SELECT id, name FROM artists WHERE id IN (SELECT DISTINCT artist_id FROM albums) 
             ORDER BY name COLLATE NOCASE LIMIT ? OFFSET ?",
        )
        .bind(DATA_CHUNK_SIZE as i64)
        .bind(offset as i64)
        .fetch_all(&*db_pool)
        .await;

        match query_result {
            Ok(rows) => {
                let rows_len = rows.len();
                if rows_len == 0 {
                    // No more data
                    break;
                }

                // Convert rows to Artist structs
                let artists: Vec<Artist> = rows
                    .into_iter()
                    .map(|row| Artist {
                        id: row.get("id"),
                        name: row.get("name"),
                    })
                    .collect();

                processed += artists.len();

                // Send progress update
                let _ = sender.send(DataLoaderMessage::Progress(processed, total_count));

                // Send artist data chunk
                if !artists.is_empty() {
                    let _ = sender.send(DataLoaderMessage::ArtistData(artists));
                }

                // If we got less than a full chunk, we're done
                if rows_len < DATA_CHUNK_SIZE {
                    break;
                }

                offset += DATA_CHUNK_SIZE;
            }
            Err(e) => {
                let _ = sender.send(DataLoaderMessage::Error(format!(
                    "Failed to load artists: {}",
                    e
                )));
                return;
            }
        }
    }

    // Send completion message
    let _ = sender.send(DataLoaderMessage::Completed);
}

/// Spawns an async task to load albums and sends data to the UI
///
/// This function creates a channel for communication between the background
/// loading task and the UI, and spawns the loading task on the Tokio runtime.
pub fn spawn_album_loader(
    db_pool: Arc<SqlitePool>,
) -> (
    mpsc::UnboundedReceiver<DataLoaderMessage>,
    tokio::task::JoinHandle<()>,
) {
    let (sender, receiver) = mpsc::unbounded_channel::<DataLoaderMessage>();

    let handle = tokio::spawn(async move {
        load_albums_async(db_pool, sender).await;
    });

    (receiver, handle)
}

/// Spawns an async task to load artists and sends data to the UI
pub fn spawn_artist_loader(
    db_pool: Arc<SqlitePool>,
) -> (
    mpsc::UnboundedReceiver<DataLoaderMessage>,
    tokio::task::JoinHandle<()>,
) {
    let (sender, receiver) = mpsc::unbounded_channel::<DataLoaderMessage>();

    let handle = tokio::spawn(async move {
        load_artists_async(db_pool, sender).await;
    });

    (receiver, handle)
}
