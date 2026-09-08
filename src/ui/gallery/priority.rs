//! Album sort indices and priority-ordered comparison.

use std::{
    cmp::Ordering::{self, Equal},
    collections::HashMap,
    hash::BuildHasher,
    sync::{Arc, atomic::Ordering::Relaxed},
};

use crate::{
    app::runtime::{AppState, CachedAlbumData},
    storage::{
        catalog::Album,
        sort_rules::{
            AlbumSortCriteria::{Artist, BitDepth, Format, SampleRate, Title, Year},
            AlbumSortItem,
            SortOrder::Descending,
        },
    },
    ui::gallery::order_memo::memoized_sort_indices,
};

/// Compare two albums by a single sort item, applying the configured order.
fn cmp_albums<S: BuildHasher>(
    a: &Album,
    b: &Album,
    item: AlbumSortItem,
    artist_names: &HashMap<i64, String, S>,
) -> Ordering {
    let mut cmp = match item.criteria {
        Title => a.title.cmp(&b.title),
        Artist => {
            let a_name = artist_names.get(&a.artist_id).map_or("", |s| s.as_str());
            let b_name = artist_names.get(&b.artist_id).map_or("", |s| s.as_str());
            a_name.cmp(b_name)
        }
        Year => a.year.cmp(&b.year),
        Format => a.format.cmp(&b.format),
        BitDepth => a.bit_depth.cmp(&b.bit_depth),
        SampleRate => a.sample_rate.cmp(&b.sample_rate),
    };
    if item.order == Descending {
        cmp = cmp.reverse();
    }
    cmp
}

/// Compute a display order for `albums` as a sorted index vector.
///
/// Single‑pass `sort_unstable_by` over indices — each criteria is checked
/// in priority order until a non‑equal comparison is found. Borrows the
/// album data so rebuilds never deep‑clone the `Vec<Album>`.
pub fn sorted_album_indices<S: BuildHasher>(
    albums: &[Album],
    artist_names: &HashMap<i64, String, S>,
    sort_items: &[AlbumSortItem],
) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..albums.len()).collect();
    indices.sort_unstable_by(|&a, &b| {
        let Some(a_album) = albums.get(a) else {
            return Equal;
        };
        let Some(b_album) = albums.get(b) else {
            return Equal;
        };
        sort_items
            .iter()
            .find_map(|item| {
                let cmp = cmp_albums(a_album, b_album, *item, artist_names);
                (cmp != Equal).then_some(cmp)
            })
            .unwrap_or(Equal)
    });
    indices
}

/// Memoized display order for the cached album data.
///
/// Returns the sort indices from the grid's memo when the library generation
/// and the current sort configuration both match, so repeated tab/mode
/// switches reuse the sort instead of re-comparing every album. Computes,
/// caches, and returns them otherwise.
pub fn album_sort_indices(state: &Arc<AppState>, cached: &CachedAlbumData) -> Arc<[usize]> {
    let generation = state.album_grid.generation.load(Relaxed);
    let config = state.storage.get_albums_sort();
    memoized_sort_indices(generation, config, &state.album_grid.memo, |config| {
        sorted_album_indices(&cached.albums, &cached.artist_names, config)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        storage::{
            catalog::Album,
            sort_rules::{
                AlbumSortCriteria::{Artist, Title},
                AlbumSortItem,
                SortOrder::{Ascending, Descending},
            },
        },
        ui::gallery::priority::sorted_album_indices,
    };

    fn mock_album(id: i64, title: &str, artist_id: i64) -> Album {
        Album {
            id,
            title: title.into(),
            artist_id,
            year: Some(2020),
            genre: None,
            artwork_path: None,
            track_count: 10,
            total_duration: 300.0,
            format_summary: String::new(),
            lossless: true,
            format: "FLAC".into(),
            bit_depth: Some(24),
            sample_rate: Some(96000),
        }
    }

    #[test]
    fn sorted_album_indices_orders_by_priority_items() {
        let albums = vec![
            mock_album(1, "Beta", 10),
            mock_album(2, "Alpha", 10),
            mock_album(3, "Gamma", 20),
        ];
        let mut artist_names = HashMap::new();
        assert!(
            artist_names.insert(10, "Zed".to_string()).is_none(),
            "artist map starts empty"
        );
        assert!(
            artist_names.insert(20, "Adam".to_string()).is_none(),
            "artist map starts empty"
        );

        let items = vec![
            AlbumSortItem {
                criteria: Artist,
                order: Ascending,
            },
            AlbumSortItem {
                criteria: Title,
                order: Descending,
            },
        ];

        let indices = sorted_album_indices(&albums, &artist_names, &items);
        assert_eq!(indices, vec![2, 0, 1]);
    }

    #[test]
    fn sorted_album_indices_defaults_to_ascending_title() {
        let albums = vec![
            mock_album(1, "Beta", 10),
            mock_album(2, "Alpha", 10),
            mock_album(3, "Gamma", 10),
        ];
        let artist_names = HashMap::new();
        let items = vec![AlbumSortItem {
            criteria: Title,
            order: Descending,
        }];

        let indices = sorted_album_indices(&albums, &artist_names, &items);
        assert_eq!(indices, vec![2, 0, 1]);
    }
}
