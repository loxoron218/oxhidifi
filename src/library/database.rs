//! Library database interface using sqlx with `SQLite`.
//!
//! This module provides the main `LibraryDatabase` struct that handles
//! all database operations for the music library, including querying,
//! searching, and DR value management.

use std::{collections::HashMap, path::Path};

use {
    sqlx::{Error, SqlitePool, query, query_as, query_scalar},
    thiserror::Error,
    tracing::debug,
};

use crate::library::{
    connection::create_connection_pool,
    models::{Album, Artist, SearchResults, Track},
};

/// Escapes special characters in a string for use in SQL LIKE patterns.
///
/// This prevents SQL LIKE injection by escaping `%`, `_`, and `\` characters
/// which have special meaning in LIKE patterns.
///
/// # Arguments
///
/// * `s` - The string to escape.
///
/// # Returns
///
/// A new string with special characters escaped.
#[must_use]
pub fn escape_like_pattern(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '%' => result.push_str("\\%"),
            '_' => result.push_str("\\_"),
            _ => result.push(c),
        }
    }
    result
}

/// Error type for library database operations.
#[derive(Error, Debug)]
pub enum LibraryError {
    /// Database connection or query error.
    #[error("Database error: {0}")]
    DatabaseError(#[from] Error),
    /// Invalid file path or metadata.
    #[error("Invalid data: {reason}")]
    InvalidData { reason: String },
    /// Record not found.
    #[error("Record not found: {entity} with id {id}")]
    NotFound { entity: String, id: i64 },
}

/// Main library database interface.
///
/// The `LibraryDatabase` provides async methods for all library operations,
/// including album/artist/track queries, searching, and DR value management.
#[derive(Debug, Clone)]
pub struct LibraryDatabase {
    /// `SQLite` connection pool for database operations.
    pool: SqlitePool,
}

impl LibraryDatabase {
    /// Creates a new library database instance.
    ///
    /// This method initializes the database connection pool and ensures
    /// the schema is properly set up.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `LibraryDatabase` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if database initialization fails.
    pub async fn new() -> Result<Self, LibraryError> {
        let pool = create_connection_pool().await?;
        Self::initialize_schema(&pool).await?;

        Ok(Self { pool })
    }

    /// Initializes the database schema by creating all necessary tables and indexes.
    ///
    /// # Arguments
    ///
    /// * `pool` - The `SQLite` connection pool.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if table creation fails.
    async fn initialize_schema(pool: &SqlitePool) -> Result<(), LibraryError> {
        // Artists table
        query(
            "
            CREATE TABLE IF NOT EXISTS artists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            ",
        )
        .execute(pool)
        .await?;

        // Albums table
        query(
            "
            CREATE TABLE IF NOT EXISTS albums (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                artist_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                year INTEGER,
                genre TEXT,
                compilation BOOLEAN DEFAULT FALSE,
                path TEXT NOT NULL UNIQUE,
                dr_value TEXT,
                artwork_path TEXT,
                format TEXT,
                bits_per_sample INTEGER,
                sample_rate INTEGER,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
                UNIQUE (artist_id, title, year)
            )
            ",
        )
        .execute(pool)
        .await?;

        // Tracks table
        query(
            "
            CREATE TABLE IF NOT EXISTS tracks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                album_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                track_number INTEGER,
                disc_number INTEGER DEFAULT 1,
                duration_ms INTEGER NOT NULL,
                path TEXT NOT NULL UNIQUE,
                file_size INTEGER NOT NULL,
                format TEXT NOT NULL,
                codec TEXT NOT NULL DEFAULT '',
                sample_rate INTEGER NOT NULL,
                bits_per_sample INTEGER NOT NULL,
                channels INTEGER NOT NULL,
                is_lossless BOOLEAN NOT NULL DEFAULT FALSE,
                is_high_resolution BOOLEAN NOT NULL DEFAULT FALSE,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
            )
            ",
        )
        .execute(pool)
        .await?;

        // Create indexes for performance
        query("CREATE INDEX IF NOT EXISTS idx_artists_name ON artists (name)")
            .execute(pool)
            .await?;

