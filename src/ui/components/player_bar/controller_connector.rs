use std::{rc::Rc, sync::Arc, time::Duration};

use gtk4::{
    Box,
    glib::{MainContext, timeout_future},
};
use libadwaita::prelude::{ButtonExt, ObjectExt, WidgetExt};
use tokio::{select, sync::Mutex};

use super::PlayerBar;
use crate::playback::{controller::PlaybackController, events::PlaybackState::Playing};

impl PlayerBar {
    /// Sets the main content area that needs padding adjustment when player bar visibility changes.
    ///
    /// This method should be called once during initialization to provide a reference
    /// to the main content area (typically vbox_inner from the main window builder).
    ///
    /// # Parameters
    /// * `content_area` - A reference to the main content area Box widget
    pub fn set_main_content_area(&mut self, content_area: Box) {
        *self.main_content_area.borrow_mut() = Some(content_area);
    }

    /// Connects to visibility change notifications for the player bar container.
    ///
    /// This method sets up a signal handler that monitors the "visible" property
    /// of the player bar container. When visibility changes, it adjusts the
    /// bottom margin of the main content area to prevent overlap.
    ///
    /// This method should be called after the player bar has been added to the overlay
    /// and the main content area has been set.
    pub fn connect_visibility_changes(&mut self) {
        // If we already have a handler, disconnect it first
        // Note: We can't disconnect the handler because SignalHandlerId doesn't implement Clone
        // This means we might have multiple handlers if this method is called multiple times
        // In practice, this should only be called once during initialization
        // If we have a content area, connect the visibility change handler
        if let Some(content_area) = self.main_content_area.borrow().as_ref() {
            let content_area_weak = ObjectExt::downgrade(content_area);
            let container_weak = ObjectExt::downgrade(&self.container);
            let handler_id =
                self.container
                    .connect_notify_local(Some("visible"), move |_container, _| {
                        if let (Some(content_area), Some(container_strong)) =
                            (content_area_weak.upgrade(), container_weak.upgrade())
                        {
                            if container_strong.is_visible() {
                                // When player bar becomes visible, add bottom margin to content area
                                // Get the player bar height and use it as margin
                                let height = container_strong.height();
                                content_area.set_margin_bottom(height);
                            } else {
                                // When player bar becomes hidden, remove bottom margin from content area
                                content_area.set_margin_bottom(0);
                            }
                        }
                    });
            self.visibility_handler_id = Some(Rc::new(handler_id));
        }
    }

    /// Connects the playback controller to the player bar
    ///
    /// This method stores a reference to the playback controller and connects
    /// the UI button signals to controller methods.
    ///
    /// # Parameters
    /// * `controller` - The playback controller to connect
    pub fn connect_playback_controller(&mut self, controller: Arc<Mutex<PlaybackController>>) {
        // Store the controller for later use
        self.playback_controller = Some(controller.clone());

        // Connect play button signal to controller play/pause methods
        let play_button = self._play_button.clone();
        let controller_clone = controller.clone();
        self._play_button.connect_clicked(move |_| {
            let controller_clone = controller_clone.clone();
            let play_button = play_button.clone();
            MainContext::default().spawn_local(async move {
                let mut controller = controller_clone.lock().await;

                // Check current state and toggle between play and pause
                let current_state = controller.get_current_state().clone();
                match current_state {
                    Playing => {
                        let _ = controller.pause();
                        play_button.set_icon_name("media-playback-start");
                    }
                    _ => {
                        let _ = controller.play();
                        play_button.set_icon_name("media-playback-pause");
                    }
                }
            });
        });

        // Connect previous button signal to controller previous method
        let controller_clone = controller.clone();
        let player_bar = self.clone();
        self._prev_button.connect_clicked(move |_| {
            // Clone the controller for use in the async block
            let controller_clone = controller_clone.clone();
            let player_bar = player_bar.clone();

            // Spawn async task to handle the previous song operation
            MainContext::default().spawn_local(async move {
                // Lock the controller and play the previous song
                let mut controller = controller_clone.lock().await;

                // Before navigating, get the previous song info to update UI immediately
                let prev_song_info = controller.get_previous_song_info();

                // Update the player bar UI immediately with the previous song's metadata
                if let Some(song_info) = prev_song_info {
                    player_bar.update_with_metadata(
                        &song_info.album_title,
                        &song_info.song_title,
                        &song_info.artist_name,
                        song_info.cover_art_path.as_deref(),
                        song_info.bit_depth,
                        song_info.sample_rate,
                        song_info.format.as_deref(),
                        song_info.duration,
                    );
                }

                // Now actually navigate to the previous song
                if let Err(e) = controller.previous_song() {
                    eprintln!("Error playing previous song: {}", e);
                }

                // Update button states after navigation
                player_bar.update_navigation_button_states();
            });
        });

        // Connect next button signal to controller next method
        let controller_clone = controller.clone();
        let player_bar = self.clone();
        self._next_button.connect_clicked(move |_| {
            // Clone the controller for use in the async block
            let controller_clone = controller_clone.clone();
            let player_bar = player_bar.clone();

            // Spawn async task to handle the next song operation
            MainContext::default().spawn_local(async move {
                // Lock the controller and play the next song
                let mut controller = controller_clone.lock().await;

                // Before navigating, get the next song info to update UI immediately
                let next_song_info = controller.get_next_song_info();

                // Update the player bar UI immediately with the next song's metadata
                if let Some(song_info) = next_song_info {
                    player_bar.update_with_metadata(
                        &song_info.album_title,
                        &song_info.song_title,
                        &song_info.artist_name,
                        song_info.cover_art_path.as_deref(),
                        song_info.bit_depth,
                        song_info.sample_rate,
                        song_info.format.as_deref(),
                        song_info.duration,
                    );
                }

                // Now actually navigate to the next song
                if let Err(e) = controller.next_song() {
                    eprintln!("Error playing next song: {}", e);
                }

                // Update button states after navigation
                player_bar.update_navigation_button_states();
            });
        });

        // Update button states initially
        self.update_navigation_button_states();

        // Update the play button state based on the current playback state
        self.update_play_button_state();

        // Set up an event-driven approach using the controller's event handling.
        // This involves spawning a local asynchronous task that continuously
        // listens for playback events from the controller.
        let controller_clone = controller.clone();
        let player_bar = self.clone();
        let cancellation_token = player_bar.cancellation_token.clone();

        // Spawn a task on the GLib main context to listen for events from the controller.
        // This approach avoids blocking the UI thread and allows for efficient event processing.
        MainContext::default().spawn_local(async move {
            loop {
                // Attempt to acquire a lock on the controller without blocking.
                // This ensures the UI remains responsive even if the controller is busy.
                if let Ok(mut controller) = controller_clone.try_lock() {
                    let mut events = Vec::new();

                    // Collect all available events from the controller.
                    while let Some(event) = controller.try_get_event() {
                        events.push(event);
                    }

                    // Explicitly drop the lock guard to release the controller lock
                    // as soon as event collection is complete.
                    drop(controller);

                    // If any events were received, process them by calling the player bar's
                    // event handler.
                    if !events.is_empty() {
                        for event in events {
                            player_bar.handle_playback_event(event);
                        }
                    }
                }

                // Wait for a short duration (100ms) or until the cancellation token is triggered.
                // This prevents busy-waiting and allows the task to be gracefully shut down.
                select! {
                    // Wait for a short period
                    _ = timeout_future(Duration::from_millis(100)) => {},

                    // Break loop if cancellation is requested
                    _ = cancellation_token.cancelled() => {
                        break;
                    }
                }
            }
        });
    }
}
