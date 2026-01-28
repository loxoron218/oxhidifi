//! Audio playback engine orchestrator.
//!
//! This module provides the main `AudioEngine` that coordinates the audio
//! decoder, output, and playback state management for high-fidelity playback.

use std::{
    mem::drop,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::SeqCst},
    },
    thread::{JoinHandle, spawn},
};

use {
    async_channel::{Receiver, Sender, unbounded},
    cpal::Stream,
    libadwaita::glib::MainContext,
    parking_lot::{Mutex, RwLock},
    rtrb::RingBuffer,
    serde::{Deserialize, Serialize},
    thiserror::Error,
    tokio::{
        runtime::Builder,
        select,
        task::spawn_blocking,
        time::{Duration, timeout},
    },
    tracing::{debug, error, warn},
};

use crate::audio::{
    decoder::{AudioDecoder, AudioFormat, AudioProducer, DecoderError},
    metadata::{MetadataError, TagReader, TrackMetadata},
    output::{AudioConsumer, AudioOutput, OutputConfig, OutputError},
    resampler::ResamplingAudioConsumer,
};

/// Current playback state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlaybackState {
    /// No track is loaded or playing.
    Stopped,
    /// Track is loaded and ready to play.
    Ready,
    /// Track is currently playing.
    Playing,
    /// Track is paused.
    Paused,
    /// Buffering data before playback can start.
    Buffering,
}

/// Information about the currently loaded track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    /// Path to the audio file.
    pub path: String,
    /// Extracted metadata.
    pub metadata: TrackMetadata,
    /// Audio format information.
    pub format: AudioFormat,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// Error type for audio engine operations.
