//! Tests for library database search functionality.
//!
//! This module contains database tests for `search_tracks`, `search_albums`, and `search_artists`.

use {
    anyhow::{Result, bail},
    sqlx::{
        query,
        sqlite::{SqliteConnectOptions, SqlitePool},
    },
    tempfile::TempDir,
};

use crate::library::database::{
    LibraryDatabase,
    LibraryError::{InvalidData, NotFound},
    escape_like_pattern,
};

#[test]
fn library_error_display() {
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

#[test]
fn escape_like_pattern_basic() {
    assert_eq!(escape_like_pattern("hello"), "hello");
    assert_eq!(escape_like_pattern("hello world"), "hello world");
}

#[test]
fn escape_like_pattern_special_chars() {
    assert_eq!(escape_like_pattern("100%"), "100\\%");
    assert_eq!(escape_like_pattern("test_name"), "test\\_name");
    assert_eq!(escape_like_pattern("path\\to\\file"), "path\\\\to\\\\file");
    assert_eq!(escape_like_pattern("a%b_c\\d"), "a\\%b\\_c\\\\d");
}

#[test]
fn escape_like_pattern_empty() {
    assert_eq!(escape_like_pattern(""), "");
}

async fn create_test_database() -> Result<(LibraryDatabase, TempDir)> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);

    let pool = SqlitePool::connect_with(options).await?;

    query(
        "CREATE TABLE IF NOT EXISTS artists (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await?;

    query(
        "CREATE TABLE IF NOT EXISTS albums (
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
        )",
    )
    .execute(&pool)
    .await?;

    query(
        "CREATE TABLE IF NOT EXISTS tracks (
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
        )",
    )
    .execute(&pool)
    .await?;

    query("CREATE INDEX IF NOT EXISTS idx_artists_name ON artists (name)")
        .execute(&pool)
        .await?;

    query("CREATE INDEX IF NOT EXISTS idx_albums_artist_id ON albums (artist_id)")
        .execute(&pool)
        .await?;

    query("CREATE INDEX IF NOT EXISTS idx_albums_title ON albums (title)")
        .execute(&pool)
        .await?;

    query("CREATE INDEX IF NOT EXISTS idx_tracks_album_id ON tracks (album_id)")
        .execute(&pool)
        .await?;

    let db = LibraryDatabase::new_with_pool(pool).await?;
    Ok((db, temp_dir))
}

#[tokio::test]
async fn search_tracks_empty_database() -> Result<()> {
    let (db, _temp_dir) = create_test_database().await?;

    let results = db.search_tracks("test").await?;
    if !results.is_empty() {
        bail!("Expected empty results for empty database");
    }

    Ok(())
}

#[tokio::test]
async fn search_tracks_with_data() -> Result<()> {
    let (db, _temp_dir) = create_test_database().await?;

    query("INSERT INTO artists (id, name) VALUES (1, 'Test Artist')")
        .execute(db.pool())
        .await?;

    query(
        "INSERT INTO albums (id, artist_id, title, path) VALUES (1, 1, 'Test Album', \
         '/path/to/album')",
    )
    .execute(db.pool())
    .await?;

    query(
        "INSERT INTO tracks (id, album_id, title, duration_ms, path, file_size, format, codec, \
         sample_rate, bits_per_sample, channels, is_lossless, is_high_resolution) VALUES (1, 1, \
         'Great Track', 180000, '/path/to/track.flac', 1000000, 'FLAC', 'FLAC', 96000, 24, 2, \
         true, true)",
    )
    .execute(db.pool())
    .await?;

    query(
        "INSERT INTO tracks (id, album_id, title, duration_ms, path, file_size, format, codec, \
         sample_rate, bits_per_sample, channels, is_lossless, is_high_resolution) VALUES (2, 1, \
         'Another Song', 200000, '/path/to/track2.flac', 2000000, 'FLAC', 'FLAC', 48000, 16, 2, \
         true, false)",
    )
    .execute(db.pool())
    .await?;

    let results = db.search_tracks("Great").await?;
    if results.len() != 1 {
        bail!("Expected 1 track matching 'Great', got {}", results.len());
    }
    if results[0].track.title != "Great Track" {
        bail!(
            "Expected track title 'Great Track', got {}",
            results[0].track.title
        );
    }
    if results[0].artist_name != "Test Artist" {
        bail!(
            "Expected artist name 'Test Artist', got {}",
            results[0].artist_name
        );
    }
    if results[0].album_title != "Test Album" {
        bail!(
            "Expected album title 'Test Album', got {}",
            results[0].album_title
        );
    }

    let results = db.search_tracks("Track").await?;
    if results.len() != 1 {
        bail!("Expected 1 track matching 'Track', got {}", results.len());
    }

    let results = db.search_tracks("nonexistent").await?;
    if !results.is_empty() {
        bail!("Expected no results for nonexistent query");
    }

    Ok(())
}

