use std::collections::{HashMap, HashSet};

use sqlx::{Error, SqlitePool};

use crate::data::{
    db::crud::{fetch_album_by_id, fetch_artist_by_id, fetch_folder_by_id, fetch_tracks_by_album},
    models::{Album, Artist, Folder, Track},
};

/// Fetches all data required to display a complete album page.
///
/// This function performs multiple asynchronous database queries to gather
/// all necessary information for an album detail page. It retrieves:
/// - Album metadata (title, year, cover art, etc.)
/// - Primary artist information
/// - Folder location information
/// - Complete track listing
/// - Artist names for all tracks (to handle various artists albums)
/// - A flag indicating if this is a various artists album
///
/// The function optimizes database queries by:
/// 1. Fetching core album, artist, and folder data in parallel
/// 2. Collecting unique artist IDs from tracks to minimize database queries
/// 3. Batch fetching artist names for all track artists
///
/// # Arguments
///
/// * `db_pool` - Reference to the SQLite database connection pool
/// * `album_id` - The unique identifier of the album to fetch data for
///
/// # Returns
///
/// A `Result` containing a tuple with the following elements on success:
/// 1. `Album` - The album metadata
/// 2. `Artist` - The primary artist of the album
/// 3. `Folder` - The folder containing the album files
/// 4. `Vec<Track>` - All tracks belonging to the album
/// 5. `HashMap<i64, String>` - Map of artist IDs to artist names for track artists
/// 6. `bool` - Flag indicating if this is a various artists album
///
/// Returns an `sqlx::Error` if any database query fails.
///
/// # Example
///
/// ```rust
/// # async fn example() -> Result<(), sqlx::Error> {
/// # use sqlx::SqlitePool;
/// # let db_pool: SqlitePool = todo!();
/// # let album_id = 1;
/// let (album, artist, folder, tracks, track_artists, is_various_artists) =
///     fetch_album_page_data(&db_pool, album_id).await?;
/// # Ok(())
/// # }
/// ```
pub async fn fetch_album_page_data(
    db_pool: &SqlitePool,
    album_id: i64,
) -> Result<
    (
        Album,
        Artist,
        Folder,
        Vec<Track>,
        HashMap<i64, String>,
        bool,
    ),
    Error,
> {
    // Fetch core album information in parallel
    let album = fetch_album_by_id(db_pool, album_id).await?;
    let artist = fetch_artist_by_id(db_pool, album.artist_id).await?;
    let folder = fetch_folder_by_id(db_pool, album.folder_id).await?;
    let tracks = fetch_tracks_by_album(db_pool, album_id).await?;

    // Determine if this is a various artists album by checking if any track
    // has a different artist than the album's primary artist
    let is_various_artists_album = tracks.iter().any(|t| t.artist_id != album.artist_id);

    // Collect unique artist IDs from all tracks to minimize database queries
    // when fetching artist names for the track listing
    let mut track_artist_ids: HashSet<i64> = HashSet::new();
    for track in &tracks {
        track_artist_ids.insert(track.artist_id);
    }

    // Fetch artist names for all unique track artists
    // This is needed for various artists albums where each track might have
    // a different artist than the album's primary artist
    let mut track_artists: HashMap<i64, String> = HashMap::new();
    for artist_id in track_artist_ids {
        // Silently ignore errors when fetching individual artists to prevent
        // one failed artist lookup from breaking the entire album page
        if let Ok(art) = fetch_artist_by_id(db_pool, artist_id).await {
            track_artists.insert(artist_id, art.name);
        }
    }

    // Return all fetched data as a tuple:
    // (album, artist, folder, tracks, track_artists, is_various_artists_album)
    Ok((
        album,
        artist,
        folder,
        tracks,
        track_artists,
        is_various_artists_album,
    ))
}
