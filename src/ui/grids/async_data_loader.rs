use std::{path::PathBuf, sync::Arc};

use sqlx::{Row, SqlitePool, query};
use tokio::{
    spawn,
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::JoinHandle,
};

use crate::{data::models::Artist, ui::grids::album_grid_state::AlbumGridItem};

/// Chunk size for data loading - number of items to load in each batch
///
/// This constant determines how many records are loaded and sent to the UI
/// in each iteration to maintain responsiveness.
const DATA_CHUNK_SIZE: usize = 100;

/// Message type for communication between async loader and UI
///
/// This enum defines the different types of messages that can be sent from
/// the async data loader to the UI components.
pub enum DataLoaderMessage {
    /// Contains a chunk of album data to be displayed
    AlbumData(Vec<AlbumGridItem>),
    /// Contains a chunk of artist data to be displayed
    ArtistData(Vec<Artist>),
    /// Progress update message (processed items, total items)
    Progress(usize, usize),
    /// Indicates that all data has been loaded successfully
    Completed,
    /// Indicates an error occurred during data loading
    Error(String),
}

/// Asynchronously loads album data in chunks to prevent UI blocking
///
/// This function performs database queries in background threads and sends
/// data to the UI in chunks, allowing the UI to remain responsive. It first
/// queries the total count of albums to enable progress songing, then loads
/// album data in chunks using pagination.
///
/// # Parameters
///
/// * `db_pool` - Shared database connection pool
/// * `sender` - Channel sender for communicating with the UI
///
/// # Database Query Details
///
/// The query joins albums with artists and folders, and left joins with songs
/// to get format information. It groups by album ID to avoid duplicates and
/// orders by artist name and album title (case-insensitive).
pub async fn load_albums_async(
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<DataLoaderMessage>,
) {
    // First, get the total count of albums for progress songing
    let total_count: Result<i64, sqlx::Error> = query(
        "SELECT COUNT(*) as count FROM (SELECT DISTINCT albums.id FROM albums
         JOIN artists ON albums.artist_id = artists.id
         JOIN folders ON albums.folder_id = folders.id)",
    )
    .fetch_one(&*db_pool)
    .await
    .map(|row| row.get("count"));

    // Handle potential error in getting total count
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

    // Send initial progress update (0 items processed)
    let _ = sender.send(DataLoaderMessage::Progress(0, total_count));

    // Initialize pagination variables
    let mut offset = 0;
    let mut processed = 0;

    // Load albums in chunks using pagination
    loop {
        // Execute the query to fetch album display information in chunks
        let query_result = query(
            r#"
            SELECT
                albums.id,
                albums.title,
                artists.name AS artist,
                albums.year,
                albums.cover_art,
                songs.format,
                songs.bit_depth,
                songs.sample_rate,
                albums.dr_value,
                albums.dr_is_best,
                albums.original_release_date,
                folders.path AS folder_path
            FROM albums
            JOIN artists ON albums.artist_id = artists.id
            LEFT JOIN songs ON songs.album_id = albums.id
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

                // If no rows returned, we've loaded all data
                if rows_len == 0 {
                    break;
                }

                // Convert database rows to AlbumGridItem structs
                let albums: Vec<AlbumGridItem> = rows
                    .into_iter()
                    .map(|row| AlbumGridItem {
                        id: row.get("id"),
                        title: row.get("title"),
                        artist: row.get("artist"),
                        year: row.get("year"),
                        cover_art: row.get("cover_art"),
                        format: row.get("format"),

                        // Convert u32 to i32 for compatibility with AlbumGridItem
                        bit_depth: row.get::<Option<u32>, _>("bit_depth").map(|bd| bd as i32),

                        // Convert u32 to i32 for compatibility with AlbumGridItem
                        sample_rate: row.get::<Option<u32>, _>("sample_rate").map(|f| f as i32),

                        // Convert u8 to i32 for compatibility with AlbumGridItem
                        dr_value: row.get::<Option<u8>, _>("dr_value").map(|dr| dr as i32),
                        dr_is_best: row.get("dr_is_best"),
                        original_release_date: row.get("original_release_date"),

                        // Convert string path to PathBuf for folder_path
                        folder_path: PathBuf::from(row.get::<String, _>("folder_path")),
                    })
                    .collect();

                // Update processed count
                processed += albums.len();

                // Send progress update to UI
                let _ = sender.send(DataLoaderMessage::Progress(processed, total_count));

                // Send album data chunk to UI if not empty
                if !albums.is_empty() {
                    let _ = sender.send(DataLoaderMessage::AlbumData(albums));
                }

                // If we got less than a full chunk, we're done loading
                if rows_len < DATA_CHUNK_SIZE {
                    break;
                }

                // Move to next chunk
                offset += DATA_CHUNK_SIZE;
            }
            Err(e) => {
                // Handle database query error
                let _ = sender.send(DataLoaderMessage::Error(format!(
                    "Failed to load albums: {}",
                    e
                )));
                return;
            }
        }
    }

    // Send completion message to indicate all albums have been loaded
    let _ = sender.send(DataLoaderMessage::Completed);
}

