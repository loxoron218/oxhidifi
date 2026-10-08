//! Navigation page tags for detail views.
//!
//! Centralizes the per-entity detail tags so drill-down pushes never reuse a
//! live tag (`Duplicate page tag in AdwNavigationView: detail`), split from
//! `navigation` to keep each module under the file-size limit.

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

    use crate::ui::nav_tags::{detail_album_tag, detail_artist_tag, is_detail_tag};

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
}
