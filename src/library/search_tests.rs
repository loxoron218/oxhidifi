//! Tests for the fuzzy search index.
//!
//! Covers fuzzy matching, relational expansion, deduplication, and edge cases.

use crate::library::{
    models::{Album, Artist, Track, TrackSearchResult},
    search::SearchIndex,
};

fn make_artist(id: i64, name: &str) -> Artist {
    Artist {
        id,
        name: name.to_string(),
        album_count: 0,
        created_at: None,
        updated_at: None,
    }
}

fn make_album(id: i64, artist_id: i64, title: &str) -> Album {
    Album {
        id,
        artist_id,
        title: title.to_string(),
        ..Default::default()
    }
}

fn make_track_result(
    track_id: i64,
    album_id: i64,
    artist_id: i64,
    title: &str,
    album_title: &str,
    artist_name: &str,
) -> TrackSearchResult {
    TrackSearchResult {
        track: Track {
            id: track_id,
            album_id,
            title: title.to_string(),
            ..Default::default()
        },
        album_id,
        album_title: album_title.to_string(),
        artist_id,
        artist_name: artist_name.to_string(),
        artwork_path: None,
    }
}

fn build_test_index() -> SearchIndex {
    let artists = vec![make_artist(1, "Pink Floyd"), make_artist(2, "The Beatles")];

    let albums = vec![
        make_album(10, 1, "The Dark Side of the Moon"),
        make_album(20, 2, "Abbey Road"),
    ];

    let tracks = vec![
        make_track_result(
            100,
            10,
            1,
            "Time",
            "The Dark Side of the Moon",
            "Pink Floyd",
        ),
        make_track_result(
            101,
            10,
            1,
            "Money",
            "The Dark Side of the Moon",
            "Pink Floyd",
        ),
        make_track_result(200, 20, 2, "Come Together", "Abbey Road", "The Beatles"),
    ];

    let mut index = SearchIndex::new();
    index.refresh(&artists, &albums, &tracks);
    index
}

#[test]
fn empty_query() {
    let index = build_test_index();
    let results = index.search("");
    assert!(results.tracks.is_empty());
    assert!(results.albums.is_empty());
    assert!(results.artists.is_empty());
}

#[test]
fn empty_index() {
    let index = SearchIndex::new();
    let results = index.search("pink");
    assert!(results.tracks.is_empty());
    assert!(results.albums.is_empty());
    assert!(results.artists.is_empty());
}

#[test]
fn artist_match_expands_albums_and_tracks() {
    let index = build_test_index();
    let results = index.search("pink");

    assert!(results.artists.iter().any(|a| a.name == "Pink Floyd"));
    assert!(
        results
            .albums
            .iter()
            .any(|a| a.title == "The Dark Side of the Moon")
    );
    assert!(results.tracks.iter().any(|t| t.track.title == "Time"));
    assert!(results.tracks.iter().any(|t| t.track.title == "Money"));

    assert!(!results.artists.iter().any(|a| a.name == "The Beatles"));
}

#[test]
fn album_match_expands_artist_and_tracks() {
    let index = build_test_index();
    let results = index.search("abbey");

    assert!(results.albums.iter().any(|a| a.title == "Abbey Road"));
    assert!(results.artists.iter().any(|a| a.name == "The Beatles"));
    assert!(
        results
            .tracks
            .iter()
            .any(|t| t.track.title == "Come Together")
    );
}

#[test]
fn track_match_expands_album_and_artist() {
    let index = build_test_index();
    let results = index.search("money");

    assert!(results.tracks.iter().any(|t| t.track.title == "Money"));
    assert!(
        results
            .albums
            .iter()
            .any(|a| a.title == "The Dark Side of the Moon")
    );
    assert!(results.artists.iter().any(|a| a.name == "Pink Floyd"));
}

#[test]
fn deduplication() {
    let index = build_test_index();
    let results = index.search("pink");

    let artist_count = results
        .artists
        .iter()
        .filter(|a| a.name == "Pink Floyd")
        .count();
    assert_eq!(artist_count, 1);

    let album_count = results
        .albums
        .iter()
        .filter(|a| a.title == "The Dark Side of the Moon")
        .count();
    assert_eq!(album_count, 1);
}

#[test]
fn no_results() {
    let index = build_test_index();
    let results = index.search("zzzznonexistent");
    assert!(results.tracks.is_empty());
    assert!(results.albums.is_empty());
    assert!(results.artists.is_empty());
}

#[test]
fn fuzzy_matching() {
    let index = build_test_index();
    let results = index.search("pkfld");

    assert!(results.artists.iter().any(|a| a.name == "Pink Floyd"));
}

#[test]
fn is_populated() {
    let index = build_test_index();
    assert!(index.is_populated());

    let empty = SearchIndex::new();
    assert!(!empty.is_populated());
}

#[test]
fn cross_type_expansion_does_not_duplicate_direct_matches() {
    let index = build_test_index();
    let results = index.search("dark");

    assert!(
        results
            .albums
            .iter()
            .any(|a| a.title == "The Dark Side of the Moon")
    );
    assert!(results.artists.iter().any(|a| a.name == "Pink Floyd"));

    let artist_count = results
        .artists
        .iter()
        .filter(|a| a.name == "Pink Floyd")
        .count();
    assert_eq!(artist_count, 1);

    let album_count = results
        .albums
        .iter()
        .filter(|a| a.title == "The Dark Side of the Moon")
        .count();
    assert_eq!(album_count, 1);
}

