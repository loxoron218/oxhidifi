use crate::playback::events::{
    PlaybackEvent,
    PlaybackEvent::{EndOfStream, Error, PositionChanged, SongChanged, StateChanged},
};

use super::main::PlaybackController;

impl PlaybackController {
    /// Attempts to get an event from the receiver without blocking.
    ///
    /// # Returns
    ///
    /// An `Option<PlaybackEvent>` containing the event if one was available,
    /// or `None` if no event was immediately available.
    pub fn try_get_event(&mut self) -> Option<PlaybackEvent> {
        match self.event_receiver.try_recv() {
            Ok(event) => {
                self.process_event(event.clone());
                Some(event)
            }

            // No event available
            Err(_) => None,
        }
    }

    /// Processes a playback event and updates internal state
    ///
    /// This method handles the internal processing of playback events,
    /// updating the controller's state as needed.
    ///
    /// # Parameters
    ///
    /// * `event` - The playback event to process
    fn process_event(&mut self, event: PlaybackEvent) {
        match &event {
            SongChanged(_) => {
                // Metadata changes are handled by the player bar
            }
            StateChanged(_state) => {
                // State changes are handled by the player bar
            }
            PositionChanged(position) => {
                // Update our internal position songing
                self.position = *position;
            }
            EndOfStream => {
                // When the current song ends, try to play the next song in the queue
                if let Err(e) = self.next_song() {
                    eprintln!("Error playing next song: {}", e);
                }
            }
            Error(error) => {
                // Handle playback errors
                eprintln!("Playback error: {}", error);
            }
        }
    }
}
