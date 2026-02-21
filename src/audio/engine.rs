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
    cpal::{Stream, traits::StreamTrait},
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
    decoder::AudioDecoder,
    decoder_types::{AudioFormat, DecoderError},
    metadata::{MetadataError, TagReader, TrackMetadata},
    output::{
        AudioConsumer, AudioOutput, OutputConfig,
        OutputError::{self, RingBufferError},
    },
    prebuffer::{Prebuffer, PrebufferError},
    producer::AudioProducer,
    resampler::ResamplingAudioConsumer,
};

/// Type alias for the error callback function.
type ErrorCallbackFn = Box<dyn Fn(String) + Send + Sync>;

/// Type alias for the thread-safe error callback wrapper.
type ErrorCallback = Arc<RwLock<Option<ErrorCallbackFn>>>;

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
    /// Pre-buffer error.
    #[error("Pre-buffer error: {0}")]
    PrebufferError(#[from] PrebufferError),
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
    output_config: Arc<RwLock<OutputConfig>>,
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
    /// Pre-buffer manager for gapless playback.
    prebuffer: Arc<RwLock<Option<Arc<Prebuffer>>>>,
    /// Error callback for reporting playback errors to UI.
    error_callback: ErrorCallback,
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
    /// Seek to position and start playback (used for restart after config changes).
    SeekAndPlay(u64),
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
                                    if let Err(e) = tx.try_send(msg.clone()) {
                                        error!(task_name = %$task_name, error = %e, "Failed to send message to subscriber");
                                    }
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

        let default_config = OutputConfig::default();

        let engine = Self {
            state: Arc::new(RwLock::new(PlaybackState::Stopped)),
            current_track: Arc::new(RwLock::new(None)),
            output_config: Arc::new(RwLock::new(default_config)),
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
            prebuffer: Arc::new(RwLock::new(None)),
            error_callback: ErrorCallback::new(RwLock::new(None)),
        };

        // Start the control loop in a background thread
        let engine_clone = engine.clone();
        spawn(move || {
            if let Err(e) = engine_clone.control_loop() {
                error!(error = %e, "Control loop error");
            }
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
            format: decoder.format,
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
        self.current_track
            .read()
            .is_some()
            .then(|| self.current_position.load(SeqCst))
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
        if let Err(e) = tx.try_send(current_state) {
            error!(error = %e, "Failed to send initial state to subscriber");
        }

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

    /// Gets the current output configuration.
    ///
    /// # Returns
    ///
    /// A clone of the current `OutputConfig`.
    #[must_use]
    pub fn output_config(&self) -> OutputConfig {
        self.output_config.read().clone()
    }

    /// Updates the output configuration with new settings.
    ///
    /// # Arguments
    ///
    /// * `new_config` - New output configuration to apply.
    pub fn update_output_config(&self, new_config: OutputConfig) {
        *self.output_config.write() = new_config;
    }

    /// Restarts playback with the current output configuration.
    ///
    /// This method stops the current playback and restarts it at the same
    /// position with the updated output configuration. Used when audio settings
    /// (such as exclusive mode) have changed.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `AudioError::NoTrackLoaded` if no track is currently loaded.
    /// Returns `AudioError::OutputError` if restart fails due to device issues.
    pub async fn restart_playback(&self) -> Result<(), AudioError> {
        let position_ms = self.current_position.load(SeqCst);

        if self.current_track.read().is_none() {
            return Err(AudioError::NoTrackLoaded);
        }

        self.control_tx
            .send(ControlMessage::SeekAndPlay(position_ms))
            .await
            .map_err(|e| AudioError::InvalidOperation {
                reason: format!("Failed to restart playback: {e}"),
            })?;

        Ok(())
    }

    /// Checks if the prebuffer has a track ready for gapless playback.
    ///
    /// # Returns
    ///
    /// `true` if the prebuffer is active and has a track ready.
    #[must_use]
    pub fn is_prebuffer_active(&self) -> bool {
        self.prebuffer
            .read()
            .as_ref()
            .is_some_and(|pb| pb.is_ready())
    }

    /// Sets the error callback for reporting playback errors to the UI.
    ///
    /// # Arguments
    ///
    /// * `callback` - A function that will be called with error messages.
    pub fn set_error_callback<F>(&self, callback: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        *self.error_callback.write() = Some(Box::new(callback));
    }

    /// Reports an error through the error callback if one is set.
    ///
    /// # Arguments
    ///
    /// * `error_message` - The error message to report.
    ///
    /// # Returns
    ///
    /// Always returns `()`; errors are silently ignored if no callback is set.
    fn report_error(&self, error_message: String) {
        if let Some(callback) = &*self.error_callback.read() {
            callback(error_message);
        }
    }

    /// Handles playback errors with consistent recovery.
    ///
    /// Reports all playback errors to the UI for user feedback, then cleans
    /// up the stream and resets the playback state.
    ///
    /// # Arguments
    ///
    /// * `error` - The error that occurred.
    async fn handle_playback_error(&self, error: AudioError) {
        error!(error = %error, "Playback error");

        // Report all errors to the UI for user feedback
        self.report_error(error.to_string());

        // Clean up stream on any failure to prevent state inconsistency
        self.stop_stream().await;
        *self.state.write() = PlaybackState::Ready;
        self.notify_state_change();
    }

    /// Main control loop that processes commands and manages playback.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure of the control loop.
    ///
    /// # Errors
    ///
    /// Returns an error if the tokio runtime cannot be created or if any
    /// playback operation fails.
    fn control_loop(&self) -> Result<(), AudioError> {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| AudioError::InvalidOperation {
                reason: format!("Failed to create tokio runtime: {e}"),
            })?;
        runtime.block_on(async {
            while let Ok(message) = self.control_rx.recv().await {
                match message {
                    ControlMessage::Play => {
                        if let Err(e) = self.handle_play().await {
                            self.handle_playback_error(e).await;
                        }
                    }
                    ControlMessage::SeekAndPlay(position_ms) => {
                        if let Err(e) = self.handle_seek_and_play(position_ms).await {
                            self.handle_playback_error(e).await;
                        }
                    }
                    ControlMessage::Pause => {
                        self.handle_pause().await;
                    }
                    ControlMessage::Resume => {
                        if let Err(e) = self.handle_resume().await {
                            error!(error = %e, "Failed to handle resume command");
                        }
                    }
                    ControlMessage::Stop => {
                        self.handle_stop().await;
                    }
                    ControlMessage::Seek(position_ms) => {
                        if let Err(e) = self.handle_seek(position_ms).await {
                            error!(error = %e, "Failed to handle seek command");
                        }
                    }
                }
            }

            Ok(())
        })
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
    ///
    /// # Errors
    ///
    /// Returns an error if no track is loaded, if the audio output cannot be
    /// created, or if the decoder fails.
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

        // Get current output configuration for stream creation
        let current_config = self.output_config.read().clone();

        // Create ring buffer for audio samples
        // Buffer size must be power of 2 for rtrb ring buffer efficient bitmask wrapping
        // Use larger buffer to handle resampling lag, rate mismatches, and packet bursts
        let buffer_size = current_config.buffer_config.main_buffer_size;
        let (producer, consumer) = RingBuffer::<f32>::new(buffer_size);
        let buffer_capacity = buffer_size;

        // Create audio output
        let output = AudioOutput::new(Some(current_config))?;

        // Create decoder to get signal spec
        let mut decoder = AudioDecoder::new(&track_info.path)?;
        if let Some(pos) = initial_position_ms {
            decoder.seek(pos)?;
        }
        let signal_spec = decoder.signal_spec;

        // Create audio consumer
        let (consumer, target_output_config) = AudioConsumer::new(
            output,
            consumer,
            &track_info.format,
            &signal_spec,
            self.current_position.clone(),
        )?;

        *self.output_config.write() = target_output_config;

        // Create audio producer
        let track_finished_tx = self.track_finished_tx.clone();
        let producer =
            AudioProducer::new(decoder, producer, buffer_capacity, Some(track_finished_tx));

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
    ///
    /// # Errors
    ///
    /// Returns an error if the playback stream cannot be set up.
    async fn handle_play(&self) -> Result<(), AudioError> {
        self.setup_playback_stream(None).await
    }

    /// Handles the resume command.
    ///
    /// # Errors
    ///
    /// Returns an error if the playback stream cannot be set up at the
    /// current position.
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

    /// Recreates the playback stream at the specified position.
    ///
    /// # Arguments
    ///
    /// * `position_ms` - Position to seek to in milliseconds
    /// * `always_playing` - If true, always set state to Playing; otherwise preserve previous state
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns an error if the decoder cannot be created, if seeking fails,
    /// or if the audio output cannot be created.
    async fn recreate_stream_at_position(
        &self,
        position_ms: u64,
        always_playing: bool,
    ) -> Result<(), AudioError> {
        self.stop_stream().await;

        if let Some(track_info) = self.current_track.read().clone() {
            // Create new decoder and seek
            let mut decoder = AudioDecoder::new(&track_info.path)?;
            decoder.seek(position_ms)?;

            // Update current position
            self.current_position.store(position_ms, SeqCst);

            let signal_spec = decoder.signal_spec;

            let config_for_output = self.output_config.read().clone();

            // Recreate the playback stream
            // Buffer size must be power of 2 for rtrb ring buffer efficient bitmask wrapping
            // Use larger buffer to handle resampling lag, rate mismatches, and packet bursts
            let buffer_size = config_for_output.buffer_config.main_buffer_size;
            let (producer, consumer) = RingBuffer::<f32>::new(buffer_size);
            let buffer_capacity = buffer_size;

            let output = AudioOutput::new(Some(config_for_output))?;
            let (consumer, target_output_config) = AudioConsumer::new(
                output,
                consumer,
                &track_info.format,
                &signal_spec,
                self.current_position.clone(),
            )?;

            *self.output_config.write() = target_output_config;

            let track_finished_tx = self.track_finished_tx.clone();
            let producer =
                AudioProducer::new(decoder, producer, buffer_capacity, Some(track_finished_tx));

            let decoder_handle = spawn(move || producer.run());

            let (stream, resampling_consumer) = consumer.run(&track_info.format, &signal_spec)?;

            if !always_playing {
                stream.pause().map_err(|e| AudioError::InvalidOperation {
                    reason: format!("Failed to pause stream: {e}"),
                })?;
            }

            *self.stream_handle.write() = Some(StreamHandle {
                stream,
                decoder_handle: Some(decoder_handle),
                resampling_consumer,
            });

            *self.state.write() = if always_playing {
                PlaybackState::Playing
            } else {
                PlaybackState::Paused
            };
            self.notify_state_change();
        }

        Ok(())
    }

    /// Handles the seek command.
    ///
    /// # Errors
    ///
    /// Returns an error if the playback stream cannot be recreated at the
    /// specified position.
    async fn handle_seek(&self, position_ms: u64) -> Result<(), AudioError> {
        // For now, we'll stop and restart playback at the new position
        // A more sophisticated implementation would seek within the current stream
        let current_state = self.state.read().clone();
        let was_playing = matches!(current_state, PlaybackState::Playing);

        self.recreate_stream_at_position(position_ms, was_playing)
            .await
    }

    /// Handles the seek and play command (used for restart after config changes).
    ///
    /// # Errors
    ///
    /// Returns an error if the playback stream cannot be recreated at the
    /// specified position.
    async fn handle_seek_and_play(&self, position_ms: u64) -> Result<(), AudioError> {
        self.recreate_stream_at_position(position_ms, true).await
    }

    /// Stops the current audio stream gracefully.
    async fn stop_stream(&self) {
        let handle_opt = {
            let mut guard = self.stream_handle.write();
            guard.take()
        };

        if let Some(mut handle) = handle_opt {
            debug!("AudioEngine: Stopping audio stream");

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

    /// Preloads the next track for gapless playback.
    ///
    /// This method initiates pre-buffering of the specified track to enable
    /// seamless transitions when the current track finishes.
    ///
    /// # Arguments
    ///
    /// * `track_path` - Path to the next track to preload.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `AudioError` if the track cannot be loaded or pre-buffering fails.
    ///
    /// # Panics
    ///
    /// Panics if the prebuffer cannot be obtained (this indicates a bug in initialization).
    pub fn preload_next_track<P: AsRef<Path>>(&self, track_path: P) -> Result<(), AudioError> {
        let path = track_path.as_ref();

        // Get or create prebuffer
        let mut prebuffer_guard = self.prebuffer.write();
        if prebuffer_guard.is_none() {
            *prebuffer_guard = Some(Arc::new(Prebuffer::new()));
        }

        let Some(prebuffer) = prebuffer_guard.as_ref().cloned() else {
            return Err(AudioError::OutputError(RingBufferError(
                "Prebuffer initialization failed".to_string(),
            )));
        };
        drop(prebuffer_guard);

        // Preload the track
        prebuffer.preload_track(path)?;

        debug!("Prebuffer: Preloaded next track for gapless playback");

        Ok(())
    }

    /// Checks if the next track is pre-buffered and ready for playback.
    ///
    /// # Returns
    ///
    /// `true` if a track is pre-buffered and ready, `false` otherwise.
    #[must_use]
    pub fn is_next_track_ready(&self) -> bool {
        self.prebuffer
            .read()
            .as_ref()
            .is_some_and(|prebuffer| prebuffer.is_ready())
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, bail},
        serde_json::{from_str, to_string},
    };

    use crate::{audio::engine::PlaybackState, error::AudioError};

    #[test]
    fn test_playback_state_serialization() -> Result<()> {
        let states = vec![
            PlaybackState::Stopped,
            PlaybackState::Ready,
            PlaybackState::Playing,
            PlaybackState::Paused,
            PlaybackState::Buffering,
        ];

        for state in states {
            let serialized = to_string(&state)?;
            let deserialized: PlaybackState = from_str(&serialized)?;
            if state != deserialized {
                bail!("Expected {state:?}, got {deserialized:?}");
            }
        }
        Ok(())
    }

    #[test]
    fn test_audio_error_display() -> Result<()> {
        let no_track_error = AudioError::NoTrackLoaded;
        if no_track_error.to_string() != "No track loaded" {
            bail!("Expected 'No track loaded', got '{no_track_error}'");
        }

        let invalid_op_error = AudioError::InvalidOperation {
            reason: "test reason".to_string(),
        };
        if invalid_op_error.to_string() != "Invalid operation: test reason" {
            bail!("Expected 'Invalid operation: test reason', got '{invalid_op_error}'");
        }
        Ok(())
    }
}
