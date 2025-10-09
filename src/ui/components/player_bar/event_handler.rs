use std::path::Path;

use gtk4::{gdk_pixbuf::Pixbuf, glib::MainContext};
use libadwaita::prelude::{ButtonExt, RangeExt, WidgetExt};

use crate::{
    playback::events::{
        PlaybackEvent::{self, EndOfStream, Error, PositionChanged, SongChanged, StateChanged},
        PlaybackState::{Paused, Playing, Stopped},
    },
    utils::formatting::format_sample_rate_value,
};

use super::PlayerBar;

impl PlayerBar {
    /// Updates the player bar with song metadata and makes it visible.
    ///
    /// This method is called when a song starts playing to display its information
    /// in the player bar at the bottom of the window. The information is displayed
    /// in the following order to follow GNOME Human Interface Guidelines:
    /// 1. Song title
    /// 2. Artist name
    /// 3. Album title
    /// 4. Bit depth and sample rate
    /// 5. Audio format
    ///
    /// # Parameters
    /// - `album_title`: The title of the album
    /// - `song_title`: The title of the currently playing song
    /// - `song_artist`: The artist of the currently playing song
    /// - `cover_art_path`: Optional path to the album art image file
    /// - `bit_depth`: Optional bit depth of the audio file
    /// - `sample_rate`: Optional sample rate of the audio file
    /// - `format`: Optional format of the audio file
    /// - `duration`: Optional duration of the song in seconds
    ///
    /// # Behavior
    /// - Updates all labels with the provided metadata
    /// - Attempts to load album art from the provided path, falling back to a
    ///   placeholder icon if loading fails or no path is provided
    /// - Makes the player bar visible
    pub fn update_with_metadata(
        &self,
        album_title: &str,
        song_title: &str,
        song_artist: &str,
        cover_art_path: Option<&Path>,
        bit_depth: Option<u32>,
        sample_rate: Option<u32>,
        format: Option<&str>,
        duration: Option<u32>,
    ) {
        // Update the song title label with the provided song title
        self.song_title.set_label(song_title);

        // Update the artist label with the provided artist name
        self.song_artist.set_label(song_artist);

        // Update the album title label
        self.album_title.set_label(album_title);

        // Format and update bit depth/sample rate information
        let bit_depth_sample_rate_text = match (bit_depth, sample_rate) {
            (Some(bit), Some(freq)) => {
                format!("{}-Bit/{} kHz", bit, format_sample_rate_value(freq))
            }
            (Some(bit), None) => format!("{}-Bit", bit),
            (None, Some(freq)) => format!("{} kHz", format_sample_rate_value(freq)),
            (None, None) => String::new(),
        };

        // Format information
        let format_text = format.map(|f| f.to_uppercase()).unwrap_or_default();

        // Combine bit depth/sample rate and format information with a separator
        let combined_text = if !bit_depth_sample_rate_text.is_empty() && !format_text.is_empty() {
            format!("{} · {}", bit_depth_sample_rate_text, format_text)
        } else if !bit_depth_sample_rate_text.is_empty() {
            bit_depth_sample_rate_text
        } else if !format_text.is_empty() {
            format_text
        } else {
            String::new()
        };

        // Update the bit depth/sample rate label with the combined information
        self.bit_depth_sample_rate.set_label(&combined_text);

        // Hide the separate format label since we're now combining the information
        self.format.set_visible(false);

        // Determine the label and progress range based on the duration
        let range_end = if let Some(duration_secs) = duration {
            let minutes = duration_secs / 60;
            let seconds = duration_secs % 60;

            // Note the {:02} to correctly pad seconds (e.g., 1:07)
            let duration_text = format!("{}:{:02}", minutes, seconds);
            self.time_label_end.set_label(&duration_text);
            duration_secs as f64
        } else {
            self.time_label_end.set_label("0:00");
            100.0
        };

        // Set the initial start time to 0:00
        self.time_label_start.set_label("0:00");

        // Set the progress bar range
        self.progress_bar.set_range(0.0, range_end);

        // Chain the operations: start with an optional path, then try to load from it.
        // .and_then() is perfect for this. .ok() converts the Result into an Option.
        let pixbuf =
            cover_art_path.and_then(|path| Pixbuf::from_file_at_scale(path, 96, 96, true).ok());

        // Now we have an Option<Pixbuf>. We can act on it in one place.
        if let Some(p) = pixbuf.as_ref() {
            self.album_art.set_from_pixbuf(Some(p));

            // Ensure the image maintains its requested size after setting the pixbuf
            self.album_art.set_pixel_size(96);
        } else {
            // This single else block now handles both "no path" and "failed to load" cases.
            self.album_art.set_icon_name(Some("image-missing"));

            // Ensure the image maintains its requested size when showing icon
            self.album_art.set_pixel_size(96);
        }

        // Make the player bar visible now that it has song information
        self.container.set_visible(true);

        // Adjust the main content area padding to prevent overlap
        if let Some(content_area) = self.main_content_area.borrow().as_ref() {
            // Use a fixed height for the margin based on the player bar's design
            // The player bar has a fixed height request of 96 pixels for the album art,
            // plus some padding from the CSS (12px top/bottom), so we'll use 120 pixels
            content_area.set_margin_bottom(120);
        }

        // Store the duration for progress calculations
        self.duration.set(range_end);

        // Update navigation button states as the queue position may have changed
        self.update_navigation_button_states();

        // Update play button state to reflect current playback state
        self.update_play_button_state();
    }

