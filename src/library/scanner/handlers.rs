//! File system event handlers for the library scanner.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use {
    parking_lot::RwLock,
    sqlx::{Sqlite, Transaction, query, query_scalar},
    tracing::{debug, warn},
};

use crate::{
    audio::{
        artwork::{
            ArtworkSource::{Embedded, External},
            extract_artwork, find_external_artwork, save_embedded_artwork,
        },
        metadata::{TagReader, TrackMetadata},
    },
    config::settings::UserSettings,
    error::domain::LibraryError,
    library::database::LibraryDatabase,
};

/// Handles files that have been created or modified.
///
/// # Arguments
///
/// * `paths` - Paths of changed files.
/// * `database` - Database interface.
/// * `settings` - User settings.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// Returns `LibraryError` if processing fails.
pub async fn handle_files_changed(
    paths: Vec<PathBuf>,
    database: &LibraryDatabase,
    _settings: &RwLock<UserSettings>,
) -> Result<(), LibraryError> {
    // Group files by album directory
    let mut files_by_album: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for path in paths {
        if let Some(parent) = path.parent() {
            files_by_album
                .entry(parent.to_path_buf())
                .or_default()
                .push(path);
        }
    }

    // Process each album directory
    for (album_dir, album_files) in files_by_album {
        debug!("Processing album directory: {:?}", album_dir);

        // Extract metadata for all files in the album
        let mut tracks_metadata = Vec::new();
        for file_path in &album_files {
            match TagReader::read_metadata(file_path) {
                Ok(metadata) => {
                    tracks_metadata.push((file_path.clone(), metadata));
                }
                Err(e) => {
                    warn!("Failed to read metadata for {:?}: {}", file_path, e);

                    // Continue processing other files
                }
            }
        }

        if tracks_metadata.is_empty() {
            continue;
        }

        // Determine if this is a compilation album
        let is_compilation = is_compilation_album(&tracks_metadata);

        // Extract album and artist information
        let (album_info, artist_info) = extract_album_artist_info(&tracks_metadata, is_compilation);

        // Update database with new/modified tracks
        update_album_in_database(
            &album_dir,
            &album_info,
            &artist_info,
            &tracks_metadata,
            database,
            is_compilation,
        )
        .await?;
    }

    Ok(())
}

/// Handles files that have been removed.
///
/// # Arguments
///
/// * `paths` - Paths of removed files.
/// * `database` - Database interface.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// Returns `LibraryError` if processing fails.
pub async fn handle_files_removed(
    paths: Vec<PathBuf>,
    database: &LibraryDatabase,
) -> Result<(), LibraryError> {
    // Remove tracks from database
    let pool = database.pool();

    // Begin transaction
    let mut tx = pool.begin().await?;

    for path in paths {
        let path_str = path.to_string_lossy().to_string();
        let path_pattern = format!("{}/%", path_str);

        debug!(
            "Attempting to delete tracks for path: {} (pattern: {})",
            path_str, path_pattern
        );

        // Remove track from database (handle directories too)
        let result = query("DELETE FROM tracks WHERE path = ? OR path LIKE ?")
            .bind(&path_str)
            .bind(&path_pattern)
            .execute(&mut *tx)
            .await?;

        debug!(
            "Deleted {} tracks for path {}",
            result.rows_affected(),
            path_str
        );
    }

    // Clean up empty albums
    let album_result =
        query("DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)")
            .execute(&mut *tx)
            .await?;
    debug!("Deleted {} empty albums", album_result.rows_affected());

    // Clean up empty artists
    let artist_result =
        query("DELETE FROM artists WHERE id NOT IN (SELECT DISTINCT artist_id FROM albums)")
            .execute(&mut *tx)
            .await?;
    debug!("Deleted {} empty artists", artist_result.rows_affected());

    // Commit transaction
    tx.commit().await?;

    Ok(())
}

