use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use sqlx::{QueryBuilder, Result, Row, Sqlite, SqlitePool, Transaction, query};

use crate::{
    data::models::{Album, Artist, Folder, Track},
    utils::performance_monitor::get_metrics,
};

#[derive(Debug)]
pub struct AlbumForInsert {
    /// The title of the album.
    pub title: String,
    /// The ID of the primary artist associated with the album.
    pub artist_id: i64,
    /// The ID of the folder where the album's files are located.
    pub folder_id: i64,
    /// The release year of the album, if available.
    pub year: Option<i32>,
    /// The path to the album's cover art image file, if available.
    pub cover_art_path: Option<PathBuf>,
    /// The Dynamic Range (DR) value of the album, if calculated or available.
    pub dr_value: Option<u8>,
    /// The original release date of the album, typically in "YYYY-MM-DD" format.
    pub original_release_date: Option<String>,
}

#[derive(Debug)]
pub struct TrackForInsert {
    /// The title of the track.
    pub title: String,
    /// The ID of the album to which this track belongs.
    pub album_id: i64,
    /// The ID of the primary artist for this track.
    pub artist_id: i64,
    /// The absolute file system path of the track file.
    pub path: PathBuf,
    /// The duration of the track in seconds.
    pub duration: Option<u32>,
    /// The track number within its album (e.g., 1, 2, 3...).
    pub track_no: Option<u32>,
    /// The disc number if the album spans multiple discs.
    pub disc_no: Option<u32>,
    /// The audio format of the track (e.g., "FLAC", "MP3", "WAV").
    pub format: Option<String>,
    /// The bit depth of the audio (e.g., 16, 24).
    pub bit_depth: Option<u32>,
    /// The sample rate frequency of the audio (e.g., 44100, 96000).
    pub frequency: Option<u32>,
}

/// Inserts a new folder into the database if it doesn't already exist,
/// or returns the ID of the existing folder if a matching path is found.
///
/// This optimized version completely eliminates conditional logic in Rust
/// by using a single SQL query that handles both cases.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `path` - The file system path of the folder.
///
/// # Returns
/// A `Result` containing the ID (`i64`) of the inserted or existing folder on success,
/// or an `sqlx::Error` on failure.
pub async fn insert_or_get_folder(pool: &SqlitePool, path: &Path) -> Result<i64> {
    get_metrics().record_db_operation();
    let path_str = path.to_str().unwrap_or_default();

    // Use a single query that handles both insert and select cases
    let row = query(
        "INSERT OR IGNORE INTO folders (path) VALUES (?);
         SELECT id FROM folders WHERE path = ?",
    )
    .bind(path_str)
    .bind(path_str)
    .fetch_one(pool)
    .await?;
    Ok(row.get(0))
}

/// Enhanced batch processing with better error handling and performance optimizations
pub async fn upsert_tracks_batch_enhanced(
    tx: &mut Transaction<'_, Sqlite>,
    tracks: &[TrackForInsert],
    batch_size: usize,
) -> Result<()> {
    if tracks.is_empty() {
        return Ok(());
    }

    // Process in chunks to avoid excessive memory usage and improve performance
    for chunk in tracks.chunks(batch_size) {
        let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            "INSERT INTO tracks (title, album_id, artist_id, path, duration, track_no, disc_no, format, bit_depth, frequency)",
        );
        query_builder.push_values(chunk, |mut b, track| {
            let path_str = track.path.to_str().unwrap_or_default();
            b.push_bind(&track.title)
                .push_bind(track.album_id)
                .push_bind(track.artist_id)
                .push_bind(path_str)
                .push_bind(track.duration)
                .push_bind(track.track_no)
                .push_bind(track.disc_no)
                .push_bind(&track.format)
                .push_bind(track.bit_depth)
                .push_bind(track.frequency);
        });
        query_builder.push(
            " ON CONFLICT(path) DO UPDATE SET
                title = excluded.title,
                album_id = excluded.album_id,
                artist_id = excluded.artist_id,
                duration = excluded.duration,
                track_no = excluded.track_no,
                disc_no = excluded.disc_no,
                format = excluded.format,
                bit_depth = excluded.bit_depth,
                frequency = excluded.frequency",
        );
        let query = query_builder.build();
        query.execute(&mut **tx).await?;
    }
    Ok(())
}

