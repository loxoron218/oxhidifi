//! Integration tests for queue management functionality.
//!
//! This module contains integration tests that verify queue manager behavior
//! including auto-advance, state synchronization, and navigation.

#[cfg(test)]
mod tests {
    use std::{future::pending, sync::Arc};

    use {
        anyhow::{Result, bail},
        async_channel::unbounded,
        parking_lot::RwLock,
        tokio::time::{Duration, timeout},
    };

    use crate::{
        audio::{engine::AudioEngine, queue_manager::QueueManager},
        config::settings::SettingsManager,
        library::models::Track,
        state::app_state::{AppState, AppStateEvent, PlaybackQueue},
    };

    // Default timeout in milliseconds for test async operations
    const TEST_TIMEOUT_MS: u64 = 1000;

    fn create_test_tracks(count: usize) -> Vec<Track> {
        (0..count)
            .map(|i| {
                let id = i64::try_from(i).unwrap_or({
                    // Test count will never exceed i64::MAX in practice
                    i64::MAX
                });
                Track {
                    id,
                    album_id: 0,
                    title: format!("Track {i}"),
                    track_number: Some(id),
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
                }
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
            tracks,
            current_index: Some(0),
        };
        let cloned = queue;
        assert_eq!(cloned.tracks.len(), 2);
        assert_eq!(cloned.current_index, Some(0));
    }

    #[tokio::test]
    #[ignore = "Requires GTK display for UI testing"]
    async fn test_auto_advance_on_track_completion() -> Result<()> {
        let tracks = create_test_tracks(3);

        let (track_finished_tx, track_finished_rx) = unbounded();

        let engine = AudioEngine::new()?;
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new()?;
        let app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        let queue_manager = QueueManager::new(
            Arc::new(AudioEngine::new()?),
            Arc::new(app_state),
            track_finished_rx,
        );

        queue_manager.set_queue(tracks);

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(0) {
                pending::<()>().await;
            }
        })
        .await?;

        let initial_index = queue_manager.get_queue().current_index;
        if initial_index != Some(0) {
            bail!("Expected Some(0), got {initial_index:?}");
        }

        track_finished_tx.send(()).await?;

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(1) {
                pending::<()>().await;
            }
        })
        .await?;

        let updated_index = queue_manager.get_queue().current_index;
        if updated_index != Some(1) {
            bail!("Expected Some(1), got {updated_index:?}");
        }

        track_finished_tx.send(()).await?;

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(2) {
                pending::<()>().await;
            }
        })
        .await?;

        let final_index = queue_manager.get_queue().current_index;
        if final_index != Some(2) {
            bail!("Expected Some(2), got {final_index:?}");
        }

        track_finished_tx.send(()).await?;

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index == Some(2) {
                pending::<()>().await;
            }
        })
        .await?;

        let end_index = queue_manager.get_queue().current_index;
        if end_index != Some(2) {
            bail!("Expected Some(2) at queue end, got {end_index:?}");
        }
        Ok(())
    }

    #[tokio::test]
    #[ignore = "Requires GTK display for UI testing"]
    async fn test_queue_state_synchronization() -> Result<()> {
        let tracks = create_test_tracks(3);

        let (_, track_finished_rx) = unbounded();

        let engine = AudioEngine::new()?;
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new()?;
        let app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        let state_rx = app_state.subscribe();

        let queue_manager = QueueManager::new(
            Arc::new(AudioEngine::new()?),
            Arc::new(app_state.clone()),
            track_finished_rx,
        );

        queue_manager.set_queue(tracks);

        timeout(Duration::from_millis(200), async {
            loop {
                if let AppStateEvent::QueueChanged(queue) = state_rx.recv().await? {
                    if queue.tracks.len() != 3 {
                        bail!("Expected 3 tracks, got {}", queue.tracks.len());
                    }
                    if queue.current_index != Some(0) {
                        bail!("Expected Some(0), got {:?}", queue.current_index);
                    }
                    break;
                }
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;

        queue_manager.next_track();

        timeout(Duration::from_millis(200), async {
            loop {
                if let AppStateEvent::QueueChanged(queue) = state_rx.recv().await? {
                    if queue.current_index != Some(1) {
                        bail!("Expected Some(1), got {:?}", queue.current_index);
                    }
                    break;
                }
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;

        queue_manager.previous_track();

        timeout(Duration::from_millis(200), async {
            loop {
                if let AppStateEvent::QueueChanged(queue) = state_rx.recv().await? {
                    if queue.current_index != Some(0) {
                        bail!("Expected Some(0), got {:?}", queue.current_index);
                    }
                    break;
                }
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "Requires GTK display for UI testing"]
    async fn test_next_previous_button_state_updates() -> Result<()> {
        let tracks = create_test_tracks(3);

        let (_, track_finished_rx) = unbounded();

        let engine = AudioEngine::new()?;
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new()?;
        let app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        let queue_manager = QueueManager::new(
            Arc::new(AudioEngine::new()?),
            Arc::new(app_state),
            track_finished_rx,
        );

        queue_manager.set_queue(tracks);

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(0) {
                pending::<()>().await;
            }
        })
        .await?;

        let initial_queue = queue_manager.get_queue();
        if initial_queue.current_index != Some(0) {
            bail!("Expected Some(0), got {:?}", initial_queue.current_index);
        }
        if initial_queue.tracks.len() != 3 {
            bail!("Expected 3 tracks, got {}", initial_queue.tracks.len());
        }

        queue_manager.previous_track();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(0) {
                pending::<()>().await;
            }
        })
        .await?;

        let after_prev = queue_manager.get_queue();
        if after_prev.current_index != Some(0) {
            bail!("Expected Some(0), got {:?}", after_prev.current_index);
        }

        queue_manager.next_track();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(1) {
                pending::<()>().await;
            }
        })
        .await?;

        let after_next_1 = queue_manager.get_queue();
        if after_next_1.current_index != Some(1) {
            bail!("Expected Some(1), got {:?}", after_next_1.current_index);
        }

        queue_manager.next_track();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(2) {
                pending::<()>().await;
            }
        })
        .await?;

        let after_next_2 = queue_manager.get_queue();
        if after_next_2.current_index != Some(2) {
            bail!("Expected Some(2), got {:?}", after_next_2.current_index);
        }

        queue_manager.next_track();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(2) {
                pending::<()>().await;
            }
        })
        .await?;

        let at_end = queue_manager.get_queue();
        if at_end.current_index != Some(2) {
            bail!("Expected Some(2), got {:?}", at_end.current_index);
        }

        queue_manager.previous_track();

        timeout(Duration::from_millis(TEST_TIMEOUT_MS), async {
            while queue_manager.get_queue().current_index != Some(1) {
                pending::<()>().await;
            }
        })
        .await?;

        let final_queue = queue_manager.get_queue();
        if final_queue.current_index != Some(1) {
            bail!("Expected Some(1), got {:?}", final_queue.current_index);
        }
        Ok(())
    }
}
