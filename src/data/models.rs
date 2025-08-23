use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Represents a music folder in the file system.
///
/// This struct is used to store information about directories that contain
/// music files, primarily their unique ID and absolute path.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Folder {
    /// Unique identifier for the folder.
    pub id: i64,
    /// The absolute file system path of the folder.
    pub path: PathBuf,
}

/// Represents a music artist.
///
/// This struct stores basic information about an artist, identified by a unique ID and name.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artist {
    /// Unique identifier for the artist.
    pub id: i64,
    /// The name of the artist.
    pub name: String,
}

/// Represents a music album.
///
/// This struct holds comprehensive metadata for an album, linking it to an artist,
/// a folder, and including details like release year, cover art, and Dynamic Range (DR) value.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Album {
    /// Unique identifier for the album.
    pub id: i64,
    /// The title of the album.
    pub title: String,
    /// The ID of the primary artist associated with the album.
    pub artist_id: i64,
    /// The release year of the album, if available.
    pub year: Option<i32>,
    /// The original release date of the album, typically in "YYYY-MM-DD" format.
    pub original_release_date: Option<String>,
    /// The path to the album's cached cover art image file.
    pub cover_art: Option<PathBuf>,
    /// The ID of the folder where the album's files are located.
    pub folder_id: i64,
    /// The Dynamic Range (DR) value of the album, if calculated or available.
    pub dr_value: Option<u8>,
    /// A boolean flag indicating whether the DR value for this album has been
    /// manually marked as completed or verified by the user.
    pub dr_completed: bool,
}

/// Represents a single music track.
///
/// This struct contains detailed metadata for an individual track, including its
/// associated album and artist, file path, duration, and audio technical specifications.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    /// Unique identifier for the track.
    pub id: i64,
    /// The title of the track.
    pub title: String,
    /// The ID of the album to which this track belongs.
    pub album_id: i64,
    /// The ID of the primary artist for this track.
    pub artist_id: i64,
    /// The absolute file system path of the track file.
    pub path: PathBuf,
    /// The duration of the track in seconds.
    pub duration: Option<u32>,
    /// The track number within its album (e.g., 1, 2, 3...).
    pub track_no: Option<u32>,
    /// The disc number if the album spans multiple discs.
    pub disc_no: Option<u32>,
    /// The audio format of the track (e.g., "FLAC", "MP3", "WAV").
    pub format: Option<String>,
    /// The bit depth of the audio (e.g., 16, 24).
    pub bit_depth: Option<u32>,
    /// The sample rate frequency of the audio (e.g., 44100, 96000).
    pub frequency: Option<u32>,
}