#[tokio::test]
async fn search_tracks_case_insensitive() -> Result<()> {
    let (db, _temp_dir) = create_test_database().await?;

    query("INSERT INTO artists (id, name) VALUES (1, 'Artist')")
        .execute(db.pool())
        .await?;

    query("INSERT INTO albums (id, artist_id, title, path) VALUES (1, 1, 'Album', '/path')")
        .execute(db.pool())
        .await?;

    query(
        "INSERT INTO tracks (id, album_id, title, duration_ms, path, file_size, format, codec, \
         sample_rate, bits_per_sample, channels, is_lossless, is_high_resolution) VALUES (1, 1, \
         'LOWER', 1000, '/p1', 1, 'F', 'C', 1, 1, 1, false, false)",
    )
    .execute(db.pool())
    .await?;

    let results = db.search_tracks("lower").await?;
    if results.len() != 1 {
        bail!("Expected 1 result for 'lower', got {}", results.len());
    }

    let results = db.search_tracks("LOWER").await?;
    if results.len() != 1 {
        bail!("Expected 1 result for 'LOWER', got {}", results.len());
    }

    let results = db.search_tracks("LoWeR").await?;
    if results.len() != 1 {
        bail!("Expected 1 result for 'LoWeR', got {}", results.len());
    }

    Ok(())
}

#[tokio::test]
async fn search_albums_empty_database() -> Result<()> {
    let (db, _temp_dir) = create_test_database().await?;

    let results = db.search_albums("test").await?;
    if !results.is_empty() {
        bail!("Expected empty results for empty database");
    }

    Ok(())
}

#[tokio::test]
async fn search_albums_with_data() -> Result<()> {
    let (db, _temp_dir) = create_test_database().await?;

    query("INSERT INTO artists (id, name) VALUES (1, 'Test Artist')")
        .execute(db.pool())
        .await?;

    query(
        "INSERT INTO albums (id, artist_id, title, year, genre, path) VALUES (1, 1, 'Blue Album', \
         2023, 'Jazz', '/path/to/blue')",
    )
    .execute(db.pool())
    .await?;

    query(
        "INSERT INTO albums (id, artist_id, title, year, genre, path) VALUES (2, 1, 'Red Album', \
         2022, 'Rock', '/path/to/red')",
    )
    .execute(db.pool())
    .await?;

    let results = db.search_albums("Blue").await?;
    if results.len() != 1 {
        bail!("Expected 1 album matching 'Blue', got {}", results.len());
    }
    if results[0].title != "Blue Album" {
        bail!(
            "Expected album title 'Blue Album', got {}",
            results[0].title
        );
    }

    let results = db.search_albums("Album").await?;
    if results.len() != 2 {
        bail!("Expected 2 albums matching 'Album', got {}", results.len());
    }

    let results = db.search_albums("nonexistent").await?;
    if !results.is_empty() {
        bail!("Expected no results for nonexistent query");
    }

    Ok(())
}

#[tokio::test]
async fn search_artists_empty_database() -> Result<()> {
    let (db, _temp_dir) = create_test_database().await?;

    let results = db.search_artists("test").await?;
    if !results.is_empty() {
        bail!("Expected empty results for empty database");
    }

    Ok(())
}

#[tokio::test]
async fn search_artists_with_data() -> Result<()> {
    let (db, _temp_dir) = create_test_database().await?;

    query("INSERT INTO artists (id, name) VALUES (1, 'Jazz Master')")
        .execute(db.pool())
        .await?;

    query("INSERT INTO artists (id, name) VALUES (2, 'Rock Star')")
        .execute(db.pool())
        .await?;

    query("INSERT INTO artists (id, name) VALUES (3, 'Classical Genius')")
        .execute(db.pool())
        .await?;

    query("INSERT INTO albums (id, artist_id, title, path) VALUES (1, 1, 'Album 1', '/path1')")
        .execute(db.pool())
        .await?;

    query("INSERT INTO albums (id, artist_id, title, path) VALUES (2, 1, 'Album 2', '/path2')")
        .execute(db.pool())
        .await?;

    let results = db.search_artists("Master").await?;
    if results.len() != 1 {
        bail!("Expected 1 artist matching 'Master', got {}", results.len());
    }
    if results[0].name != "Jazz Master" {
        bail!(
            "Expected artist name 'Jazz Master', got {}",
            results[0].name
        );
    }
    if results[0].album_count != 2 {
        bail!("Expected album count of 2, got {}", results[0].album_count);
    }

    let results = db.search_artists("Star").await?;
    if results.len() != 1 {
        bail!("Expected 1 artist matching 'Star', got {}", results.len());
    }
    if results[0].album_count != 0 {
        bail!("Expected album count of 0, got {}", results[0].album_count);
    }

    let results = db.search_artists("nonexistent").await?;
    if !results.is_empty() {
        bail!("Expected no results for nonexistent query");
    }

    Ok(())
}

#[tokio::test]
async fn search_with_special_characters() -> Result<()> {
    let (db, _temp_dir) = create_test_database().await?;

    query("INSERT INTO artists (id, name) VALUES (1, 'Artist%100')")
        .execute(db.pool())
        .await?;

    query("INSERT INTO albums (id, artist_id, title, path) VALUES (1, 1, 'Album_Test', '/path')")
        .execute(db.pool())
        .await?;

    query(
        "INSERT INTO tracks (id, album_id, title, duration_ms, path, file_size, format, codec, \
         sample_rate, bits_per_sample, channels, is_lossless, is_high_resolution) VALUES (1, 1, \
         'Track%100', 1000, '/p', 1, 'F', 'C', 1, 1, 1, false, false)",
    )
    .execute(db.pool())
    .await?;

    let results = db.search_artists("100").await?;
    if results.len() != 1 {
        bail!("Expected 1 artist matching '100', got {}", results.len());
    }

    let results = db.search_albums("Test").await?;
    if results.len() != 1 {
        bail!("Expected 1 album matching 'Test', got {}", results.len());
    }

    let results = db.search_tracks("100").await?;
    if results.len() != 1 {
        bail!("Expected 1 track matching '100', got {}", results.len());
    }

    Ok(())
}
