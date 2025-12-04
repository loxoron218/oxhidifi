//! Persistent bottom player control bar with comprehensive metadata.
//!
//! This module implements the player bar component that provides
//! playback controls, progress display, and Hi-Fi metadata information.

use libadwaita::gtk::prelude::*;

/// Minimal player bar placeholder with basic structure.
///
/// The `PlayerBar` provides essential playback controls and displays
/// current track information and technical metadata.
pub struct PlayerBar {
    /// The underlying GTK box widget.
    pub widget: libadwaita::gtk::Box,
    /// Play/pause toggle button.
    pub play_button: libadwaita::gtk::ToggleButton,
    /// Previous track button.
    pub prev_button: libadwaita::gtk::Button,
    /// Next track button.
    pub next_button: libadwaita::gtk::Button,
    /// Progress scale.
    pub progress_scale: libadwaita::gtk::Scale,
    /// Volume scale.
    pub volume_scale: libadwaita::gtk::Scale,
}

impl PlayerBar {
    /// Creates a new player bar instance.
    ///
    /// # Returns
    ///
    /// A new `PlayerBar` instance.
    pub fn new() -> Self {
        let widget = libadwaita::gtk::Box::builder()
            .orientation(libadwaita::gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .css_classes(vec!["player-bar".to_string()])
            .build();

        // Album artwork placeholder
        let artwork = libadwaita::gtk::Picture::builder()
            .width_request(48)
            .height_request(48)
            .build();
        widget.append(&artwork);

        // Track info placeholder
        let track_info = libadwaita::gtk::Box::builder()
            .orientation(libadwaita::gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        
        let title_label = libadwaita::gtk::Label::builder()
            .label("Track Title")
            .halign(libadwaita::gtk::Align::Start)
            .xalign(0.0)
            .build();
        track_info.append(&title_label);
        
        let artist_label = libadwaita::gtk::Label::builder()
            .label("Artist Name")
            .halign(libadwaita::gtk::Align::Start)
            .xalign(0.0)
            .css_classes(vec!["dim-label".to_string()])
            .build();
        track_info.append(&artist_label);
        
        widget.append(&track_info);

        // Player controls
        let controls = libadwaita::gtk::Box::builder()
            .orientation(libadwaita::gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        
        let prev_button = libadwaita::gtk::Button::builder()
            .icon_name("media-skip-backward-symbolic")
            .tooltip_text("Previous")
            .build();
        controls.append(&prev_button);
        
        let play_button = libadwaita::gtk::ToggleButton::builder()
            .icon_name("media-playback-start-symbolic")
            .tooltip_text("Play")
            .build();
        controls.append(&play_button);
        
        let next_button = libadwaita::gtk::Button::builder()
            .icon_name("media-skip-forward-symbolic")
            .tooltip_text("Next")
            .build();
        controls.append(&next_button);
        
        widget.append(&controls);

        // Progress bar
        let progress_scale = libadwaita::gtk::Scale::builder()
            .orientation(libadwaita::gtk::Orientation::Horizontal)
            .hexpand(true)
            .draw_value(false)
            .build();
        widget.append(&progress_scale);

        // Volume control
        let volume_scale = libadwaita::gtk::Scale::builder()
            .orientation(libadwaita::gtk::Orientation::Horizontal)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_bar_creation() {
        libadwaita::gtk::init().unwrap_or(());
        let player_bar = PlayerBar::new();
        assert!(player_bar.widget.is_valid());
        assert_eq!(player_bar.play_button.icon_name(), Some("media-playback-start-symbolic"));
        assert_eq!(player_bar.prev_button.icon_name(), Some("media-skip-backward-symbolic"));
        assert_eq!(player_bar.next_button.icon_name(), Some("media-skip-forward-symbolic"));
    }
}