#[derive(Error, Debug)]
pub enum AudioError {
    /// Decoder error.
    #[error("Decoder error: {0}")]
    DecoderError(#[from] DecoderError),
    /// Output error.
    #[error("Output error: {0}")]
    OutputError(#[from] OutputError),
    /// Metadata error.
    #[error("Metadata error: {0}")]
    MetadataError(#[from] MetadataError),
    /// Invalid operation for current state.
    #[error("Invalid operation: {reason}")]
    InvalidOperation { reason: String },
    /// Track not found or not loaded.
    #[error("No track loaded")]
    NoTrackLoaded,
}

/// Main audio playback engine.
///
/// The `AudioEngine` orchestrates the entire audio playback pipeline,
/// managing the lifecycle of decoders, outputs, and playback state.
///
/// All fields are wrapped in `Arc<>` or implement `Clone` directly,
/// enabling safe cloning for concurrent access across the application.
#[derive(Clone)]
pub struct AudioEngine {
    /// Current playback state.
    state: Arc<RwLock<PlaybackState>>,
    /// Information about the currently loaded track.
    current_track: Arc<RwLock<Option<TrackInfo>>>,
    /// Audio output configuration.
    output_config: OutputConfig,
    /// Sender for track completion notifications.
    track_finished_tx: Sender<()>,
    /// Subscribers for track completion notifications.
    track_completion_subscribers: Arc<Mutex<Vec<Sender<()>>>>,
    /// Sender for state change notifications.
    state_tx: Sender<PlaybackState>,
    /// Subscribers for state change notifications.
    state_subscribers: Arc<Mutex<Vec<Sender<PlaybackState>>>>,
    /// Receiver for internal control messages.
    control_rx: Receiver<ControlMessage>,
    /// Sender for internal control messages.
    control_tx: Sender<ControlMessage>,
    /// Handle to the current audio stream (if any).
    stream_handle: Arc<RwLock<Option<StreamHandle>>>,
    /// Thread-safe playback position in milliseconds.
    current_position: Arc<AtomicU64>,
    /// Shutdown sender for track completion forwarding task.
    track_completion_shutdown_tx: Arc<Mutex<Option<Sender<()>>>>,
    /// Shutdown sender for state change forwarding task.
    state_change_shutdown_tx: Arc<Mutex<Option<Sender<()>>>>,
}

/// Internal control messages for the audio engine.
#[derive(Debug)]
enum ControlMessage {
    /// Start playback.
    Play,
    /// Pause playback.
    Pause,
    /// Resume playback.
    Resume,
    /// Stop playback.
    Stop,
    /// Seek to specified position in milliseconds.
    Seek(u64),
}

/// Handle to a running audio stream.
struct StreamHandle {
    /// The CPAL audio stream.
    stream: Stream,
    /// Join handle for the decoder thread.
    decoder_handle: Option<JoinHandle<Result<(), DecoderError>>>,
    /// Resampling consumer for graceful shutdown.
    resampling_consumer: Option<ResamplingAudioConsumer>,
}

/// Macro to spawn forwarding tasks for notification channels.
///
/// This macro eliminates boilerplate for spawning async tasks that:
/// 1. Receive messages from a source channel
/// 2. Forward them to all subscribers
/// 3. Handle shutdown signals gracefully
macro_rules! spawn_forwarding_task {
    ($subscribers:expr, $rx:expr, $shutdown_rx:expr, $task_name:expr) => {{
        let subscribers_clone = Arc::clone(&$subscribers);
        MainContext::default().spawn_local(async move {
            loop {
                select! {
                    result = $rx.recv() => {
                        match result {
                            Ok(msg) => {
                                for tx in subscribers_clone.lock().iter() {
                                    let _ = tx.try_send(msg.clone());
                                }
                            }
                            Err(_) => {
                                debug!("{} channel closed, exiting forwarding task", $task_name);
                                break;
                            }
                        }
                    }
                    _ = $shutdown_rx.recv() => {
                        debug!("{} forwarding task received shutdown signal", $task_name);
                        break;
                    }
                }
            }
        });
    }};
}

impl AudioEngine {
    /// Creates a new audio engine.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `AudioEngine` or an `AudioError`.
    ///
    /// # Errors
    ///
    /// Returns `AudioError` if initialization fails.
    pub fn new() -> Result<Self, AudioError> {
        let (track_finished_tx, track_finished_rx) = unbounded::<()>();
        let (state_tx, state_rx) = unbounded::<PlaybackState>();
        let (control_tx, control_rx) = unbounded();

        let (track_completion_shutdown_tx, track_completion_shutdown_rx) = unbounded::<()>();
        let (state_change_shutdown_tx, state_change_shutdown_rx) = unbounded::<()>();

        let track_completion_subscribers: Arc<Mutex<Vec<Sender<()>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let state_subscribers: Arc<Mutex<Vec<Sender<PlaybackState>>>> =
            Arc::new(Mutex::new(Vec::new()));

        // Start forwarding task for track completion notifications
        spawn_forwarding_task!(
            track_completion_subscribers,
            track_finished_rx,
            track_completion_shutdown_rx,
            "Track completion"
        );

        // Start forwarding task for state change notifications
        spawn_forwarding_task!(
            state_subscribers,
            state_rx,
            state_change_shutdown_rx,
            "State change"
        );

        let engine = AudioEngine {
            state: Arc::new(RwLock::new(PlaybackState::Stopped)),
            current_track: Arc::new(RwLock::new(None)),
            output_config: OutputConfig::default(),
            track_finished_tx,
            track_completion_subscribers,
            state_tx,
            state_subscribers,
            control_rx,
            control_tx,
            stream_handle: Arc::new(RwLock::new(None)),
            current_position: Arc::new(AtomicU64::new(0)),
            track_completion_shutdown_tx: Arc::new(Mutex::new(Some(track_completion_shutdown_tx))),
            state_change_shutdown_tx: Arc::new(Mutex::new(Some(state_change_shutdown_tx))),
        };

        // Start the control loop in a background thread
        let engine_clone = engine.clone();
        spawn(move || {
            engine_clone.control_loop();
        });

        Ok(engine)
    }

    /// Loads a track for playback without starting playback.
    ///
    /// # Arguments
    ///
    /// * `track_path` - Path to the audio file.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `AudioError` if the track cannot be loaded or metadata extracted.
    pub fn load_track<P: AsRef<Path>>(&self, track_path: P) -> Result<(), AudioError> {
        let path = track_path.as_ref();

        // Extract metadata
        let metadata = TagReader::read_metadata(path)?;

        // Create decoder to get format info
        let decoder = AudioDecoder::new(path)?;
        let duration_ms = decoder
            .duration_ms()
            .unwrap_or(metadata.technical.duration_ms);

        let track_info = TrackInfo {
            path: path.to_string_lossy().to_string(),
            metadata,
            format: decoder.format.clone(),
            duration_ms,
        };

        *self.current_track.write() = Some(track_info);
        *self.state.write() = PlaybackState::Ready;
        self.notify_state_change();

        Ok(())
    }

    /// Starts playback of the currently loaded track.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `AudioError` if no track is loaded or playback fails to start.
    pub async fn play(&self) -> Result<(), AudioError> {
        if self.current_track.read().is_none() {
            return Err(AudioError::NoTrackLoaded);
        }

        self.control_tx
            .send(ControlMessage::Play)
            .await
            .map_err(|e| AudioError::InvalidOperation {
                reason: format!("Failed to send play command: {e}"),
            })?;

        Ok(())
    }

    /// Pauses the current playback.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `AudioError` if no track is playing.
    pub async fn pause(&self) -> Result<(), AudioError> {
        let state = self.state.read().clone();
        if matches!(state, PlaybackState::Stopped | PlaybackState::Ready) {
            return Err(AudioError::InvalidOperation {
                reason: "Cannot pause when not playing".to_string(),
            });
        }

        self.control_tx
            .send(ControlMessage::Pause)
            .await
            .map_err(|e| AudioError::InvalidOperation {
                reason: format!("Failed to send pause command: {e}"),
            })?;

        Ok(())
    }

    /// Resumes playback after pausing.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `AudioError` if no track is paused.
    pub async fn resume(&self) -> Result<(), AudioError> {
        let state = self.state.read().clone();
        if !matches!(state, PlaybackState::Paused) {
            return Err(AudioError::InvalidOperation {
                reason: "Cannot resume when not paused".to_string(),
            });
        }

        self.control_tx
            .send(ControlMessage::Resume)
            .await
            .map_err(|e| AudioError::InvalidOperation {
                reason: format!("Failed to send resume command: {e}"),
            })?;

        Ok(())
    }

    /// Stops playback and unloads the current track.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `AudioError::InvalidOperation` if the stop command cannot be sent.
    pub async fn stop(&self) -> Result<(), AudioError> {
        self.control_tx
            .send(ControlMessage::Stop)
            .await
            .map_err(|e| AudioError::InvalidOperation {
                reason: format!("Failed to send stop command: {e}"),
            })?;

        Ok(())
    }

    /// Seeks to the specified position in the current track.
    ///
    /// # Arguments
    ///
    /// * `position_ms` - Target position in milliseconds.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `AudioError` if no track is loaded or seeking fails.
    pub async fn seek(&self, position_ms: u64) -> Result<(), AudioError> {
        if self.current_track.read().is_none() {
            return Err(AudioError::NoTrackLoaded);
        }

        self.control_tx
            .send(ControlMessage::Seek(position_ms))
            .await
            .map_err(|e| AudioError::InvalidOperation {
                reason: format!("Failed to send seek command: {e}"),
            })?;

        Ok(())
    }

    /// Gets the current playback state.
    ///
    /// # Returns
    ///
    /// The current `PlaybackState`.
    #[must_use]
    pub fn current_playback_state(&self) -> PlaybackState {
        self.state.read().clone()
    }

    /// Gets information about the currently loaded track.
    ///
    /// # Returns
    ///
    /// An `Option` containing the `TrackInfo` if a track is loaded.
    #[must_use]
    pub fn current_track_info(&self) -> Option<TrackInfo> {
        self.current_track.read().clone()
    }

    /// Gets the current playback position in milliseconds.
    ///
    /// # Returns
    ///
    /// The current position in milliseconds, or None if no track is loaded.
    #[must_use]
    pub fn current_position(&self) -> Option<u64> {
        if self.current_track.read().is_some() {
            Some(self.current_position.load(SeqCst))
        } else {
            None
        }
    }

    /// Subscribes to playback state changes.
    ///
    /// # Returns
    ///
    /// An `async_channel::Receiver<PlaybackState>` that receives state updates,
    /// including the current state immediately upon subscription.
    #[must_use]
    pub fn subscribe_to_state_changes(&self) -> Receiver<PlaybackState> {
        let (tx, rx) = unbounded();

        // Send current state immediately
        let current_state = self.state.read().clone();
        let _ = tx.try_send(current_state);

        // Add to subscribers list
        self.state_subscribers.lock().push(tx);

        rx
    }

    /// Subscribes to track completion events.
    ///
    /// # Returns
    ///
    /// A `Receiver` that receives notification when a track finishes.
    #[must_use]
    pub fn subscribe_to_track_completion(&self) -> Receiver<()> {
        let (tx, rx) = unbounded();

        // Add to subscribers list
        self.track_completion_subscribers.lock().push(tx);

        rx
    }

    /// Main control loop that processes commands and manages playback.
    fn control_loop(&self) {
        Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                while let Ok(message) = self.control_rx.recv().await {
                    match message {
                        ControlMessage::Play => {
                            if let Err(e) = self.handle_play().await {
                                error!("Failed to handle play command: {e}");
                            }
                        }
                        ControlMessage::Pause => {
                            self.handle_pause().await;
                        }
                        ControlMessage::Resume => {
                            if let Err(e) = self.handle_resume().await {
                                error!("Failed to handle resume command: {e}");
                            }
                        }
                        ControlMessage::Stop => {
                            self.handle_stop().await;
                        }
                        ControlMessage::Seek(position_ms) => {
                            if let Err(e) = self.handle_seek(position_ms).await {
                                error!("Failed to handle seek command: {e}");
                            }
                        }
                    }
                }
            });
    }

    /// Sets up playback stream with optional initial position.
    ///
    /// # Arguments
    ///
    /// * `initial_position_ms` - Optional position to seek to in milliseconds.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    async fn setup_playback_stream(
        &self,
        initial_position_ms: Option<u64>,
    ) -> Result<(), AudioError> {
        let track_info = self
            .current_track
            .read()
            .clone()
            .ok_or(AudioError::NoTrackLoaded)?;

        // Stop any existing playback
        self.stop_stream().await;

        // Reset current position
        if let Some(pos) = initial_position_ms {
            self.current_position.store(pos, SeqCst);
        } else {
            self.current_position.store(0, SeqCst);
        }

        // Create ring buffer for audio samples
        // Buffer size must be power of 2 for rtrb ring buffer efficient bitmask wrapping
        let buffer_size = 4096;
        let (producer, consumer) = RingBuffer::<f32>::new(buffer_size);

        // Create audio output
        let output = AudioOutput::new(Some(self.output_config.clone()))?;

        // Create decoder to get signal spec
        let mut decoder = AudioDecoder::new(&track_info.path)?;
        if let Some(pos) = initial_position_ms {
            decoder.seek(pos)?;
        }
        let signal_spec = decoder.signal_spec;

        // Create audio consumer
        let consumer = AudioConsumer::new(
            output,
            consumer,
            &track_info.format,
            &signal_spec,
            self.current_position.clone(),
        )?;

        // Create audio producer
        let track_finished_tx = self.track_finished_tx.clone();
        let producer = AudioProducer::new(decoder, producer, Some(track_finished_tx));

        // Start decoder thread
        let decoder_handle = spawn(move || producer.run());

        // Start audio stream
        let (stream, resampling_consumer) = consumer.run(&track_info.format, &signal_spec)?;

        // Store stream handle
        *self.stream_handle.write() = Some(StreamHandle {
            stream,
            decoder_handle: Some(decoder_handle),
            resampling_consumer,
        });

        *self.state.write() = PlaybackState::Playing;
        self.notify_state_change();

        Ok(())
    }

