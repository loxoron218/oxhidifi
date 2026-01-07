//! Audio playback engine orchestrator.
//!
//! This module provides the main `AudioEngine` that coordinates the audio
//! decoder, output, and playback state management for high-fidelity playback.

use std::{
    path::Path,
    sync::{Arc, RwLock},
    thread::{JoinHandle, spawn},
};

use {
    async_channel::{Receiver, Sender, unbounded},
    cpal::{Stream, traits::StreamTrait},
    parking_lot::RwLock as ParkingRwLock,
    rtrb::RingBuffer,
    serde::{Deserialize, Serialize},
    thiserror::Error,
    tokio::{
        runtime::Builder,
        sync::broadcast::{Receiver as TokioReceiver, Sender as TokioSender, channel},
    },
};

use crate::audio::{
    decoder::{AudioDecoder, AudioFormat, AudioProducer, DecoderError},
    metadata::{MetadataError, TagReader, TrackMetadata},
    output::{AudioConsumer, AudioOutput, OutputConfig, OutputError},
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
pub struct AudioEngine {
    /// Current playback state.
    state: Arc<ParkingRwLock<PlaybackState>>,
    /// Information about the currently loaded track.
    current_track: Arc<ParkingRwLock<Option<TrackInfo>>>,
    /// Audio output configuration.
    output_config: OutputConfig,
    /// Broadcast channel for state change notifications.
    state_tx: TokioSender<PlaybackState>,
    /// Receiver for internal control messages.
    control_rx: Receiver<ControlMessage>,
    /// Sender for internal control messages.
    control_tx: Sender<ControlMessage>,
    /// Handle to the current audio stream (if any).
    stream_handle: Arc<RwLock<Option<StreamHandle>>>,
}

impl Clone for AudioEngine {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            current_track: Arc::clone(&self.current_track),
            output_config: self.output_config.clone(),
            state_tx: self.state_tx.clone(),
            control_rx: self.control_rx.clone(),
            control_tx: self.control_tx.clone(),
            stream_handle: Arc::clone(&self.stream_handle),
        }
    }
}

/// Internal control messages for the audio engine.
#[derive(Debug)]
enum ControlMessage {
    Play,
    Pause,
    Stop,
    Seek(u64),
}

/// Handle to a running audio stream.
struct StreamHandle {
    /// The CPAL audio stream.
    stream: Stream,
    /// Join handle for the decoder thread.
    decoder_handle: Option<JoinHandle<Result<(), DecoderError>>>,
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
        let (state_tx, _) = channel(16);
        let (control_tx, control_rx) = unbounded();

        let engine = AudioEngine {
            state: Arc::new(ParkingRwLock::new(PlaybackState::Stopped)),
            current_track: Arc::new(ParkingRwLock::new(None)),
            output_config: OutputConfig::default(),
            state_tx,
            control_rx,
            control_tx,
            stream_handle: Arc::new(RwLock::new(None)),
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
    pub async fn load_track<P: AsRef<Path>>(&self, track_path: P) -> Result<(), AudioError> {
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
                reason: format!("Failed to send play command: {}", e),
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
                reason: format!("Failed to send pause command: {}", e),
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
            .send(ControlMessage::Play)
            .await
            .map_err(|e| AudioError::InvalidOperation {
                reason: format!("Failed to send resume command: {}", e),
            })?;

        Ok(())
    }

    /// Stops playback and unloads the current track.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    pub async fn stop(&self) -> Result<(), AudioError> {
        self.control_tx
            .send(ControlMessage::Stop)
            .await
            .map_err(|e| AudioError::InvalidOperation {
                reason: format!("Failed to send stop command: {}", e),
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
                reason: format!("Failed to send seek command: {}", e),
            })?;

        Ok(())
    }

    /// Gets the current playback state.
    ///
    /// # Returns
    ///
    /// The current `PlaybackState`.
    pub fn current_playback_state(&self) -> PlaybackState {
        self.state.read().clone()
    }

    /// Gets information about the currently loaded track.
    ///
    /// # Returns
    ///
    /// An `Option` containing the `TrackInfo` if a track is loaded.
    pub fn current_track_info(&self) -> Option<TrackInfo> {
        self.current_track.read().clone()
    }