/// Asynchronously loads artist data in chunks to prevent UI blocking
///
/// This function performs database queries in background threads to load
/// artist data and sends it to the UI in chunks. It first queries the total
/// count of artists for progress songing, then loads artist data in chunks
/// using pagination.
///
/// # Parameters
///
/// * `db_pool` - Shared database connection pool
/// * `sender` - Channel sender for communicating with the UI
pub async fn load_artists_async(
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<DataLoaderMessage>,
) {
    // First, get the total count of artists for progress songing
    let total_count: Result<i64, sqlx::Error> = query(
        "SELECT COUNT(*) as count FROM (SELECT DISTINCT artists.id FROM artists
         WHERE artists.id IN (SELECT DISTINCT artist_id FROM albums))",
    )
    .fetch_one(&*db_pool)
    .await
    .map(|row| row.get("count"));

    // Handle potential error in getting total count
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

    // Send initial progress update (0 items processed)
    let _ = sender.send(DataLoaderMessage::Progress(0, total_count));

    // Initialize pagination variables
    let mut offset = 0;
    let mut processed = 0;

    // Load artists in chunks using pagination
    loop {
        // Execute query to fetch artists in chunks
        let query_result = query(
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

                // If no rows returned, we've loaded all data
                if rows_len == 0 {
                    break;
                }

                // Convert database rows to Artist structs
                let artists: Vec<Artist> = rows
                    .into_iter()
                    .map(|row| Artist {
                        id: row.get("id"),
                        name: row.get("name"),
                    })
                    .collect();

                // Update processed count
                processed += artists.len();

                // Send progress update to UI
                let _ = sender.send(DataLoaderMessage::Progress(processed, total_count));

                // Send artist data chunk to UI if not empty
                if !artists.is_empty() {
                    let _ = sender.send(DataLoaderMessage::ArtistData(artists));
                }

                // If we got less than a full chunk, we're done loading
                if rows_len < DATA_CHUNK_SIZE {
                    break;
                }

                // Move to next chunk
                offset += DATA_CHUNK_SIZE;
            }
            Err(e) => {
                // Handle database query error
                let _ = sender.send(DataLoaderMessage::Error(format!(
                    "Failed to load artists: {}",
                    e
                )));
                return;
            }
        }
    }

    // Send completion message to indicate all artists have been loaded
    let _ = sender.send(DataLoaderMessage::Completed);
}

/// Spawns an async task to load albums and sends data to the UI
///
/// This function creates a channel for communication between the background
/// loading task and the UI, and spawns the loading task on the Tokio runtime.
/// It returns the receiver end of the channel and the join handle for the task.
///
/// # Parameters
///
/// * `db_pool` - Shared database connection pool
///
/// # Returns
///
/// A tuple containing:
/// * `UnboundedReceiver<DataLoaderMessage>` - Receiver for data loader messages
/// * `JoinHandle<()>` - Handle for the spawned async task
pub fn spawn_album_loader(
    db_pool: Arc<SqlitePool>,
) -> (UnboundedReceiver<DataLoaderMessage>, JoinHandle<()>) {
    // Create an unbounded channel for communication between loader and UI
    let (sender, receiver) = unbounded_channel::<DataLoaderMessage>();

    // Spawn the async task to load albums
    let handle = spawn(async move {
        load_albums_async(db_pool, sender).await;
    });

    (receiver, handle)
}

/// Spawns an async task to load artists and sends data to the UI
///
/// This function creates a channel for communication between the background
/// loading task and the UI, and spawns the loading task on the Tokio runtime.
/// It returns the receiver end of the channel and the join handle for the task.
///
/// # Parameters
///
/// * `db_pool` - Shared database connection pool
///
/// # Returns
///
/// A tuple containing:
/// * `UnboundedReceiver<DataLoaderMessage>` - Receiver for data loader messages
/// * `JoinHandle<()>` - Handle for the spawned async task
pub fn spawn_artist_loader(
    db_pool: Arc<SqlitePool>,
) -> (UnboundedReceiver<DataLoaderMessage>, JoinHandle<()>) {
    // Create an unbounded channel for communication between loader and UI
    let (sender, receiver) = unbounded_channel::<DataLoaderMessage>();

    // Spawn the async task to load artists
    let handle = spawn(async move {
        load_artists_async(db_pool, sender).await;
    });

    (receiver, handle)
}
