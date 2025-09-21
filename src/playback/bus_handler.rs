use std::sync::mpsc::Sender;

use gstreamer::{
    Message,
    MessageView::{Eos, Error, StateChanged},
    Pipeline,
    glib::ControlFlow::Continue,
    prelude::ElementExt,
};

use super::{
    error::PlaybackError,
    events::{PlaybackEvent, PlaybackEvent::EndOfStream},
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
    bus_watch: Option<gstreamer::bus::BusWatchGuard>,
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

        // Add a watch to handle bus messages asynchronously
        // The closure is called for each message received on the bus
        let bus_watch = bus.add_watch(move |_, message| {
            // Process the GStreamer message and convert it to a playback event
            // Errors during message handling are logged but don't stop the watch
            if let Err(e) = Self::handle_message(&event_sender, message) {
                eprintln!("Error handling GStreamer message: {}", e);
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
        match message.view() {
            Eos(..) => {
                // End of stream reached, send EndOfStream event
                // This indicates that playback has finished normally
                let result = event_sender.send(EndOfStream);
                if let Err(e) = result {
                    eprintln!("BusHandler: Error sending EndOfStream event: {}", e);
                }
            }
            Error(err) => {
                // Error occurred in the GStreamer pipeline
                // Extract the error message and send it as a PlaybackEvent::Error
                let error_msg = format!("GStreamer error: {}", err.error());
                let _ = event_sender.send(PlaybackEvent::Error(error_msg));
                // The result is ignored here because there's not much we can do
                // if sending the error event also fails
            }
            StateChanged(state_changed) => {
                // State changed, we could send StateChanged events here
                // but the engine already handles state tracking
                // This is left empty intentionally to avoid duplicate state notifications
            }
            _ => {
                // Ignore other message types
                // GStreamer produces many message types, but we only care about
                // end-of-stream, errors, and potentially state changes
            }
        }
        Ok(())
    }
}