    /// Handles the play command.
    async fn handle_play(&self) -> Result<(), AudioError> {
        self.setup_playback_stream(None).await
    }

    /// Handles the resume command.
    async fn handle_resume(&self) -> Result<(), AudioError> {
        let position_ms = self.current_position.load(SeqCst);
        self.setup_playback_stream(Some(position_ms)).await
    }

    /// Handles the pause command.
    async fn handle_pause(&self) {
        // For now, we'll just stop the stream and keep the track loaded
        // In a more sophisticated implementation, we'd actually pause the stream
        self.stop_stream().await;

        *self.state.write() = PlaybackState::Paused;
        self.notify_state_change();
    }

    /// Handles the stop command.
    async fn handle_stop(&self) {
        self.stop_stream().await;

        *self.state.write() = PlaybackState::Stopped;
        *self.current_track.write() = None;
        self.notify_state_change();
    }

    /// Handles the seek command.
    async fn handle_seek(&self, position_ms: u64) -> Result<(), AudioError> {
        // For now, we'll stop and restart playback at the new position
        // A more sophisticated implementation would seek within the current stream
        let current_state = self.state.read().clone();
        let was_playing = matches!(current_state, PlaybackState::Playing);

        self.stop_stream().await;

        if let Some(track_info) = self.current_track.read().clone() {
            // Create new decoder and seek
            let mut decoder = AudioDecoder::new(&track_info.path)?;
            decoder.seek(position_ms)?;

            // Update current position
            self.current_position.store(position_ms, SeqCst);

            let signal_spec = decoder.signal_spec;

            // Recreate the playback stream
            // Buffer size must be power of 2 for rtrb ring buffer efficient bitmask wrapping
            let buffer_size = 4096;
            let (producer, consumer) = RingBuffer::<f32>::new(buffer_size);

            let output = AudioOutput::new(Some(self.output_config.clone()))?;
            let consumer = AudioConsumer::new(
                output,
                consumer,
                &track_info.format,
                &signal_spec,
                self.current_position.clone(),
            )?;
            let track_finished_tx = self.track_finished_tx.clone();
            let producer = AudioProducer::new(decoder, producer, Some(track_finished_tx));

            let decoder_handle = spawn(move || producer.run());

            let (stream, resampling_consumer) = consumer.run(&track_info.format, &signal_spec)?;

            *self.stream_handle.write() = Some(StreamHandle {
                stream,
                decoder_handle: Some(decoder_handle),
                resampling_consumer,
            });

            *self.state.write() = if was_playing {
                PlaybackState::Playing
            } else {
                PlaybackState::Paused
            };
            self.notify_state_change();
        }

        Ok(())
    }

