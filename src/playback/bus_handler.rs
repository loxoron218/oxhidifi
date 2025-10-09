use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use gstreamer::{
    ClockTime, Message, MessageView, Pipeline,
    State::{Null, Paused, Playing, Ready},
    bus::BusWatchGuard,
    glib::{
        ControlFlow::{Break, Continue},
        SourceId, WeakRef, timeout_add_local,
    },
    prelude::{Cast, ElementExt, ElementExtManual, ObjectExt},
};
use tokio::sync::mpsc::UnboundedSender;

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
    event_sender: UnboundedSender<PlaybackEvent>,
    bus_watch: Option<BusWatchGuard>,
    position_update_source: Arc<Mutex<Option<SourceId>>>,
}

impl BusHandler {
    /// Creates a new `BusHandler` instance.
    ///
    /// Initializes the handler with the GStreamer pipeline and a sender for
    /// playback events. The `bus_watch` and `position_update_source` are
    /// initialized to `None` and an empty `RefCell` respectively, as they
    /// will be set up later when `setup_bus_watch` is called.
    ///
    /// # Arguments
    ///
    /// * `pipeline` - The GStreamer pipeline to monitor.
    /// * `event_sender` - An `UnboundedSender` to send `PlaybackEvent`s to the UI or other components.
    ///
    /// # Returns
    ///
    /// A new `BusHandler` instance.
    pub fn new(pipeline: Pipeline, event_sender: UnboundedSender<PlaybackEvent>) -> Self {
        Self {
            pipeline,
            event_sender,
            bus_watch: None,
            position_update_source: Arc::new(Mutex::new(None)),
        }
    }

    /// Sets up a watch on the GStreamer bus to handle pipeline messages.
    ///
    /// This method retrieves the GStreamer bus from the pipeline and adds a
    /// local watch. The watch's callback (`handle_message`) processes incoming
    /// messages (e.g., EndOfStream, Error, StateChanged) and converts them
    /// into `PlaybackEvent`s, which are then sent via the `event_sender`.
    /// It also manages a `SourceId` for position updates, ensuring it's
    /// properly removed when playback state changes.
    ///
    /// # Errors
    ///
    /// Returns a `PlaybackError` if:
    /// - The pipeline's bus cannot be retrieved.
    /// - The bus watch cannot be added.
    pub fn setup_bus_watch(&mut self) -> Result<(), PlaybackError> {
        let bus = self
            .pipeline
            .bus()
            .ok_or_else(|| PlaybackError::Pipeline("Failed to get pipeline bus".to_string()))?;
        let event_sender = self.event_sender.clone();
        let pipeline_weak = self.pipeline.downgrade();
        let position_update_source = self.position_update_source.clone();
        let bus_watch = bus.add_watch_local(move |_, message| {
            Self::handle_message(
                &event_sender,
                message,
                &pipeline_weak,
                &position_update_source,
            );
            Continue
        })?;
        self.bus_watch = Some(bus_watch);
        Ok(())
    }

    /// Handles incoming GStreamer bus messages and dispatches corresponding `PlaybackEvent`s.
    ///
    /// This function is the core of the `BusHandler`, processing different types of
    /// GStreamer messages:
    /// - `Eos`: Sends an `EndOfStream` event.
    /// - `Error`: Extracts the error message and sends a `PlaybackEvent::Error`.
    /// - `StateChanged`: Filters for pipeline state changes, maps GStreamer states
    ///   to `PlaybackState`, and sends a `PlaybackEvent::StateChanged`.
    ///   When the state changes to `Playing`, it starts a 1-second interval timer
    ///   to periodically send `PositionChanged` events. When the state changes to
    ///   `Paused` or `Stopped`, it stops this timer.
    ///
    /// # Arguments
    ///
    /// * `event_sender` - A reference to the `UnboundedSender` for sending `PlaybackEvent`s.
    /// * `message` - The GStreamer `Message` to handle.
    /// * `pipeline_weak` - A `WeakRef` to the GStreamer `Pipeline` for querying position.
    /// * `position_update_source` - An `Rc<RefCell<Option<SourceId>>>` to manage the
    ///   timer for position updates.
    fn handle_message(
        event_sender: &UnboundedSender<PlaybackEvent>,
        message: &Message,
        pipeline_weak: &WeakRef<Pipeline>,
        position_update_source: &Arc<Mutex<Option<SourceId>>>,
    ) {
        match message.view() {
            MessageView::Eos(..) => {
                // End of stream message: send an EndOfStream event
                let _ = event_sender.send(EndOfStream);
            }
            MessageView::Error(err) => {
                // Error message: format the error and send a PlaybackEvent::Error
                let error_msg = format!("GStreamer error: {}", err.error());
                let _ = event_sender.send(PlaybackEvent::Error(error_msg));
            }
            MessageView::StateChanged(state_changed) => {
                // Only process state changes from the main pipeline, not its elements
                if state_changed
                    .src()
                    .is_none_or(|s| s.downcast_ref::<Pipeline>().is_none())
                {
                    return;
                }

                // Map GStreamer states to PlaybackState
                let new_state = match state_changed.current() {
                    Playing => Some(PlaybackState::Playing),
                    Paused => Some(PlaybackState::Paused),
                    Ready | Null => Some(PlaybackState::Stopped),
                    // Ignore other GStreamer states
                    _ => None,
                };
                if let Some(state) = new_state {
                    match state {
                        PlaybackState::Playing => {
                            // When playing, start a timer to send position updates
                            // Stop any existing timer before starting a new one
                            if let Some(source) = position_update_source.lock().unwrap().take() {
                                source.remove();
                            }
                            let sender = event_sender.clone();
                            let pipeline_clone = pipeline_weak.clone();

                            // Set up a 1-second interval timer for position updates
                            let source = timeout_add_local(Duration::from_secs(1), move || {
                                if let Some(pipeline) = pipeline_clone.upgrade() {
                                    if let Some(pos) = pipeline.query_position::<ClockTime>() {
                                        // Send position changed event, break if sender fails
                                        if sender.send(PositionChanged(pos.nseconds())).is_err() {
                                            return Break;
                                        }
                                    }

                                    // Continue the timer
                                    Continue
                                } else {
                                    // Stop the timer if pipeline is no longer available
                                    Break
                                }
                            });
                            *position_update_source.lock().unwrap() = Some(source);
                        }
                        PlaybackState::Paused | PlaybackState::Stopped => {
                            // When paused or stopped, remove the position update timer
                            if let Some(source) = position_update_source.lock().unwrap().take() {
                                source.remove();
                            }
                        }
                    }

                    // Send the state changed event
                    let _ = event_sender.send(PlaybackEvent::StateChanged(state));
                }
            }

            // Ignore other GStreamer messages
            _ => (),
        }
    }
}
