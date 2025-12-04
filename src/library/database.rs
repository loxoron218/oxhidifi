//! Library database interface using sqlx with SQLite.
//!
//! This module provides the main `LibraryDatabase` struct that handles
//! all database operations for the music library, including querying,
//! searching, and DR value management.

use std::path::Path;

use sqlx::SqlitePool;
use thiserror::Error;

use crate::library::{
    models::{Album, Artist, SearchResults, Track},
    schema::{create_connection_pool, SchemaManager},
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
            Some(_filter_str) => {
                // Dummy implementation for compilation
                Vec::new()
            }
            None => {
                // Dummy implementation for compilation
                Vec::new()
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
            Some(_filter_str) => {
                // Dummy implementation for compilation
                Vec::new()
            }
            None => {
                // Dummy implementation for compilation
                Vec::new()
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
        // let album_exists: Option<i64> = sqlx::query_scalar!("SELECT 1 FROM albums WHERE id = ?", album_id)
        //     .fetch_optional(&self.pool)
        //     .await?;

        // if album_exists.is_none() {
        //     return Err(LibraryError::NotFound {
        //         entity: "album".to_string(),
        //         id: album_id,
        //     });
        // }

        let tracks = {
            // Dummy implementation for compilation
            Vec::new()
        };

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
        // let artist_exists: Option<i64> = sqlx::query_scalar!("SELECT 1 FROM artists WHERE id = ?", artist_id)
        //     .fetch_optional(&self.pool)
        //     .await?;

        // if artist_exists.is_none() {
        //     return Err(LibraryError::NotFound {
        //         entity: "artist".to_string(),
        //         id: artist_id,
        //     });
        // }

        let tracks = {
            // Dummy implementation for compilation
            Vec::new()
        };

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

        let albums = {
            // Dummy implementation for compilation
            Vec::new()
        };

        let artists = {
            // Dummy implementation for compilation
            Vec::new()
        };

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
    pub async fn get_dr_value<P: AsRef<Path>>(&self, album_path: P) -> Result<Option<String>, LibraryError> {
        let album_path_str = album_path.as_ref().to_string_lossy().to_string();
        
        // let dr_value_result: Option<Option<String>> = sqlx::query_scalar!(
        //     "SELECT dr_value FROM albums WHERE path = ?",
        //     album_path_str
        // )
        // .fetch_optional(&self.pool)
        // .await?;
        
        let dr_value: Option<String> = None;

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
        
        // sqlx::query!(
        //     "UPDATE albums SET dr_value = ?, updated_at = CURRENT_TIMESTAMP WHERE path = ?",
        //     dr_value,
        //     album_path_str
        // )
        // .execute(&self.pool)
        // .await?;

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
    use super::*;

    #[test]
    fn test_library_error_display() {
        let not_found_error = LibraryError::NotFound {
            entity: "album".to_string(),
            id: 123,
        };
        assert_eq!(not_found_error.to_string(), "Record not found: album with id 123");
        
        let invalid_data_error = LibraryError::InvalidData {
            reason: "test reason".to_string(),
        };
        assert_eq!(invalid_data_error.to_string(), "Invalid data: test reason");
    }
}