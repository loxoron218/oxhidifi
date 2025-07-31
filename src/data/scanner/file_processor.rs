use std::{borrow::Cow, error::Error, path::Path};

use lofty::{
    probe::Probe,
    tag::items::Timestamp,
    prelude::{Accessor, AudioFile, TaggedFileExt},
    prelude::ItemKey::{AlbumArtist, OriginalReleaseDate}};
use sqlx::SqlitePool;

use crate::data::db::db_crud::{insert_or_get_album, insert_or_get_artist, insert_track};

/// Processes a single audio file by extracting its metadata (tags and properties)
/// and inserting or updating the corresponding entries in the database.
///
/// This function handles potential missing metadata by providing sensible fallbacks
/// (e.g., using filename as title, "Unknown Artist/Album").
/// It also associates the track with its folder and a discovered DR value if available.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `path` - The file system path to the audio file.
/// * `folder_id` - The database ID of the folder containing this file.
/// * `dr_value` - An `Option<u8>` representing the Dynamic Range (DR) value for the folder, if found.
///
/// # Returns
/// A `Result` indicating success or an `Box<dyn Error>` on failure.
/// If `lofty` fails to probe or read the file, it returns `Ok(())` (skips the file)
/// to allow the scanning process to continue for other files.
pub async fn process_file(
    pool: &SqlitePool,
    path: &Path,
    folder_id: i64,
    dr_value: Option<u8>,
) -> Result<(), Box<dyn Error>> {

    // Probe the file to read its metadata. If probing or reading fails, skip this file.
    let tagged_file = match Probe::open(path) {
        Ok(probe) => match probe.read() {
            Ok(tf) => tf,
            Err(e) => {
                eprintln!("Error reading audio file {}: {}", path.display(), e);
                return Ok(()); // Skip this file, but don't stop the overall scan.
            }
        },
        Err(e) => {
            eprintln!("Error probing audio file {}: {}", path.display(), e);
            return Ok(()); // Skip this file, but don't stop the overall scan.
        }
    };
    let tag = tagged_file.primary_tag(); // Get the primary tag (e.g., ID3v2, Vorbis Comments).
    let properties = tagged_file.properties(); // Get audio properties (duration, sample rate, etc.).

    // --- Extract metadata with fallbacks ---
    // Title: Use tag title, or fallback to filename if tag is missing.
    let title = tag
        .and_then(|t| t.title())
        .map(Cow::into_owned)
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(String::from)
                .unwrap_or_else(|| "Unknown Title".to_string()) // Default if filename is also problematic.
        });

    // Artist: Use tag artist, or fallback to "Unknown Artist".
    let artist_name = tag
        .and_then(|t| t.artist())
        .map(Cow::into_owned)
        .unwrap_or_else(|| "Unknown Artist".to_string());

    // Album: Use tag album, or fallback to "Unknown Album".
    let album_title = tag
        .and_then(|t| t.album())
        .map(Cow::into_owned)
        .unwrap_or_else(|| "Unknown Album".to_string());

    // Album Artist: Prefer dedicated tag, fallback to track artist name.
    let album_artist_name = tag
        .and_then(|t| t.get_string(&AlbumArtist))
        .map(|s| s.to_string())
        .unwrap_or_else(|| artist_name.clone());

    // Other metadata fields, defaulting to None if not present.
    let year = tag.and_then(|t| t.year()).map(|y| y as i32);
    let track_no = tag.and_then(|t| t.track());
    let disc_no = tag.and_then(|t| t.disk());

    // Extract cover art as Vec<u8> from the first picture found in the tag.
    let cover_art = tag.and_then(|t| t.pictures().first().map(|pic| pic.data().to_vec()));

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
    let frequency = properties.sample_rate();

    // --- Database Operations ---
    // Insert or retrieve artists, getting their database IDs.
    let artist_id = insert_or_get_artist(pool, &artist_name).await?;
    let album_artist_id = insert_or_get_artist(pool, &album_artist_name).await?;

    // Insert or retrieve the album, linking it to the album artist and folder.
    // The DR value found in the folder scan is passed here.
    let album_id = insert_or_get_album(
        pool,
        &album_title,
        album_artist_id,
        year,
        cover_art,
        folder_id,
        dr_value,
        original_release_date,
    )
    .await?;

    // Insert or update the track, linking it to the album and track artist.
    insert_track(
        pool,
        &title,
        album_id,
        artist_id,
        path.to_string_lossy().as_ref(), // Convert Path to &str.
        duration,
        track_no,
        disc_no,
        format,
        bit_depth,
        frequency,
    )
    .await?;
    Ok(())
}