/// Handles files that have been renamed/moved.
///
/// # Arguments
///
/// * `paths` - Original and new paths of renamed files.
/// * `database` - Database interface.
/// * `settings` - User settings.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// Returns `LibraryError` if processing fails.
pub async fn handle_files_renamed(
    paths: Vec<(PathBuf, PathBuf)>,
    database: &LibraryDatabase,
    settings: &RwLock<UserSettings>,
) -> Result<(), LibraryError> {
    // Handle renames as remove + add
    let removed_paths: Vec<PathBuf> = paths.iter().map(|(from, _)| from.clone()).collect();
    let added_paths: Vec<PathBuf> = paths.iter().map(|(_, to)| to.clone()).collect();

    handle_files_removed(removed_paths, database).await?;
    handle_files_changed(added_paths, database, settings).await?;

    Ok(())
}

/// Determines if an album is a compilation.
///
/// # Arguments
///
/// * `tracks_metadata` - Metadata for tracks in the album.
///
/// # Returns
///
/// `true` if the album is a compilation, `false` otherwise.
fn is_compilation_album(tracks_metadata: &[(PathBuf, TrackMetadata)]) -> bool {
    if tracks_metadata.len() <= 1 {
        return false;
    }

    let mut artists = HashSet::new();
    for (_, metadata) in tracks_metadata {
        if let Some(artist) = &metadata.standard.artist {
            artists.insert(artist.clone());
        }
    }

    // If we have multiple different artists, it's likely a compilation
    artists.len() > 1
}

/// Extracts album and artist information from track metadata.
///
/// # Arguments
///
/// * `tracks_metadata` - Metadata for tracks in the album.
/// * `is_compilation` - Whether the album is a compilation.
///
/// # Returns
///
/// Tuple of `(album_info, artist_info)` where:
/// - `album_info`: Best available album information
/// - `artist_info`: Best available artist information
fn extract_album_artist_info(
    tracks_metadata: &[(PathBuf, TrackMetadata)],
    is_compilation: bool,
) -> (Option<String>, Option<String>) {
    let mut album_candidates = HashMap::new();
    let mut artist_candidates = HashMap::new();

    for (_, metadata) in tracks_metadata {
        // Count album titles
        if let Some(album) = &metadata.standard.album {
            *album_candidates.entry(album.clone()).or_insert(0) += 1;
        }

        // Count artists
        if let Some(artist) = if is_compilation {
            // For compilations, prefer album artist
            metadata
                .standard
                .album_artist
                .as_ref()
                .or(metadata.standard.artist.as_ref())
        } else {
            // For regular albums, use track artist
            metadata.standard.artist.as_ref()
        } {
            *artist_candidates.entry(artist.clone()).or_insert(0) += 1;
        }
    }

    // Select most common album title
    let album_info = album_candidates
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(album, _)| album);

    // Select most common artist
    let artist_info = artist_candidates
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(artist, _)| artist);

    (album_info, artist_info)
}

/// Updates an album in the database with new track information.
///
/// # Arguments
///
/// * `album_dir` - Path to the album directory.
/// * `album_info` - Album information.
/// * `artist_info` - Artist information.
/// * `tracks_metadata` - Metadata for tracks in the album.
/// * `database` - Database interface.
/// * `is_compilation` - Whether the album is a compilation.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// Returns `LibraryError` if database operations fail.
async fn update_album_in_database(
    album_dir: &Path,
    album_info: &Option<String>,
    artist_info: &Option<String>,
    tracks_metadata: &[(PathBuf, TrackMetadata)],
    database: &LibraryDatabase,
    is_compilation: bool,
) -> Result<(), LibraryError> {
    let pool = database.pool();

    // Extract audio file paths from tracks_metadata
    let audio_files: Vec<PathBuf> = tracks_metadata
        .iter()
        .map(|(path, _)| path.clone())
        .collect();

    // Extract artwork path
    let artwork_path = extract_album_artwork_path(album_dir, &audio_files).await;

    // Begin transaction
    let mut tx = pool.begin().await?;

    // Get or create artist
    let artist_id = get_or_create_artist(&mut tx, artist_info).await?;

    // Get or create album
    let album_id = get_or_create_album(
        &mut tx,
        artist_id,
        album_info,
        tracks_metadata,
        album_dir,
        is_compilation,
        artwork_path.as_deref(),
    )
    .await?;

    // Update tracks
    for (track_path, metadata) in tracks_metadata {
        update_track_in_transaction(&mut tx, album_id, track_path, metadata).await?;
    }

    // Commit transaction
    tx.commit().await?;

    Ok(())
}

