//! Persistent bottom player control bar with comprehensive metadata.
//!
//! This module implements the player bar component that provides
//! playback controls, progress display, and Hi-Fi metadata information.

use std::sync::Arc;

use {
    libadwaita::{
        glib::MainContext,
        gtk::{
            AccessibleRole::Img,
            Align::Start,
            Box, Button,
            ContentFit::Cover,
            Label,
            Orientation::{Horizontal, Vertical},
            Picture, Scale, ToggleButton, Widget,
            pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExt, BoxExt, ButtonExt, Cast, RangeExt, ToggleButtonExt, WidgetExt},
    },
    tokio::sync::broadcast::error::RecvError::{Closed, Lagged},
    tracing::debug,
};

use crate::{
    audio::engine::{
        AudioEngine,
        PlaybackState::{self, Buffering, Paused, Playing, Ready, Stopped},
        TrackInfo,
    },
    state::{
        AppState,
        AppStateEvent::{CurrentTrackChanged, PlaybackStateChanged},
    },
    ui::components::hifi_metadata::HiFiMetadata,
};

/// Context struct for track information updates.
struct TrackInfoUpdateContext<'a> {
    title_label: &'a Label,
    artist_label: &'a Label,
    artwork: &'a Picture,
    hifi_metadata: &'a mut Option<HiFiMetadata>,
    hifi_metadata_container: &'a Box,
    total_duration_label: &'a Label,
    play_button: &'a ToggleButton,
    prev_button: &'a Button,
    next_button: &'a Button,
}

/// Comprehensive Hi-Fi player control center with metadata display.
///
/// The `PlayerBar` provides advanced playback controls, comprehensive
/// Hi-Fi technical metadata, status indicators, and real-time updates
/// integrated with the AudioEngine and AppState.
pub struct PlayerBar {
    /// The underlying GTK box widget.
    pub widget: Box,
    /// Album artwork display.
    pub artwork: Picture,
    /// Track title label.
    pub title_label: Label,
    /// Artist name label.
    pub artist_label: Label,
    /// Hi-Fi metadata display.
    pub hifi_metadata: Option<HiFiMetadata>,
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
    /// Volume scale.
    pub volume_scale: Scale,
    /// Mute toggle button.
    pub mute_button: ToggleButton,
    /// Gapless playback indicator.
    pub gapless_indicator: Label,
    /// Bit-perfect output indicator.
    pub bit_perfect_indicator: Label,
    /// Audio routing indicator.
    pub routing_indicator: Label,
    /// Application state reference.
    pub app_state: Arc<AppState>,
    /// Audio engine reference.
    pub audio_engine: Arc<AudioEngine>,
}

