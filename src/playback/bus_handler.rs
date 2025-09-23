use std::sync::mpsc::Sender;

use gstreamer::{
    ClockTime, Message,
    MessageView::{Eos, Error, StateChanged},
    Pipeline,
    State::{Null, Paused, Playing, Ready},
    bus::BusWatchGuard,
    glib::ControlFlow::Continue,
    prelude::{ElementExt, ElementExtManual, ObjectExt},
};

use super::{
    error::PlaybackError,
    events::{
        PlaybackEvent::{self, EndOfStream, PositionChanged},
        PlaybackState,
    },
};

/// Handles GStreamer bus messages and converts them to playback events
///
/// The `BusHandler` is responsible for setting up a GStreamer bus watch
/// and processing messages from the pipeline. It converts GStreamer messages
/// into [`PlaybackEvent`]s that can be handled by the playback system.
///
/// # Fields
///
/// * `pipeline` - The GStreamer pipeline to watch for messages
/// * `event_sender` - Channel sender for transmitting playback events
pub struct BusHandler {
    pipeline: Pipeline,
    event_sender: Sender<PlaybackEvent>,
    bus_watch: Option<BusWatchGuard>,
}

impl BusHandler {
    /// Creates a new bus handler
    ///
    /// # Parameters
    ///
    /// * `pipeline` - The GStreamer pipeline to watch for messages
    /// * `event_sender` - A channel sender for transmitting playback events
    ///
    /// # Returns
    ///
    /// Returns a new `BusHandler` instance
    pub fn new(pipeline: Pipeline, event_sender: Sender<PlaybackEvent>) -> Self {
        Self {
            pipeline,
            event_sender,
            bus_watch: None,
        }
    }

    /// Sets up the GStreamer bus watch
    ///
    /// This method gets the bus from the pipeline and adds a watch
    /// to handle messages. The watch will call the message handler
    /// function for each message received.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the bus watch was successfully set up,
    /// or a [`PlaybackError`] if setting up the bus watch fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * The pipeline bus cannot be obtained
    pub fn setup_bus_watch(&mut self) -> Result<(), PlaybackError> {
        // Get the bus from the pipeline, returning an error if it fails
        let bus = self
            .pipeline
            .bus()
            .ok_or_else(|| PlaybackError::Pipeline("Failed to get pipeline bus".to_string()))?;

        // Clone the event sender for use in the callback closure
        // This is necessary because the closure takes ownership of captured variables
        let event_sender = self.event_sender.clone();
        let pipeline_weak = self.pipeline.downgrade();

        // Add a watch to handle bus messages asynchronously
        // The closure is called for each message received on the bus
        let bus_watch = bus.add_watch_local(move |_, message| {
            // Add diagnostic logging
            println!("GStreamer bus message received: {:?}", message.type_());

            // Process the GStreamer message and convert it to a playback event
            // Errors during message handling are logged but don't stop the watch
            if let Err(e) = Self::handle_message(&event_sender, message) {
                eprintln!("Error handling GStreamer message: {}", e);
            }

            // Send periodic position updates only for specific message types to reduce frequency
            // Only send position updates for messages that indicate playback progress
            match message.view() {
                StateChanged(_) | Eos(_) | Error(_) => {
                    // Send position update for state changes, end of stream, and errors
                    if let Some(pipeline) = pipeline_weak.upgrade()
                        && let Some(position) = pipeline.query_position::<ClockTime>()
                    {
                        println!(
                            "Sending position update: {} nanoseconds",
                            position.nseconds()
                        );
                        let _ = event_sender.send(PositionChanged(position.nseconds()));
                    }
                }
                _ => {
                    // For other message types, we don't send position updates to reduce event frequency
                }
            }

            // Continue watching for more messages (don't remove the watch)
            Continue
        })?;

        // Store the bus watch guard to prevent it from being dropped
        // This is necessary to keep the bus watch active
        // Without storing the guard, the watch would be removed when it goes out of scope
        self.bus_watch = Some(bus_watch);
        Ok(())
    }

    /// Handles GStreamer messages and converts them to playback events
    ///
    /// This method processes different types of GStreamer messages
    /// and generates appropriate [`PlaybackEvent`]s.
    ///
    /// # Parameters
    ///
    /// * `event_sender` - The channel sender to use for transmitting events
    /// * `message` - The GStreamer message to process
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the message was successfully processed,
    /// or a [`PlaybackError`] if processing the message fails.
    fn handle_message(
        event_sender: &Sender<PlaybackEvent>,
        message: &Message,
    ) -> Result<(), PlaybackError> {
        // Process different types of GStreamer messages and convert them to playback events
        match message.view() {
            // Handle end of stream message - playback has completed
            Eos(..) => {
                // End of stream reached, send EndOfStream event
                // This indicates that playback has finished normally
                let result = event_sender.send(EndOfStream);
                if let Err(e) = result {
                    eprintln!("BusHandler: Error sending EndOfStream event: {}", e);
                }
            }

            // Handle error message - pipeline encountered an error
            Error(err) => {
                // Error occurred in the GStreamer pipeline
                // Extract the error message and send it as a PlaybackEvent::Error
                let error_msg = format!("GStreamer error: {}", err.error());

                // Send error event to playback system
                let _ = event_sender.send(PlaybackEvent::Error(error_msg));
                // The result is ignored here because there's not much we can do
                // if sending the error event also fails
            }

            // Handle state change message - pipeline state has changed
            StateChanged(state_changed) => {
                // Handle state change messages
                let new_state = match state_changed.current() {
                    Playing => PlaybackState::Playing,
                    Paused => PlaybackState::Paused,
                    Ready => PlaybackState::Stopped,
                    Null => PlaybackState::Stopped,

                    // Map buffering state to our Buffering variant
                    _ => {
                        // Check if it's a buffering state
                        if state_changed.current() == Playing && state_changed.pending() == Playing
                        {
                            PlaybackState::Buffering
                        } else {
                            PlaybackState::Stopped
                        }
                    }
                };

                // Send state change event to playback system
                let _ = event_sender.send(PlaybackEvent::StateChanged(new_state));
            }

            // Handle all other message types - ignore them
            _ => {
                // Ignore other message types
                // GStreamer produces many message types, but we only care about
                // end-of-stream, errors, and state changes
            }
        }
        Ok(())
    }
}
