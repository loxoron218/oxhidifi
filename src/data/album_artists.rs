use std::collections::HashSet;

use lofty::tag::{ItemKey::AlbumArtist, Tag};
use lofty::prelude::Accessor;

/// Determines the album artist for a collection of tracks,
/// prioritizing explicit `album_artist` metadata where available.
/// Falls back to `"Various Artists"` if track artists differ,
/// or a single track artist if consistent.
///
/// This is useful for grouping tracks under the correct album display
/// in a music library UI.
///
/// Returns a human-readable album artist string.
pub fn get_album_artist(tracks: &[&Tag]) -> String {
    let mut album_artists = HashSet::new();
    let mut artists = HashSet::new();
    for track in tracks {
        if let Some(album_artist) = track.get_string(&AlbumArtist) {
            album_artists.insert(album_artist.to_string());
        }
        if let Some(artist) = track.artist() {
            artists.insert(artist.to_string());
        }
    }

    // Rule 1: If all tracks have the same non-empty album_artist, use that.
    if album_artists.len() == 1 && !album_artists.iter().next().unwrap().is_empty() {
        return album_artists.into_iter().next().unwrap();
    }

    // Rule 2: If album_artist is missing or inconsistent across tracks, AND the artist field varies, return "Various Artists" as the album artist.
    if artists.len() > 1 {
        return "Various Artists".to_string();
    }

    // Rule 3: Otherwise, return the one unique artist as the fallback album artist.
    artists.into_iter().next().unwrap_or_else(|| "Unknown Artist".to_string())
}