#[test]
fn max_results_truncation() {
    let mut artists = Vec::with_capacity(60);
    let mut albums = Vec::with_capacity(60);
    let mut tracks = Vec::with_capacity(60);

    for i in 0..60 {
        artists.push(make_artist(i, &format!("Artist {i}")));
        albums.push(make_album(i, i, &format!("Album {i}")));
        tracks.push(make_track_result(
            i,
            i,
            i,
            &format!("Track {i}"),
            &format!("Album {i}"),
            &format!("Artist {i}"),
        ));
    }

    let mut index = SearchIndex::new();
    index.refresh(&artists, &albums, &tracks);

    let results = index.search("Artist");

    assert!(
        results.artists.len() <= 50,
        "Expected at most 50 artists, got {}",
        results.artists.len()
    );
    assert!(
        results.albums.len() <= 50,
        "Expected at most 50 albums, got {}",
        results.albums.len()
    );
    assert!(
        results.tracks.len() <= 50,
        "Expected at most 50 tracks, got {}",
        results.tracks.len()
    );
}

#[test]
fn relational_map_consistency() {
    let artists = vec![make_artist(1, "Artist One"), make_artist(2, "Artist Two")];

    let albums = vec![make_album(10, 1, "Album A"), make_album(20, 2, "Album B")];

    let tracks = vec![
        make_track_result(100, 10, 1, "Track 1", "Album A", "Artist One"),
        make_track_result(101, 10, 1, "Track 2", "Album A", "Artist One"),
        make_track_result(200, 20, 2, "Track 3", "Album B", "Artist Two"),
    ];

    let mut index = SearchIndex::new();
    index.refresh(&artists, &albums, &tracks);

    let results = index.search("Artist");

    for track in &results.tracks {
        assert!(
            artists.iter().any(|a| a.id == track.artist_id),
            "Track '{}' references artist_id {} not in artists",
            track.track.title,
            track.artist_id
        );
    }

    for album in &results.albums {
        assert!(
            artists.iter().any(|a| a.id == album.artist_id),
            "Album '{}' references artist_id {} not in artists",
            album.title,
            album.artist_id
        );
    }

    let results = index.search("Album");
    for track in &results.tracks {
        assert!(
            albums.iter().any(|a| a.id == track.album_id),
            "Track '{}' references album_id {} not in albums",
            track.track.title,
            track.album_id
        );
    }
}

#[test]
fn artist_context_boosts_album_scores_over_false_fuzzy_matches() {
    let artists = vec![
        make_artist(1, "Aphex Twin"),
        make_artist(2, "Black Country, New Road"),
    ];

    let albums = vec![
        make_album(10, 1, "Selected Ambient Works 85-92"),
        make_album(20, 2, "Ants From Up There"),
    ];

    let tracks = vec![
        make_track_result(
            100,
            10,
            1,
            "Xtal",
            "Selected Ambient Works 85-92",
            "Aphex Twin",
        ),
        make_track_result(
            200,
            20,
            2,
            "Concorde",
            "Ants From Up There",
            "Black Country, New Road",
        ),
    ];

    let mut index = SearchIndex::new();
    index.refresh(&artists, &albums, &tracks);

    let results = index.search("aph");

    assert!(
        results.artists.iter().any(|a| a.name == "Aphex Twin"),
        "Aphex Twin should appear in artist results for 'aph'"
    );
    assert!(
        results
            .albums
            .iter()
            .any(|a| a.title == "Selected Ambient Works 85-92"),
        "Aphex Twin album should appear for 'aph'"
    );

    let aphex_album_pos = results
        .albums
        .iter()
        .position(|a| a.title == "Selected Ambient Works 85-92");
    let bcnr_album_pos = results
        .albums
        .iter()
        .position(|a| a.title == "Ants From Up There");

    if let (Some(aphex_pos), Some(bcnr_pos)) = (aphex_album_pos, bcnr_album_pos) {
        assert!(
            aphex_pos < bcnr_pos,
            "Aphex Twin album (pos {aphex_pos}) should rank above BCNR album (pos {bcnr_pos}) for \
             'aph'"
        );
    }
}

#[test]
fn track_search_includes_artist_and_album_context() {
    let artists = vec![make_artist(1, "Radiohead")];
    let albums = vec![make_album(10, 1, "OK Computer")];
    let tracks = vec![make_track_result(
        100,
        10,
        1,
        "Paranoid Android",
        "OK Computer",
        "Radiohead",
    )];

    let mut index = SearchIndex::new();
    index.refresh(&artists, &albums, &tracks);

    let results = index.search("radiohead paranoid");
    assert!(
        results
            .tracks
            .iter()
            .any(|t| t.track.title == "Paranoid Android"),
        "Track should match when searching artist + track name together"
    );

    let results = index.search("ok computer");
    assert!(
        results
            .tracks
            .iter()
            .any(|t| t.track.title == "Paranoid Android"),
        "Track should match when searching album title"
    );
}

#[test]
fn album_search_includes_artist_context() {
    let artists = vec![make_artist(1, "Daft Punk")];
    let albums = vec![make_album(10, 1, "Random Access Memories")];
    let tracks = vec![make_track_result(
        100,
        10,
        1,
        "Get Lucky",
        "Random Access Memories",
        "Daft Punk",
    )];

    let mut index = SearchIndex::new();
    index.refresh(&artists, &albums, &tracks);

    let results = index.search("daft");
    assert!(
        results
            .albums
            .iter()
            .any(|a| a.title == "Random Access Memories"),
        "Album should match when searching artist name"
    );

    let results = index.search("daft memories");
    assert!(
        results
            .albums
            .iter()
            .any(|a| a.title == "Random Access Memories"),
        "Album should fuzzy-match across artist + title"
    );
}
