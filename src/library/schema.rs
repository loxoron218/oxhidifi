//! Database schema definition and versioning for the music library.
//!
//! This module defines the SQLite database schema and provides schema
//! versioning capabilities for future migrations.

use std::{env::var, fs::create_dir_all, path::PathBuf};

use {
    sqlx::{
        Error as SqlxError, SqlitePool, query, query_scalar,
        sqlite::{SqliteConnectOptions, SqliteJournalMode::Wal, SqliteSynchronous::Normal},
    },
    thiserror::Error,
};

/// Error type for schema operations.
#[derive(Error, Debug)]
pub enum SchemaError {
    /// Database connection error.
    #[error("Database connection error: {0}")]
    ConnectionError(#[from] SqlxError),
    /// Schema migration error.
    #[error("Schema migration error: {reason}")]
    MigrationError { reason: String },
}

/// Current schema version.
pub const CURRENT_SCHEMA_VERSION: i32 = 4;

/// Database schema definition.
pub struct SchemaManager {
    /// SQLite connection pool for schema operations.
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
        query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Check current schema version
        let current_version: Option<i32> =
            query_scalar("SELECT version FROM schema_version LIMIT 1")
                .fetch_optional(&self.pool)
                .await?;

        match current_version {
            None => {
                // Fresh database, create all tables and set version
                self.create_tables().await?;
                query("INSERT INTO schema_version (version) VALUES (?)")
                    .bind(CURRENT_SCHEMA_VERSION)
                    .execute(&self.pool)
                    .await?;
            }
            Some(version) if version == CURRENT_SCHEMA_VERSION => {
                // Schema is up to date
            }
            Some(version) => {
                // Handle migrations
                self.migrate_schema(version).await?;
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
        query(
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
        query(
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
                artwork_path TEXT,
                format TEXT,
                bits_per_sample INTEGER,
                sample_rate INTEGER,
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
        query(
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
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes for performance
        query("CREATE INDEX idx_artists_name ON artists (name)")
            .execute(&self.pool)
            .await?;

        query("CREATE INDEX idx_albums_artist_id ON albums (artist_id)")
            .execute(&self.pool)
            .await?;

        query("CREATE INDEX idx_albums_title ON albums (title)")
            .execute(&self.pool)
            .await?;

        query("CREATE INDEX idx_tracks_album_id ON tracks (album_id)")
            .execute(&self.pool)
            .await?;

        query("CREATE INDEX idx_tracks_path ON tracks (path)")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Migrates the database schema from an older version.
    ///
    /// # Arguments
    ///
    /// * `from_version` - The current schema version.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    async fn migrate_schema(&self, from_version: i32) -> Result<(), SchemaError> {
        if from_version == 1 && CURRENT_SCHEMA_VERSION >= 2 {
            // Migration from v1 to v2: Add artwork_path column to albums table
            query("ALTER TABLE albums ADD COLUMN artwork_path TEXT")
                .execute(&self.pool)
                .await?;

            // Update schema version to 2
            query("UPDATE schema_version SET version = 2")
                .execute(&self.pool)
                .await?;

            // Now migrate from v2 to v3 if needed
            if CURRENT_SCHEMA_VERSION >= 3 {
                // Migration from v2 to v3: Add codec, is_lossless, and is_high_resolution columns to tracks table
                // and format column to albums table
                query("ALTER TABLE tracks ADD COLUMN codec TEXT NOT NULL DEFAULT ''")
                    .execute(&self.pool)
                    .await?;

                query("ALTER TABLE tracks ADD COLUMN is_lossless BOOLEAN NOT NULL DEFAULT FALSE")
                    .execute(&self.pool)
                    .await?;

                query("ALTER TABLE tracks ADD COLUMN is_high_resolution BOOLEAN NOT NULL DEFAULT FALSE")
                    .execute(&self.pool)
                    .await?;

                query("ALTER TABLE albums ADD COLUMN format TEXT")
                    .execute(&self.pool)
                    .await?;

                // Update schema version to 3
                query("UPDATE schema_version SET version = ?")
                    .bind(CURRENT_SCHEMA_VERSION)
                    .execute(&self.pool)
                    .await?;
            }
        } else if from_version == 2 && CURRENT_SCHEMA_VERSION >= 3 {
            // Migration from v2 to v3: Add codec, is_lossless, and is_high_resolution columns to tracks table
            // and format column to albums table
            query("ALTER TABLE tracks ADD COLUMN codec TEXT NOT NULL DEFAULT ''")
                .execute(&self.pool)
                .await?;

            query("ALTER TABLE tracks ADD COLUMN is_lossless BOOLEAN NOT NULL DEFAULT FALSE")
                .execute(&self.pool)
                .await?;

            query(
                "ALTER TABLE tracks ADD COLUMN is_high_resolution BOOLEAN NOT NULL DEFAULT FALSE",
            )
            .execute(&self.pool)
            .await?;

            query("ALTER TABLE albums ADD COLUMN format TEXT")
                .execute(&self.pool)
                .await?;

            // Update schema version
            query("UPDATE schema_version SET version = ?")
                .bind(CURRENT_SCHEMA_VERSION)
                .execute(&self.pool)
                .await?;
        } else if from_version == 3 && CURRENT_SCHEMA_VERSION >= 4 {
            // Migration from v3 to v4: Add bits_per_sample and sample_rate columns to albums table
            query("ALTER TABLE albums ADD COLUMN bits_per_sample INTEGER")
                .execute(&self.pool)
                .await?;

            query("ALTER TABLE albums ADD COLUMN sample_rate INTEGER")
                .execute(&self.pool)
                .await?;

            // Update schema version
            query("UPDATE schema_version SET version = ?")
                .bind(CURRENT_SCHEMA_VERSION)
                .execute(&self.pool)
                .await?;
        } else {
            return Err(SchemaError::MigrationError {
                reason: format!(
                    "Schema migration from version {} to {} not implemented",
                    from_version, CURRENT_SCHEMA_VERSION
                ),
            });
        }

        Ok(())
    }

    /// Gets the current schema version.
    ///
    /// # Returns
    ///
    /// The current schema version, or 0 if not initialized.
    pub async fn get_current_version(&self) -> Result<i32, SchemaError> {
        let version: Option<i32> = query_scalar("SELECT version FROM schema_version LIMIT 1")
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
    let mut config_dir = get_xdg_config_home();
    config_dir.push("oxhidifi");
    config_dir.push("library.db");

    // Create parent directories if they don't exist
    if let Some(parent) = config_dir.parent() {
        create_dir_all(parent).ok();
    }

    format!("sqlite://{}", config_dir.to_string_lossy())
}

/// Gets the XDG config home directory following XDG Base Directory specification.
///
/// Uses XDG_CONFIG_HOME environment variable if set, otherwise defaults to $HOME/.config
fn get_xdg_config_home() -> PathBuf {
    if let Ok(config_home) = var("XDG_CONFIG_HOME")
        && !config_home.is_empty()
    {
        return PathBuf::from(config_home);
    }

    if let Ok(home) = var("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".config");
        return path;
    }

    // Fallback to current directory if HOME is not set (shouldn't happen on Unix)
    PathBuf::from(".")
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
        .journal_mode(Wal)
        .synchronous(Normal);

    let pool = SqlitePool::connect_with(options).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use crate::library::{CURRENT_SCHEMA_VERSION, schema::SchemaError};

    #[test]
    fn test_schema_version_constant() {
        assert_eq!(CURRENT_SCHEMA_VERSION, 4);
    }

    #[test]
    fn test_schema_error_display() {
        let migration_error = SchemaError::MigrationError {
            reason: "test error".to_string(),
        };
        assert_eq!(
            migration_error.to_string(),
            "Schema migration error: test error"
        );
    }
}
