//! Persistent bottom player control bar with comprehensive metadata.
//!
//! This module provides the player bar component that displays track information,
//! playback controls, volume control, and Hi-Fi audio quality indicators.

#[cfg(test)]
pub mod hifi_calculations_tests;

pub mod center_section;
pub mod hifi_calculations;
pub mod hifi_popover;
pub mod left_section;
pub mod progress_tracker;
pub mod right_section;
pub mod seek_handler;
pub mod shared_state;
pub mod state_subscription;
pub mod volume_popover;

use std::{boxed::Box, pin::Pin, sync::Arc};

use {
    libadwaita::{
        glib::MainContext,
        gtk::{
            Box as GtkBox, Button, Label, Orientation::Horizontal, Picture, Popover, Scale, Switch,
            ToggleButton,
        },
        prelude::{BoxExt, ButtonExt, PopoverExt},
    },
    tracing::error,
};

use crate::{
    audio::{
        engine::{
            AudioEngine, AudioError,
            PlaybackState::{Buffering, Paused, Playing, Ready, Stopped},
        },
        queue_manager::QueueManager,
    },
    error::audio_reporting::handle_exclusive_mode_error,
    state::app_state::AppState,
    ui::player_bar::{
        center_section::create_center_section,
        left_section::create_left_section,
        right_section::create_right_section,
        seek_handler::connect_seek_handler,
        shared_state::PlayerBarState,
        state_subscription::{StateSubscriptionContext, subscribe_to_state_changes},
        volume_popover::connect_volume_handlers,
    },
};

/// Comprehensive Hi-Fi player control center with metadata display.
///
/// The `PlayerBar` provides advanced playback controls, comprehensive
/// Hi-Fi technical metadata, status indicators, and real-time updates
/// integrated with the `AudioEngine` and `AppState`.
pub struct PlayerBar {
    /// The underlying GTK box widget.
    pub widget: GtkBox,
    /// Album artwork display.
    pub artwork: Picture,
    /// Track title label.
    pub title_label: Label,
    /// Album name label.
    pub album_label: Label,
    /// Artist name label.
    pub artist_label: Label,
    /// Compact format info label.
    pub format_label: Label,
    /// Play/pause toggle button.
    pub play_button: ToggleButton,
    /// Previous track button.
    pub prev_button: Button,
    /// Next track button.
    pub next_button: Button,
    /// Progress scale with time indicators.
    pub progress_scale: Scale,
    /// Current time label.
    pub current_time_label: Label,
    /// Total duration label.
    pub total_duration_label: Label,
    /// Volume button (clickable icon).
    pub volume_button: Button,
    /// Volume scale.
    pub volume_scale: Scale,
    /// Mute toggle button.
    pub mute_button: ToggleButton,
    /// Volume mode switch (System vs App volume).
    pub volume_mode_switch: Switch,
    /// Volume control popover.
    pub volume_popover: Popover,
    /// Hi-Fi indicator button.
    pub hifi_button: Button,
    /// Hi-Fi details popover.
    pub hifi_popover: Popover,
    /// Source format label in popover.
    pub popover_source_format: Label,
    /// Source sample rate label in popover.
    pub popover_source_rate: Label,
    /// Source bit depth label in popover.
    pub popover_source_bits: Label,
    /// Processing status label in popover.
    pub popover_processing: Label,
    /// Output device label in popover.
    pub popover_output_device: Label,
    /// Output format label in popover.
    pub popover_output_format: Label,
    /// Bit-perfect badge widget.
    pub bitperfect_badge: Label,
    /// Gapless badge widget.
    pub gapless_badge: Label,
    /// Hi-Res badge widget.
    pub hires_badge: Label,
    /// Exclusive mode indicator button.
    pub exclusive_mode_button: Button,
    /// Application state reference.
    pub app_state: Arc<AppState>,
    /// Audio engine reference.
    pub audio_engine: Arc<AudioEngine>,
    /// Queue manager reference.
    pub queue_manager: Option<Arc<QueueManager>>,
}

