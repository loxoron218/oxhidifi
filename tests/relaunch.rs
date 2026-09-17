//! Library persistence verification (T057) per FR-028.
//!
//! Populates the library with tracks, albums, and artists, drops the storage
//! connection, reconnects to the same SQLite file, and asserts everything is
//! reloaded without re-scanning.

pub mod scratch_store;

use {
    anyhow::{Context, Result, ensure},
    tempfile::TempDir,
};

use oxhidifi::storage::{Storage, catalog::NewArtist, database::SqliteStorage};

use crate::scratch_store::{make_album, make_track};

/// Insert one track and return its id.
async fn insert_track(
    storage: &SqliteStorage,
    dir: &TempDir,
    artist_id: i64,
    album_id: i64,
    a: i32,
    alb: i32,
    t: i32,
) -> Result<i64> {
    let path = dir.path().join(format!("a{a}_alb{alb}_t{t}.flac"));
    let mut track = make_track(&format!("Track {a}-{alb}-{t}"), &path, Some(album_id));
    track.audio.content_hash = Some(format!("hash{a}{alb}{t}"));
    track.audio.artist_id = Some(artist_id);
    let track_id = storage.insert_track(track).await?;
    Ok(track_id)
}

/// Insert one album and its tracks, returning the album id and track ids.
async fn insert_album(
    storage: &SqliteStorage,
    dir: &TempDir,
    artist_id: i64,
    a: i32,
    alb: i32,
) -> Result<(i64, Vec<i64>)> {
    let album_id = storage
        .insert_album(make_album(
            &format!("Album {a}-{alb}"),
            artist_id,
            2020_i32.saturating_add(alb),
        ))
        .await?;
    let mut track_ids = Vec::new();
    for t in 0..3 {
        let track_id = insert_track(storage, dir, artist_id, album_id, a, alb, t).await?;
        track_ids.push(track_id);
    }
    Ok((album_id, track_ids))
}

/// Populate the library and return the ids we expect to persist.
async fn populate(
    storage: &SqliteStorage,
    dir: &TempDir,
) -> Result<(Vec<i64>, Vec<i64>, Vec<i64>)> {
    let mut artist_ids = Vec::new();
    let mut album_ids = Vec::new();
    let mut track_ids = Vec::new();

    for a in 0..3 {
        let artist_id = storage
            .insert_artist(NewArtist {
                name: format!("Artist {a}"),
            })
            .await?;
        artist_ids.push(artist_id);

        for alb in 0..2 {
            let (album_id, tracks) = insert_album(storage, dir, artist_id, a, alb).await?;
            album_ids.push(album_id);
            track_ids.extend(tracks);
        }
    }

    Ok((artist_ids, album_ids, track_ids))
}

/// Assert that every expected artist was reloaded after reconnect.
async fn assert_artists_reloaded(storage: &SqliteStorage, artist_ids: &[i64]) -> Result<()> {
    for (i, id) in artist_ids.iter().enumerate() {
        let artist = storage
            .get_artist(*id)
            .await?
            .context("artist not found after reconnect")?;
        ensure!(artist.name == format!("Artist {i}"), "artist name mismatch");
    }
    Ok(())
}

/// Assert that every expected album was reloaded, returning its track count.
async fn assert_albums_reloaded(storage: &SqliteStorage, album_ids: &[i64]) -> Result<usize> {
    let mut track_count: usize = 0;
    for id in album_ids {
        let tracks = storage.get_tracks_by_album(*id).await?;
        ensure!(tracks.len() == 3, "album {id} missing tracks");
        track_count = track_count.saturating_add(tracks.len());
    }
    Ok(track_count)
}

/// Assert that every expected track was reloaded after reconnect.
async fn assert_tracks_reloaded(storage: &SqliteStorage, track_ids: &[i64]) -> Result<()> {
    for id in track_ids {
        ensure!(
            storage.get_track(*id).await?.is_some(),
            "track {id} not reloaded after reconnect"
        );
    }
    Ok(())
}

/// Assert that every artist has its expected track count after reconnect.
async fn assert_artist_track_counts(storage: &SqliteStorage, artist_ids: &[i64]) -> Result<()> {
    for id in artist_ids {
        let tracks = storage.get_tracks_by_artist(*id).await?;
        ensure!(tracks.len() == 6, "artist {id} missing tracks");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        tokio::test,
    };

    use oxhidifi::storage::{Storage, database::SqliteStorage};

    use crate::{
        assert_albums_reloaded, assert_artist_track_counts, assert_artists_reloaded,
        assert_tracks_reloaded, populate, scratch_store::test_storage,
    };

    #[test]
    async fn library_survives_reconnect_without_rescan() -> Result<()> {
        let (storage, dir) = test_storage().await?;
        let db_path = dir.path().join("test.db");
        let settings_path = dir.path().join("settings.json");

        let (artist_ids, album_ids, track_ids) = populate(&storage, &dir).await?;
        drop(storage);

        ensure!(artist_ids.len() == 3, "expected 3 artists");
        ensure!(album_ids.len() == 6, "expected 6 albums");
        ensure!(track_ids.len() == 18, "expected 18 tracks");

        let storage = SqliteStorage::connect_with_settings_path(&db_path, &settings_path).await?;

        let artists = storage.get_all_artists().await?;
        ensure!(
            artists.len() == 3,
            "artists not reloaded: expected 3, got {}",
            artists.len()
        );
        assert_artists_reloaded(&storage, &artist_ids).await?;

        let albums = storage.get_all_albums().await?;
        ensure!(
            albums.len() == 6,
            "albums not reloaded: expected 6, got {}",
            albums.len()
        );

        let track_count = assert_albums_reloaded(&storage, &album_ids).await?;
        ensure!(track_count == 18, "track total mismatch: {track_count}");

        assert_tracks_reloaded(&storage, &track_ids).await?;
        assert_artist_track_counts(&storage, &artist_ids).await?;

        drop(storage);
        drop(dir);
        Ok(())
    }
}
