//! Queue persistence verification (T052b) per FR-028.
//!
//! Populates the playback queue with contexts, drops the storage connection,
//! reconnects to the same SQLite file, and asserts the queue order, track IDs,
//! and context are preserved.

mod db_setup;

use std::path::Path;

use anyhow::Result;

use oxhidifi::storage::{
    Storage,
    catalog::{
        NewArtist, NewQueueEntry,
        QueueContext::{self, Album, Artist, Manual},
        QueueEntry,
    },
    database::SqliteStorage,
};

use crate::db_setup::{make_album, make_track};

/// Return the (`context_type`, `context_id`) pair of a queue entry at `idx`,
/// or `(None, None)` when the entry does not exist.
fn context_at(queue: &[QueueEntry], idx: usize) -> (Option<&str>, Option<i64>) {
    queue.get(idx).map_or((None, None), |entry| {
        (entry.context_type.as_deref(), entry.context_id)
    })
}

/// Build a track, artist, and album fixture and return the track id.
async fn insert_track(storage: &SqliteStorage, title: &str, path: &Path) -> Result<i64> {
    let artist_name = format!("Artist for {title}");
    let artist_id = storage
        .insert_artist(NewArtist { name: artist_name })
        .await?;
    let album_id = storage
        .insert_album(make_album(&format!("Album for {title}"), artist_id, 2024))
        .await?;
    let track = make_track(title, path, Some(album_id));
    let track_id = storage.insert_track(track).await?;
    Ok(track_id)
}

/// Encode a `QueueContext` into the wire format used by `NewQueueEntry`.
fn encode_context(context: &QueueContext) -> (Option<String>, Option<i64>) {
    match context {
        Album(id) => (Some("album".to_string()), Some(*id)),
        Artist(id) => (Some("artist".to_string()), Some(*id)),
        Manual => (None, None),
    }
}

/// Populate the queue with four tracks and return their ids.
async fn populate_queue(db_path: &Path, settings_path: &Path, dir: &Path) -> Result<[i64; 4]> {
    let storage = SqliteStorage::connect_with_settings_path(db_path, settings_path).await?;
    let t1 = insert_track(&storage, "Track One", &dir.join("one.flac")).await?;
    let t2 = insert_track(&storage, "Track Two", &dir.join("two.flac")).await?;
    let t3 = insert_track(&storage, "Track Three", &dir.join("three.flac")).await?;
    let t4 = insert_track(&storage, "Track Four", &dir.join("four.flac")).await?;

    let contexts = [
        (t1, Album(11)),
        (t2, Manual),
        (t3, Artist(22)),
        (t4, Album(33)),
    ];

    let entries: Vec<NewQueueEntry> = contexts
        .iter()
        .enumerate()
        .map(|(i, (track_id, context))| {
            let (context_type, context_id) = encode_context(context);
            NewQueueEntry {
                track_id: *track_id,
                position: i32::try_from(i).unwrap_or(0),
                context_type,
                context_id,
            }
        })
        .collect();

    storage.set_queue(&entries).await?;
    Ok([t1, t2, t3, t4])
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Context, Result, ensure},
        tempfile::tempdir,
        tokio::test,
    };

    use oxhidifi::storage::{Storage, catalog::NewQueueEntry, database::SqliteStorage};

    use crate::{context_at, db_setup::test_storage, insert_track, populate_queue};

    #[test]
    async fn queue_order_ids_and_context_survive_reconnect() -> Result<()> {
        let dir = tempdir().context("failed to create temp dir")?;
        let db_path = dir.path().join("test.db");
        let settings_path = dir.path().join("settings.json");

        let track_ids = populate_queue(&db_path, &settings_path, dir.path()).await?;

        let storage = SqliteStorage::connect_with_settings_path(&db_path, &settings_path).await?;
        let queue = storage.get_queue().await?;

        ensure!(
            queue.len() == 4,
            "expected 4 queue entries, got {}",
            queue.len()
        );
        let actual_ids: Vec<i64> = queue.iter().map(|e| e.track_id).collect();
        ensure!(
            actual_ids == track_ids,
            "queue track ids changed after reconnect: {actual_ids:?} != {track_ids:?}"
        );

        let positions: Vec<i32> = queue.iter().map(|e| e.position).collect();
        ensure!(
            positions == vec![0, 1, 2, 3],
            "queue order changed after reconnect: {positions:?}"
        );

        let (ctx_type, ctx_id) = context_at(&queue, 0);
        ensure!(
            ctx_type == Some("album") && ctx_id == Some(11),
            "album context not preserved for entry 0"
        );
        let (ctx_type, ctx_id) = context_at(&queue, 1);
        ensure!(
            ctx_type.is_none() && ctx_id.is_none(),
            "manual context not preserved for entry 1"
        );
        let (ctx_type, ctx_id) = context_at(&queue, 2);
        ensure!(
            ctx_type == Some("artist") && ctx_id == Some(22),
            "artist context not preserved for entry 2"
        );
        let (ctx_type, ctx_id) = context_at(&queue, 3);
        ensure!(
            ctx_type == Some("album") && ctx_id == Some(33),
            "album context not preserved for entry 3"
        );

        drop(storage);
        drop(dir);
        Ok(())
    }

    #[test]
    async fn queue_persistence_via_trait_round_trip() -> Result<()> {
        let (storage, dir) = test_storage().await?;
        let t1 = insert_track(&storage, "A", &dir.path().join("a.flac")).await?;
        let t2 = insert_track(&storage, "B", &dir.path().join("b.flac")).await?;

        let entries = vec![
            NewQueueEntry {
                track_id: t1,
                position: 0,
                context_type: Some("album".to_string()),
                context_id: Some(1),
            },
            NewQueueEntry {
                track_id: t2,
                position: 1,
                context_type: None,
                context_id: None,
            },
        ];
        storage.set_queue(&entries).await?;

        let restored = storage.get_queue().await?;
        ensure!(
            restored.len() == 2,
            "expected 2 entries, got {}",
            restored.len()
        );
        ensure!(
            restored.first().is_some_and(|e| e.track_id == t1),
            "track 1 not first"
        );
        ensure!(
            restored.get(1).is_some_and(|e| e.track_id == t2),
            "track 2 not second"
        );
        let (ctx_type, ctx_id) = context_at(&restored, 0);
        ensure!(
            ctx_type == Some("album") && ctx_id == Some(1),
            "context not preserved via trait"
        );
        Ok(())
    }
}