/// Gets or creates an artist in the database transaction.
///
/// # Arguments
///
/// * `tx` - Database transaction.
/// * `artist_info` - Artist information.
///
/// # Returns
///
/// The artist ID.
///
/// # Errors
///
/// Returns `LibraryError` if database operations fail.
async fn get_or_create_artist(
    tx: &mut Transaction<'_, Sqlite>,
    artist_info: &Option<String>,
) -> Result<i64, LibraryError> {
    let artist_name = artist_info.as_deref().unwrap_or("Unknown Artist");

    let existing_artist: Option<i64> = query_scalar("SELECT id FROM artists WHERE name = ?")
        .bind(artist_name)
        .fetch_optional(&mut **tx)
        .await?;

    match existing_artist {
        Some(id) => Ok(id),
        None => {
            // Create new artist
            let id: i64 = query_scalar("INSERT INTO artists (name) VALUES (?) RETURNING id")
                .bind(artist_name)
                .fetch_one(&mut **tx)
                .await?;
            Ok(id)
        }
    }
}

/// Gets or creates an album in the database transaction.
///
/// # Arguments
///
/// * `tx` - Database transaction.
/// * `artist_id` - Artist ID.
/// * `album_info` - Album information.
/// * `tracks_metadata` - Track metadata.
/// * `album_dir` - Album directory path.
/// * `is_compilation` - Whether it's a compilation.
/// * `artwork_path` - Optional path to album artwork file.
///
/// # Returns
///
/// The album ID.
///
/// # Errors
///
/// Returns `LibraryError` if database operations fail.
async fn get_or_create_album(
    tx: &mut Transaction<'_, Sqlite>,
    artist_id: i64,
    album_info: &Option<String>,
    tracks_metadata: &[(PathBuf, TrackMetadata)],
    album_dir: &Path,
    is_compilation: bool,
    artwork_path: Option<&str>,
) -> Result<i64, LibraryError> {
    let album_title = album_info.as_deref().unwrap_or("Unknown Album");
    let year = tracks_metadata
        .iter()
        .find_map(|(_, metadata)| metadata.standard.year)
        .map(|y| y as i64);
    let genre = tracks_metadata
        .iter()
        .find_map(|(_, metadata)| metadata.standard.genre.clone());

    // Check if album exists
    let existing_album: Option<i64> =
        query_scalar("SELECT id FROM albums WHERE artist_id = ? AND title = ? AND year IS ?")
            .bind(artist_id)
            .bind(album_title)
            .bind(year)
            .fetch_optional(&mut **tx)
            .await?;

    match existing_album {
        Some(id) => {
            // Update existing album
            query(
                "UPDATE albums SET path = ?, compilation = ?, artwork_path = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"
            )
            .bind(album_dir.to_string_lossy().to_string())
            .bind(is_compilation)
            .bind(artwork_path)
            .bind(id)
            .execute(&mut **tx)
            .await?;
            Ok(id)
        }
        None => {
            // Create new album
            let id: i64 = query_scalar(
                "INSERT INTO albums (artist_id, title, year, genre, compilation, path, artwork_path) VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id"
            )
            .bind(artist_id)
            .bind(album_title)
            .bind(year)
            .bind(genre)
            .bind(is_compilation)
            .bind(album_dir.to_string_lossy().to_string())
            .bind(artwork_path)
            .fetch_one(&mut **tx)
            .await?;
            Ok(id)
        }
    }
}

