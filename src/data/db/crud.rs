use std::path::{Path, PathBuf};

use sqlx::{Result, Row, SqlitePool, query};

use crate::data::models::{Album, Artist, Folder, Track};

/// Inserts a new folder into the database if it doesn't already exist,
/// or returns the ID of the existing folder if a matching path is found.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `path` - The file system path of the folder.
///
/// # Returns
/// A `Result` containing the ID (`i64`) of the inserted or existing folder on success,
/// or an `sqlx::Error` on failure.
pub async fn insert_or_get_folder(pool: &SqlitePool, path: &Path) -> Result<i64> {
    let path_str = path.to_str().unwrap_or_default();
    if let Some(row) = query("SELECT id FROM folders WHERE path = ?")
        .bind(path_str)
        .fetch_optional(pool)
        .await?
    {
        Ok(row.get(0))
    } else {
        let res = query("INSERT INTO folders (path) VALUES (?)")
            .bind(path_str)
            .execute(pool)
            .await?;
        Ok(res.last_insert_rowid())
    }
}

/// Inserts a new artist into the database if they don't already exist,
/// or returns the ID of the existing artist if a matching name is found.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `name` - The name of the artist.
///
/// # Returns
/// A `Result` containing the ID (`i64`) of the inserted or existing artist on success,
/// or an `sqlx::Error` on failure.
pub async fn insert_or_get_artist(pool: &SqlitePool, name: &str) -> Result<i64> {
    if let Some(row) = query("SELECT id FROM artists WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await?
    {
        Ok(row.get(0))
    } else {
        let res = query("INSERT INTO artists (name) VALUES (?)")
            .bind(name)
            .execute(pool)
            .await?;
        Ok(res.last_insert_rowid())
    }
}

/// Inserts a new album into the database if it doesn't already exist,
/// or updates an existing album's metadata (year, cover art, DR value, original release date)
/// if a matching album is found.
///
/// A match is determined by the combination of `title`, `artist_id`, and `folder_id`.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `title` - The title of the album.
/// * `artist_id` - The ID of the primary artist of the album.
/// * `year` - The release year of the album (optional).
/// * `cover_art_path` - The path to the album's cached cover art (optional).
/// * `folder_id` - The ID of the folder where the album's files are located.
/// * `dr_value` - The Dynamic Range (DR) value for the album (optional).
/// * `original_release_date` - The original release date of the album as a string (optional).
///
/// # Returns
/// A `Result` containing the ID (`i64`) of the inserted or updated album on success,
/// or an `sqlx::Error` on failure.
pub async fn insert_or_get_album(
    pool: &SqlitePool,
    title: &str,
    artist_id: i64,
    year: Option<i32>,
    cover_art_path: Option<&Path>,
    folder_id: i64,
    dr_value: Option<u8>,
    original_release_date: Option<String>,
) -> Result<i64> {
    let cover_art_path_str = cover_art_path.and_then(|p| p.to_str());
    if let Some(row) =
        query("SELECT id FROM albums WHERE title = ? AND artist_id = ? AND folder_id = ?")
            .bind(title)
            .bind(artist_id)
            .bind(folder_id)
            .fetch_optional(pool)
            .await?
    {
        let album_id: i64 = row.get(0);

        // Album exists, update its metadata
        query("UPDATE albums SET year = ?, cover_art = ?, dr_value = ?, original_release_date = ? WHERE id = ?")
            .bind(year)
            .bind(cover_art_path_str)
            .bind(dr_value)
            .bind(original_release_date)
            .bind(album_id)
            .execute(pool)
            .await?;
        Ok(album_id)
    } else {
        // Album doesn't exist, insert it as a new record
        let res = query("INSERT INTO albums (title, artist_id, year, cover_art, folder_id, dr_value, dr_completed, original_release_date) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(title)
            .bind(artist_id)
            .bind(year)
            .bind(cover_art_path_str)
            .bind(folder_id)
            .bind(dr_value)
            .bind(false)
            .bind(original_release_date)
            .execute(pool)
            .await?;
        Ok(res.last_insert_rowid())
    }
}

/// Inserts a new track into the database. If a track with the same path already exists,
/// its metadata will be updated.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `title` - The title of the track.
/// * `album_id` - The ID of the album this track belongs to.
/// * `artist_id` - The ID of the primary artist of the track.
/// * `path` - The file system path of the track file (must be unique).
/// * `duration` - The duration of the track in seconds.
/// * `track_no` - The track number within the album (optional).
/// * `disc_no` - The disc number if the album has multiple discs (optional).
/// * `format` - The audio format (e.g., "FLAC", "MP3", optional).
/// * `bit_depth` - The bit depth of the audio (e.g., 16, 24, optional).
/// * `frequency` - The sample rate frequency of the audio (e.g., 44100, 96000, optional).
///
/// # Returns
/// A `Result` containing the ID (`i64`) of the inserted or updated track on success,
/// or an `sqlx::Error` on failure.
pub async fn insert_track(
    pool: &SqlitePool,
    title: &str,
    album_id: i64,
    artist_id: i64,
    path: &Path,
    duration: u32,
    track_no: Option<u32>,
    disc_no: Option<u32>,
    format: Option<String>,
    bit_depth: Option<u32>,
    frequency: Option<u32>,
) -> Result<i64> {
    let path_str = path.to_str().unwrap_or_default();
    if let Some(row) = query("SELECT id FROM tracks WHERE path = ?")
        .bind(path_str)
        .fetch_optional(pool)
        .await?
    {
        let track_id: i64 = row.get(0);

        // Track exists: update its metadata
        query("UPDATE tracks SET title = ?, album_id = ?, artist_id = ?, duration = ?, track_no = ?, disc_no = ?, format = ?, bit_depth = ?, frequency = ? WHERE id = ?")
            .bind(title)
            .bind(album_id)
            .bind(artist_id)
            .bind(duration)
            .bind(track_no)
            .bind(disc_no)
            .bind(format)
            .bind(bit_depth)
            .bind(frequency)
            .bind(track_id)
            .execute(pool)
            .await?;
        Ok(track_id)
    } else {
        let res = query("INSERT INTO tracks (title, album_id, artist_id, path, duration, track_no, disc_no, format, bit_depth, frequency) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(title)
            .bind(album_id)
            .bind(artist_id)
            .bind(path_str)
            .bind(duration)
            .bind(track_no)
            .bind(disc_no)
            .bind(format)
            .bind(bit_depth)
            .bind(frequency)
            .execute(pool)
            .await?;
        Ok(res.last_insert_rowid())
    }
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
    query("UPDATE albums SET dr_completed = ? WHERE id = ?")
        .bind(completed)
        .bind(album_id)
        .execute(pool)
        .await?;
    Ok(())
}
