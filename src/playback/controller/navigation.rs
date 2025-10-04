use crate::playback::{error::PlaybackError, queue::QueueItem};

use super::main::PlaybackController;

impl PlaybackController {
    /// Checks if navigation to the next song is possible
    ///
    /// Returns true if there is a next song in the queue, false otherwise
    pub fn can_go_next(&self) -> bool {
        self.queue.can_go_next()
    }

    /// Checks if navigation to the previous song is possible
    ///
    /// Returns true if there is a previous song in the queue, false otherwise
    pub fn can_go_previous(&self) -> bool {
        self.queue.can_go_previous()
    }

    /// Plays the next song in the queue
    ///
    /// This method checks if there is a next song, increments the current index,
    /// and loads and plays the next song.
    ///
    /// # Returns
    /// A `Result` indicating success or a `PlaybackError` on failure
    pub fn next_song(&mut self) -> Result<(), PlaybackError> {
        // Get current index
        let current_index = self.queue.current_index;

        // Check if there is a next song
        if let Some(index) = current_index {
            if index + 1 < self.queue.items.len() {
                // Increment current index
                self.queue.current_index = Some(index + 1);

                // Get the next song
                if let Some(next_song) = self.queue.current_song() {
                    // Load and play the next song
                    self.load_song(next_song.song_path.clone())?;
                    self.play()?;
                    return Ok(());
                } else {
                    println!("Controller: No next song found");
                }
            }
        } else {
            println!("Controller: Current index is None");
        }

        // No next song, stop playback
        self.stop()?;
        Ok(())
    }

    /// Plays the previous song in the queue
    ///
    /// This method checks if there is a previous song, decrements the current index,
    /// and loads and plays the previous song.
    ///
    /// # Returns
    /// A `Result` indicating success or a `PlaybackError` on failure
    pub fn previous_song(&mut self) -> Result<(), PlaybackError> {
        // Get current index
        let current_index = self.queue.current_index;

        // Check if there is a previous song
        if let Some(index) = current_index {
            if index > 0 {
                // Decrement current index
                self.queue.current_index = Some(index - 1);

                // Get the previous song
                if let Some(prev_song) = self.queue.current_song() {
                    // Load and play the previous song
                    self.load_song(prev_song.song_path.clone())?;
                    self.play()?;
                    return Ok(());
                }
            } else {
                // No previous song, just restart current song from beginning
                self.stop()?;
                if let Some(current_song) = self.queue.current_song() {
                    self.load_song(current_song.song_path.clone())?;
                    self.play()?;
                }
                return Ok(());
            }
        }
        Ok(())
    }

    /// Gets the previous song information from the queue
    ///
    /// # Returns
    /// An `Option<QueueItem>` containing the previous song information if available
    pub fn get_previous_song_info(&self) -> Option<QueueItem> {
        if let Some(current_index) = self.queue.current_index {
            if current_index > 0 && current_index <= self.queue.items.len() {
                self.queue.items.get(current_index - 1).cloned()
            } else {
                // If at the first song, return the current song (for restart behavior)
                self.queue.current_song().cloned()
            }
        } else {
            None
        }
    }

    /// Gets the next song information from the queue
    ///
    /// # Returns
    /// An `Option<QueueItem>` containing the next song information if available
    pub fn get_next_song_info(&self) -> Option<QueueItem> {
        if let Some(current_index) = self.queue.current_index {
            if current_index + 1 < self.queue.items.len() {
                self.queue.items.get(current_index + 1).cloned()
            } else {
                None
            }
        } else {
            None
        }
    }
}