        query("CREATE INDEX IF NOT EXISTS idx_albums_artist_id ON albums (artist_id)")
            .execute(pool)
            .await?;

        query("CREATE INDEX IF NOT EXISTS idx_albums_title ON albums (title)")
            .execute(pool)
            .await?;

        query("CREATE INDEX IF NOT EXISTS idx_tracks_album_id ON tracks (album_id)")
            .execute(pool)
            .await?;

        query("CREATE INDEX IF NOT EXISTS idx_tracks_path ON tracks (path)")
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Gets all albums in the library.
    ///
    /// # Arguments
    ///
    /// * `filter` - Optional filter string to match against album titles.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Album` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_albums(&self, filter: Option<&str>) -> Result<Vec<Album>, LibraryError> {
        let albums = match filter {
            Some(filter_str) => {
                let search_pattern = format!("%{filter_str}%");
                query_as::<_, Album>(
                    "
                    SELECT a.id, a.artist_id, a.title, a.year, a.genre, a.compilation,
                           a.path, a.dr_value, a.artwork_path, a.format, a.bits_per_sample,
                           a.sample_rate, a.created_at, a.updated_at,
                           COUNT(t.id) as track_count,
                           MAX(t.channels) as channels
                    FROM albums a
                    LEFT JOIN tracks t ON a.id = t.album_id
                    WHERE a.title LIKE ? ESCAPE '\\'
                    GROUP BY a.id
                    ORDER BY a.title, a.year
                    ",
                )
                .bind(search_pattern)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                query_as::<_, Album>(
                    "
                    SELECT a.id, a.artist_id, a.title, a.year, a.genre, a.compilation,
                           a.path, a.dr_value, a.artwork_path, a.format, a.bits_per_sample,
                           a.sample_rate, a.created_at, a.updated_at,
                           COUNT(t.id) as track_count,
                           MAX(t.channels) as channels
                    FROM albums a
                    LEFT JOIN tracks t ON a.id = t.album_id
                    GROUP BY a.id
                    ORDER BY a.title, a.year
                    ",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(albums)
    }

    /// Gets all artists in the library.
    ///
    /// # Arguments
    ///
    /// * `filter` - Optional filter string to match against artist names.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Artist` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_artists(&self, filter: Option<&str>) -> Result<Vec<Artist>, LibraryError> {
        let artists = match filter {
            Some(filter_str) => {
                let search_pattern = format!("%{filter_str}%");
                query_as::<_, Artist>(
                    "
                    SELECT a.id, a.name, COUNT(a2.id) as album_count, a.created_at, a.updated_at
                    FROM artists a
                    LEFT JOIN albums a2 ON a.id = a2.artist_id
                    WHERE a.name LIKE ? ESCAPE '\\'
                    GROUP BY a.id, a.name, a.created_at, a.updated_at
                    ORDER BY a.name
                    ",
                )
                .bind(search_pattern)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                query_as::<_, Artist>(
                    "
                    SELECT a.id, a.name, COUNT(a2.id) as album_count, a.created_at, a.updated_at
                    FROM artists a
                    LEFT JOIN albums a2 ON a.id = a2.artist_id
                    GROUP BY a.id, a.name, a.created_at, a.updated_at
                    ORDER BY a.name
                    ",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(artists)
    }

    /// Gets albums in a specific directory.
    ///
    /// # Arguments
    ///
    /// * `directory_path` - Path to the directory containing albums.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Album` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_albums_by_directory<P: AsRef<Path>>(
        &self,
        directory_path: P,
    ) -> Result<Vec<Album>, LibraryError> {
        let escaped_path = escape_like_pattern(&directory_path.as_ref().to_string_lossy());
        let path_pattern = format!("{escaped_path}/%");

        let albums = query_as::<_, Album>(
            "
            SELECT a.id, a.artist_id, a.title, a.year, a.genre, a.compilation,
                   a.path, a.dr_value, a.artwork_path, a.format, a.bits_per_sample,
                   a.sample_rate, a.created_at, a.updated_at,
                   COUNT(t.id) as track_count,
                   MAX(t.channels) as channels
            FROM albums a
            LEFT JOIN tracks t ON a.id = t.album_id
            WHERE a.path LIKE ? ESCAPE '\\'
            GROUP BY a.id
            ORDER BY a.title, a.year
            ",
        )
        .bind(path_pattern)
        .fetch_all(&self.pool)
        .await?;

        Ok(albums)
    }

    /// Gets artists that have albums in a specific directory.
    ///
    /// # Arguments
    ///
    /// * `directory_path` - Path to the directory containing albums.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Artist` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_artists_by_directory<P: AsRef<Path>>(
        &self,
        directory_path: P,
    ) -> Result<Vec<Artist>, LibraryError> {
        let escaped_path = escape_like_pattern(&directory_path.as_ref().to_string_lossy());
        let path_pattern = format!("{escaped_path}/%");

        let artists = query_as::<_, Artist>(
            "
            SELECT a.id, a.name, COUNT(a2.id) as album_count, a.created_at, a.updated_at
            FROM artists a
            LEFT JOIN albums a2 ON a.id = a2.artist_id
            WHERE a2.id IN (SELECT id FROM albums WHERE path LIKE ? ESCAPE '\\')
            GROUP BY a.id, a.name, a.created_at, a.updated_at
            ORDER BY a.name
            ",
        )
        .bind(path_pattern)
        .fetch_all(&self.pool)
        .await?;

        Ok(artists)
    }

    /// Gets all albums with their current track counts (for incremental updates).
    ///
    /// This is more efficient than `get_albums(None)` when we need to update
    /// specific albums' track counts after operations.
    ///
    /// # Arguments
    ///
    /// * `album_ids` - List of album IDs to fetch.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Album` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_albums_by_ids(&self, album_ids: &[i64]) -> Result<Vec<Album>, LibraryError> {
        if album_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = album_ids.iter().map(|_| "?".to_string()).collect();
        let query_str = format!(
            "
            SELECT a.id, a.artist_id, a.title, a.year, a.genre, a.compilation,
                   a.path, a.dr_value, a.artwork_path, a.format, a.bits_per_sample,
                   a.sample_rate, a.created_at, a.updated_at,
                   COUNT(t.id) as track_count,
                   MAX(t.channels) as channels
            FROM albums a
            LEFT JOIN tracks t ON a.id = t.album_id
            WHERE a.id IN ({})
            GROUP BY a.id
            ORDER BY a.title, a.year
            ",
            placeholders.join(",")
        );

        let mut query = query_as::<_, Album>(&query_str);
        for id in album_ids {
            query = query.bind(id);
        }

        let albums = query.fetch_all(&self.pool).await?;

        Ok(albums)
    }

    /// Gets all artists with their current album counts (for incremental updates).
    ///
    /// This is more efficient than `get_artists(None)` when we need to update
    /// specific artists' album counts after operations.
    ///
    /// # Arguments
    ///
    /// * `artist_ids` - List of artist IDs to fetch.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Artist` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_artists_by_ids(
        &self,
        artist_ids: &[i64],
    ) -> Result<Vec<Artist>, LibraryError> {
        if artist_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = artist_ids.iter().map(|_| "?".to_string()).collect();
        let query_str = format!(
            "
            SELECT a.id, a.name, COUNT(a2.id) as album_count, a.created_at, a.updated_at
            FROM artists a
            LEFT JOIN albums a2 ON a.id = a2.artist_id
            WHERE a.id IN ({})
            GROUP BY a.id, a.name, a.created_at, a.updated_at
            ORDER BY a.name
            ",
            placeholders.join(",")
        );

        let mut query = query_as::<_, Artist>(&query_str);
        for id in artist_ids {
            query = query.bind(id);
        }

        let artists = query.fetch_all(&self.pool).await?;

        Ok(artists)
    }