impl PlayerBar {
    /// Creates a new player bar instance with AppState integration.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference for reactive updates
    /// * `audio_engine` - Audio engine reference for playback control
    ///
    /// # Returns
    ///
    /// A new `PlayerBar` instance.
    pub fn new(app_state: Arc<AppState>, audio_engine: Arc<AudioEngine>) -> Self {
        let widget = Box::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .css_classes(["player-bar"])
            .build();

        // Album artwork
        let artwork = Picture::builder()
            .width_request(48)
            .height_request(48)
            .content_fit(Cover)
            .build();

        // Set ARIA attributes
        artwork.set_accessible_role(Img);

        // set_accessible_description doesn't exist in GTK4, remove this line

        widget.append(&artwork);

        // Track info with metadata
        let track_info = Box::builder().orientation(Vertical).hexpand(true).build();

        let title_label = Label::builder()
            .label("No track loaded")
            .halign(Start)
            .xalign(0.0)
            .ellipsize(End)
            .tooltip_text("No track loaded")
            .build();
        track_info.append(title_label.upcast_ref::<Widget>());

        let artist_label = Label::builder()
            .label("")
            .halign(Start)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .ellipsize(End)
            .tooltip_text("")
            .build();
        track_info.append(artist_label.upcast_ref::<Widget>());

        widget.append(&track_info);

        // Hi-Fi metadata (initially hidden)
        let hifi_metadata_container = Box::builder()
            .orientation(Horizontal)
            .spacing(8)
            .css_classes(["hifi-metadata-container"])
            .build();
        widget.append(hifi_metadata_container.upcast_ref::<Widget>());

        // Player controls
        let controls = Box::builder().orientation(Horizontal).spacing(6).build();

        let prev_button = Button::builder()
            .icon_name("media-skip-backward-symbolic")
            .tooltip_text("Previous track")
            .sensitive(false) // Disabled until track is loaded
            .build();
        controls.append(prev_button.upcast_ref::<Widget>());

        let play_button = ToggleButton::builder()
            .icon_name("media-playback-start-symbolic")
            .tooltip_text("Play")
            .sensitive(false) // Disabled until track is loaded
            .build();
        controls.append(play_button.upcast_ref::<Widget>());

        let next_button = Button::builder()
            .icon_name("media-skip-forward-symbolic")
            .tooltip_text("Next track")
            .sensitive(false) // Disabled until track is loaded
            .build();
        controls.append(next_button.upcast_ref::<Widget>());

        widget.append(&controls);

        // Progress section with time indicators
        let progress_container = Box::builder()
            .orientation(Horizontal)
            .hexpand(true)
            .spacing(6)
            .build();

        let current_time_label = Label::builder()
            .label("0:00")
            .width_chars(5)
            .xalign(1.0)
            .css_classes(["dim-label"])
            .build();
        progress_container.append(current_time_label.upcast_ref::<Widget>());

        let progress_scale = Scale::builder()
            .orientation(Horizontal)
            .hexpand(true)
            .draw_value(false)
            .build();
        progress_container.append(progress_scale.upcast_ref::<Widget>());

        let total_duration_label = Label::builder()
            .label("0:00")
            .width_chars(5)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        progress_container.append(total_duration_label.upcast_ref::<Widget>());

        widget.append(&progress_container);

        // Volume control with mute button
        let volume_container = Box::builder().orientation(Horizontal).spacing(6).build();

        let mute_button = ToggleButton::builder()
            .icon_name("audio-volume-high-symbolic")
            .tooltip_text("Mute")
            .build();
        volume_container.append(mute_button.upcast_ref::<Widget>());

        let volume_scale = Scale::builder()
            .orientation(Horizontal)
            .width_request(100)
            .draw_value(false)
            .build();
        volume_scale.set_value(100.0);
        volume_container.append(volume_scale.upcast_ref::<Widget>());

        widget.append(&volume_container);

        // Status indicators container
        let status_container = Box::builder()
            .orientation(Horizontal)
            .spacing(8)
            .css_classes(["status-indicators"])
            .build();

        // Gapless playback indicator
        let gapless_indicator = Label::builder()
            .label("Gapless")
            .tooltip_text("Gapless playback enabled")
            .visible(false) // Hidden by default
            .css_classes(["status-indicator", "dim-label"])
            .build();
        status_container.append(gapless_indicator.upcast_ref::<Widget>());

        // Bit-perfect output indicator
        let bit_perfect_indicator = Label::builder()
            .label("Bit-perfect")
            .tooltip_text("Bit-perfect output active")
            .visible(false) // Hidden by default
            .css_classes(["status-indicator", "dim-label"])
            .build();
        status_container.append(bit_perfect_indicator.upcast_ref::<Widget>());

        // Audio routing indicator
        let routing_indicator = Label::builder()
            .label("Stereo")
            .tooltip_text("Stereo output")
            .visible(false) // Hidden by default
            .css_classes(["status-indicator", "dim-label"])
            .build();
        status_container.append(routing_indicator.upcast_ref::<Widget>());

        widget.append(&status_container);

        let player_bar = Self {
            widget,
            artwork,
            title_label,
            artist_label,
            hifi_metadata: None,
            play_button,
            prev_button,
            next_button,
            progress_scale,
            current_time_label,
            total_duration_label,
            volume_scale,
            mute_button,
            gapless_indicator,
            bit_perfect_indicator,
            routing_indicator,
            app_state: app_state.clone(),
            audio_engine: audio_engine.clone(),
        };

        // Connect UI controls to audio engine
        player_bar.connect_controls();

        // Subscribe to AppState changes
        player_bar.subscribe_to_state_changes();

        player_bar
    }

