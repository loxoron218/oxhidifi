//! Artist sort indices and precedence-ordered comparison.

use std::{
    cmp::Ordering::{self, Equal},
    sync::{Arc, atomic::Ordering::Relaxed},
};

use crate::{
    app::runtime::{AppState, CachedArtistData},
    storage::{
        catalog::Artist,
        sort_rules::{
            ArtistSortCriteria::{AlbumCount, Name},
            ArtistSortItem,
            SortOrder::Descending,
        },
    },
    ui::gallery::order_memo::memoized_sort_indices,
};

/// Compare two artists by a single sort item, applying the configured order.
fn cmp_artists(a: &Artist, b: &Artist, item: ArtistSortItem) -> Ordering {
    let mut cmp = match item.criteria {
        Name => a.name.cmp(&b.name),
        AlbumCount => a.album_count.cmp(&b.album_count),
    };
    if item.order == Descending {
        cmp = cmp.reverse();
    }
    cmp
}

/// Compute a display order for `artists` as a sorted index vector.
///
/// Single‑pass `sort_unstable_by` over indices — each criteria is checked
/// in priority order until a non‑equal comparison is found. Borrows the
/// artist data so rebuilds never deep‑clone the `Vec<Artist>`.
#[must_use]
pub fn sorted_artist_indices(artists: &[Artist], sort_items: &[ArtistSortItem]) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..artists.len()).collect();
    indices.sort_unstable_by(|&a, &b| {
        let Some(a_artist) = artists.get(a) else {
            return Equal;
        };
        let Some(b_artist) = artists.get(b) else {
            return Equal;
        };
        sort_items
            .iter()
            .find_map(|item| {
                let cmp = cmp_artists(a_artist, b_artist, *item);
                (cmp != Equal).then_some(cmp)
            })
            .unwrap_or(Equal)
    });
    indices
}

/// Memoized display order for the cached artist data.
///
/// Returns the sort indices from the grid's memo when the library generation
/// and the current sort configuration both match, so repeated tab/mode
/// switches reuse the sort instead of re-comparing every artist. Computes,
/// caches, and returns them otherwise.
pub fn artist_sort_indices(state: &Arc<AppState>, cached: &CachedArtistData) -> Arc<[usize]> {
    let generation = state.artist_grid.generation.load(Relaxed);
    let config = state.storage.get_artists_sort();
    memoized_sort_indices(generation, config, &state.artist_grid.memo, |config| {
        sorted_artist_indices(&cached.artists, config)
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::{
            catalog::Artist,
            sort_rules::{
                ArtistSortCriteria::{AlbumCount, Name},
                ArtistSortItem,
                SortOrder::{Ascending, Descending},
            },
        },
        ui::gallery::precedence::sorted_artist_indices,
    };

    fn mock_artist(id: i64, name: &str, album_count: i32) -> Artist {
        Artist {
            id,
            name: name.into(),
            album_count,
        }
    }

    #[test]
    fn sorted_artist_indices_orders_by_priority_items() {
        let artists = vec![
            mock_artist(1, "Beta", 2),
            mock_artist(2, "Alpha", 5),
            mock_artist(3, "Gamma", 5),
        ];

        let items = vec![
            ArtistSortItem {
                criteria: AlbumCount,
                order: Descending,
            },
            ArtistSortItem {
                criteria: Name,
                order: Ascending,
            },
        ];

        let indices = sorted_artist_indices(&artists, &items);
        assert_eq!(indices, vec![1, 2, 0]);
    }

    #[test]
    fn sorted_artist_indices_orders_by_name() {
        let artists = vec![mock_artist(1, "Beta", 1), mock_artist(2, "Alpha", 1)];

        let items = vec![ArtistSortItem {
            criteria: Name,
            order: Ascending,
        }];

        let indices = sorted_artist_indices(&artists, &items);
        assert_eq!(indices, vec![1, 0]);
    }
}
