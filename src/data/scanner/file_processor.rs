use std::{
    borrow::Cow, collections::HashSet, error::Error, path::Path, path::PathBuf, time::Instant,
};

use lofty::{
    prelude::{
        Accessor, AudioFile,
        ItemKey::{AlbumArtist, OriginalReleaseDate},
        TaggedFileExt,
    },
    probe::Probe,
    tag::items::Timestamp,
};
use sqlx::SqlitePool;

use crate::{
    data::{
        db::crud::{
            AlbumForInsert, TrackForInsert, insert_or_get_artists_batch, upsert_albums_batch,
            upsert_tracks_batch_enhanced,
        },
        scanner::individual_dr_scanner::scan_individual_dr_values,
    },
    utils::{
        image::cache::thumbnail::process_images_concurrently, performance_monitor::get_metrics,
    },
};

/// A temporary struct to hold metadata extracted from audio files before we have database IDs.
///
/// This struct is used during the file processing phase to store all relevant metadata
/// extracted from audio files. The data is later used to create proper database records
/// with assigned IDs after batch processing.
struct TempMetadata {
    /// The title of the track.
    title: String,
    /// The name of the track artist.
    artist_name: String,
    /// The title of the album the track belongs to.
    album_title: String,
    /// The name of the album artist (may differ from track artist).
    album_artist_name: String,
    /// The file system path to the audio file.
    path: PathBuf,
    /// The duration of the track in seconds.
    duration: u32,
    /// The track number within the album, if available.
    track_no: Option<u32>,
    /// The disc number for multi-disc albums, if available.
    disc_no: Option<u32>,
    /// The release year of the album, if available.
    year: Option<i32>,
    /// The original release date in "YYYY-MM-DD" format, if available.
    original_release_date: Option<String>,
    /// The path to the cached cover art image file, if available.
    cover_art_path: Option<PathBuf>,
    /// The audio format of the file (e.g., "flac", "mp3", "wav").
    format: Option<String>,
    /// The bit depth of the audio, if available.
    bit_depth: Option<u32>,
    /// The sample rate of the audio, if available.
    sample_rate: Option<u32>,
}

/// Processes a batch of audio files, extracts their metadata, and upserts them into the database.
///
/// This function handles the complete pipeline of processing multiple audio files:
/// 1. Extracts metadata from each file
/// 2. Collects all unique artist names
/// 3. Creates or retrieves artist IDs in a batch operation
/// 4. Creates or updates album records with the provided DR value
/// 5. Creates or updates track records with all extracted metadata
///
/// The function uses database transactions to ensure data consistency and efficiency
/// when processing large batches of files.
///
/// # Arguments
///
/// * `pool` - A reference to the SQLite database connection pool
/// * `paths` - A slice of file paths to process
/// * `folder_id` - The ID of the folder containing these files
/// * `dr_value` - An optional DR (Dynamic Range) value to assign to all processed albums
///
/// # Returns
///
/// A `Result` indicating success or an error if processing failed
pub async fn process_files_batch(
    pool: &SqlitePool,
    paths: &[PathBuf],
    folder_id: i64,
    dr_value: Option<u8>,
) -> Result<(), Box<dyn Error>> {
    // Use optimized batch processing with a reasonable batch size
    process_files_batch_optimized(pool, paths, folder_id, dr_value, 100).await
}