    /// Stops the current audio stream gracefully.
    async fn stop_stream(&self) {
        let handle_opt = {
            let mut guard = self.stream_handle.write();
            guard.take()
        };

        if let Some(mut handle) = handle_opt {
            debug!("Stopping audio stream");

            let resampling_consumer = handle.resampling_consumer.take();
            let stream = handle.stream;
            let decoder_handle = handle.decoder_handle.take();

            if let Some(mut resampling_consumer) = resampling_consumer {
                resampling_consumer.stop();
            }

            drop(stream);

            if let Some(decoder_handle) = decoder_handle {
                match timeout(
                    Duration::from_secs(2),
                    spawn_blocking(move || decoder_handle.join()),
                )
                .await
                {
                    Ok(Ok(Ok(Ok(())))) => debug!("Decoder thread stopped successfully"),
                    Ok(Ok(Ok(Err(DecoderError::IoError(e))))) => {
                        error!("Decoder thread stopped with IO error: {:?}", e);
                    }
                    Ok(Ok(Ok(Err(e)))) => error!("Decoder thread stopped with error: {:?}", e),
                    Ok(Ok(Err(e))) => error!("Decoder thread panicked: {:?}", e),
                    Ok(Err(e)) => error!("Failed to spawn blocking task: {:?}", e),
                    Err(_) => error!("Timeout waiting for decoder thread to stop"),
                }
            }
        }
    }

