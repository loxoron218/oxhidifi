//! Fuzzy search index using `nucleo-matcher` for in-memory matching.
//!
//! Provides a `SearchIndex` that caches all library data and performs
//! fuzzy matching with relational expansion of results.

use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet},
    fmt::{Debug, Formatter, Result as FmtResult},
    sync::Arc,
};

use {
    nucleo_matcher::{
        Config, Matcher, Utf32String,
        pattern::{Atom, AtomKind, CaseMatching::Ignore, Normalization::Never},
    },
    parking_lot::Mutex,
    tracing::debug,
};

use crate::library::models::{Album, Artist, FuzzySearchResults, TrackSearchResult};

/// Maximum number of results per entity type.
const MAX_RESULTS: usize = 50;

/// State holder for result expansion during fuzzy search.
struct ExpandState {
    /// Deduplicates seen artist IDs.
    seen_artist_ids: HashSet<i64>,
    /// Deduplicates seen album IDs.
    seen_album_ids: HashSet<i64>,
    /// Deduplicates seen track IDs.
    seen_track_ids: HashSet<i64>,
    /// Final artist results.
    result_artists: Vec<Arc<Artist>>,
    /// Final album results.
    result_albums: Vec<Arc<Album>>,
    /// Final track results.
    result_tracks: Vec<TrackSearchResult>,
    /// Indices of expanded artists to sort and append.
    expanded_artist_indices: Vec<usize>,
    /// Indices of expanded albums to sort and append.
    expanded_album_indices: Vec<usize>,
    /// Indices of expanded tracks to sort and append.
    expanded_track_indices: Vec<usize>,
}

impl Default for ExpandState {
    fn default() -> Self {
        Self {
            seen_artist_ids: HashSet::new(),
            seen_album_ids: HashSet::new(),
            seen_track_ids: HashSet::new(),
            result_artists: Vec::with_capacity(MAX_RESULTS),
            result_albums: Vec::with_capacity(MAX_RESULTS),
            result_tracks: Vec::with_capacity(MAX_RESULTS),
            expanded_artist_indices: Vec::new(),
            expanded_album_indices: Vec::new(),
            expanded_track_indices: Vec::new(),
        }
    }
}

pub struct SearchIndex {
    /// Cached artist records.
    artists: Vec<Arc<Artist>>,
    /// Cached album records.
    albums: Vec<Arc<Album>>,
    /// Cached track records with metadata.
    tracks: Vec<TrackSearchResult>,
    /// Pre-converted artist name strings for matching.
    artist_needles: Vec<Utf32String>,
    /// Pre-converted album title strings for matching.
    album_needles: Vec<Utf32String>,
    /// Pre-converted track title strings for matching.
    track_needles: Vec<Utf32String>,
    /// Maps artist ID to album indices.
    artist_to_albums: HashMap<i64, Vec<usize>>,
    /// Maps artist ID to track indices.
    artist_to_tracks: HashMap<i64, Vec<usize>>,
    /// Maps album ID to track indices.
    album_to_tracks: HashMap<i64, Vec<usize>>,
    /// Maps album ID to artist index.
    album_to_artist_idx: HashMap<i64, usize>,
    /// Maps album ID to album index.
    album_id_to_idx: HashMap<i64, usize>,
    /// Maps artist ID to artist index.
    artist_id_to_idx: HashMap<i64, usize>,
    /// Reusable matcher for fuzzy search.
    matcher: Mutex<Matcher>,
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self {
            artists: Vec::new(),
            artist_needles: Vec::new(),
            albums: Vec::new(),
            album_needles: Vec::new(),
            tracks: Vec::new(),
            track_needles: Vec::new(),
            artist_to_albums: HashMap::new(),
            artist_to_tracks: HashMap::new(),
            album_to_tracks: HashMap::new(),
            album_to_artist_idx: HashMap::new(),
            album_id_to_idx: HashMap::new(),
            artist_id_to_idx: HashMap::new(),
            matcher: Mutex::new(Matcher::new(Config::DEFAULT)),
        }
    }
}

impl Debug for SearchIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("SearchIndex")
            .field("artist_count", &self.artists.len())
            .field("album_count", &self.albums.len())
            .field("track_count", &self.tracks.len())
            .finish_non_exhaustive()
    }
}