    /// Handles playback events from the controller
    ///
    /// This method updates the UI based on playback events from the controller.
    ///
    /// # Parameters
    /// * `event` - The playback event to handle
    pub fn handle_playback_event(&self, event: PlaybackEvent) {
        match event {
            SongChanged(item) => {
                self.update_with_metadata(
                    &item.album_title,
                    &item.song_title,
                    &item.artist_name,
                    item.cover_art_path.as_deref(),
                    item.bit_depth,
                    item.sample_rate,
                    item.format.as_deref(),
                    item.duration,
                );
            }
            StateChanged(state) => {
                // Update the play button icon based on the new state
                match state {
                    Playing => {
                        self._play_button.set_icon_name("media-playback-pause");

                        // Ensure player bar is visible when playing starts
                        if !self.container.is_visible() {
                            self.container.set_visible(true);
                            if let Some(content_area) = self.main_content_area.borrow().as_ref() {
                                content_area.set_margin_bottom(120);
                            }
                        }
                    }
                    Paused => {
                        self._play_button.set_icon_name("media-playback-start");
                    }
                    Stopped => {
                        self._play_button.set_icon_name("media-playback-start");
                    }
                }
            }
            PositionChanged(position_ns) => {
                // Update the progress bar with the new position
                let position_secs = position_ns as f64 / 1_000_000_000.0;
                self.progress_bar.set_value(position_secs);

                // Format the current position
                let position_minutes = (position_secs / 60.0) as u32;
                let position_seconds = (position_secs % 60.0) as u32;

                // Create the time label text for current position
                let position_text = format!("{}:{:02}", position_minutes, position_seconds);

                // Update the start time label
                self.time_label_start.set_label(&position_text);
            }

            EndOfStream => {
                // When the song ends, reset the play button icon
                self._play_button.set_icon_name("media-playback-start");

                // Update navigation button states as the queue position may have changed
                self.update_navigation_button_states();
            }

            Error(error) => {
                // Log the error
                eprintln!("Playback error: {}", error);
            }
        }
    }

    /// Updates the state of the previous and next buttons based on queue navigation possibilities
    ///
    /// This method checks if navigation to the previous or next song is possible
    /// and updates internal state accordingly. The buttons remain visually enabled
    /// but will only function when navigation is actually possible.
    pub fn update_navigation_button_states(&self) {
        // Only proceed if we have a playback controller
        if let Some(controller) = &self.playback_controller {
            let controller_clone = controller.clone();
            let prev_button = self._prev_button.clone();
            let next_button = self._next_button.clone();
            let can_go_prev = self.can_go_prev.clone();
            let can_go_next = self.can_go_next.clone();

            // Use the main context to ensure UI updates happen on the main thread
            MainContext::default().spawn_local(async move {
                // Lock the controller to get the current state
                let controller = controller_clone.lock().await;

                // Get the current navigation capabilities
                let current_can_prev = controller.can_go_previous();
                let current_can_next = controller.can_go_next();

                // Update internal state
                can_go_prev.set(current_can_prev);
                can_go_next.set(current_can_next);

                // Update button styling based on navigation possibility
                if current_can_prev {
                    prev_button.remove_css_class("navigation-disabled");
                } else {
                    prev_button.add_css_class("navigation-disabled");
                }
                if current_can_next {
                    next_button.remove_css_class("navigation-disabled");
                } else {
                    next_button.add_css_class("navigation-disabled");
                }
            });
        } else {
            // If no controller is available, disable both buttons
            self.can_go_prev.set(false);
            self.can_go_next.set(false);
            self._prev_button.add_css_class("navigation-disabled");
            self._next_button.add_css_class("navigation-disabled");
        }
    }

    /// Updates the play button icon based on the current playback state
    ///
    /// This method queries the playback controller for the current state
    /// and updates the play button icon accordingly.
    pub fn update_play_button_state(&self) {
        if let Some(controller) = &self.playback_controller {
            let controller_clone = controller.clone();
            let play_button = self._play_button.clone();

            // Use the main context to ensure UI updates happen on the main thread
            MainContext::default().spawn_local(async move {
                // Lock the controller to get the current state
                let controller = controller_clone.lock().await;

                // Get the current playback state and update the button icon
                match controller.get_current_state() {
                    Playing => {
                        play_button.set_icon_name("media-playback-pause");
                    }
                    _ => {
                        play_button.set_icon_name("media-playback-start");
                    }
                }
            });
        } else {
            // If no controller is available, default to showing the play icon
            self._play_button.set_icon_name("media-playback-start");
        }
    }
}
