//! Integration tests for queue management functionality.
//!
//! This module contains integration tests that verify queue manager behavior
//! including auto-advance, state synchronization, and navigation.

#[cfg(test)]
mod tests {
    use std::{future::pending, sync::Arc};

    use {
        async_channel::unbounded,
        parking_lot::RwLock,
        tokio::time::{Duration, timeout},
    };

    use crate::{
        audio::{engine::AudioEngine, queue_manager::QueueManager},
        config::SettingsManager,
        library::Track,
        state::{AppState, AppStateEvent, PlaybackQueue},
    };

    // Default timeout in milliseconds for test async operations
    const TEST_TIMEOUT_MS: u64 = 1000;

    fn create_test_tracks(count: usize) -> Vec<Track> {
        (0..count)
            .map(|i| Track {
                id: i64::try_from(i).unwrap(),
                album_id: 0,
                title: format!("Track {i}"),
                track_number: Some(i64::try_from(i).unwrap()),
                disc_number: 1,
                duration_ms: 300_000,
                path: format!("/path/to/track_{i}.flac"),
                file_size: 1024,
                format: "FLAC".to_string(),
                codec: "FLAC".to_string(),
                sample_rate: 96000,
                bits_per_sample: 24,
                channels: 2,
                is_lossless: true,
                is_high_resolution: true,
                created_at: None,
                updated_at: None,
            })
            .collect()
    }

    #[test]
    fn test_playback_queue_default() {
        let queue = PlaybackQueue::default();
        assert!(queue.tracks.is_empty());
        assert!(queue.current_index.is_none());
    }

    #[test]
    fn test_playback_queue_cloned() {
        let tracks = vec![Track::default(), Track::default()];
        let queue = PlaybackQueue {
            tracks: tracks.clone(),
            current_index: Some(0),
        };
        let cloned = queue.clone();
        assert_eq!(cloned.tracks.len(), 2);
        assert_eq!(cloned.current_index, Some(0));
    }

    #[tokio::test]
    #[ignore = "Requires GTK display for UI testing"]
    async fn test_auto_advance_on_track_completion() {
        let tracks = create_test_tracks(3);

        let (track_finished_tx, track_finished_rx) = unbounded();

        let engine = AudioEngine::new().unwrap();
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new().unwrap();
        let app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        let queue_manager = QueueManager::new(
            Arc::new(AudioEngine::new().unwrap()),
            Arc::new(app_state),
            track_finished_rx,
        );

        queue_manager.set_queue(tracks);

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(0) {
                pending::<()>().await;
            }
        })
        .await
        .expect("Initial queue state not set");

        let initial_index = queue_manager.get_queue().current_index;
        assert_eq!(initial_index, Some(0), "Initial track index should be 0");

        track_finished_tx.send(()).await.unwrap();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(1) {
                pending::<()>().await;
            }
        })
        .await
        .expect("Queue did not auto-advance after track completion");

        let updated_index = queue_manager.get_queue().current_index;
        assert_eq!(updated_index, Some(1), "Queue should advance to next track");

        track_finished_tx.send(()).await.unwrap();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(2) {
                pending::<()>().await;
            }
        })
        .await
        .expect("Queue did not auto-advance to third track");

        let final_index = queue_manager.get_queue().current_index;
        assert_eq!(final_index, Some(2), "Queue should advance to third track");

        track_finished_tx.send(()).await.unwrap();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index == Some(2) {
                pending::<()>().await;
            }
        })
        .await
        .expect("Queue state did not change after end of queue");

        let end_index = queue_manager.get_queue().current_index;
        assert_eq!(
            end_index,
            Some(2),
            "Queue should not advance beyond last track"
        );
    }

    #[tokio::test]
    #[ignore = "Requires GTK display for UI testing"]
    async fn test_queue_state_synchronization() {
        let tracks = create_test_tracks(3);

        let (_, track_finished_rx) = unbounded();

        let engine = AudioEngine::new().unwrap();
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new().unwrap();
        let app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        let state_rx = app_state.subscribe();

        let queue_manager = QueueManager::new(
            Arc::new(AudioEngine::new().unwrap()),
            Arc::new(app_state.clone()),
            track_finished_rx,
        );

        queue_manager.set_queue(tracks);

        timeout(Duration::from_millis(200), async {
            loop {
                if let AppStateEvent::QueueChanged(queue) =
                    state_rx.recv().await.expect("State receiver closed")
                {
                    assert_eq!(queue.tracks.len(), 3);
                    assert_eq!(queue.current_index, Some(0));
                    break;
                }
            }
        })
        .await
        .expect("Did not receive queue changed event");

        queue_manager.next_track();

        timeout(Duration::from_millis(200), async {
            loop {
                if let AppStateEvent::QueueChanged(queue) =
                    state_rx.recv().await.expect("State receiver closed")
                {
                    assert_eq!(queue.current_index, Some(1));
                    break;
                }
            }
        })
        .await
        .expect("Did not receive next track queue change");

        queue_manager.previous_track();

        timeout(Duration::from_millis(200), async {
            loop {
                if let AppStateEvent::QueueChanged(queue) =
                    state_rx.recv().await.expect("State receiver closed")
                {
                    assert_eq!(queue.current_index, Some(0));
                    break;
                }
            }
        })
        .await
        .expect("Did not receive previous track queue change");
    }

    #[tokio::test]
    #[ignore = "Requires GTK display for UI testing"]
    async fn test_next_previous_button_state_updates() {
        let tracks = create_test_tracks(3);

        let (_, track_finished_rx) = unbounded();

        let engine = AudioEngine::new().unwrap();
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new().unwrap();
        let app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        let queue_manager = QueueManager::new(
            Arc::new(AudioEngine::new().unwrap()),
            Arc::new(app_state),
            track_finished_rx,
        );

        queue_manager.set_queue(tracks);

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(0) {
                pending::<()>().await;
            }
        })
        .await
        .expect("Initial queue state not set");

        let initial_queue = queue_manager.get_queue();
        assert_eq!(initial_queue.current_index, Some(0));
        assert_eq!(initial_queue.tracks.len(), 3);

        queue_manager.previous_track();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(0) {
                pending::<()>().await;
            }
        })
        .await
        .expect("Queue state did not stabilize");

        let after_prev = queue_manager.get_queue();
        assert_eq!(
            after_prev.current_index,
            Some(0),
            "Previous track at start should not change index"
        );

        queue_manager.next_track();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(1) {
                pending::<()>().await;
            }
        })
        .await
        .expect("Queue did not advance to next track");

        let after_next_1 = queue_manager.get_queue();
        assert_eq!(after_next_1.current_index, Some(1));

        queue_manager.next_track();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(2) {
                pending::<()>().await;
            }
        })
        .await
        .expect("Queue did not advance to last track");

        let after_next_2 = queue_manager.get_queue();
        assert_eq!(after_next_2.current_index, Some(2));

        queue_manager.next_track();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(2) {
                pending::<()>().await;
            }
        })
        .await
        .expect("Queue state did not stabilize after end");

        let at_end = queue_manager.get_queue();
        assert_eq!(
            at_end.current_index,
            Some(2),
            "Next track at end should not change index"
        );

        queue_manager.previous_track();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(1) {
                pending::<()>().await;
            }
        })
        .await
        .expect("Queue did not go back to previous track");

        let final_queue = queue_manager.get_queue();
        assert_eq!(final_queue.current_index, Some(1));
    }
}