    /// Gets track counts for all albums in the library (lightweight count only).
    ///
    /// This is used for efficient UI updates where we just need to know
    /// which albums had track count changes.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `HashMap` of `album_id` to `track_count`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_album_track_counts(&self) -> Result<HashMap<i64, i64>, LibraryError> {
        let counts: Vec<(i64, i64)> =
            query_as("SELECT album_id, COUNT(*) as count FROM tracks GROUP BY album_id")
                .fetch_all(&self.pool)
                .await?;

        Ok(counts.into_iter().collect())
    }

    /// Gets album counts for all artists in the library (lightweight count only).
    ///
    /// This is used for efficient UI updates where we just need to know
    /// which artists had album count changes.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `HashMap` of `artist_id` to `album_count`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_artist_album_counts(&self) -> Result<HashMap<i64, i64>, LibraryError> {
        let counts: Vec<(i64, i64)> = query_as(
            "SELECT artist_id, COUNT(*) as count FROM albums WHERE artist_id IS NOT NULL GROUP BY artist_id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(counts.into_iter().collect())
    }

    /// Gets all album IDs in a specific directory.
    ///
    /// # Arguments
    ///
    /// * `directory_path` - Path to the directory containing albums.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of album IDs.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_album_ids_by_directory<P: AsRef<Path>>(
        &self,
        directory_path: P,
    ) -> Result<Vec<i64>, LibraryError> {
        let escaped_path = escape_like_pattern(&directory_path.as_ref().to_string_lossy());
        let path_pattern = format!("{escaped_path}/%");

        let ids: Vec<i64> = query_scalar("SELECT id FROM albums WHERE path LIKE ? ESCAPE '\\'")
            .bind(path_pattern)
            .fetch_all(&self.pool)
            .await?;

        Ok(ids)
    }

    /// Gets all artist IDs that have albums in a specific directory.
    ///
    /// # Arguments
    ///
    /// * `directory_path` - Path to the directory containing albums.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of artist IDs.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_artist_ids_by_directory<P: AsRef<Path>>(
        &self,
        directory_path: P,
    ) -> Result<Vec<i64>, LibraryError> {
        let escaped_path = escape_like_pattern(&directory_path.as_ref().to_string_lossy());
        let path_pattern = format!("{escaped_path}/%");

        let ids: Vec<i64> = query_scalar(
            "SELECT DISTINCT artist_id FROM albums WHERE path LIKE ? ESCAPE '\\' AND artist_id IS NOT NULL",
        )
        .bind(path_pattern)
        .fetch_all(&self.pool)
        .await?;

        Ok(ids)
    }

    /// Gets all remaining album IDs in the library.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of album IDs.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_all_album_ids(&self) -> Result<Vec<i64>, LibraryError> {
        let ids: Vec<i64> = query_scalar("SELECT id FROM albums")
            .fetch_all(&self.pool)
            .await?;

        Ok(ids)
    }

    /// Gets all remaining artist IDs in the library.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of artist IDs.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_all_artist_ids(&self) -> Result<Vec<i64>, LibraryError> {
        let ids: Vec<i64> = query_scalar("SELECT id FROM artists")
            .fetch_all(&self.pool)
            .await?;

        Ok(ids)
    }

    /// Gets all tracks for a specific album.
    ///
    /// # Arguments
    ///
    /// * `album_id` - The ID of the album.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Track` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails or the album doesn't exist.
    pub async fn get_tracks_by_album(&self, album_id: i64) -> Result<Vec<Track>, LibraryError> {
        // Verify album exists
        let album_exists: Option<i64> = query_scalar("SELECT id FROM albums WHERE id = ?")
            .bind(album_id)
            .fetch_optional(&self.pool)
            .await?;

        if album_exists.is_none() {
            return Err(LibraryError::NotFound {
                entity: "album".to_string(),
                id: album_id,
            });
        }

        let tracks = query_as::<_, Track>(
            "
            SELECT id, album_id, title, track_number, disc_number, duration_ms, path,
                   file_size, format, codec, sample_rate, bits_per_sample, channels, is_lossless, is_high_resolution, created_at, updated_at
            FROM tracks
            WHERE album_id = ?
            ORDER BY disc_number, track_number, title
            ",
        )
        .bind(album_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(tracks)
    }

    /// Gets all tracks for a specific artist.
    ///
    /// # Arguments
    ///
    /// * `artist_id` - The ID of the artist.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Track` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails or the artist doesn't exist.
    pub async fn get_tracks_by_artist(&self, artist_id: i64) -> Result<Vec<Track>, LibraryError> {
        // Verify artist exists
        let artist_exists: Option<i64> = query_scalar("SELECT id FROM artists WHERE id = ?")
            .bind(artist_id)
            .fetch_optional(&self.pool)
            .await?;

        if artist_exists.is_none() {
            return Err(LibraryError::NotFound {
                entity: "artist".to_string(),
                id: artist_id,
            });
        }

        let tracks = query_as::<_, Track>(
            "
            SELECT t.id, t.album_id, t.title, t.track_number, t.disc_number, t.duration_ms, t.path,
                   t.file_size, t.format, t.codec, t.sample_rate, t.bits_per_sample, t.channels, t.is_lossless, t.is_high_resolution, t.created_at, t.updated_at
            FROM tracks t
            JOIN albums a ON t.album_id = a.id
            WHERE a.artist_id = ?
            ORDER BY a.title, t.disc_number, t.track_number, t.title
            "
        )
        .bind(artist_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(tracks)
    }

    /// Searches the library for albums and artists matching the query.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string.
    ///
    /// # Returns
    ///
    /// A `Result` containing `SearchResults` or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the queries fail.
    pub async fn search_library(&self, query: &str) -> Result<SearchResults, LibraryError> {
        let search_pattern = format!("%{query}%");

        let albums = query_as::<_, Album>(
            "
            SELECT id, artist_id, title, year, genre, compilation, path, dr_value,
                   artwork_path, format, bits_per_sample, sample_rate, created_at, updated_at
            FROM albums
            WHERE title LIKE ? ESCAPE '\\'
            ORDER BY title, year
            ",
        )
        .bind(&search_pattern)
        .fetch_all(&self.pool)
        .await?;

        let artists = query_as::<_, Artist>(
            "
            SELECT id, name, created_at, updated_at
            FROM artists
            WHERE name LIKE ? ESCAPE '\\'
            ORDER BY name
            ",
        )
        .bind(&search_pattern)
        .fetch_all(&self.pool)
        .await?;

        Ok(SearchResults { albums, artists })
    }

    /// Gets the DR (Dynamic Range) value for an album.
    ///
    /// # Arguments
    ///
    /// * `album_path` - Path to the album directory.
    ///
    /// # Returns
    ///
    /// A `Result` containing an `Option<String>` with the DR value or a `LibraryError`.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the query fails.
    pub async fn get_dr_value<P: AsRef<Path>>(
        &self,
        album_path: P,
    ) -> Result<Option<String>, LibraryError> {
        let album_path_str = album_path.as_ref().to_string_lossy().to_string();

        let dr_value: Option<String> = query_scalar("SELECT dr_value FROM albums WHERE path = ?")
            .bind(album_path_str)
            .fetch_optional(&self.pool)
            .await?;

        Ok(dr_value)
    }

    /// Updates the DR value for an album.
    ///
    /// # Arguments
    ///
    /// * `album_path` - Path to the album directory.
    /// * `dr_value` - The DR value to set (None to clear).
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the update fails.
    pub async fn update_dr_value<P: AsRef<Path>>(
        &self,
        album_path: P,
        dr_value: Option<&str>,
    ) -> Result<(), LibraryError> {
        let album_path_str = album_path.as_ref().to_string_lossy().to_string();

        query("UPDATE albums SET dr_value = ?, updated_at = CURRENT_TIMESTAMP WHERE path = ?")
            .bind(dr_value)
            .bind(album_path_str)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Updates multiple tracks in a single transaction.
    ///
    /// # Arguments
    ///
    /// * `tracks` - Vector of tracks to update or insert.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the batch update fails.
    pub async fn batch_update_tracks(&self, tracks: Vec<Track>) -> Result<(), LibraryError> {
        let mut tx = self.pool.begin().await?;

        for track in tracks {
            query(
                "INSERT INTO tracks (album_id, title, track_number, disc_number, duration_ms, path, file_size, format, codec, sample_rate, bits_per_sample, channels, is_lossless, is_high_resolution)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(path) DO UPDATE SET
                    album_id = excluded.album_id,
                    title = excluded.title,
                    track_number = excluded.track_number,
                    disc_number = excluded.disc_number,
                    duration_ms = excluded.duration_ms,
                    file_size = excluded.file_size,
                    format = excluded.format,
                    codec = excluded.codec,
                    sample_rate = excluded.sample_rate,
                    bits_per_sample = excluded.bits_per_sample,
                    channels = excluded.channels,
                    is_lossless = excluded.is_lossless,
                    is_high_resolution = excluded.is_high_resolution,
                    updated_at = CURRENT_TIMESTAMP",
            )
            .bind(track.album_id)
            .bind(track.title)
            .bind(track.track_number)
            .bind(track.disc_number)
            .bind(track.duration_ms)
            .bind(track.path)
            .bind(track.file_size)
            .bind(track.format)
            .bind(track.codec)
            .bind(track.sample_rate)
            .bind(track.bits_per_sample)
            .bind(track.channels)
            .bind(track.is_lossless)
            .bind(track.is_high_resolution)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Updates multiple albums in a single transaction.
    ///
    /// # Arguments
    ///
    /// * `albums` - Vector of albums to update or insert.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the batch update fails.
    pub async fn batch_update_albums(&self, albums: Vec<Album>) -> Result<(), LibraryError> {
        let mut tx = self.pool.begin().await?;

        for album in albums {
            // Update existing album
            query(
                "INSERT INTO albums (artist_id, title, year, genre, compilation, path, dr_value, artwork_path, format, bits_per_sample, sample_rate)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(artist_id, title, year) DO UPDATE SET
                    genre = excluded.genre,
                    compilation = excluded.compilation,
                    path = excluded.path,
                    dr_value = excluded.dr_value,
                    artwork_path = excluded.artwork_path,
                    format = excluded.format,
                    bits_per_sample = excluded.bits_per_sample,
                    sample_rate = excluded.sample_rate,
                    updated_at = CURRENT_TIMESTAMP",
            )
            // Insert new album
            .bind(album.artist_id)
            .bind(album.title)
            .bind(album.year)
            .bind(album.genre)
            .bind(album.compilation)
            .bind(album.path)
            .bind(album.dr_value)
            .bind(album.artwork_path)
            .bind(album.format)
            .bind(album.bits_per_sample)
            .bind(album.sample_rate)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Removes multiple tracks and cleans up empty albums/artists.
    ///
    /// # Arguments
    ///
    /// * `track_paths` - Vector of track paths to remove.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the batch removal fails.
    pub async fn batch_remove_tracks(&self, track_paths: Vec<String>) -> Result<(), LibraryError> {
        if track_paths.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        let placeholders: Vec<&str> = track_paths.iter().map(|_| "?").collect();
        let query_string = format!(
            "DELETE FROM tracks WHERE path IN ({})",
            placeholders.join(", ")
        );

        let mut delete_query = query(&query_string);
        for path in &track_paths {
            delete_query = delete_query.bind(path);
        }
        delete_query.execute(&mut *tx).await?;

        // Clean up empty albums
        query("DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)")
            .execute(&mut *tx)
            .await?;

        // Clean up empty artists
        query("DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM albums)")
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Removes all tracks within a directory and its subdirectories.
    ///
    /// # Arguments
    ///
    /// * `directory_path` - Path to the directory to remove tracks from.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the operation fails.
    pub async fn remove_tracks_in_directory<P: AsRef<Path>>(
        &self,
        directory_path: P,
    ) -> Result<(), LibraryError> {
        let directory_path_str = directory_path.as_ref().to_string_lossy().to_string();
        let escaped_path = escape_like_pattern(&directory_path_str);
        let path_pattern = format!("{escaped_path}/%");

        let mut tx = self.pool.begin().await?;

        // Remove tracks in the directory
        let result = query("DELETE FROM tracks WHERE path LIKE ? ESCAPE '\\'")
            .bind(&path_pattern)
            .execute(&mut *tx)
            .await?;

        debug!(
            "Deleted {} tracks from directory {}",
            result.rows_affected(),
            directory_path_str
        );

        // Clean up empty albums
        let _album_result =
            query("DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)")
                .execute(&mut *tx)
                .await?;

        // Clean up empty artists
        let _artist_result =
            query("DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM albums)")
                .execute(&mut *tx)
                .await?;

        // Clear DR values for the deleted directory (in case album wasn't fully deleted)
        query("UPDATE albums SET dr_value = NULL WHERE path = ?")
            .bind(&directory_path_str)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Validates and cleans up orphaned records that reference non-existent files/directories.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `LibraryError` if the cleanup fails.
    pub async fn cleanup_orphaned_records(&self) -> Result<(), LibraryError> {
        debug!("Performing validation to clean up orphaned records");

        let mut tx = self.pool.begin().await?;

        // Find albums with non-existent directories
        let orphaned_albums: Vec<String> = query_scalar::<_, String>("SELECT path FROM albums")
            .fetch_all(&mut *tx)
            .await?;

        let albums_to_delete: Vec<String> = orphaned_albums
            .into_iter()
            .filter(|album_path| !Path::new(album_path).exists())
            .collect();

        if !albums_to_delete.is_empty() {
            debug!("Found {} orphaned albums to delete", albums_to_delete.len());

            // Delete orphaned albums (this will cascade to tracks)
            let placeholders: Vec<&str> = albums_to_delete.iter().map(|_| "?").collect();
            let query_str = format!(
                "DELETE FROM albums WHERE path IN ({})",
                placeholders.join(",")
            );
            let mut delete_query = query(&query_str);
            for path in &albums_to_delete {
                delete_query = delete_query.bind(path);
            }
            let album_result = delete_query.execute(&mut *tx).await?;

            debug!(
                "Deleted {} orphaned albums ({} rows)",
                albums_to_delete.len(),
                album_result.rows_affected()
            );

            // Clean up empty artists
            let artist_result = query(
                "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM albums)",
            )
            .execute(&mut *tx)
            .await?;

            debug!(
                "Deleted {} empty artists during cleanup",
                artist_result.rows_affected()
            );
        }

        // Also check for tracks with non-existent files
        let orphaned_tracks: Vec<String> = query_scalar::<_, String>("SELECT path FROM tracks")
            .fetch_all(&mut *tx)
            .await?;

        let tracks_to_delete: Vec<String> = orphaned_tracks
            .into_iter()
            .filter(|track_path| !Path::new(track_path).exists())
            .collect();

        if !tracks_to_delete.is_empty() {
            debug!("Found {} orphaned tracks to delete", tracks_to_delete.len());

            let placeholders: Vec<&str> = tracks_to_delete.iter().map(|_| "?").collect();
            let query_str = format!(
                "DELETE FROM tracks WHERE path IN ({})",
                placeholders.join(",")
            );
            let mut delete_query = query(&query_str);
            for path in &tracks_to_delete {
                delete_query = delete_query.bind(path);
            }
            let track_result = delete_query.execute(&mut *tx).await?;

            debug!(
                "Deleted {} orphaned tracks ({} rows)",
                tracks_to_delete.len(),
                track_result.rows_affected()
            );

            // Clean up empty albums and artists again
            let album_result =
                query("DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)")
                    .execute(&mut *tx)
                    .await?;

            debug!(
                "Deleted {} empty albums during track cleanup",
                album_result.rows_affected()
            );

            let artist_result = query(
                "DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM albums)",
            )
            .execute(&mut *tx)
            .await?;

            debug!(
                "Deleted {} empty artists during track cleanup",
                artist_result.rows_affected()
            );
        }

        tx.commit().await?;
        debug!("Validation cleanup completed successfully");
        Ok(())
    }

    /// Gets the database connection pool for advanced operations.
    ///
    /// # Returns
    ///
    /// A reference to the internal `SqlitePool`.
    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use crate::library::database::LibraryError::{InvalidData, NotFound};

    #[test]
    fn test_library_error_display() {
        let not_found_error = NotFound {
            entity: "album".to_string(),
            id: 123,
        };
        assert_eq!(
            not_found_error.to_string(),
            "Record not found: album with id 123"
        );

        let invalid_data_error = InvalidData {
            reason: "test reason".to_string(),
        };
        assert_eq!(invalid_data_error.to_string(), "Invalid data: test reason");
    }
}
