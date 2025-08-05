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
            AlbumGridState::Loading => "loading_state",
            AlbumGridState::Empty => "empty_state",
            AlbumGridState::NoResults => "no_results_state",
            AlbumGridState::Scanning => "scanning_state",
            AlbumGridState::Populated => "populated_grid",
        }
    }
}
