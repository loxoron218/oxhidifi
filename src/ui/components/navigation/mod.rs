pub mod core;
pub mod shortcuts;
pub mod tabs;

/// Constants for `ViewStack` child names, improving readability and reducing magic strings.
/// These constants are used to identify different UI pages or header states within the application's
/// main `ViewStack` and header `ViewStack`. Using constants helps prevent typos and makes the code
/// more maintainable when navigating between different views.
///
/// - `VIEW_STACK_ALBUMS`: Represents the main albums grid view.
/// - `VIEW_STACK_ARTISTS`: Represents the main artists grid view.
/// - `VIEW_STACK_ALBUM_DETAIL`: Represents the detailed view for a specific album.
/// - `VIEW_STACK_ARTIST_DETAIL`: Represents the detailed view for a specific artist.
/// - `VIEW_STACK_MAIN_HEADER`: Represents the default header state with main navigation buttons (e.g., tabs, search).
/// - `VIEW_STACK_BACK_HEADER`: Represents the header state with a back button, typically shown on detail pages.
pub const VIEW_STACK_ALBUMS: &str = "albums";
pub const VIEW_STACK_ARTISTS: &str = "artists";
pub const VIEW_STACK_ALBUM_DETAIL: &str = "album_detail";
pub const VIEW_STACK_ARTIST_DETAIL: &str = "artist_detail";
pub const VIEW_STACK_MAIN_HEADER: &str = "main";
pub const VIEW_STACK_BACK_HEADER: &str = "back";
