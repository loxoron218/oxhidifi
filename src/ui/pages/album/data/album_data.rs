use std::collections::{HashMap, HashSet};

use sqlx::{Error, SqlitePool};

use crate::data::{
    db::crud::{fetch_album_by_id, fetch_artist_by_id, fetch_folder_by_id, fetch_songs_by_album},
    models::{Album, Artist, Folder, Song},
};

/// Fetches all data required to display a complete album page.
///
/// This function performs multiple asynchronous database queries to gather
/// all necessary information for an album detail page. It retrieves:
/// - Album metadata (title, year, cover art, etc.)
/// - Primary artist information
/// - Folder location information
/// - Complete song listing
/// - Artist names for all songs (to handle various artists albums)
/// - A flag indicating if this is a various artists album
///
/// The function optimizes database queries by:
/// 1. Fetching core album, artist, and folder data in parallel
/// 2. Collecting unique artist IDs from songs to minimize database queries
/// 3. Batch fetching artist names for all song artists
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
/// 4. `Vec<Song>` - All songs belonging to the album
/// 5. `HashMap<i64, String>` - Map of artist IDs to artist names for song artists
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
/// let (album, artist, folder, songs, song_artists, is_various_artists) =
///     fetch_album_page_data(&db_pool, album_id).await?;
/// # Ok(())
/// # }
/// ```
pub async fn fetch_album_page_data(
    db_pool: &SqlitePool,
    album_id: i64,
) -> Result<(Album, Artist, Folder, Vec<Song>, HashMap<i64, String>, bool), Error> {
    // Fetch core album information in parallel
    let album = fetch_album_by_id(db_pool, album_id).await?;
    let artist = fetch_artist_by_id(db_pool, album.artist_id).await?;
    let folder = fetch_folder_by_id(db_pool, album.folder_id).await?;
    let songs = fetch_songs_by_album(db_pool, album_id).await?;

    // Determine if this is a various artists album by checking if any song
    // has a different artist than the album's primary artist
    let is_various_artists_album = songs.iter().any(|t| t.artist_id != album.artist_id);

    // Collect unique artist IDs from all songs to minimize database queries
    // when fetching artist names for the song listing
    let mut song_artist_ids: HashSet<i64> = HashSet::new();
    for song in &songs {
        song_artist_ids.insert(song.artist_id);
    }

    // Fetch artist names for all unique song artists
    // This is needed for various artists albums where each song might have
    // a different artist than the album's primary artist
    let mut song_artists: HashMap<i64, String> = HashMap::new();
    for artist_id in song_artist_ids {
        // Silently ignore errors when fetching individual artists to prevent
        // one failed artist lookup from breaking the entire album page
        if let Ok(art) = fetch_artist_by_id(db_pool, artist_id).await {
            song_artists.insert(artist_id, art.name);
        }
    }

    // Return all fetched data as a tuple:
    // (album, artist, folder, songs, song_artists, is_various_artists_album)
    Ok((
        album,
        artist,
        folder,
        songs,
        song_artists,
        is_various_artists_album,
    ))
}
