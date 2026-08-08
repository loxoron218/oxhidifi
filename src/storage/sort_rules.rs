//! Sort criteria and ordering rules for library grid views.

use std::fmt::{Display, Formatter, Result};

use serde::{Deserialize, Serialize};

/// Generate `discriminator`, `from_discriminator`, and `Display` for a sort
/// criteria enum from a single variant list, so the three never drift apart.
///
/// Each entry maps a variant to its numeric widget discriminator and its
/// human‑readable display label. Discriminator numbers are part of the
/// persisted widget names and must stay stable across releases.
macro_rules! impl_criteria_helpers {
    ($ty:ident { $($variant:ident => ($disc:literal, $label:literal)),+ $(,)? }) => {
        impl $ty {
            /// Numeric discriminator used as a compact widget identifier.
            #[must_use]
            pub const fn discriminator(&self) -> u8 {
                match self {
                    $(Self::$variant => $disc,)+
                }
            }

            /// Recover a variant from its numeric discriminator.
            #[must_use]
            pub const fn from_discriminator(d: u8) -> Option<Self> {
                match d {
                    $($disc => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl Display for $ty {
            fn fmt(&self, f: &mut Formatter<'_>) -> Result {
                let s = match self {
                    $(Self::$variant => $label,)+
                };
                write!(f, "{s}")
            }
        }
    };
}

/// Criteria for sorting the album grid view.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlbumSortCriteria {
    /// Sort by album title.
    Title,
    /// Sort by artist name.
    Artist,
    /// Sort by release year.
    Year,
    /// Sort by audio format.
    Format,
    /// Sort by bits per sample.
    BitDepth,
    /// Sort by sample rate.
    SampleRate,
}

impl_criteria_helpers!(AlbumSortCriteria {
    Title => (0, "Title"),
    Artist => (1, "Artist"),
    Year => (2, "Year"),
    Format => (3, "Format"),
    BitDepth => (4, "Bit Depth"),
    SampleRate => (5, "Sample Rate"),
});

/// A single sort rule for the album grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumSortItem {
    /// The sorting criteria.
    pub criteria: AlbumSortCriteria,
    /// The sorting order.
    pub order: SortOrder,
}

/// Criteria for sorting the artist grid view.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtistSortCriteria {
    /// Sort by artist name.
    Name,
    /// Sort by number of albums.
    AlbumCount,
}

impl_criteria_helpers!(ArtistSortCriteria {
    Name => (0, "Name"),
    AlbumCount => (1, "Album Count"),
});

/// A single sort rule for the artist grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistSortItem {
    /// The sorting criteria.
    pub criteria: ArtistSortCriteria,
    /// The sorting order.
    pub order: SortOrder,
}

/// Sort order for library grid view sorting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortOrder {
    /// Ascending sort order.
    #[default]
    Ascending,
    /// Descending sort order.
    Descending,
}

/// Default albums grid sort configuration.
#[must_use]
pub fn default_albums_sort() -> Vec<AlbumSortItem> {
    vec![
        AlbumSortItem {
            criteria: AlbumSortCriteria::Artist,
            order: SortOrder::Ascending,
        },
        AlbumSortItem {
            criteria: AlbumSortCriteria::Year,
            order: SortOrder::Ascending,
        },
        AlbumSortItem {
            criteria: AlbumSortCriteria::Title,
            order: SortOrder::Ascending,
        },
        AlbumSortItem {
            criteria: AlbumSortCriteria::Format,
            order: SortOrder::Ascending,
        },
        AlbumSortItem {
            criteria: AlbumSortCriteria::BitDepth,
            order: SortOrder::Descending,
        },
        AlbumSortItem {
            criteria: AlbumSortCriteria::SampleRate,
            order: SortOrder::Descending,
        },
    ]
}

