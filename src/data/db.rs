use sqlx::{query, Result, Row, SqlitePool};

use crate::data::models::{Album, Artist, Folder, Track};

/// Remove a folder and all its albums and tracks by folder ID.
pub async fn remove_folder_and_albums(pool: &SqlitePool, folder_id: i64) -> Result<()> {

    // Remove tracks belonging to albums in this folder
    query("DELETE FROM tracks WHERE album_id IN (SELECT id FROM albums WHERE folder_id = ?)")
        .bind(folder_id)
        .execute(pool)
        .await?;

    // Remove albums in this folder
    query("DELETE FROM albums WHERE folder_id = ?")
        .bind(folder_id)
        .execute(pool)
        .await?;

    // Remove orphaned artists (no albums left)
    query("DELETE FROM artists WHERE id NOT IN (SELECT artist_id FROM albums)")
        .execute(pool)
        .await?;

    // Remove the folder itself
    query("DELETE FROM folders WHERE id = ?")
        .bind(folder_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Fetch a single album by its ID.
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
        cover_art: row.get("cover_art"),
        folder_id: row.get("folder_id"),
        dr_value: row.get("dr_value"),
        dr_completed: row.get("dr_completed"),
        original_release_date: row.get("original_release_date"),
    })
}

/// Remove an album and all its tracks by album ID.
pub async fn remove_album_and_tracks(pool: &SqlitePool, album_id: i64) -> Result<()> {
    query("DELETE FROM tracks WHERE album_id = ?")
        .bind(album_id)
        .execute(pool)
        .await?;
    query("DELETE FROM albums WHERE id = ?")
        .bind(album_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Remove artists that have no albums left in the database.
pub async fn remove_artists_with_no_albums(pool: &SqlitePool) -> Result<()> {
    let _result = query("DELETE FROM artists WHERE id NOT IN (SELECT artist_id FROM albums)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Remove albums that have no tracks left in the database.
pub async fn remove_albums_with_no_tracks(pool: &SqlitePool) -> Result<()> {
    let _result = query("DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Remove tracks whose files no longer exist on disk.
pub async fn remove_orphaned_tracks(pool: &SqlitePool) -> Result<()> {
    let tracks_in_db = query("SELECT id, path FROM tracks")
        .fetch_all(pool)
        .await?;
    for track_row in tracks_in_db {
        let track_id: i64 = track_row.get("id");
        let track_path: String = track_row.get("path");
        if !std::path::Path::new(&track_path).exists() {
            query("DELETE FROM tracks WHERE id = ?")
                .bind(track_id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

/// Clear all DR values from the albums table.
pub async fn clear_all_dr_values(pool: &SqlitePool) -> Result<()> {
    query("UPDATE albums SET dr_value = NULL")
        .execute(pool)
        .await?;
    Ok(())
}

/// Fetch a single artist by its ID.
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

/// Fetch all tracks for a given album, ordered by disc and track number.
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
            path: row.get("path"),
            duration: row.get("duration"),
            track_no: row.get("track_no"),
            disc_no: row.get("disc_no"),
            format: row.get("format"),
            bit_depth: row.get("bit_depth"),
            frequency: row.get("frequency"),
        })
        .collect())
}

/// Fetch all folders in the database.
pub async fn fetch_all_folders(pool: &SqlitePool) -> Result<Vec<Folder>> {
    let rows = query("SELECT id, path FROM folders")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| Folder {
            id: row.get("id"),
            path: row.get("path"),
        })
        .collect())
}

/// Struct for displaying album info in the UI, including artist and format details.
#[derive(Clone)]
pub struct AlbumDisplayInfo {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub cover_art: Option<Vec<u8>>,
    pub format: Option<String>,
    pub bit_depth: Option<u32>,
    pub frequency: Option<u32>,
    pub _dr_value: Option<u8>,
    pub dr_completed: bool,
    pub original_release_date: Option<String>,
}

/// Fetch all albums with display info, joining artist and track format data.
pub async fn fetch_album_display_info(pool: &SqlitePool) -> Result<Vec<AlbumDisplayInfo>> {
    let rows = query(
        r#"SELECT albums.id, albums.title, artists.name as artist, albums.year, albums.cover_art,
                     tracks.format, tracks.bit_depth, tracks.frequency, albums.dr_value, albums.dr_completed, albums.original_release_date
            FROM albums
            JOIN artists ON albums.artist_id = artists.id
            LEFT JOIN tracks ON tracks.album_id = albums.id
            GROUP BY albums.id
            ORDER BY artists.name COLLATE NOCASE, albums.title COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| AlbumDisplayInfo {
            id: row.get("id"),
            title: row.get("title"),
            artist: row.get("artist"),
            year: row.get("year"),
            cover_art: row.get("cover_art"),
            format: row.get("format"),
            bit_depth: row.get("bit_depth"),
            frequency: row.get("frequency"),
            _dr_value: row.get("dr_value"),
            dr_completed: row.get("dr_completed"),
            original_release_date: row.get("original_release_date"),
        })
        .collect())
}

/// Update the DR completion status for an album.
pub async fn update_album_dr_completed(pool: &SqlitePool, album_id: i64, completed: bool) -> Result<()> {
    query("UPDATE albums SET dr_completed = ? WHERE id = ?")
        .bind(completed)
        .bind(album_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Initialize the database schema if not present.
/// Creates all required tables for folders, artists, albums, and tracks.
pub async fn init_db(pool: &SqlitePool) -> Result<()> {
    query(
        "CREATE TABLE IF NOT EXISTS folders (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE
        )",
    )
    .execute(pool)
    .await?;
    query(
        "CREATE TABLE IF NOT EXISTS artists (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE
        )",
    )
    .execute(pool)
    .await?;
    query(
        "CREATE TABLE IF NOT EXISTS albums (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            artist_id INTEGER NOT NULL,
            year INTEGER,
            cover_art BLOB,
            folder_id INTEGER NOT NULL,
            dr_value INTEGER,
            dr_completed BOOLEAN DEFAULT FALSE,
            original_release_date TEXT
        )",
    )
    .execute(pool)
    .await?;
    query("ALTER TABLE albums ADD COLUMN original_release_date TEXT")
        .execute(pool)
        .await
        .ok();
    query(
        "CREATE TABLE IF NOT EXISTS tracks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            album_id INTEGER NOT NULL,
            artist_id INTEGER NOT NULL,
            path TEXT NOT NULL UNIQUE,
            duration INTEGER,
            track_no INTEGER,
            disc_no INTEGER,
            format TEXT,
            bit_depth INTEGER,
            frequency INTEGER,
            FOREIGN KEY(album_id) REFERENCES albums(id),
            FOREIGN KEY(artist_id) REFERENCES artists(id)
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert a folder if it doesn't exist, or return its ID if it does.
pub async fn insert_or_get_folder(pool: &SqlitePool, path: &str) -> Result<i64> {
    if let Some(row) = query("SELECT id FROM folders WHERE path = ?")
        .bind(path)
        .fetch_optional(pool)
        .await?
    {
        Ok(row.get(0))
    } else {
        let res = query("INSERT INTO folders (path) VALUES (?)")
            .bind(path)
            .execute(pool)
            .await?;
        Ok(res.last_insert_rowid())
    }
}

/// Insert an artist if it doesn't exist, or return its ID if it does.
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

/// Insert an album if it doesn't exist, or return its ID if it does.
/// Also stores optional year, cover art, folder, and DR value.
pub async fn insert_or_get_album(
    pool: &SqlitePool,
    title: &str,
    artist_id: i64,
    year: Option<i32>,
    cover_art: Option<Vec<u8>>,
    folder_id: i64,
    dr_value: Option<u8>,
    original_release_date: Option<String>,
) -> Result<i64> {
    if let Some(row) =
        query("SELECT id FROM albums WHERE title = ? AND artist_id = ? AND folder_id = ?")
            .bind(title)
            .bind(artist_id)
            .bind(folder_id)
            .fetch_optional(pool)
            .await?
    {
        let album_id: i64 = row.get(0);

        // Album exists, update it
        query("UPDATE albums SET year = ?, cover_art = ?, dr_value = ?, original_release_date = ? WHERE id = ?")
            .bind(year)
            .bind(cover_art)
            .bind(dr_value)
            .bind(original_release_date)
            .bind(album_id)
            .execute(pool)
            .await?;
        Ok(album_id)
    } else {

        // Album doesn't exist, insert it
        let res = query("INSERT INTO albums (title, artist_id, year, cover_art, folder_id, dr_value, original_release_date) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(title)
            .bind(artist_id)
            .bind(year)
            .bind(cover_art)
            .bind(folder_id)
            .bind(dr_value)
            .bind(original_release_date)
            .execute(pool)
            .await?;
        Ok(res.last_insert_rowid())
    }
}

/// Fetch all artists in the database that are associated with at least one album.
pub async fn fetch_all_artists(pool: &SqlitePool) -> Result<Vec<Artist>> {
    let rows = query("SELECT id, name FROM artists WHERE id IN (SELECT DISTINCT artist_id FROM albums)")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| Artist {
            id: row.get("id"),
            name: row.get("name"),
        })
        .collect())
}

/// Insert a track if it doesn't exist, or update its metadata if it does.
/// Returns the track ID.
pub async fn insert_track(
    pool: &SqlitePool,
    title: &str,
    album_id: i64,
    artist_id: i64,
    path: &str,
    duration: u32,
    track_no: Option<u32>,
    disc_no: Option<u32>,
    format: Option<String>,
    bit_depth: Option<u32>,
    frequency: Option<u32>,
) -> Result<i64> {
    if let Some(row) = query("SELECT id FROM tracks WHERE path = ?")
        .bind(path)
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
            .bind(path)
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

/// Search albums by substring in title or artist (case-insensitive).
pub async fn search_album_display_info(
    pool: &SqlitePool,
    search_term: &str,
) -> Result<Vec<AlbumDisplayInfo>> {
    let pattern = format!("%{}%", search_term.to_lowercase());
    let rows = query(
        r#"SELECT albums.id, albums.title, artists.name as artist, albums.year, albums.cover_art,
                     tracks.format, tracks.bit_depth, tracks.frequency, albums.dr_value, albums.dr_completed, albums.original_release_date
            FROM albums
            JOIN artists ON albums.artist_id = artists.id
            LEFT JOIN tracks ON tracks.album_id = albums.id
            WHERE lower(albums.title) LIKE ? OR lower(artists.name) LIKE ?
            GROUP BY albums.id
            ORDER BY artists.name COLLATE NOCASE, albums.title COLLATE NOCASE"#,
    )
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| AlbumDisplayInfo {
            id: row.get("id"),
            title: row.get("title"),
            artist: row.get("artist"),
            year: row.get("year"),
            cover_art: row.get("cover_art"),
            format: row.get("format"),
            bit_depth: row.get("bit_depth"),
            frequency: row.get("frequency"),
            _dr_value: row.get("dr_value"),
            dr_completed: row.get("dr_completed"),
            original_release_date: row.get("original_release_date"),
        })
        .collect())
}

/// Search artists by substring in name (case-insensitive).
pub async fn search_artists(pool: &SqlitePool, search_term: &str) -> Result<Vec<Artist>> {
    let pattern = format!("%{}%", search_term.to_lowercase());
    let rows = query("SELECT id, name FROM artists WHERE lower(name) LIKE ?")
        .bind(&pattern)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| Artist {
            id: row.get("id"),
            name: row.get("name"),
        })
        .collect())
}