/// Optimized version of process_files_batch with configurable batch size
pub async fn process_files_batch_optimized(
    pool: &SqlitePool,
    paths: &[PathBuf],
    folder_id: i64,
    dr_value: Option<u8>,
    batch_size: usize,
) -> Result<(), Box<dyn Error>> {
    let start_time = Instant::now();
    let file_count = paths.len();

    // Extract the folder path from the first file path to scan for individual DR values
    let folder_path = if let Some(first_path) = paths.first() {
        first_path.parent().unwrap_or_else(|| Path::new(""))
    } else {
        Path::new("")
    };

    // Scan for individual DR values in the folder
    let individual_dr_values = scan_individual_dr_values(folder_path).unwrap_or_else(|e| {
        eprintln!("Error scanning individual DR values: {}", e);
        Vec::new()
    });

    // Process files in batches to reduce memory usage
    for chunk in paths.chunks(batch_size) {
        let mut all_metadata = Vec::new();
        let mut image_data_list = Vec::new();
        let mut image_indices = Vec::new();

        // First pass: Extract metadata and collect image data
        for (index, path) in chunk.iter().enumerate() {
            let tagged_file = match Probe::open(path) {
                Ok(probe) => match probe.read() {
                    Ok(tf) => tf,
                    Err(e) => {
                        eprintln!("Error reading audio file {}: {}", path.display(), e);
                        continue;
                    }
                },
                Err(e) => {
                    eprintln!("Error probing audio file {}: {}", path.display(), e);
                    continue;
                }
            };
            let tag = tagged_file.primary_tag();
            let properties = tagged_file.properties();

            // --- Extract metadata with fallbacks ---
            // Title: Use tag title, or fallback to filename if tag is missing.
            let title = tag
                .and_then(|t| t.title())
                .map(Cow::into_owned)
                .unwrap_or_else(|| {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .map(String::from)
                        .unwrap_or_else(|| "Unknown Title".to_string())
                });

            // Artist: Use tag artist, or fallback to "Unknown Artist".
            let artist_name = tag
                .and_then(|t| t.artist())
                .map(Cow::into_owned)
                .unwrap_or_else(|| "Unknown Artist".to_string());
            let album_title = tag
                .and_then(|t| t.album())
                .map(Cow::into_owned)
                .unwrap_or_else(|| "Unknown Album".to_string());

            // Album Artist: Prefer dedicated tag, fallback to track artist name.
            let album_artist_name = tag
                .and_then(|t| t.get_string(&AlbumArtist))
                .map(|s| s.to_string())
                .unwrap_or_else(|| artist_name.clone());

            // Extract cover art data for later concurrent processing
            let cover_art_data = tag.and_then(|t| t.pictures().first()).map(|picture| {
                (
                    picture.data().to_vec(),
                    album_title.clone(),
                    album_artist_name.clone(),
                )
            });

            // Other metadata fields, defaulting to None if not present.
            let year = tag.and_then(|t| t.year()).map(|y| y as i32);
            let track_no = tag.and_then(|t| t.track());
            let disc_no = tag.and_then(|t| t.disk());

            // Parse original release date from string to Timestamp, then back to String for storage.
            let original_release_date = tag.and_then(|t| {
                t.get_string(&OriginalReleaseDate)
                    .and_then(|date_str| date_str.parse::<Timestamp>().ok())
                    .map(|ts| ts.to_string())
            });

            // Extract audio properties.
            let duration = properties.duration().as_secs() as u32;
            let format = path
                .extension()
                .and_then(|s| s.to_str())
                .map(str::to_lowercase);
            let bit_depth = properties.bit_depth().map(|b| b as u32);
            let sample_rate = properties.sample_rate();

            // Store image data for concurrent processing if available
            if let Some(image_data) = cover_art_data {
                image_data_list.push(image_data);
                image_indices.push(index);
            }
            all_metadata.push(TempMetadata {
                title,
                artist_name,
                album_title,
                album_artist_name,
                path: path.to_path_buf(),
                duration,
                track_no,
                disc_no,
                year,
                original_release_date,
                cover_art_path: None,
                format,
                bit_depth,
                sample_rate,
            });
        }
        if all_metadata.is_empty() {
            continue;
        }

        // Process all images concurrently
        let cover_art_paths = process_images_concurrently(image_data_list).await;

        // Update metadata with processed cover art paths
        for (result, &index) in cover_art_paths.into_iter().zip(&image_indices) {
            match result {
                Ok(path) => {
                    if let Some(metadata) = all_metadata.get_mut(index) {
                        metadata.cover_art_path = Some(path);
                    }
                }
                Err(e) => eprintln!("Error processing cover art: {:?}", e),
            }
        }

        // Collect unique artist names
        let mut artist_names = HashSet::new();
        for meta in &all_metadata {
            artist_names.insert(meta.artist_name.clone());
            artist_names.insert(meta.album_artist_name.clone());
        }
        let artist_names: Vec<String> = artist_names.into_iter().collect();

        // Begin a database transaction for batch processing
        let mut tx = pool.begin().await?;

        // Batch insert or retrieve artist IDs
        let artist_ids = insert_or_get_artists_batch(&mut tx, &artist_names).await?;

        // Prepare album records for batch upsert
        let mut albums_to_insert = Vec::new();
        for meta in &all_metadata {
            let album_artist_id = *artist_ids.get(&meta.album_artist_name).unwrap();
            albums_to_insert.push(AlbumForInsert {
                title: meta.album_title.clone(),
                artist_id: album_artist_id,
                folder_id,
                year: meta.year,
                cover_art_path: meta.cover_art_path.clone(),
                dr_value,
                dr_is_best: false,
                original_release_date: meta.original_release_date.clone(),
            });
        }

        // Batch upsert albums and retrieve their IDs
        let album_ids = upsert_albums_batch(&mut tx, &albums_to_insert).await?;

        // Prepare track records for batch upsert
        let mut tracks_to_insert = Vec::new();
        for (index, meta) in all_metadata.iter().enumerate() {
            let artist_id = *artist_ids.get(&meta.artist_name).unwrap();
            let album_artist_id = *artist_ids.get(&meta.album_artist_name).unwrap();
            let album_id = *album_ids
                .get(&(meta.album_title.clone(), album_artist_id, folder_id))
                .unwrap();

            // Get the DR value for this track if available
            let dr_value = if index < individual_dr_values.len() {
                individual_dr_values[index]
            } else {
                None
            };
            tracks_to_insert.push(TrackForInsert {
                title: meta.title.clone(),
                album_id,
                artist_id,
                path: meta.path.clone(),
                duration: Some(meta.duration),
                track_no: meta.track_no,
                disc_no: meta.disc_no,
                format: meta.format.clone(),
                bit_depth: meta.bit_depth,
                sample_rate: meta.sample_rate,
                dr_value,
            });
        }

        // Batch upsert tracks with enhanced batching
        upsert_tracks_batch_enhanced(&mut tx, &tracks_to_insert, batch_size).await?;

        // Commit the transaction
        tx.commit().await?;
    }

    // Record metrics
    let duration = start_time.elapsed();
    get_metrics().record_scan_time(duration);
    for _ in 0..file_count {
        get_metrics().record_file_processed();
    }
    Ok(())
}