    /// Notifies subscribers of state changes.
    fn notify_state_change(&self) {
        let state = self.state.read().clone();
        if let Err(e) = self.state_tx.try_send(state) {
            warn!("AudioEngine: Failed to send state change notification: {e}");
        }
    }

    /// Shuts down the audio engine and terminates all background tasks.
    ///
    /// This method stops the forwarding tasks for track completion and state change
    /// notifications by sending shutdown signals.
    pub fn shutdown(&self) {
        debug!("Shutting down audio engine");

        // Send shutdown signals to forwarding tasks by dropping the shutdown senders
        drop(self.track_completion_shutdown_tx.lock().take());
        drop(self.state_change_shutdown_tx.lock().take());

        // Close the main channels to stop any remaining operations
        drop(self.track_finished_tx.clone());
        drop(self.state_tx.clone());
        drop(self.control_tx.clone());
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{from_str, to_string};

    use crate::{audio::engine::PlaybackState, error::AudioError};

    #[test]
    fn test_playback_state_serialization() {
        let states = vec![
            PlaybackState::Stopped,
            PlaybackState::Ready,
            PlaybackState::Playing,
            PlaybackState::Paused,
            PlaybackState::Buffering,
        ];

        for state in states {
            let serialized = to_string(&state).unwrap();
            let deserialized: PlaybackState = from_str(&serialized).unwrap();
            assert_eq!(state, deserialized);
        }
    }

    #[test]
    fn test_audio_error_display() {
        let no_track_error = AudioError::NoTrackLoaded;
        assert_eq!(no_track_error.to_string(), "No track loaded");

        let invalid_op_error = AudioError::InvalidOperation {
            reason: "test reason".to_string(),
        };
        assert_eq!(
            invalid_op_error.to_string(),
            "Invalid operation: test reason"
        );
    }
}