impl PlayerBar {
    /// Creates a new player bar instance with `AppState` integration.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference for reactive updates
    /// * `audio_engine` - Audio engine reference for playback control
    /// * `queue_manager` - Optional queue manager reference for queue navigation
    ///
    /// # Returns
    ///
    /// A new `PlayerBar` instance.
    #[must_use]
    pub fn new(
        app_state: &Arc<AppState>,
        audio_engine: &Arc<AudioEngine>,
        queue_manager: Option<&Arc<QueueManager>>,
    ) -> Self {
        let widget = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(12)
            .css_classes(["player-bar"])
            .build();

        let (left_section, artwork, title_label, album_label, artist_label, format_label) =
            create_left_section();

        widget.append(&left_section);

        let (
            center_section,
            progress_scale,
            current_time_label,
            total_duration_label,
            prev_button,
            play_button,
            next_button,
        ) = create_center_section();

        widget.append(&center_section);

        let right_section_widgets = create_right_section();

        widget.append(&right_section_widgets.container);

        let state = PlayerBarState::new();

        Self {
            widget,
            artwork,
            title_label,
            album_label,
            artist_label,
            format_label,
            play_button,
            prev_button,
            next_button,
            progress_scale,
            current_time_label,
            total_duration_label,
            volume_button: right_section_widgets.volume_button,
            volume_scale: right_section_widgets.volume_scale,
            mute_button: right_section_widgets.mute_button,
            volume_mode_switch: right_section_widgets.volume_mode_switch,
            volume_popover: right_section_widgets.volume_popover,
            hifi_button: right_section_widgets.hifi_button,
            hifi_popover: right_section_widgets.hifi_popover,
            popover_source_format: right_section_widgets.popover_source_format,
            popover_source_rate: right_section_widgets.popover_source_rate,
            popover_source_bits: right_section_widgets.popover_source_bits,
            popover_processing: right_section_widgets.popover_processing,
            popover_output_device: right_section_widgets.popover_output_device,
            popover_output_format: right_section_widgets.popover_output_format,
            bitperfect_badge: right_section_widgets.bitperfect_badge,
            gapless_badge: right_section_widgets.gapless_badge,
            hires_badge: right_section_widgets.hires_badge,
            exclusive_mode_button: right_section_widgets.exclusive_mode_button,
            app_state: app_state.clone(),
            audio_engine: audio_engine.clone(),
            queue_manager: queue_manager.cloned(),
        }
        .connect_controls(&state)
        .subscribe_to_state_changes(state)
    }

    /// Connects UI controls to audio engine methods.
    ///
    /// # Arguments
    ///
    /// * `state` - Player bar shared state
    ///
    /// # Returns
    ///
    /// Self for method chaining.
    fn connect_controls(self, state: &PlayerBarState) -> Self {
        Self::connect_play_button(&self.play_button, &self.audio_engine, &self.app_state);
        Self::connect_prev_button(&self.prev_button, self.queue_manager.as_ref());
        Self::connect_next_button(&self.next_button, self.queue_manager.as_ref());

        connect_seek_handler(
            &self.progress_scale,
            &self.current_time_label,
            self.audio_engine.clone(),
            state,
        );

        connect_volume_handlers(
            &self.volume_button,
            &self.volume_scale,
            &self.mute_button,
            &self.volume_mode_switch,
        );

        Self::connect_hifi_popover(&self.hifi_button, &self.hifi_popover);
        Self::connect_volume_popover(&self.volume_button, &self.volume_popover);
        Self::connect_exclusive_mode_button(
            &self.exclusive_mode_button,
            &self.app_state,
            &self.audio_engine,
        );

        self
    }

