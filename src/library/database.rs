//! Library database interface using sqlx with SQLite.
//!
//! This module provides the main `LibraryDatabase` struct that handles
//! all database operations for the music library, including querying,
//! searching, and DR value management.

use std::path::Path;

use {
    sqlx::{SqlitePool, query, query_as, query_scalar},
    thiserror::Error,
};

use crate::library::{
    models::{Album, Artist, SearchResults, Track},
    schema::{SchemaManager, create_connection_pool},
};

/// Error type for library database operations.
#[derive(Error, Debug)]
pub enum LibraryError {
    /// Database connection or query error.
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    /// Schema initialization error.
    #[error("Schema error: {0}")]
    SchemaError(#[from] crate::library::schema::SchemaError),
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
pub struct LibraryDatabase {
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
        let schema_manager = SchemaManager::new(pool.clone());
        schema_manager.initialize_schema().await?;

        Ok(LibraryDatabase { pool })
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
                let search_pattern = format!("%{}%", filter_str);
                query_as::<_, Album>(
                    r#"
                    SELECT id, artist_id, title, year, genre, compilation, path, dr_value,
                           artwork_path, created_at, updated_at
                    FROM albums
                    WHERE title LIKE ?
                    ORDER BY title, year
                    "#,
                )
                .bind(search_pattern)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                query_as::<_, Album>(
                    r#"
                    SELECT id, artist_id, title, year, genre, compilation, path, dr_value,
                           artwork_path, created_at, updated_at
                    FROM albums
                    ORDER BY title, year
                    "#,
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
                let search_pattern = format!("%{}%", filter_str);
                query_as::<_, Artist>(
                    r#"
                    SELECT id, name, created_at, updated_at
                    FROM artists
                    WHERE name LIKE ?
                    ORDER BY name
                    "#,
                )
                .bind(search_pattern)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                query_as::<_, Artist>(
                    r#"
                    SELECT id, name, created_at, updated_at
                    FROM artists
                    ORDER BY name
                    "#,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(artists)
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
            r#"
            SELECT id, album_id, title, track_number, disc_number, duration_ms, path,
                   file_size, format, sample_rate, bits_per_sample, channels, created_at, updated_at
            FROM tracks
            WHERE album_id = ?
            ORDER BY disc_number, track_number, title
            "#,
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
            r#"
            SELECT t.id, t.album_id, t.title, t.track_number, t.disc_number, t.duration_ms, t.path,
                   t.file_size, t.format, t.sample_rate, t.bits_per_sample, t.channels, t.created_at, t.updated_at
            FROM tracks t
            JOIN albums a ON t.album_id = a.id
            WHERE a.artist_id = ?
            ORDER BY a.title, t.disc_number, t.track_number, t.title
            "#
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
        let search_pattern = format!("%{}%", query);

        let albums = query_as::<_, Album>(
            r#"
            SELECT id, artist_id, title, year, genre, compilation, path, dr_value,
                   artwork_path, created_at, updated_at
            FROM albums
            WHERE title LIKE ?
            ORDER BY title, year
            "#,
        )
        .bind(&search_pattern)
        .fetch_all(&self.pool)
        .await?;

        let artists = query_as::<_, Artist>(
            r#"
            SELECT id, name, created_at, updated_at
            FROM artists
            WHERE name LIKE ?
            ORDER BY name
            "#,
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
            // Check if track exists
            let existing_track: Option<i64> = query_scalar("SELECT id FROM tracks WHERE path = ?")
                .bind(&track.path)
                .fetch_optional(&mut *tx)
                .await?;

            if let Some(track_id) = existing_track {
                // Update existing track
                query(
                    "UPDATE tracks SET album_id = ?, title = ?, track_number = ?, disc_number = ?, duration_ms = ?, file_size = ?, format = ?, sample_rate = ?, bits_per_sample = ?, channels = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"
                )
                .bind(track.album_id)
                .bind(track.title)
                .bind(track.track_number)
                .bind(track.disc_number)
                .bind(track.duration_ms)
                .bind(track.file_size)
                .bind(track.format)
                .bind(track.sample_rate)
                .bind(track.bits_per_sample)
                .bind(track.channels)
                .bind(track_id)
                .execute(&mut *tx)
                .await?;
            } else {
                // Insert new track
                query(
                    "INSERT INTO tracks (album_id, title, track_number, disc_number, duration_ms, path, file_size, format, sample_rate, bits_per_sample, channels) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(track.album_id)
                .bind(track.title)
                .bind(track.track_number)
                .bind(track.disc_number)
                .bind(track.duration_ms)
                .bind(track.path)
                .bind(track.file_size)
                .bind(track.format)
                .bind(track.sample_rate)
                .bind(track.bits_per_sample)
                .bind(track.channels)
                .execute(&mut *tx)
                .await?;
            }
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
            // Check if album exists
            let existing_album: Option<i64> = query_scalar(
                "SELECT id FROM albums WHERE artist_id = ? AND title = ? AND year IS ?",
            )
            .bind(album.artist_id)
            .bind(&album.title)
            .bind(album.year)
            .fetch_optional(&mut *tx)
            .await?;

            if let Some(album_id) = existing_album {
                // Update existing album
                query(
                    "UPDATE albums SET genre = ?, compilation = ?, path = ?, dr_value = ?, artwork_path = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"
                )
                .bind(album.genre)
                .bind(album.compilation)
                .bind(album.path)
                .bind(album.dr_value)
                .bind(album.artwork_path)
                .bind(album_id)
                .execute(&mut *tx)
                .await?;
            } else {
                // Insert new album
                query(
                    "INSERT INTO albums (artist_id, title, year, genre, compilation, path, dr_value, artwork_path) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(album.artist_id)
                .bind(album.title)
                .bind(album.year)
                .bind(album.genre)
                .bind(album.compilation)
                .bind(album.path)
                .bind(album.dr_value)
                .bind(album.artwork_path)
                .execute(&mut *tx)
                .await?;
            }
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
        let mut tx = self.pool.begin().await?;

        // Remove tracks
        for path in track_paths {
            query("DELETE FROM tracks WHERE path = ?")
                .bind(path)
                .execute(&mut *tx)
                .await?;
        }

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

    /// Gets the database connection pool for advanced operations.
    ///
    /// # Returns
    ///
    /// A reference to the internal `SqlitePool`.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use crate::library::database::LibraryError;

    #[test]
    fn test_library_error_display() {
        let not_found_error = LibraryError::NotFound {
            entity: "album".to_string(),
            id: 123,
        };
        assert_eq!(
            not_found_error.to_string(),
            "Record not found: album with id 123"
        );

        let invalid_data_error = LibraryError::InvalidData {
            reason: "test reason".to_string(),
        };
        assert_eq!(invalid_data_error.to_string(), "Invalid data: test reason");
    }
}
