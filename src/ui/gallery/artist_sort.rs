//! Artist album sort orders for playback.
//!
//! Sorts an artist's albums per the user's preferred [`ArtistPlayOrder`],
//! shared by artist-wide playback. Split from `play_action` to keep each
//! module under the file-size limit.

use crate::storage::{
    catalog::Album,
    sort_rules::ArtistPlayOrder::{self, DateAsc, DateDesc, TitleAsc, TitleDesc},
};

/// Sort albums for artist-wide playback according to the user preference.
///
/// `DateAsc` matches the artist detail display order (year, then title, then
/// id); unknown years (`None`) sort first, mirroring SQLite `ORDER BY year`
/// null handling.
///
/// # Arguments
///
/// * `albums` - Albums to sort in place.
/// * `order` - Preferred album ordering from settings.
pub fn sort_albums_for_playback(albums: &mut [Album], order: ArtistPlayOrder) {
    match order {
        DateAsc => albums.sort_by(|a, b| {
            a.year
                .cmp(&b.year)
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| a.id.cmp(&b.id))
        }),
        DateDesc => albums.sort_by(|a, b| {
            b.year
                .cmp(&a.year)
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| a.id.cmp(&b.id))
        }),
        TitleAsc => albums.sort_by(|a, b| {
            a.title
                .cmp(&b.title)
                .then_with(|| a.year.cmp(&b.year))
                .then_with(|| a.id.cmp(&b.id))
        }),
        TitleDesc => albums.sort_by(|a, b| {
            b.title
                .cmp(&a.title)
                .then_with(|| a.year.cmp(&b.year))
                .then_with(|| a.id.cmp(&b.id))
        }),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::{
        storage::{
            catalog::Album,
            sort_rules::ArtistPlayOrder::{DateAsc, DateDesc, TitleAsc, TitleDesc},
        },
        ui::gallery::artist_sort::sort_albums_for_playback,
    };

    fn test_album(id: i64, title: &str, year: Option<i32>) -> Album {
        Album {
            id,
            title: title.to_string(),
            artist_id: 1,
            year,
            genre: None,
            artwork_path: None,
            track_count: 0,
            total_duration: 0.0,
            format_summary: String::new(),
            lossless: true,
            format: String::new(),
            bit_depth: None,
            sample_rate: None,
        }
    }

    fn album_titles(albums: &[Album]) -> Vec<&str> {
        albums.iter().map(|a| a.title.as_str()).collect()
    }

    #[test]
    fn date_asc_matches_display_order_with_unknown_years_first() -> Result<()> {
        let mut albums = vec![
            test_album(3, "Zulu", Some(2005)),
            test_album(1, "Alpha", None),
            test_album(2, "Mike", Some(1999)),
        ];
        sort_albums_for_playback(&mut albums, DateAsc);
        ensure!(
            album_titles(&albums) == ["Alpha", "Mike", "Zulu"],
            "DateAsc must sort None years first, then by year"
        );
        Ok(())
    }

    #[test]
    fn date_desc_keeps_title_tiebreaker() -> Result<()> {
        let mut albums = vec![
            test_album(1, "Zulu", Some(2000)),
            test_album(2, "Alpha", Some(2000)),
            test_album(3, "Mike", Some(2010)),
        ];
        sort_albums_for_playback(&mut albums, DateDesc);
        ensure!(
            album_titles(&albums) == ["Mike", "Alpha", "Zulu"],
            "DateDesc must sort newest first with title tiebreaker"
        );
        Ok(())
    }

    #[test]
    fn title_orders_ignore_year() -> Result<()> {
        let mut albums = vec![
            test_album(1, "Zulu", Some(1990)),
            test_album(2, "Alpha", Some(2020)),
            test_album(3, "Mike", None),
        ];
        sort_albums_for_playback(&mut albums, TitleAsc);
        ensure!(album_titles(&albums) == ["Alpha", "Mike", "Zulu"]);
        sort_albums_for_playback(&mut albums, TitleDesc);
        ensure!(album_titles(&albums) == ["Zulu", "Mike", "Alpha"]);
        Ok(())
    }
}