impl SearchIndex {
    /// Creates a new empty `SearchIndex`.
    ///
    /// # Returns
    ///
    /// A `SearchIndex` with no cached data.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds the search index with fresh library data.
    ///
    /// Pre-converts all entity names to `Utf32String` needles and constructs
    /// relational lookup maps for cross-type expansion during search.
    ///
    /// # Arguments
    ///
    /// * `artists` - All artists in the library
    /// * `albums` - All albums in the library
    /// * `tracks` - All tracks with metadata in the library
    pub fn refresh(&mut self, artists: &[Artist], albums: &[Album], tracks: &[TrackSearchResult]) {
        debug!(
            "Refreshing search index: {} artists, {} albums, {} tracks",
            artists.len(),
            albums.len(),
            tracks.len()
        );

        self.artist_id_to_idx.clear();
        self.artists = artists.iter().cloned().map(Arc::new).collect();
        for (i, artist) in self.artists.iter().enumerate() {
            self.artist_id_to_idx.insert(artist.id, i);
        }

        self.album_id_to_idx.clear();
        self.album_to_artist_idx.clear();
        self.albums = albums.iter().cloned().map(Arc::new).collect();
        for (i, album) in self.albums.iter().enumerate() {
            self.album_id_to_idx.insert(album.id, i);
            if let Some(&artist_idx) = self.artist_id_to_idx.get(&album.artist_id) {
                self.album_to_artist_idx.insert(album.id, artist_idx);
            }
        }

        self.tracks = tracks.to_vec();

        self.artist_to_albums.clear();
        self.artist_to_tracks.clear();
        self.album_to_tracks.clear();

        for (album_idx, album) in self.albums.iter().enumerate() {
            self.artist_to_albums
                .entry(album.artist_id)
                .or_default()
                .push(album_idx);
        }

        for (track_idx, track_result) in self.tracks.iter().enumerate() {
            self.album_to_tracks
                .entry(track_result.album_id)
                .or_default()
                .push(track_idx);
            self.artist_to_tracks
                .entry(track_result.artist_id)
                .or_default()
                .push(track_idx);
        }

        self.artist_needles = Vec::with_capacity(self.artists.len());
        for artist in &self.artists {
            let mut needle = artist.name.clone();
            if let Some(album_indices) = self.artist_to_albums.get(&artist.id) {
                for &idx in album_indices {
                    needle.push(' ');
                    needle.push_str(&self.albums[idx].title);
                }
            }
            self.artist_needles.push(Utf32String::from(needle.as_str()));
        }

        self.album_needles = Vec::with_capacity(self.albums.len());
        for album in &self.albums {
            if let Some(&artist_idx) = self.album_to_artist_idx.get(&album.id) {
                let artist_name = &self.artists[artist_idx].name;
                self.album_needles.push(Utf32String::from(
                    format!("{artist_name} {}", album.title).as_str(),
                ));
            } else {
                self.album_needles
                    .push(Utf32String::from(album.title.as_str()));
            }
        }

        self.track_needles = Vec::with_capacity(self.tracks.len());
        for track_result in &self.tracks {
            let needle = format!(
                "{} {} {}",
                track_result.artist_name, track_result.album_title, track_result.track.title
            );
            self.track_needles.push(Utf32String::from(needle.as_str()));
        }
    }

    /// Performs a fuzzy search across all cached entities and expands results
    /// to include related entities (e.g. a matching artist also returns their
    /// albums and tracks).
    ///
    /// # Arguments
    ///
    /// * `query` - The search query string
    ///
    /// # Returns
    ///
    /// A `FuzzySearchResults` containing deduplicated artists, albums, and
    /// tracks. Direct matches are ordered by fuzzy score descending;
    /// expanded matches follow alphabetically. Each entity type is capped
    /// at `MAX_RESULTS`.
    pub fn search(&self, query: &str) -> FuzzySearchResults {
        if query.is_empty()
            || (self.artists.is_empty() && self.albums.is_empty() && self.tracks.is_empty())
        {
            return FuzzySearchResults::default();
        }

        let mut matcher = self.matcher.lock();
        let atom = Atom::new(query, Ignore, Never, AtomKind::Fuzzy, false);

        let scored_artists = score_entities(&atom, &self.artist_needles, &mut matcher);
        let scored_albums = score_entities(&atom, &self.album_needles, &mut matcher);
        let scored_tracks = score_entities(&atom, &self.track_needles, &mut matcher);

        let results = self.expand_matches(&scored_artists, &scored_albums, &scored_tracks);

        drop(matcher);

        debug!(
            "Fuzzy search '{}' found {} artists, {} albums, {} tracks",
            query,
            results.artists.len(),
            results.albums.len(),
            results.tracks.len()
        );

        results
    }