    /// Connects the play button to toggle playback state.
    ///
    /// # Arguments
    ///
    /// * `play_button` - The play/pause toggle button
    /// * `audio_engine` - Reference to the audio engine
    /// * `app_state` - Reference to application state
    fn connect_play_button(
        play_button: &ToggleButton,
        audio_engine: &Arc<AudioEngine>,
        app_state: &Arc<AppState>,
    ) {
        let audio_engine_clone = audio_engine.clone();
        let app_state_report = app_state.clone();

        play_button.connect_clicked(move |_| {
            let audio_engine_clone = audio_engine_clone.clone();
            let app_state_report = app_state_report.clone();

            MainContext::default().spawn_local(async move {
                let playback_state = audio_engine_clone.current_playback_state();

                match playback_state {
                    Playing => {
                        handle_playback_action(
                            &audio_engine_clone,
                            &app_state_report,
                            |ae| Box::pin(ae.pause()),
                            "pause",
                        )
                        .await;
                    }
                    Paused => {
                        handle_playback_action(
                            &audio_engine_clone,
                            &app_state_report,
                            |ae| Box::pin(ae.resume()),
                            "resume",
                        )
                        .await;
                    }
                    Ready => {
                        handle_playback_action(
                            &audio_engine_clone,
                            &app_state_report,
                            |ae| Box::pin(ae.play()),
                            "play",
                        )
                        .await;
                    }
                    Buffering | Stopped => {}
                }
            });
        });
    }

    /// Connects the previous button to navigate to the previous track.
    ///
    /// # Arguments
    ///
    /// * `prev_button` - The previous track button
    /// * `queue_manager` - Optional queue manager for navigation
    fn connect_prev_button(prev_button: &Button, queue_manager: Option<&Arc<QueueManager>>) {
        let queue_manager = queue_manager.cloned();
        prev_button.connect_clicked(move |_| {
            if let Some(qm) = &queue_manager {
                qm.previous_track();
            }
        });
    }

    /// Connects the next button to navigate to the next track.
    ///
    /// # Arguments
    ///
    /// * `next_button` - The next track button
    /// * `queue_manager` - Optional queue manager for navigation
    fn connect_next_button(next_button: &Button, queue_manager: Option<&Arc<QueueManager>>) {
        let queue_manager = queue_manager.cloned();
        next_button.connect_clicked(move |_| {
            if let Some(qm) = &queue_manager {
                qm.next_track();
            }
        });
    }

    /// Connects the Hi-Fi button to show its popover.
    ///
    /// # Arguments
    ///
    /// * `hifi_button` - The Hi-Fi indicator button
    /// * `hifi_popover` - The Hi-Fi details popover
    fn connect_hifi_popover(hifi_button: &Button, hifi_popover: &Popover) {
        let hifi_popover_clone = hifi_popover.clone();
        hifi_button.connect_clicked(move |_| {
            hifi_popover_clone.popup();
        });
    }

    /// Connects the volume button to show its popover.
    ///
    /// # Arguments
    ///
    /// * `volume_button` - The volume button widget
    /// * `volume_popover` - The volume control popover
    fn connect_volume_popover(volume_button: &Button, volume_popover: &Popover) {
        let volume_popover_clone = volume_popover.clone();
        volume_button.connect_clicked(move |_| {
            volume_popover_clone.popup();
        });
    }

