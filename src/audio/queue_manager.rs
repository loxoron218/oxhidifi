//! Playback queue manager with auto-advance support.

use std::sync::Arc;

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::glib::MainContext,
    parking_lot::RwLock,
    tracing::debug,
};

use crate::{
    audio::{
        decoder::AudioFormat,
        engine::{AudioEngine, PlaybackState::Playing, TrackInfo},
        metadata::TagReader,
    },
    library::Track,
    state::{AppState, PlaybackQueue},
};

/// Internal queue control messages.
enum QueueControlMessage {
    /// Set a new queue.
    SetQueue(Vec<Track>),
    /// Navigate to next track.
    NextTrack,
    /// Navigate to previous track.
    PreviousTrack,
}

/// Playback queue manager with auto-advance support.
///
/// The `QueueManager` coordinates queue state, handles auto-advance when tracks finish,
/// and provides navigation methods for prev/next track playback.
pub struct QueueManager {
    /// Current playback queue state.
    queue: Arc<RwLock<PlaybackQueue>>,
    /// Audio engine reference for track loading/playback.
    audio_engine: Arc<AudioEngine>,
    /// Application state reference for broadcasting changes.
    app_state: Arc<AppState>,
    /// Sender for internal control messages.
    control_tx: Sender<QueueControlMessage>,
    /// Track completion event receiver from audio engine.
    track_finished_rx: Receiver<()>,
}

impl QueueManager {
    /// Creates a new queue manager.
    ///
    /// # Arguments
    ///
    /// * `audio_engine` - Audio engine reference for track loading/playback
    /// * `app_state` - Application state reference for broadcasting changes
    /// * `track_finished_rx` - Receiver for track completion events from audio engine
    ///
    /// # Returns
    ///
    /// A new `QueueManager` instance.
    #[must_use]
    pub fn new(
        audio_engine: Arc<AudioEngine>,
        app_state: Arc<AppState>,
        track_finished_rx: Receiver<()>,
    ) -> Self {
        let (control_tx, control_rx) = unbounded();
        let queue_manager = Self {
            queue: Arc::new(RwLock::new(PlaybackQueue::default())),
            audio_engine,
            app_state,
            control_tx,
            track_finished_rx,
        };

        queue_manager.start_control_loop(control_rx);
        queue_manager.start_auto_advance();

        queue_manager
    }

    /// Sets a new playback queue, replacing any existing queue.
    ///
    /// # Arguments
    ///
    /// * `tracks` - List of tracks to set as the new queue
    pub fn set_queue(&self, tracks: Vec<Track>) {
        if let Err(e) = self
            .control_tx
            .send_blocking(QueueControlMessage::SetQueue(tracks))
        {
            debug!("QueueManager: Failed to send SetQueue message: {e}");
        }
    }

    /// Navigates to the next track in the queue.
    pub fn next_track(&self) {
        if let Err(e) = self
            .control_tx
            .send_blocking(QueueControlMessage::NextTrack)
        {
            debug!("QueueManager: Failed to send NextTrack message: {e}");
        }
    }

    /// Navigates to the previous track in the queue.
    pub fn previous_track(&self) {
        if let Err(e) = self
            .control_tx
            .send_blocking(QueueControlMessage::PreviousTrack)
        {
            debug!("QueueManager: Failed to send PreviousTrack message: {e}");
        }
    }

    /// Gets the current queue state.
    ///
    /// # Returns
    ///
    /// A clone of the current `PlaybackQueue`.
    #[must_use]
    pub fn get_queue(&self) -> PlaybackQueue {
        self.queue.read().clone()
    }

    /// Starts the control loop processing queue commands.
    fn start_control_loop(&self, control_rx: Receiver<QueueControlMessage>) {
        let queue = self.queue.clone();
        let app_state = self.app_state.clone();
        let audio_engine = self.audio_engine.clone();

        MainContext::default().spawn_local(async move {
            while let Ok(msg) = control_rx.recv().await {
                match msg {
                    QueueControlMessage::SetQueue(tracks) => {
                        debug!(
                            "QueueManager: Setting new queue with {} tracks",
                            tracks.len()
                        );

                        let mut queue_state = queue.write();
                        queue_state.tracks = tracks;
                        queue_state.current_index = if queue_state.tracks.is_empty() {
                            None
                        } else {
                            Some(0)
                        };
                        drop(queue_state);

                        Self::broadcast_queue_change(&queue, &app_state);
                    }
                    QueueControlMessage::NextTrack => {
                        Self::handle_next_track(&queue, &app_state, &audio_engine).await;
                    }
                    QueueControlMessage::PreviousTrack => {
                        Self::handle_previous_track(&queue, &app_state, &audio_engine).await;
                    }
                }
            }
        });
    }