/// Inserts or updates a batch of albums in the database.
/// If an album with the same title, artist_id, and folder_id already exists, it is updated;
/// otherwise, it is inserted. This function should be called within an existing transaction.
///
/// After upserting the albums, this function retrieves the database IDs of all processed albums
/// and returns them in a HashMap for further use.
///
/// # Arguments
/// * `tx` - A mutable reference to a SQLite transaction.
/// * `albums` - A slice of `AlbumForInsert` objects to insert or update.
///
/// # Returns
/// A `Result` containing a `HashMap<(String, i64, i64), i64>` where:
/// - Keys are tuples of (album_title, artist_id, folder_id)
/// - Values are the corresponding database IDs
/// Returns an `sqlx::Error` on failure.
pub async fn upsert_albums_batch(
    tx: &mut Transaction<'_, Sqlite>,
    albums: &[AlbumForInsert],
) -> Result<HashMap<(String, i64, i64), i64>> {
    if albums.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query_builder = QueryBuilder::new(
        "INSERT INTO albums (title, artist_id, folder_id, year, cover_art, dr_value, original_release_date, dr_completed)",
    );
    query_builder.push_values(albums, |mut b, album| {
        b.push_bind(album.title.clone())
            .push_bind(album.artist_id)
            .push_bind(album.folder_id)
            .push_bind(album.year)
            .push_bind(album.cover_art_path.as_ref().and_then(|p| p.to_str()))
            .push_bind(album.dr_value)
            .push_bind(album.original_release_date.clone())
            .push_bind(false);
    });
    query_builder.push(
        " ON CONFLICT(title, artist_id, folder_id) DO UPDATE SET
            year = excluded.year,
            cover_art = excluded.cover_art,
            dr_value = excluded.dr_value,
            original_release_date = excluded.original_release_date",
    );
    query_builder.build().execute(&mut **tx).await?;
    let mut placeholders = Vec::new();
    for _ in albums {
        placeholders.push("(?, ?, ?)");
    }
    let sql = format!(
        "SELECT id, title, artist_id, folder_id FROM albums WHERE (title, artist_id, folder_id) IN ({})",
        placeholders.join(", ")
    );
    let mut query = query(&sql);
    for album in albums {
        query = query
            .bind(album.title.clone())
            .bind(album.artist_id)
            .bind(album.folder_id);
    }
    let album_ids: HashMap<(String, i64, i64), i64> = query
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|row| {
            (
                (row.get("title"), row.get("artist_id"), row.get("folder_id")),
                row.get("id"),
            )
        })
        .collect();
    Ok(album_ids)
}

/// Inserts a batch of new artists into the database if they don't already exist,
/// and returns a map of artist names to their corresponding database IDs.
/// This function should be called within an existing transaction.
///
/// # Arguments
/// * `tx` - A mutable reference to a SQLite transaction.
/// * `names` - A slice of artist names to insert or retrieve.
///
/// # Returns
/// A `Result` containing a `HashMap<String, i64>` where keys are artist names
/// and values are their corresponding database IDs. Returns an `sqlx::Error` on failure.
pub async fn insert_or_get_artists_batch(
    tx: &mut Transaction<'_, Sqlite>,
    names: &[String],
) -> Result<HashMap<String, i64>> {
    if names.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query_builder = QueryBuilder::new("INSERT OR IGNORE INTO artists (name) ");
    query_builder.push_values(names, |mut b, name| {
        b.push_bind(name);
    });
    let query = query_builder.build();
    query.execute(&mut **tx).await?;
    let sql = format!(
        "SELECT id, name FROM artists WHERE name IN ({})",
        names.iter().map(|_| "?").collect::<Vec<_>>().join(", ")
    );
    let mut select_query = sqlx::query(&sql);
    for name in names {
        select_query = select_query.bind(name);
    }
    let artists: HashMap<String, i64> = select_query
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|row| (row.get("name"), row.get("id")))
        .collect();
    Ok(artists)
}