/// Updates a track in the database transaction.
///
/// # Arguments
///
/// * `tx` - Database transaction.
/// * `album_id` - Album ID.
/// * `track_path` - Track file path.
/// * `metadata` - Track metadata.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// Returns `LibraryError` if database operations fail.
async fn update_track_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    album_id: i64,
    track_path: &Path,
    metadata: &TrackMetadata,
) -> Result<(), LibraryError> {
    let track_title = metadata
        .standard
        .title
        .as_deref()
        .unwrap_or("Unknown Track");
    let track_number = metadata.standard.track_number.map(|n| n as i64);
    let disc_number = metadata.standard.disc_number.unwrap_or(1) as i64;
    let duration_ms = metadata.technical.duration_ms as i64;
    let file_size = metadata.technical.file_size as i64;
    let format = &metadata.technical.format;
    let sample_rate = metadata.technical.sample_rate as i64;
    let bits_per_sample = metadata.technical.bits_per_sample as i64;
    let channels = metadata.technical.channels as i64;

    // Check if track exists
    let existing_track: Option<i64> = query_scalar("SELECT id FROM tracks WHERE path = ?")
        .bind(track_path.to_string_lossy().to_string())
        .fetch_optional(&mut **tx)
        .await?;

    if let Some(track_id) = existing_track {
        // Update existing track
        query(
            "UPDATE tracks SET album_id = ?, title = ?, track_number = ?, disc_number = ?, duration_ms = ?, file_size = ?, format = ?, sample_rate = ?, bits_per_sample = ?, channels = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?"
        )
        .bind(album_id)
        .bind(track_title)
        .bind(track_number)
        .bind(disc_number)
        .bind(duration_ms)
        .bind(file_size)
        .bind(format)
        .bind(sample_rate)
        .bind(bits_per_sample)
        .bind(channels)
        .bind(track_id)
        .execute(&mut **tx)
        .await?;
    } else {
        // Create new track
        query(
            "INSERT INTO tracks (album_id, title, track_number, disc_number, duration_ms, path, file_size, format, sample_rate, bits_per_sample, channels) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(album_id)
        .bind(track_title)
        .bind(track_number)
        .bind(disc_number)
        .bind(duration_ms)
        .bind(track_path.to_string_lossy().to_string())
        .bind(file_size)
        .bind(format)
        .bind(sample_rate)
        .bind(bits_per_sample)
        .bind(channels)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

/// Extracts artwork path for an album directory.
///
/// Attempts to find artwork by checking the first audio file in the album
/// for embedded artwork, then falls back to external artwork files.
///
/// # Arguments
///
/// * `album_dir` - Path to the album directory.
/// * `audio_files` - List of audio files in the album.
///
/// # Returns
///
/// An `Option<String>` containing the artwork file path if found.
async fn extract_album_artwork_path(album_dir: &Path, audio_files: &[PathBuf]) -> Option<String> {
    // Try to extract artwork from the first audio file
    if let Some(first_file) = audio_files.first() {
        match extract_artwork(first_file) {
            Ok(Embedded(data, _mime_type)) => {
                // Save embedded artwork to a file in the album directory
                let artwork_path = album_dir.join("folder.jpg");
                if save_embedded_artwork(&data, &artwork_path).is_ok() {
                    return Some(artwork_path.to_string_lossy().to_string());
                }
            }
            Ok(External(path)) => {
                return Some(path.to_string_lossy().to_string());
            }
            Err(_) => {
                // Continue to external file search
            }
        }
    }

    // Look for external artwork files directly
    if let Ok(Some(external_path)) = find_external_artwork(album_dir) {
        return Some(external_path.to_string_lossy().to_string());
    }

    None
}