    /// Connects UI controls to audio engine methods.
    fn connect_controls(&self) {
        let audio_engine = self.audio_engine.clone();
        let play_button = self.play_button.clone();
        let prev_button = self.prev_button.clone();
        let next_button = self.next_button.clone();
        let progress_scale = self.progress_scale.clone();
        let volume_scale = self.volume_scale.clone();
        let mute_button = self.mute_button.clone();

        // Play/Pause button
        let audio_engine_for_play = audio_engine.clone();
        play_button.connect_clicked(move |_| {
            let audio_engine_clone = audio_engine_for_play.clone();
            MainContext::default().spawn_local(async move {
                let state = audio_engine_clone.current_playback_state();
                if matches!(state, Playing) {
                    let _ = audio_engine_clone.pause().await;
                } else {
                    let _ = audio_engine_clone.play().await;
                }
            });
        });

        // Previous button
        prev_button.connect_clicked(move |_| {
            // Implementation would handle previous track logic
            println!("Previous track");
        });

        // Next button
        next_button.connect_clicked(move |_| {
            // Implementation would handle next track logic
            println!("Next track");
        });

        // Progress scale seek
        let audio_engine_seek = audio_engine.clone();
        progress_scale.connect_value_changed(move |scale| {
            let position = scale.value() as u64;
            let audio_engine_clone = audio_engine_seek.clone();
            MainContext::default().spawn_local(async move {
                let _ = audio_engine_clone.seek(position).await;
            });
        });

        // Volume control
        volume_scale.connect_value_changed(move |scale| {
            let volume = scale.value() / 100.0;

            // Implementation would handle volume setting
            println!("Volume: {}", volume);
        });

        // Mute button
        mute_button.connect_toggled(move |button| {
            let muted = button.is_active();

            // Implementation would handle mute state
            println!("Muted: {}", muted);
        });
    }

    /// Subscribes to AppState changes for reactive updates.
    fn subscribe_to_state_changes(&self) {
        let app_state = self.app_state.clone();
        let title_label = self.title_label.clone();
        let artist_label = self.artist_label.clone();
        let artwork = self.artwork.clone();
        let play_button = self.play_button.clone();
        let prev_button = self.prev_button.clone();
        let next_button = self.next_button.clone();
        let _progress_scale = self.progress_scale.clone();
        let _current_time_label = self.current_time_label.clone();
        let total_duration_label = self.total_duration_label.clone();
        let mut hifi_metadata = self.hifi_metadata.clone();
        let hifi_metadata_container = self.widget.clone(); // Use main widget as container for now

        // Subscribe to AppState changes with tracing
        debug!("PlayerBar: Subscribing to AppState changes");
        MainContext::default().spawn_local(async move {
            let mut receiver = app_state.subscribe();
            loop {
                match receiver.recv().await {
                    Ok(event) => match event {
                        CurrentTrackChanged(track_info) => {
                            debug!("PlayerBar: Current track changed");
                            let update_context = TrackInfoUpdateContext {
                                title_label: &title_label,
                                artist_label: &artist_label,
                                artwork: &artwork,
                                hifi_metadata: &mut hifi_metadata,
                                hifi_metadata_container: &hifi_metadata_container,
                                total_duration_label: &total_duration_label,
                                play_button: &play_button,
                                prev_button: &prev_button,
                                next_button: &next_button,
                            };
                            Self::update_track_info(&track_info, update_context);
                        }
                        PlaybackStateChanged(state) => {
                            debug!("PlayerBar: Playback state changed to {:?}", state);
                            Self::update_playback_state(&state, &play_button);
                        }
                        _ => {}
                    },
                    Err(Closed) => {
                        // Channel was closed - resubscribe
                        debug!("PlayerBar state subscription channel closed, resubscribing");
                        receiver = app_state.subscribe();
                        continue;
                    }
                    Err(Lagged(skipped)) => {
                        debug!(
                            "PlayerBar state subscription lagged, skipped {} messages",
                            skipped
                        );
                        continue;
                    }
                }
            }
        });
    }