    /// Starts the auto-advance listener for track completion.
    fn start_auto_advance(&self) {
        let track_finished_rx = self.track_finished_rx.clone();
        debug!("QueueManager: Set up track finished receiver for auto-advance");

        let queue = self.queue.clone();
        let app_state = self.app_state.clone();
        let audio_engine = self.audio_engine.clone();

        MainContext::default().spawn_local(async move {
            while let Ok(()) = track_finished_rx.recv().await {
                debug!("QueueManager: Track finished event received, auto-advancing");
                Self::handle_next_track(&queue, &app_state, &audio_engine).await;
            }
        });
    }

    /// Handles navigation to the next track.
    async fn handle_next_track(
        queue: &Arc<RwLock<PlaybackQueue>>,
        app_state: &Arc<AppState>,
        audio_engine: &Arc<AudioEngine>,
    ) {
        let (next_track, new_index) = {
            let queue_state = queue.read();

            if queue_state.tracks.is_empty() {
                return;
            }

            let Some(idx) = queue_state.current_index else {
                return;
            };

            if idx + 1 >= queue_state.tracks.len() {
                debug!("QueueManager: At end of queue, no next track");
                return;
            }

            let next_idx = idx + 1;
            (queue_state.tracks[next_idx].clone(), next_idx)
        };

        Self::play_track_and_update_state(queue, app_state, audio_engine, &next_track, new_index)
            .await;
    }

    /// Loads and plays a track, updating queue and application state.
    async fn play_track_and_update_state(
        queue: &Arc<RwLock<PlaybackQueue>>,
        app_state: &Arc<AppState>,
        audio_engine: &Arc<AudioEngine>,
        track: &Track,
        new_index: usize,
    ) {
        if let Err(e) = audio_engine.load_track(&track.path) {
            debug!("QueueManager: Failed to load track: {e}");
            return;
        }

        if let Err(e) = audio_engine.play().await {
            debug!("QueueManager: Failed to play track: {e}");
            return;
        }

        queue.write().current_index = Some(new_index);
        app_state.update_playback_state(Playing);

        if let Ok(metadata) = TagReader::read_metadata(&track.path) {
            let track_info = TrackInfo {
                path: track.path.clone(),
                metadata,
                format: AudioFormat {
                    sample_rate: u32::try_from(track.sample_rate).unwrap_or(44100),
                    channels: u32::try_from(track.channels).unwrap_or(2),
                    bits_per_sample: u32::try_from(track.bits_per_sample).unwrap_or(16),
                    channel_mask: 0,
                },
                duration_ms: u64::try_from(track.duration_ms).unwrap_or(0),
            };
            app_state.update_current_track(Some(track_info));
        }

        Self::broadcast_queue_change(queue, app_state);
    }

    /// Handles navigation to the previous track.
    async fn handle_previous_track(
        queue: &Arc<RwLock<PlaybackQueue>>,
        app_state: &Arc<AppState>,
        audio_engine: &Arc<AudioEngine>,
    ) {
        let (prev_track, new_index) = {
            let queue_state = queue.read();

            if queue_state.tracks.is_empty() {
                return;
            }

            let Some(idx) = queue_state.current_index else {
                return;
            };

            if idx == 0 {
                debug!("QueueManager: At beginning of queue, no previous track");
                return;
            }

            let prev_idx = idx - 1;
            (queue_state.tracks[prev_idx].clone(), prev_idx)
        };

        Self::play_track_and_update_state(queue, app_state, audio_engine, &prev_track, new_index)
            .await;
    }

    /// Broadcasts queue state changes to subscribers.
    fn broadcast_queue_change(queue: &Arc<RwLock<PlaybackQueue>>, app_state: &Arc<AppState>) {
        let queue_clone = queue.read().clone();
        app_state.update_queue(queue_clone);
    }
}
