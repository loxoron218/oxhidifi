//! Incremental update handlers for file system events.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use {
    parking_lot::RwLock,
    sqlx::{Sqlite, Transaction, query, query_scalar},
    tracing::warn,
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
    library::{
        database::LibraryDatabase, dr_parser::DrParser, file_watcher::FileWatcher,
        incremental_updater::config::IncrementalUpdaterConfig,
    },
};

/// Handles files that have been created or modified incrementally.
///
/// # Arguments
///
/// * `paths` - Paths of changed files.
/// * `database` - Database interface.
/// * `dr_parser` - Optional DR parser.
/// * `settings` - User settings.
/// * `config` - Configuration.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// Returns `LibraryError` if processing fails.
pub async fn handle_files_changed_incremental(
    paths: Vec<PathBuf>,
    database: &LibraryDatabase,
    dr_parser: Option<&Arc<DrParser>>,
    settings: &RwLock<UserSettings>,
    config: &IncrementalUpdaterConfig,
) -> Result<(), LibraryError> {
    // Process files in batches
    for batch in paths.chunks(config.max_batch_size) {
        process_file_batch(batch, database, dr_parser, settings).await?;
    }

    Ok(())
}

/// Processes a batch of files.
///
/// # Arguments
///
/// * `batch` - Batch of file paths to process.
/// * `database` - Database interface.
/// * `dr_parser` - Optional DR parser.
/// * `settings` - User settings.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// Returns `LibraryError` if processing fails.
pub async fn process_file_batch(
    batch: &[PathBuf],
    database: &LibraryDatabase,
    dr_parser: Option<&Arc<DrParser>>,
    _settings: &RwLock<UserSettings>,
) -> Result<(), LibraryError> {
    let pool = database.pool();
    let mut tx = pool.begin().await?;

    // Group files by album directory
    let mut files_by_album: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for path in batch {
        if let Some(parent) = path.parent() {
            files_by_album
                .entry(parent.to_path_buf())
                .or_default()
                .push(path.clone());
        }
    }

    // Process each album
    for (album_dir, album_files) in files_by_album {
        // Extract metadata for all files
        let mut tracks_metadata = Vec::new();
        for file_path in &album_files {
            match TagReader::read_metadata(file_path) {
                Ok(metadata) => {
                    tracks_metadata.push((file_path.clone(), metadata));
                }
                Err(e) => {
                    warn!("Failed to read metadata for {:?}: {}", file_path, e);
                }
            }
        }

        if tracks_metadata.is_empty() {
            continue;
        }

        // Determine if compilation
        let is_compilation = is_compilation_album(&tracks_metadata);

        // Extract album/artist info
        let (album_info, artist_info) = extract_album_artist_info(&tracks_metadata, is_compilation);

        // Extract audio file paths from tracks_metadata
        let audio_files: Vec<PathBuf> = tracks_metadata
            .iter()
            .map(|(path, _)| path.clone())
            .collect();

        // Extract artwork path
        let artwork_path = extract_album_artwork_path(&album_dir, &audio_files);

        // Get or create artist
        let artist_id = get_or_create_artist(&mut tx, artist_info.as_ref()).await?;

        // Get or create album
        let album_id = get_or_create_album(
            &mut tx,
            artist_id,
            album_info.as_ref(),
            &tracks_metadata,
            &album_dir,
            is_compilation,
            artwork_path.as_deref(),
        )
        .await?;

        // Update tracks
        for (track_path, metadata) in &tracks_metadata {
            update_track_in_transaction(&mut tx, album_id, track_path, metadata).await?;
        }

        // Parse and update DR value if enabled
        if let Some(parser) = dr_parser
            && let Ok(Some(dr_value)) = parser.parse_dr_for_album(&album_dir).await
        {
            query("UPDATE albums SET dr_value = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(&dr_value)
                .bind(album_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

/// Handles files that have been removed incrementally.
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
pub async fn handle_files_removed_incremental(
    paths: Vec<PathBuf>,
    database: &LibraryDatabase,
) -> Result<(), LibraryError> {
    let mut individual_files = Vec::new();
    let mut directories = Vec::new();

    // Process all removal paths (including directories and unsupported files)
    // since directory deletions may affect the library
    for path in paths {
        let path_str = path.to_string_lossy().to_string();

        // Determine if this path represents a directory or file
        let is_directory = {
            let pool = database.pool();

            // First, check if this exact path exists as a track (file)
            let is_file = query_scalar::<_, Option<i64>>("SELECT 1 FROM tracks WHERE path = ?")
                .bind(&path_str)
                .fetch_optional(pool)
                .await
                .unwrap_or(None)
                .is_some();

            if is_file {
                false // It's definitely a file
            } else {
                // Check if there are any tracks under this path (directory)
                let pattern = format!("{path_str}/%");

                query_scalar::<_, i64>("SELECT COUNT(*) FROM tracks WHERE path LIKE ?")
                    .bind(&pattern)
                    .fetch_one(pool)
                    .await
                    .unwrap_or(0)
                    > 0 // If tracks exist under this path, it's a directory
            }
        };

        if is_directory {
            directories.push(path);
        } else {
            // Only add to individual files if it's an audio file
            if FileWatcher::is_supported_audio_file(&path) {
                individual_files.push(path_str);
            }

            // Note: Non-audio files are ignored for individual file processing
            // but directories containing them will still be handled above
        }
    }

    // Handle directory deletions
    for dir_path in directories {
        database.remove_tracks_in_directory(&dir_path).await?;
    }

    // Handle individual file deletions
    if !individual_files.is_empty() {
        database.batch_remove_tracks(individual_files).await?;
    }

    Ok(())
}

/// Handles files that have been renamed/moved incrementally.
///
/// # Arguments
///
/// * `paths` - Original and new paths of renamed files.
/// * `database` - Database interface.
/// * `dr_parser` - Optional DR parser.
/// * `settings` - User settings.
/// * `config` - Configuration.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// Returns `LibraryError` if processing fails.
pub async fn handle_files_renamed_incremental(
    paths: Vec<(PathBuf, PathBuf)>,
    database: &LibraryDatabase,
    dr_parser: Option<&Arc<DrParser>>,
    settings: &RwLock<UserSettings>,
    config: &IncrementalUpdaterConfig,
) -> Result<(), LibraryError> {
    // Handle as remove + add
    let removed_paths: Vec<PathBuf> = paths.iter().map(|(from, _)| from.clone()).collect();
    let added_paths: Vec<PathBuf> = paths.iter().map(|(_, to)| to.clone()).collect();

    handle_files_removed_incremental(removed_paths, database).await?;
    handle_files_changed_incremental(added_paths, database, dr_parser, settings, config).await?;

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
/// Tuple of `(album_info, artist_info)`.
fn extract_album_artist_info(
    tracks_metadata: &[(PathBuf, TrackMetadata)],
    is_compilation: bool,
) -> (Option<String>, Option<String>) {
    let mut album_candidates = HashMap::new();
    let mut artist_candidates = HashMap::new();

    for (_, metadata) in tracks_metadata {
        if let Some(album) = &metadata.standard.album {
            *album_candidates.entry(album.clone()).or_insert(0) += 1;
        }

        if let Some(artist) = if is_compilation {
            metadata
                .standard
                .album_artist
                .as_ref()
                .or(metadata.standard.artist.as_ref())
        } else {
            metadata.standard.artist.as_ref()
        } {
            *artist_candidates.entry(artist.clone()).or_insert(0) += 1;
        }
    }

    let album_info = album_candidates
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(album, _)| album);

    let artist_info = artist_candidates
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(artist, _)| artist);

    (album_info, artist_info)
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
    artist_info: Option<&String>,
) -> Result<i64, LibraryError> {
    let artist_name = artist_info.map_or("Unknown Artist", |v| v);

    let existing_artist: Option<i64> = query_scalar("SELECT id FROM artists WHERE name = ?")
        .bind(artist_name)
        .fetch_optional(&mut **tx)
        .await?;

    if let Some(id) = existing_artist {
        Ok(id)
    } else {
        let id: i64 = query_scalar("INSERT INTO artists (name) VALUES (?) RETURNING id")
            .bind(artist_name)
            .fetch_one(&mut **tx)
            .await?;
        Ok(id)
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
    album_info: Option<&String>,
    tracks_metadata: &[(PathBuf, TrackMetadata)],
    album_dir: &Path,
    is_compilation: bool,
    artwork_path: Option<&str>,
) -> Result<i64, LibraryError> {
    let album_title = album_info.map_or("Unknown Album", |v| v);
    let year = tracks_metadata
        .iter()
        .find_map(|(_, metadata)| metadata.standard.year)
        .map(i64::from);
    let genre = tracks_metadata
        .iter()
        .find_map(|(_, metadata)| metadata.standard.genre.clone());

    let existing_album: Option<i64> =
        query_scalar("SELECT id FROM albums WHERE artist_id = ? AND title = ? AND year IS ?")
            .bind(artist_id)
            .bind(album_title)
            .bind(year)
            .fetch_optional(&mut **tx)
            .await?;

    if let Some(id) = existing_album {
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
    } else {
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
fn extract_album_artwork_path(album_dir: &Path, audio_files: &[PathBuf]) -> Option<String> {
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
    let track_number = metadata.standard.track_number.map(i64::from);
    let disc_number = i64::from(metadata.standard.disc_number.unwrap_or(1));
    let duration_ms = metadata.technical.duration_ms as i64;
    let file_size = metadata.technical.file_size as i64;
    let format = &metadata.technical.format;
    let sample_rate = i64::from(metadata.technical.sample_rate);
    let bits_per_sample = i64::from(metadata.technical.bits_per_sample);
    let channels = i64::from(metadata.technical.channels);

    let existing_track: Option<i64> = query_scalar("SELECT id FROM tracks WHERE path = ?")
        .bind(track_path.to_string_lossy().to_string())
        .fetch_optional(&mut **tx)
        .await?;

    if let Some(track_id) = existing_track {
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
