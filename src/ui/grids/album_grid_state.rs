use crate::ui::grids::album_grid_state::AlbumGridState::{
    Empty, Loading, NoResults, Populated, Scanning,
};

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

impl AlbumGridState {
    /// Returns the string name associated with each state for use with `gtk4::Stack`.
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