    /// Subscribes to playback state changes.
    ///
    /// # Returns
    ///
    /// A `broadcast::Receiver` that receives `PlaybackState` updates.
    pub fn subscribe_to_state_changes(&self) -> TokioReceiver<PlaybackState> {
        self.state_tx.subscribe()
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
                                eprintln!("Error handling play command: {}", e);
                            }
                        }
                        ControlMessage::Pause => {
                            if let Err(e) = self.handle_pause().await {
                                eprintln!("Error handling pause command: {}", e);
                            }
                        }
                        ControlMessage::Stop => {
                            if let Err(e) = self.handle_stop().await {
                                eprintln!("Error handling stop command: {}", e);
                            }
                        }
                        ControlMessage::Seek(position_ms) => {
                            if let Err(e) = self.handle_seek(position_ms).await {
                                eprintln!("Error handling seek command: {}", e);
                            }
                        }
                    }
                }
            });
    }

    /// Handles the play command.
    async fn handle_play(&self) -> Result<(), AudioError> {
        let track_info = self
            .current_track
            .read()
            .clone()
            .ok_or(AudioError::NoTrackLoaded)?;

        // Stop any existing playback
        self.stop_stream().await;

        // Create ring buffer for audio samples
        let buffer_size = 4096; // Should be power of 2 for rtrb
        let (producer, consumer) = RingBuffer::<f32>::new(buffer_size);

        // Create audio output
        let output = AudioOutput::new(Some(self.output_config.clone()))?;

        // Create decoder to get signal spec
        let decoder = AudioDecoder::new(&track_info.path)?;
        let signal_spec = decoder.signal_spec;

        // Create audio consumer
        let consumer = AudioConsumer::new(output, consumer, &track_info.format, &signal_spec)?;

        // Create audio producer
        let producer = AudioProducer::new(decoder, producer);

        // Start decoder thread
        let decoder_handle = spawn(move || producer.run());

        // Start audio stream
        let stream = consumer.run(&track_info.format, &signal_spec)?;

        // Store stream handle
        *self
            .stream_handle
            .write()
            .map_err(|e| AudioError::InvalidOperation {
                reason: format!("Failed to acquire stream handle lock: {}", e),
            })? = Some(StreamHandle {
            stream,
            decoder_handle: Some(decoder_handle),
        });

        *self.state.write() = PlaybackState::Playing;
        self.notify_state_change();

        Ok(())
    }

    /// Handles the pause command.
    async fn handle_pause(&self) -> Result<(), AudioError> {
        // For now, we'll just stop the stream and keep the track loaded
        // In a more sophisticated implementation, we'd actually pause the stream
        self.stop_stream().await;

        *self.state.write() = PlaybackState::Paused;
        self.notify_state_change();

        Ok(())
    }

    /// Handles the stop command.
    async fn handle_stop(&self) -> Result<(), AudioError> {
        self.stop_stream().await;

        *self.state.write() = PlaybackState::Stopped;
        *self.current_track.write() = None;
        self.notify_state_change();

        Ok(())
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

            let signal_spec = decoder.signal_spec;

            // Recreate the playback stream
            let buffer_size = 4096;
            let (producer, consumer) = RingBuffer::<f32>::new(buffer_size);

            let output = AudioOutput::new(Some(self.output_config.clone()))?;
            let consumer = AudioConsumer::new(output, consumer, &track_info.format, &signal_spec)?;
            let producer = AudioProducer::new(decoder, producer);

            let decoder_handle = spawn(move || producer.run());

            let stream = consumer.run(&track_info.format, &signal_spec)?;

            *self
                .stream_handle
                .write()
                .map_err(|e| AudioError::InvalidOperation {
                    reason: format!("Failed to acquire stream handle lock: {}", e),
                })? = Some(StreamHandle {
                stream,
                decoder_handle: Some(decoder_handle),
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
        if let Some(mut handle) = self.stream_handle.write().ok().and_then(|mut h| h.take()) {
            // Stop the stream
            let _ = handle.stream.pause();

            // Wait for decoder thread to finish
            if let Some(decoder_handle) = handle.decoder_handle.take() {
                let _ = decoder_handle.join();
            }
        }
    }

    /// Notifies subscribers of state changes.
    fn notify_state_change(&self) {
        let state = self.state.read().clone();
        let _ = self.state_tx.send(state);
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
