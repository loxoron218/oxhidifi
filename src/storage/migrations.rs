//! `SQLite` schema migrations and index creation.

use sqlx::{SqlitePool, query};

use crate::storage::{StorageError::Database, StorageResult};

/// Run all database migrations to create tables.
///
/// # Errors
///
/// Returns an error if any SQL statement fails.
pub async fn run(pool: &SqlitePool) -> StorageResult<()> {
    query(
        "CREATE TABLE IF NOT EXISTS artists (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            album_count INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| Database(format!("Migration failed: {e}")))?;

    query(
        "CREATE TABLE IF NOT EXISTS albums (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            artist_id INTEGER NOT NULL REFERENCES artists(id),
            year INTEGER,
            genre TEXT,
            artwork_path TEXT,
            track_count INTEGER NOT NULL DEFAULT 0,
            total_duration REAL NOT NULL DEFAULT 0.0,
            format_summary TEXT NOT NULL,
            lossless INTEGER NOT NULL DEFAULT 1
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| Database(format!("Migration failed: {e}")))?;

    query(
        "CREATE TABLE IF NOT EXISTS tracks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            number INTEGER,
            disc_number INTEGER DEFAULT 1,
            duration REAL NOT NULL CHECK(duration >= 0),
            file_path TEXT NOT NULL UNIQUE,
            content_hash TEXT,
            format TEXT NOT NULL,
            sample_rate INTEGER NOT NULL CHECK(sample_rate > 0),
            bit_depth INTEGER,
            channels INTEGER NOT NULL CHECK(channels > 0),
            codec TEXT NOT NULL,
            lossless INTEGER NOT NULL,
            bitrate INTEGER,
            album_id INTEGER REFERENCES albums(id),
            artist_id INTEGER REFERENCES artists(id),
            file_size INTEGER NOT NULL CHECK(file_size > 0),
            last_modified TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| Database(format!("Migration failed: {e}")))?;

    query(
        "CREATE TABLE IF NOT EXISTS library_directories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE,
            enabled INTEGER NOT NULL DEFAULT 1,
            last_scanned TEXT,
            added_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| Database(format!("Migration failed: {e}")))?;

    query(
        "CREATE TABLE IF NOT EXISTS playback_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            track_id INTEGER NOT NULL REFERENCES tracks(id),
            position INTEGER NOT NULL CHECK(position >= 0),
            context_type TEXT,
            context_id INTEGER,
            added_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| Database(format!("Migration failed: {e}")))?;

    create_indexes(pool).await
}

/// Create database indexes for query performance.
async fn create_indexes(pool: &SqlitePool) -> StorageResult<()> {
    query("CREATE INDEX IF NOT EXISTS idx_track_album_id ON tracks(album_id)")
        .execute(pool)
        .await
        .map_err(|e| Database(format!("Index creation failed: {e}")))?;

    query("CREATE INDEX IF NOT EXISTS idx_track_artist_id ON tracks(artist_id)")
        .execute(pool)
        .await
        .map_err(|e| Database(format!("Index creation failed: {e}")))?;

    query("CREATE INDEX IF NOT EXISTS idx_track_file_path ON tracks(file_path)")
        .execute(pool)
        .await
        .map_err(|e| Database(format!("Index creation failed: {e}")))?;

    query("CREATE INDEX IF NOT EXISTS idx_track_content_hash ON tracks(content_hash)")
        .execute(pool)
        .await
        .map_err(|e| Database(format!("Index creation failed: {e}")))?;

    query("CREATE INDEX IF NOT EXISTS idx_album_artist_id ON albums(artist_id)")
        .execute(pool)
        .await
        .map_err(|e| Database(format!("Index creation failed: {e}")))?;

    query("CREATE INDEX IF NOT EXISTS idx_queue_position ON playback_queue(position)")
        .execute(pool)
        .await
        .map_err(|e| Database(format!("Index creation failed: {e}")))?;

    Ok(())
}