    /// Expands direct fuzzy matches to include related entities via
    /// relational traversal (artist → albums/tracks, album → artist/tracks,
    /// track → album/artist). Deduplicates across all types and sorts
    /// expanded matches alphabetically after direct matches.
    ///
    /// # Arguments
    ///
    /// * `scored_artists` - Scored artist indices from fuzzy matching
    /// * `scored_albums` - Scored album indices from fuzzy matching
    /// * `scored_tracks` - Scored track indices from fuzzy matching
    ///
    /// # Returns
    ///
    /// A `FuzzySearchResults` with direct matches first, then expanded
    /// relations, all deduplicated and truncated to `MAX_RESULTS` per type.
    fn expand_matches(
        &self,
        scored_artists: &[(u16, usize)],
        scored_albums: &[(u16, usize)],
        scored_tracks: &[(u16, usize)],
    ) -> FuzzySearchResults {
        let mut state = ExpandState::default();

        self.expand_from_artists(scored_artists, &mut state);
        self.expand_from_albums(scored_albums, &mut state);
        self.expand_from_tracks(scored_tracks, &mut state);
        self.sort_and_append_expanded(&mut state);

        state.result_artists.truncate(MAX_RESULTS);
        state.result_albums.truncate(MAX_RESULTS);
        state.result_tracks.truncate(MAX_RESULTS);

        FuzzySearchResults {
            artists: state.result_artists,
            albums: state.result_albums,
            tracks: state.result_tracks,
        }
    }

    /// Processes direct artist matches and expands to related albums and tracks
    /// via relational lookup.
    ///
    /// Each matched artist is added to results, and all albums and tracks
    /// associated with that artist are queued for expanded results.
    ///
    /// # Arguments
    ///
    /// * `scored_artists` - Scored artist indices from fuzzy matching
    /// * `state` - Mutable expansion state to update
    fn expand_from_artists(&self, scored_artists: &[(u16, usize)], state: &mut ExpandState) {
        for (_, idx) in scored_artists {
            let artist = &self.artists[*idx];
            state.seen_artist_ids.insert(artist.id);
            state.result_artists.push(Arc::clone(artist));

            if let Some(album_indices) = self.artist_to_albums.get(&artist.id) {
                for &album_idx in album_indices {
                    if state.seen_album_ids.insert(self.albums[album_idx].id) {
                        state
                            .result_albums
                            .push(Arc::clone(&self.albums[album_idx]));
                    }
                }
            }

            if let Some(track_indices) = self.artist_to_tracks.get(&artist.id) {
                for &track_idx in track_indices {
                    if state.seen_track_ids.insert(self.tracks[track_idx].track.id) {
                        state.result_tracks.push(self.tracks[track_idx].clone());
                    }
                }
            }
        }
    }

    /// Processes direct album matches and expands to related artist and tracks
    /// via relational lookup.
    ///
    /// Each matched album is added to results (if not already seen), and the
    /// parent artist and all tracks on the album are queued for expanded results.
    ///
    /// # Arguments
    ///
    /// * `scored_albums` - Scored album indices from fuzzy matching
    /// * `state` - Mutable expansion state to update
    fn expand_from_albums(&self, scored_albums: &[(u16, usize)], state: &mut ExpandState) {
        for (_, idx) in scored_albums {
            let album = &self.albums[*idx];
            if state.seen_album_ids.insert(album.id) {
                state.result_albums.push(Arc::clone(album));
            }

            if let Some(&artist_idx) = self.album_to_artist_idx.get(&album.id)
                && state.seen_artist_ids.insert(self.artists[artist_idx].id)
            {
                state.expanded_artist_indices.push(artist_idx);
            }

            if let Some(track_indices) = self.album_to_tracks.get(&album.id) {
                for &track_idx in track_indices {
                    if state.seen_track_ids.insert(self.tracks[track_idx].track.id) {
                        state.expanded_track_indices.push(track_idx);
                    }
                }
            }
        }
    }

