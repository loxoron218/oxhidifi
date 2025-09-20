use gstreamer::{
    ClockTime, Element, ElementFactory, Pipeline, SeekFlags,
    State::{self, Null, Paused, Playing},
    prelude::{ElementExt, ElementExtManual, GstBinExt, ObjectExt},
};

use super::error::PlaybackError;

/// Manages the GStreamer pipeline for audio playback.
///
/// This struct encapsulates a GStreamer pipeline and playbin element,
/// providing a simplified interface for audio playback operations.
///
/// # Fields
///
/// * `pipeline` - The GStreamer pipeline that manages the playback elements
/// * `playbin` - The playbin element responsible for decoding and playing audio
pub struct PipelineManager {
    pipeline: Pipeline,
    playbin: Element,
}

impl PipelineManager {
    /// Creates a new pipeline manager with an initialized GStreamer pipeline.
    ///
    /// Initializes the GStreamer framework and creates a new pipeline with a
    /// playbin3 element. The playbin element is responsible for automatically
    /// detecting and decoding various media formats.
    ///
    /// # Returns
    ///
    /// Returns `Ok(PipelineManager)` if initialization is successful, or
    /// a [`PlaybackError`] if initialization fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * GStreamer fails to initialize
    /// * The playbin3 element cannot be created
    /// * The playbin element cannot be added to the pipeline
    ///
    /// # Example
    ///
    /// ```rust
    /// # use crate::playback::pipeline::PipelineManager;
    /// let pipeline_manager = PipelineManager::new()
    ///     .expect("Failed to create pipeline manager");
    /// ```
    pub fn new() -> Result<Self, PlaybackError> {
        // Initialize the GStreamer framework
        gstreamer::init()?;

        // Create a playbin element for playback
        // playbin3 automatically handles format detection and decoding
        let playbin = ElementFactory::make("playbin3").build()?;

        // Create a new pipeline and add the playbin element to it
        let pipeline = Pipeline::new();
        pipeline.add(&playbin)?;
        Ok(Self { pipeline, playbin })
    }

    /// Sets the URI for playback.
    ///
    /// Configures the playbin element to load media from the specified URI.
    /// The URI can be a local file path (file://) or a network resource.
    ///
    /// # Parameters
    ///
    /// * `uri` - A string slice containing the URI of the media to play
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the URI was successfully set, or a [`PlaybackError`]
    /// if setting the URI fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use crate::playback::pipeline::PipelineManager;
    /// # let pipeline_manager = PipelineManager::new().unwrap();
    /// pipeline_manager.set_uri("file:///path/to/audio.mp3")
    ///     .expect("Failed to set URI");
    /// ```
    pub fn set_uri(&self, uri: &str) -> Result<(), PlaybackError> {
        self.playbin.set_property("uri", uri);
        Ok(())
    }

    /// Starts playback of the currently loaded media.
    ///
    /// Sets the pipeline state to [`Playing`](State::Playing), which begins
    /// decoding and playing the media. If no media is loaded, this will have
    /// no effect until media is loaded.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if playback was successfully started, or a [`PlaybackError`]
    /// if starting playback fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the pipeline fails to transition
    /// to the playing state.
    pub fn play(&self) -> Result<(), PlaybackError> {
        match self.pipeline.set_state(Playing) {
            Ok(_) => Ok(()),
            Err(e) => {
                println!("Error setting pipeline state to Playing: {:?}", e);
                Err(PlaybackError::Pipeline(format!(
                    "Failed to set pipeline state: {:?}",
                    e
                )))
            }
        }
    }

    /// Pauses playback of the currently playing media.
    ///
    /// Sets the pipeline state to [`Paused`](State::Paused), which temporarily
    /// stops playback while maintaining the current position. Playback can be
    /// resumed by calling [`play`](Self::play).
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if playback was successfully paused, or a [`PlaybackError`]
    /// if pausing playback fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the pipeline fails to transition
    /// to the paused state.
    pub fn pause(&self) -> Result<(), PlaybackError> {
        self.pipeline.set_state(Paused)?;
        Ok(())
    }

    /// Stops playback and resets the pipeline.
    ///
    /// Sets the pipeline state to [`Null`](State::Null), which stops playback
    /// and resets the position to the beginning. To resume playback, the media
    /// must be reloaded.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if playback was successfully stopped, or a [`PlaybackError`]
    /// if stopping playback fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the pipeline fails to transition
    /// to the null state.
    pub fn stop(&self) -> Result<(), PlaybackError> {
        self.pipeline.set_state(Null)?;
        Ok(())
    }

    /// Seeks to a specific position in the media.
    ///
    /// Changes the playback position to the specified time in nanoseconds.
    /// This operation can be performed during playback or when paused.
    ///
    /// # Parameters
    ///
    /// * `position_ns` - The target position in nanoseconds
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if seeking was successful, or a [`PlaybackError`]
    /// if seeking fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the pipeline fails to seek to
    /// the specified position.
    pub fn seek(&self, position_ns: u64) -> Result<(), PlaybackError> {
        // Convert nanoseconds to GStreamer ClockTime
        let position = ClockTime::from_nseconds(position_ns);

        // Perform the seek operation with FLUSH and KEY_UNIT flags
        // FLUSH: Discard all data in the pipeline before seeking
        // KEY_UNIT: Seek to the nearest key frame for faster seeking
        self.pipeline
            .seek_simple(SeekFlags::FLUSH | SeekFlags::KEY_UNIT, position)?;
        Ok(())
    }

    /// Gets the duration of the current media.
    ///
    /// Queries the pipeline for the total duration of the currently loaded media.
    /// The duration is returned in nanoseconds.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(u64))` with the duration in nanoseconds if available,
    /// `Ok(None)` if the duration is not available, or a [`PlaybackError`] if
    /// querying the duration fails.
    pub fn get_duration(&self) -> Result<Option<u64>, PlaybackError> {
        // Query the pipeline for the media duration
        let duration = self.pipeline.query_duration::<ClockTime>();

        // Convert ClockTime to nanoseconds if duration is available
        Ok(duration.map(|d| d.nseconds()))
    }

    /// Gets the current position of the playback.
    ///
    /// Queries the pipeline for the current playback position in nanoseconds.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(u64))` with the current position in nanoseconds if available,
    /// `Ok(None)` if the position is not available, or a [`PlaybackError`] if
    /// querying the position fails.
    pub fn get_position(&self) -> Result<Option<u64>, PlaybackError> {
        // Query the pipeline for the current playback position
        let position = self.pipeline.query_position::<ClockTime>();

        // Convert ClockTime to nanoseconds if position is available
        Ok(position.map(|p| p.nseconds()))
    }

    /// Gets the current state of the pipeline.
    ///
    /// Queries the pipeline for its current state, which indicates whether
    /// it is stopped, playing, paused, or buffering.
    ///
    /// # Returns
    ///
    /// Returns `Ok(State)` with the current pipeline state, or a [`PlaybackError`]
    /// if querying the state fails.
    pub fn get_state(&self) -> Result<State, PlaybackError> {
        // Query the pipeline state with no timeout (ZERO)
        let (_, state, _pending) = self.pipeline.state(ClockTime::ZERO);
        Ok(state)
    }
}
