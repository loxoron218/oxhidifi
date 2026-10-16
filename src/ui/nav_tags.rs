//! Navigation page tags for detail views.
//!
//! Centralizes the per-entity detail tags so drill-down pushes never reuse a
//! live tag (`Duplicate page tag in AdwNavigationView: detail`), split from
//! `navigation` to keep each module under the file-size limit.

/// Detail playback target encoded in a navigation tag.
///
/// Mirrors the per-entity tags built by [`detail_album_tag`] and
/// [`detail_artist_tag`] so `Space` can start playback for the visible
/// detail page without threading widget state through key controllers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTarget {
    /// Album detail page showing this album id.
    Album(i64),
    /// Artist detail page showing this artist id.
    Artist(i64),
}

/// Tag for an album detail page.
///
/// Unique per album so drill-down (artist detail → album detail) pushes a new
/// page instead of colliding with the visible detail tag.
///
/// # Arguments
///
/// * `album_id` - Album shown by the page.
///
/// # Returns
///
/// * `String` - Navigation tag for the album detail page.
#[must_use]
pub fn detail_album_tag(album_id: i64) -> String {
    format!("detail-album-{album_id}")
}

/// Tag for an artist detail page.
///
/// Unique per artist so repeated pushes never reuse a live tag.
///
/// # Arguments
///
/// * `artist_id` - Artist shown by the page.
///
/// # Returns
///
/// * `String` - Navigation tag for the artist detail page.
#[must_use]
pub fn detail_artist_tag(artist_id: i64) -> String {
    format!("detail-artist-{artist_id}")
}

/// Parse a navigation tag into its detail playback target.
///
/// Accepts only the per-entity `detail-album-{id}` and
/// `detail-artist-{id}` forms; legacy `detail`, `library`, and malformed
/// ids yield `None`.
///
/// # Arguments
///
/// * `tag` - Navigation page tag.
///
/// # Returns
///
/// * `Option<DetailTarget>` - Parsed target, or `None` when the tag is not a per-entity detail tag.
#[must_use]
pub fn parse_detail_tag(tag: &str) -> Option<DetailTarget> {
    if let Some(suffix) = tag.strip_prefix("detail-album-") {
        match suffix.parse() {
            Ok(id) => return Some(DetailTarget::Album(id)),
            Err(_) => return None,
        }
    }
    if let Some(suffix) = tag.strip_prefix("detail-artist-") {
        match suffix.parse() {
            Ok(id) => return Some(DetailTarget::Artist(id)),
            Err(_) => return None,
        }
    }
    None
}

/// Whether a navigation tag belongs to a detail page.
///
/// # Arguments
///
/// * `tag` - Navigation page tag.
///
/// # Returns
///
/// `true` for `detail`, `detail-album-*`, and `detail-artist-*` tags.
#[must_use]
pub fn is_detail_tag(tag: &str) -> bool {
    tag == "detail" || tag.starts_with("detail-")
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::ui::nav_tags::{
        DetailTarget::{Album, Artist},
        detail_album_tag, detail_artist_tag, is_detail_tag, parse_detail_tag,
    };

    #[test]
    fn detail_tags_are_unique_per_entity() -> Result<()> {
        ensure!(
            detail_album_tag(4) == "detail-album-4",
            "album tag must encode the album id"
        );
        ensure!(
            detail_artist_tag(3) == "detail-artist-3",
            "artist tag must encode the artist id"
        );
        ensure!(
            detail_album_tag(4) != detail_artist_tag(4),
            "album and artist tags must not collide"
        );
        ensure!(is_detail_tag("detail"), "legacy tag must still count");
        ensure!(
            is_detail_tag(&detail_album_tag(4)),
            "album tag must count as detail"
        );
        ensure!(!is_detail_tag("library"), "library must not count");
        Ok(())
    }

    #[test]
    fn parse_detail_tag_round_trips_per_entity_tags() -> Result<()> {
        ensure!(
            parse_detail_tag(&detail_album_tag(4)) == Some(Album(4)),
            "album tag must parse to its album id"
        );
        ensure!(
            parse_detail_tag(&detail_artist_tag(3)) == Some(Artist(3)),
            "artist tag must parse to its artist id"
        );
        Ok(())
    }

    #[test]
    fn parse_detail_tag_rejects_non_entity_tags() -> Result<()> {
        for tag in ["library", "detail", "detail-album-", "detail-artist-x"] {
            ensure!(
                parse_detail_tag(tag).is_none(),
                "non-entity tag must not parse"
            );
        }
        Ok(())
    }
}