    /// Connects the exclusive mode button to toggle exclusive audio mode.
    ///
    /// Updates settings, audio engine configuration, and restarts playback if needed.
    ///
    /// # Arguments
    ///
    /// * `exclusive_mode_button` - The exclusive mode toggle button
    /// * `app_state` - Reference to application state
    /// * `audio_engine` - Reference to audio engine
    fn connect_exclusive_mode_button(
        exclusive_mode_button: &Button,
        app_state: &Arc<AppState>,
        audio_engine: &Arc<AudioEngine>,
    ) {
        let app_state_clone = app_state.clone();
        let audio_engine_clone = audio_engine.clone();

        exclusive_mode_button.connect_clicked(move |_| {
            let settings_manager = app_state_clone.get_settings_manager();
            let old_value = settings_manager.read().get_settings().exclusive_mode;
            let new_value = !old_value;

            let settings_manager_write = settings_manager.write();
            let mut current_settings = settings_manager_write.get_settings().clone();
            current_settings.exclusive_mode = new_value;

            if let Err(e) = settings_manager_write.update_settings(current_settings) {
                error!(error = %e, "Failed to update exclusive mode");
                return;
            }
            drop(settings_manager_write);

            app_state_clone.update_exclusive_mode(new_value);

            if let Some(audio_engine) = app_state_clone.audio_engine.upgrade() {
                let output_config = audio_engine.output_config();
                let mut new_config = output_config;
                new_config.exclusive_mode = new_value;
                audio_engine.update_output_config(new_config);

                let audio_engine_for_restart = audio_engine_clone.clone();
                MainContext::default().spawn_local(async move {
                    if let Err(e) = audio_engine_for_restart.restart_playback().await {
                        error!(error = %e, "Failed to restart playback for exclusive mode");
                    }
                });
            }
        });
    }

    /// Subscribes to `AppState` changes for reactive updates.
    ///
    /// # Arguments
    ///
    /// * `state` - Player bar shared state
    ///
    /// # Returns
    ///
    /// Self for method chaining.
    fn subscribe_to_state_changes(self, state: PlayerBarState) -> Self {
        let context = StateSubscriptionContext {
            title_label: self.title_label.clone(),
            album_label: self.album_label.clone(),
            artist_label: self.artist_label.clone(),
            format_label: self.format_label.clone(),
            artwork: self.artwork.clone(),
            total_duration_label: self.total_duration_label.clone(),
            play_button: self.play_button.clone(),
            prev_button: self.prev_button.clone(),
            next_button: self.next_button.clone(),
            progress_scale: self.progress_scale.clone(),
            current_time_label: self.current_time_label.clone(),
            track_duration_ms: state.track_duration_ms.clone(),
            hifi_button: self.hifi_button.clone(),
            popover_source_format: self.popover_source_format.clone(),
            popover_source_rate: self.popover_source_rate.clone(),
            popover_source_bits: self.popover_source_bits.clone(),
            popover_processing: self.popover_processing.clone(),
            popover_output_device: self.popover_output_device.clone(),
            popover_output_format: self.popover_output_format.clone(),
            bitperfect_badge: self.bitperfect_badge.clone(),
            gapless_badge: self.gapless_badge.clone(),
            hires_badge: self.hires_badge.clone(),
            exclusive_mode_button: self.exclusive_mode_button.clone(),
        };

        subscribe_to_state_changes(
            self.app_state.clone(),
            self.audio_engine.clone(),
            context,
            state,
        );

        self
    }
}

/// Handles playback action execution with error reporting and exclusive mode handling.
///
/// Executes the provided async action on the audio engine, logs any errors,
/// and handles exclusive mode conflicts through the app state.
///
/// # Arguments
///
/// * `audio_engine` - Reference to the audio engine for action execution
/// * `app_state` - Reference to application state for error handling
/// * `action` - Async closure returning a playback action result
/// * `action_name` - Descriptive name for logging purposes
async fn handle_playback_action(
    audio_engine: &AudioEngine,
    app_state: &AppState,
    action: impl FnOnce(&AudioEngine) -> Pin<Box<dyn Future<Output = Result<(), AudioError>> + '_>>,
    action_name: &str,
) {
    match action(audio_engine).await {
        Ok(()) => {}
        Err(e) => {
            error!(error = %e, action = %action_name, "Playback action failed");

            // Handle exclusive mode conflicts (e.g., device in use by another application)
            if handle_exclusive_mode_error(&e, app_state) {}
        }
    }
}