/// Fetches a single album from the database by its unique ID.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `album_id` - The ID of the album to fetch.
///
/// # Returns
/// A `Result` containing the `Album` struct on success, or an `sqlx::Error` on failure
/// (e.g., if no album with the given ID is found).
pub async fn fetch_album_by_id(pool: &SqlitePool, album_id: i64) -> Result<Album> {
    get_metrics().record_db_operation();
    let row = query("SELECT id, title, artist_id, year, cover_art, folder_id, dr_value, dr_completed, original_release_date FROM albums WHERE id = ?")
        .bind(album_id)
        .fetch_one(pool)
        .await?;
    Ok(Album {
        id: row.get("id"),
        title: row.get("title"),
        artist_id: row.get("artist_id"),
        year: row.get("year"),
        cover_art: row.get::<Option<String>, _>("cover_art").map(PathBuf::from),
        folder_id: row.get("folder_id"),
        dr_value: row.get("dr_value"),
        dr_completed: row.get("dr_completed"),
        original_release_date: row.get("original_release_date"),
    })
}

/// Fetches a single artist from the database by their unique ID.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `artist_id` - The ID of the artist to fetch.
///
/// # Returns
/// A `Result` containing the `Artist` struct on success, or an `sqlx::Error` on failure
/// (e.g., if no artist with the given ID is found).
pub async fn fetch_artist_by_id(pool: &SqlitePool, artist_id: i64) -> Result<Artist> {
    let row = query("SELECT id, name FROM artists WHERE id = ?")
        .bind(artist_id)
        .fetch_one(pool)
        .await?;
    Ok(Artist {
        id: row.get("id"),
        name: row.get("name"),
    })
}

/// Fetches all tracks associated with a given album, ordered by disc number and track number.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `album_id` - The ID of the album for which to fetch tracks.
///
/// # Returns
/// A `Result` containing a `Vec<Track>` on success, or an `sqlx::Error` on failure.
pub async fn fetch_tracks_by_album(pool: &SqlitePool, album_id: i64) -> Result<Vec<Track>> {
    get_metrics().record_db_operation();
    let rows = query("SELECT id, title, album_id, artist_id, path, duration, track_no, disc_no, format, bit_depth, frequency FROM tracks WHERE album_id = ? ORDER BY disc_no, track_no")
        .bind(album_id)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| Track {
            id: row.get("id"),
            title: row.get("title"),
            album_id: row.get("album_id"),
            artist_id: row.get("artist_id"),
            path: PathBuf::from(row.get::<String, _>("path")),
            duration: row.get("duration"),
            track_no: row.get("track_no"),
            disc_no: row.get("disc_no"),
            format: row.get("format"),
            bit_depth: row.get("bit_depth"),
            frequency: row.get("frequency"),
        })
        .collect())
}

/// Fetches a single folder from the database by its unique ID.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `folder_id` - The ID of the folder to fetch.
///
/// # Returns
/// A `Result` containing the `Folder` struct on success, or an `sqlx::Error` on failure
/// (e.g., if no folder with the given ID is found).
pub async fn fetch_folder_by_id(pool: &SqlitePool, folder_id: i64) -> Result<Folder> {
    let row = query("SELECT id, path FROM folders WHERE id = ?")
        .bind(folder_id)
        .fetch_one(pool)
        .await?;
    Ok(Folder {
        id: row.get("id"),
        path: PathBuf::from(row.get::<String, _>("path")),
    })
}

/// Updates the `dr_completed` status for a specific album in the database.
///
/// This function is typically called when a user manually marks an album's
/// DR value as completed or uncompleted in the UI.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `album_id` - The ID of the album to update.
/// * `completed` - A boolean indicating the new `dr_completed` status.
///
/// # Returns
/// A `Result` indicating success or an `sqlx::Error` on failure.
pub async fn update_album_dr_completed(
    pool: &SqlitePool,
    album_id: i64,
    completed: bool,
) -> Result<()> {
    // Update the album's DR completion status in the database
    query("UPDATE albums SET dr_completed = ? WHERE id = ?")
        .bind(completed)
        .bind(album_id)
        .execute(pool)
        .await?;
    Ok(())
}