    /// Processes direct track matches and expands to related album and artist
    /// via relational lookup.
    ///
    /// Each matched track is added to results (if not already seen), and the
    /// parent album and artist are queued for expanded results.
    ///
    /// # Arguments
    ///
    /// * `scored_tracks` - Scored track indices from fuzzy matching
    /// * `state` - Mutable expansion state to update
    fn expand_from_tracks(&self, scored_tracks: &[(u16, usize)], state: &mut ExpandState) {
        for (_, idx) in scored_tracks {
            let track_result = &self.tracks[*idx];
            if state.seen_track_ids.insert(track_result.track.id) {
                state.result_tracks.push(track_result.clone());
            }

            if let Some(&album_idx) = self.album_id_to_idx.get(&track_result.album_id)
                && state.seen_album_ids.insert(self.albums[album_idx].id)
            {
                state.expanded_album_indices.push(album_idx);
            }

            if let Some(&artist_idx) = self.artist_id_to_idx.get(&track_result.artist_id)
                && state.seen_artist_ids.insert(self.artists[artist_idx].id)
            {
                state.expanded_artist_indices.push(artist_idx);
            }
        }
    }

    /// Sorts expanded entity indices alphabetically and appends them to the
    /// corresponding result vectors.
    ///
    /// Artists are sorted by name, albums by title, and tracks by artist name,
    /// album title, disc number, then track number.
    ///
    /// # Arguments
    ///
    /// * `state` - Mutable expansion state containing indices to sort and append
    fn sort_and_append_expanded(&self, state: &mut ExpandState) {
        state
            .expanded_artist_indices
            .sort_by(|&a, &b| self.artists[a].name.cmp(&self.artists[b].name));
        state
            .expanded_album_indices
            .sort_by(|&a, &b| self.albums[a].title.cmp(&self.albums[b].title));
        state.expanded_track_indices.sort_by(|&a, &b| {
            self.tracks[a]
                .artist_name
                .cmp(&self.tracks[b].artist_name)
                .then_with(|| self.tracks[a].album_title.cmp(&self.tracks[b].album_title))
                .then_with(|| {
                    self.tracks[a]
                        .track
                        .disc_number
                        .cmp(&self.tracks[b].track.disc_number)
                })
                .then_with(|| {
                    self.tracks[a]
                        .track
                        .track_number
                        .unwrap_or(0)
                        .cmp(&self.tracks[b].track.track_number.unwrap_or(0))
                })
        });

        for &idx in &state.expanded_artist_indices {
            state.result_artists.push(Arc::clone(&self.artists[idx]));
        }
        for &idx in &state.expanded_album_indices {
            state.result_albums.push(Arc::clone(&self.albums[idx]));
        }
        for &idx in &state.expanded_track_indices {
            state.result_tracks.push(self.tracks[idx].clone());
        }
    }

    /// Returns `true` if the index contains any cached entities.
    ///
    /// # Returns
    ///
    /// `true` if at least one artist, album, or track is indexed.
    #[must_use]
    pub fn is_populated(&self) -> bool {
        !self.artists.is_empty() || !self.albums.is_empty() || !self.tracks.is_empty()
    }
}

/// Scores entities by fuzzy matching a pattern against pre-converted needles.
///
/// # Arguments
///
/// * `atom` - The compiled fuzzy pattern to match against
/// * `needles` - Pre-converted UTF-32 strings to score
/// * `matcher` - Mutable reference to the nucleo `Matcher`
///
/// # Returns
///
/// A vector of `(score, index)` pairs sorted by score descending.
fn score_entities(
    atom: &Atom,
    needles: &[Utf32String],
    matcher: &mut Matcher,
) -> Vec<(u16, usize)> {
    let mut scored: Vec<(u16, usize)> = needles
        .iter()
        .enumerate()
        .filter_map(|(i, needle)| atom.score(needle.slice(..), matcher).map(|s| (s, i)))
        .collect();
    scored.sort_by_key(|(score, _)| Reverse(*score));
    scored
}