/// Default artists grid sort configuration.
#[must_use]
pub fn default_artists_sort() -> Vec<ArtistSortItem> {
    vec![
        ArtistSortItem {
            criteria: ArtistSortCriteria::Name,
            order: SortOrder::Ascending,
        },
        ArtistSortItem {
            criteria: ArtistSortCriteria::AlbumCount,
            order: SortOrder::Descending,
        },
    ]
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        serde_json::{from_str, to_string},
    };

    use crate::storage::sort_rules::{
        AlbumSortCriteria,
        AlbumSortCriteria::{Artist, BitDepth, Format, SampleRate, Title, Year},
        AlbumSortItem,
        ArtistSortCriteria::{self, AlbumCount, Name},
        ArtistSortItem,
        SortOrder::{Ascending, Descending},
        default_albums_sort, default_artists_sort,
    };

    #[test]
    fn album_criteria_discriminator_round_trips() {
        let cases = [
            (Title, 0),
            (Artist, 1),
            (Year, 2),
            (Format, 3),
            (BitDepth, 4),
            (SampleRate, 5),
        ];
        for (criteria, disc) in cases {
            assert_eq!(
                criteria.discriminator(),
                disc,
                "criteria must keep its stable discriminator"
            );
            assert_eq!(
                AlbumSortCriteria::from_discriminator(disc),
                Some(criteria),
                "discriminator {disc} must round-trip"
            );
        }
    }

    #[test]
    fn artist_criteria_discriminator_round_trips() {
        let cases = [(Name, 0), (AlbumCount, 1)];
        for (criteria, disc) in cases {
            assert_eq!(
                criteria.discriminator(),
                disc,
                "criteria must keep its stable discriminator"
            );
            assert_eq!(
                ArtistSortCriteria::from_discriminator(disc),
                Some(criteria),
                "discriminator {disc} must round-trip"
            );
        }
    }

    #[test]
    fn from_discriminator_returns_none_for_unknown_values() {
        assert_eq!(<AlbumSortCriteria>::from_discriminator(6), None);
        assert_eq!(<AlbumSortCriteria>::from_discriminator(255), None);
        assert_eq!(<ArtistSortCriteria>::from_discriminator(2), None);
        assert_eq!(<ArtistSortCriteria>::from_discriminator(255), None);
    }

    #[test]
    fn album_criteria_display_labels() {
        assert_eq!(Title.to_string(), "Title");
        assert_eq!(Artist.to_string(), "Artist");
        assert_eq!(Year.to_string(), "Year");
        assert_eq!(Format.to_string(), "Format");
        assert_eq!(BitDepth.to_string(), "Bit Depth");
        assert_eq!(SampleRate.to_string(), "Sample Rate");
    }

    #[test]
    fn artist_criteria_display_labels() {
        assert_eq!(Name.to_string(), "Name");
        assert_eq!(AlbumCount.to_string(), "Album Count");
    }

    #[test]
    fn default_albums_sort_orders_by_artist_year_title_then_format_quality() {
        let sort = default_albums_sort();
        let criteria: Vec<_> = sort.iter().map(|item| item.criteria.clone()).collect();
        assert_eq!(
            criteria,
            vec![Artist, Year, Title, Format, BitDepth, SampleRate],
            "default album sort priority must be artist, year, title, format, bit depth, sample \
             rate"
        );
        let orders: Vec<_> = sort.iter().map(|item| item.order).collect();
        assert_eq!(
            orders,
            vec![
                Ascending, Ascending, Ascending, Ascending, Descending, Descending
            ],
            "format quality criteria must default to descending"
        );
    }

    #[test]
    fn default_artists_sort_orders_by_name_then_album_count() {
        let sort = default_artists_sort();
        let criteria: Vec<_> = sort.iter().map(|item| item.criteria.clone()).collect();
        assert_eq!(
            criteria,
            vec![Name, AlbumCount],
            "default artist sort priority must be name, then album count"
        );
        let orders: Vec<_> = sort.iter().map(|item| item.order).collect();
        assert_eq!(orders, vec![Ascending, Descending]);
    }

    #[test]
    fn sort_items_round_trip_through_serde() -> Result<()> {
        let items = vec![
            AlbumSortItem {
                criteria: BitDepth,
                order: Descending,
            },
            AlbumSortItem {
                criteria: Title,
                order: Ascending,
            },
        ];
        let json = to_string(&items)?;
        let restored: Vec<AlbumSortItem> = from_str(&json)?;
        ensure!(restored == items, "album sort items must round-trip");

        let artists = vec![ArtistSortItem {
            criteria: AlbumCount,
            order: Descending,
        }];
        let json = to_string(&artists)?;
        let restored: Vec<ArtistSortItem> = from_str(&json)?;
        ensure!(restored == artists, "artist sort items must round-trip");
        Ok(())
    }
}
