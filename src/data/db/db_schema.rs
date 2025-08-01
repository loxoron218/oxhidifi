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
            dr_completed BOOLEAN DEFAULT FALSE,
            original_release_date TEXT
        )",
    )
    .execute(pool)
    .await?;

    // Add `dr_completed` column if it doesn't exist.
    // `.ok()` is used here to gracefully handle the error if the column already exists,
    // which is expected behavior for idempotent schema migrations.
    query("ALTER TABLE albums ADD COLUMN dr_completed BOOLEAN DEFAULT FALSE")
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
            frequency INTEGER,
            FOREIGN KEY(album_id) REFERENCES albums(id),
            FOREIGN KEY(artist_id) REFERENCES artists(id)
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}
