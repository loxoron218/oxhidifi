//! Database schema definition and versioning for the music library.
//!
//! This module defines the SQLite database schema and provides schema
//! versioning capabilities for future migrations.

use anyhow::Context;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;
use thiserror::Error;

/// Error type for schema operations.
#[derive(Error, Debug)]
pub enum SchemaError {
    /// Database connection error.
    #[error("Database connection error: {0}")]
    ConnectionError(#[from] sqlx::Error),
    /// Schema migration error.
    #[error("Schema migration error: {reason}")]
    MigrationError { reason: String },
}

/// Current schema version.
pub const CURRENT_SCHEMA_VERSION: i32 = 1;

/// Database schema definition.
pub struct SchemaManager {
    pool: SqlitePool,
}

impl SchemaManager {
    /// Creates a new schema manager.
    ///
    /// # Arguments
    ///
    /// * `pool` - The SQLite connection pool.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `SchemaManager` or a `SchemaError`.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Initializes the database schema.
    ///
    /// This method creates all necessary tables and ensures the schema
    /// is at the current version.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `SchemaError` if schema initialization fails.
    pub async fn initialize_schema(&self) -> Result<(), SchemaError> {
        // Create schema version table if it doesn't exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Check current schema version
        let current_version: Option<i32> = sqlx::query_scalar("SELECT version FROM schema_version LIMIT 1")
            .fetch_optional(&self.pool)
            .await?;

        match current_version {
            None => {
                // Fresh database, create all tables and set version
                self.create_tables().await?;
                sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
                    .bind(CURRENT_SCHEMA_VERSION)
                    .execute(&self.pool)
                    .await?;
            }
            Some(version) if version == CURRENT_SCHEMA_VERSION => {
                // Schema is up to date
            }
            Some(version) => {
                // Migration needed (not implemented in Phase 1)
                return Err(SchemaError::MigrationError {
                    reason: format!("Schema migration from version {} not implemented", version),
                });
            }
        }

        Ok(())
    }

    /// Creates all database tables.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `SchemaError` if table creation fails.
    async fn create_tables(&self) -> Result<(), SchemaError> {
        // Artists table
        sqlx::query(
            r#"
            CREATE TABLE artists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Albums table
        sqlx::query(
            r#"
            CREATE TABLE albums (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                artist_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                year INTEGER,
                genre TEXT,
                compilation BOOLEAN DEFAULT FALSE,
                path TEXT NOT NULL UNIQUE,
                dr_value TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
                UNIQUE (artist_id, title, year)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Tracks table
        sqlx::query(
            r#"
            CREATE TABLE tracks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                album_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                track_number INTEGER,
                disc_number INTEGER DEFAULT 1,
                duration_ms INTEGER NOT NULL,
                path TEXT NOT NULL UNIQUE,
                file_size INTEGER NOT NULL,
                format TEXT NOT NULL,
                sample_rate INTEGER NOT NULL,
                bits_per_sample INTEGER NOT NULL,
                channels INTEGER NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes for performance
        sqlx::query("CREATE INDEX idx_artists_name ON artists (name)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX idx_albums_artist_id ON albums (artist_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX idx_albums_title ON albums (title)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX idx_tracks_album_id ON tracks (album_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX idx_tracks_path ON tracks (path)")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Gets the current schema version.
    ///
    /// # Returns
    ///
    /// The current schema version, or 0 if not initialized.
    pub async fn get_current_version(&self) -> Result<i32, SchemaError> {
        let version: Option<i32> = sqlx::query_scalar("SELECT version FROM schema_version LIMIT 1")
            .fetch_optional(&self.pool)
            .await?;

        Ok(version.unwrap_or(0))
    }
}

/// Gets the database connection string following XDG Base Directory specification.
///
/// # Returns
///
/// The database connection string.
pub fn get_database_url() -> String {
    let mut config_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    config_dir.push("oxhidifi");
    config_dir.push("library.db");

    // Create parent directories if they don't exist
    if let Some(parent) = config_dir.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    format!("sqlite://{}", config_dir.to_string_lossy())
}

/// Creates a database connection pool.
///
/// # Returns
///
/// A `Result` containing the connection pool or a `SchemaError`.
///
/// # Errors
///
/// Returns `SchemaError` if connection pool creation fails.
pub async fn create_connection_pool() -> Result<SqlitePool, SchemaError> {
    let database_url = get_database_url();
    
    let options = SqliteConnectOptions::new()
        .filename(database_url.replace("sqlite://", ""))
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

    let pool = SqlitePool::connect_with(options).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version_constant() {
        assert_eq!(CURRENT_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_schema_error_display() {
        let migration_error = SchemaError::MigrationError { 
            reason: "test error".to_string() 
        };
        assert_eq!(migration_error.to_string(), "Schema migration error: test error");
    }
}