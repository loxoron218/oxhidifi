//! Persistent bottom player control bar with comprehensive metadata.
//!
//! This module implements the player bar component that provides
//! playback controls, progress display, and Hi-Fi metadata information.

use libadwaita::{
    gtk::{
        Align::Start,
        Box, Button, Label,
        Orientation::{Horizontal, Vertical},
        Picture, Scale, ToggleButton,
    },
    prelude::{BoxExt, RangeExt},
};

/// Minimal player bar placeholder with basic structure.
///
/// The `PlayerBar` provides essential playback controls and displays
/// current track information and technical metadata.
pub struct PlayerBar {
    /// The underlying GTK box widget.
    pub widget: Box,
    /// Play/pause toggle button.
    pub play_button: ToggleButton,
    /// Previous track button.
    pub prev_button: Button,
    /// Next track button.
    pub next_button: Button,
    /// Progress scale.
    pub progress_scale: Scale,
    /// Volume scale.
    pub volume_scale: Scale,
}

impl PlayerBar {
    /// Creates a new player bar instance.
    ///
    /// # Returns
    ///
    /// A new `PlayerBar` instance.
    pub fn new() -> Self {
        let widget = Box::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .css_classes(vec!["player-bar".to_string()])
            .build();

        // Album artwork placeholder
        let artwork = Picture::builder()
            .width_request(48)
            .height_request(48)
            .build();
        widget.append(&artwork);

        // Track info placeholder
        let track_info = Box::builder().orientation(Vertical).hexpand(true).build();

        let title_label = Label::builder()
            .label("Track Title")
            .halign(Start)
            .xalign(0.0)
            .build();
        track_info.append(&title_label);

        let artist_label = Label::builder()
            .label("Artist Name")
            .halign(Start)
            .xalign(0.0)
            .css_classes(vec!["dim-label".to_string()])
            .build();
        track_info.append(&artist_label);

        widget.append(&track_info);

        // Player controls
        let controls = Box::builder().orientation(Horizontal).spacing(6).build();

        let prev_button = Button::builder()
            .icon_name("media-skip-backward-symbolic")
            .tooltip_text("Previous")
            .build();
        controls.append(&prev_button);

        let play_button = ToggleButton::builder()
            .icon_name("media-playback-start-symbolic")
            .tooltip_text("Play")
            .build();
        controls.append(&play_button);

        let next_button = Button::builder()
            .icon_name("media-skip-forward-symbolic")
            .tooltip_text("Next")
            .build();
        controls.append(&next_button);

        widget.append(&controls);

        // Progress bar
        let progress_scale = Scale::builder()
            .orientation(Horizontal)
            .hexpand(true)
            .draw_value(false)
            .build();
        widget.append(&progress_scale);

        // Volume control
        let volume_scale = Scale::builder()
            .orientation(Horizontal)
            .width_request(100)
            // Remove value() from builder, set it after creation
            .draw_value(false)
            .build();
        volume_scale.set_value(100.0);
        widget.append(&volume_scale);

        Self {
            widget,
            play_button,
            prev_button,
            next_button,
            progress_scale,
            volume_scale,
        }
    }
}

impl Default for PlayerBar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use libadwaita::{init, prelude::ButtonExt};

    use crate::ui::player_bar::PlayerBar;

    #[test]
    fn test_player_bar_creation() {
        // Skip this test if we can't initialize GTK (e.g., in CI environments)
        if init().is_err() {
            return;
        }

        let player_bar = PlayerBar::new();

        // Check icon names without requiring widget realization
        assert_eq!(
            player_bar.play_button.icon_name().as_deref(),
            Some("media-playback-start-symbolic")
        );
        assert_eq!(
            player_bar.prev_button.icon_name().as_deref(),
            Some("media-skip-backward-symbolic")
        );
        assert_eq!(
            player_bar.next_button.icon_name().as_deref(),
            Some("media-skip-forward-symbolic")
        );
    }
}
