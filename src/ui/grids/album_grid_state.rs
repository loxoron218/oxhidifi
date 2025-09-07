use std::path::PathBuf;

use crate::ui::grids::album_grid_state::AlbumGridState::{
    Empty, Loading, NoResults, Populated, Scanning,
};

/// Represents an item in the album grid display.
///
/// This struct contains all the information needed to display an album
/// in the grid view of the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumGridItem {
    /// The unique identifier for the album
    pub id: i64,
    /// The title of the album
    pub title: String,
    /// The artist who created the album
    pub artist: String,
    /// Optional path to the album's cover art image
    pub cover_art: Option<String>,
    /// The release year of the album, if available
    pub year: Option<i32>,
    /// The original release date of the album as a string, if available
    pub original_release_date: Option<String>,
    /// The DR (Dynamic Range) value of the album, if available
    pub dr_value: Option<i32>,
    /// Indicates whether the DR (Dynamic Range) analysis for this album is the best
    pub dr_is_best: bool,
    /// The audio format of the album files (e.g., "FLAC", "MP3"), if available
    pub format: Option<String>,
    /// The bit depth of the audio files, if available
    pub bit_depth: Option<i32>,
    /// The sample rate of the audio files in Hz, if available
    pub sample_rate: Option<i32>,
    /// The file system path to the album's folder
    pub folder_path: PathBuf,
}

/// Represents the various states of the album grid display.
///
/// This enum simplifies managing the visibility of different UI sections
/// based on the current state of the album library (e.g., loading, empty, populated).
pub enum AlbumGridState {
    Loading,
    Empty,
    NoResults,
    Scanning,
    Populated,
}

/// Returns the string name associated with each state for use with `gtk4::Stack`.
impl AlbumGridState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Loading => "loading_state",
            Empty => "empty_state",
            NoResults => "no_results_state",
            Scanning => "scanning_state",
            Populated => "populated_grid",
        }
    }
}