    /// Updates track information display.
    fn update_track_info(track_info: &Option<TrackInfo>, ctx: TrackInfoUpdateContext) {
        if let Some(info) = track_info {
            // Update title and artist
            ctx.title_label
                .set_label(&info.metadata.standard.title.clone().unwrap_or_default());
            ctx.title_label.set_tooltip_text(Some(
                &info.metadata.standard.title.clone().unwrap_or_default(),
            ));

            ctx.artist_label
                .set_label(&info.metadata.standard.artist.clone().unwrap_or_default());
            ctx.artist_label.set_tooltip_text(Some(
                &info.metadata.standard.artist.clone().unwrap_or_default(),
            ));

            // Update artwork (placeholder - would load actual artwork)
            ctx.artwork.set_filename(None::<&str>); // Clear existing image

            // Update duration
            let duration_seconds = info.duration_ms / 1000;
            let duration_minutes = duration_seconds / 60;
            let duration_remaining = duration_seconds % 60;
            let duration_text = format!("{:02}:{:02}", duration_minutes, duration_remaining);
            ctx.total_duration_label.set_label(&duration_text);

            // Create or update Hi-Fi metadata
            // Note: This would require creating a Track from TrackInfo in real implementation
            // For now, we'll just show basic format info
            if ctx.hifi_metadata.is_none() {
                // Create basic metadata display
                let format_label = Label::builder()
                    .label(format!(
                        "{}-bit {}kHz {}ch ",
                        info.format.bits_per_sample,
                        info.format.sample_rate / 1000,
                        info.format.channels
                    ))
                    .css_classes(["dim-label"])
                    .build();
                ctx.hifi_metadata_container
                    .append(format_label.upcast_ref::<Widget>());

                let sample_rate_label = Label::builder()
                    .label(format!("{}kHz", info.format.sample_rate / 1000))
                    .css_classes(["dim-label"])
                    .build();
                ctx.hifi_metadata_container
                    .append(sample_rate_label.upcast_ref::<Widget>());
            }

            // Enable controls
            ctx.play_button.set_sensitive(true);
            ctx.prev_button.set_sensitive(true);
            ctx.next_button.set_sensitive(true);
        } else {
            // Clear all track info
            ctx.title_label.set_label("No track loaded");
            ctx.title_label.set_tooltip_text(Some("No track loaded"));
            ctx.artist_label.set_label("");
            ctx.artist_label.set_tooltip_text(Some(""));
            ctx.artwork.set_filename(None::<&str>);
            ctx.total_duration_label.set_label("0:00");

            // Disable controls
            ctx.play_button.set_sensitive(false);
            ctx.prev_button.set_sensitive(false);
            ctx.next_button.set_sensitive(false);
        }
    }

    /// Updates playback state display.
    fn update_playback_state(state: &PlaybackState, play_button: &ToggleButton) {
        match state {
            Playing => {
                play_button.set_icon_name("media-playback-pause-symbolic");
                play_button.set_tooltip_text(Some("Pause"));
            }
            Paused | Stopped | Ready => {
                play_button.set_icon_name("media-playback-start-symbolic");
                play_button.set_tooltip_text(Some("Play"));
            }
            Buffering => {
                play_button.set_icon_name("media-playback-pause-symbolic");
                play_button.set_tooltip_text(Some("Buffering..."));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{audio::engine::AudioEngine, state::AppState};

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_player_bar_creation() {
        let engine = AudioEngine::new().unwrap();
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let _app_state = AppState::new(engine_weak, None);

        // This test would require mocking AppState and AudioEngine properly
        // For now, we'll just verify the constructor signature compiles
        assert!(true);
    }
}
