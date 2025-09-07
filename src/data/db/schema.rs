use sqlx::{Result, SqlitePool, query};

/// Initializes the database schema, creating tables if they do not already exist.
/// This function ensures that the `folders`, `artists`, `albums`, and `tracks` tables
/// are present with the correct schema, including any necessary column additions via ALTER TABLE.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
///
/// # Returns
/// A `Result` indicating success or an `sqlx::Error` on failure.
pub async fn init_db(pool: &SqlitePool) -> Result<()> {
    // Create folders table if it doesn't exist
    query(
        "CREATE TABLE IF NOT EXISTS folders (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE
        )",
    )
    .execute(pool)
    .await?;

    // Create artists table if it doesn't exist
    query(
        "CREATE TABLE IF NOT EXISTS artists (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE
        )",
    )
    .execute(pool)
    .await?;

    // Create albums table if it doesn't exist
    query(
        "CREATE TABLE IF NOT EXISTS albums (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            artist_id INTEGER NOT NULL,
            year INTEGER,
            cover_art BLOB,
            folder_id INTEGER NOT NULL,
            dr_value INTEGER,
            dr_is_best BOOLEAN DEFAULT FALSE,
            original_release_date TEXT
        )",
    )
    .execute(pool)
    .await?;

    // Add `dr_is_best` column if it doesn't exist.
    // `.ok()` is used here to gracefully handle the error if the column already exists,
    // which is expected behavior for idempotent schema migrations.
    query("ALTER TABLE albums ADD COLUMN dr_is_best BOOLEAN DEFAULT FALSE")
        .execute(pool)
        .await
        .ok();
    // Add `original_release_date` column if it doesn't exist.
    // `.ok()` is used here to gracefully handle the error if the column already exists.
    query("ALTER TABLE albums ADD COLUMN original_release_date TEXT")
        .execute(pool)
        .await
        .ok();

    // Create tracks table if it doesn't exist
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
            sample_rate INTEGER,
            FOREIGN KEY(album_id) REFERENCES albums(id),
            FOREIGN KEY(artist_id) REFERENCES artists(id)
        )",
    )
    .execute(pool)
    .await?;

    // Create indexes for faster queries. `IF NOT EXISTS` ensures that these statements
    // can be run safely multiple times.
    // Index on album artist ID for faster lookups of albums by artist
    query("CREATE INDEX IF NOT EXISTS idx_album_artist_id ON albums(artist_id)")
        .execute(pool)
        .await?;

    // Index on album folder ID for faster lookups of albums by folder
    query("CREATE INDEX IF NOT EXISTS idx_album_folder_id ON albums(folder_id)")
        .execute(pool)
        .await?;

    // Index on track album ID for faster lookups of tracks by album
    query("CREATE INDEX IF NOT EXISTS idx_track_album_id ON tracks(album_id)")
        .execute(pool)
        .await?;

    // Index on track artist ID for faster lookups of tracks by artist
    query("CREATE INDEX IF NOT EXISTS idx_track_artist_id ON tracks(artist_id)")
        .execute(pool)
        .await?;

    // Index on tracks.path for faster file existence checks
    query("CREATE INDEX IF NOT EXISTS idx_track_path ON tracks(path)")
        .execute(pool)
        .await?;

    // Index on albums.title for faster album lookups
    query("CREATE INDEX IF NOT EXISTS idx_album_title ON albums(title)")
        .execute(pool)
        .await?;

    // Index on artists.name for faster artist lookups
    query("CREATE INDEX IF NOT EXISTS idx_artist_name ON artists(name)")
        .execute(pool)
        .await?;

    // Composite index for album lookups by title and artist
    query("CREATE INDEX IF NOT EXISTS idx_album_title_artist ON albums(title, artist_id)")
        .execute(pool)
        .await?;

    // Unique index on album title/artist/folder combination to prevent duplicates
    query("CREATE UNIQUE INDEX IF NOT EXISTS uidx_album_title_artist_folder ON albums(title, artist_id, folder_id)")
        .execute(pool)
        .await?;

    // Additional indexes for performance optimization
    // Index on albums.dr_value for faster DR-based queries
    query("CREATE INDEX IF NOT EXISTS idx_albums_dr_value ON albums(dr_value)")
        .execute(pool)
        .await?;

    // Index on albums.dr_is_best for faster DR synchronization
    query("CREATE INDEX IF NOT EXISTS idx_albums_dr_is_best ON albums(dr_is_best)")
        .execute(pool)
        .await?;

    // Index on albums.year for faster year-based sorting
    query("CREATE INDEX IF NOT EXISTS idx_albums_year ON albums(year)")
        .execute(pool)
        .await?;

    // Index on tracks.duration for potential query optimizations
    query("CREATE INDEX IF NOT EXISTS idx_tracks_duration ON tracks(duration)")
        .execute(pool)
        .await?;
    Ok(())
}
