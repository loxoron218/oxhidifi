//! Play/pause button overlays with hover states and accessibility support.
//!
//! This module implements the `PlayOverlay` component that displays play/pause
//! buttons over cover art or other media elements with proper hover effects
//! and keyboard navigation support.

use libadwaita::{
    gtk::{
        Align::{Center, Fill},
        Button,
        Orientation::Horizontal,
        Widget,
    },
    prelude::{ButtonExt, WidgetExt},
};

/// Builder pattern for configuring PlayOverlay components.
#[derive(Debug, Default)]
pub struct PlayOverlayBuilder {
    is_playing: bool,
    show_on_hover: bool,
}

impl PlayOverlayBuilder {
    /// Sets the initial playing state.
    ///
    /// # Arguments
    ///
    /// * `is_playing` - Whether the overlay should show pause icon initially
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn is_playing(mut self, is_playing: bool) -> Self {
        self.is_playing = is_playing;
        self
    }

    /// Configures whether to show the overlay only on hover.
    ///
    /// # Arguments
    ///
    /// * `show_on_hover` - Whether to show overlay only on hover
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn show_on_hover(mut self, show_on_hover: bool) -> Self {
        self.show_on_hover = show_on_hover;
        self
    }

    /// Builds the PlayOverlay component.
    ///
    /// # Returns
    ///
    /// A new `PlayOverlay` instance.
    pub fn build(self) -> PlayOverlay {
        PlayOverlay::new(self.is_playing, self.show_on_hover)
    }
}

/// Play/pause button overlay with hover states and accessibility support.
///
/// The `PlayOverlay` component displays a play or pause button that can be
/// overlaid on media elements like cover art, with proper hover effects,
/// keyboard navigation, and screen reader support.
pub struct PlayOverlay {
    /// The underlying GTK widget (button).
    pub widget: Widget,
    /// The button widget.
    pub button: Button,
    /// Current playing state.
    pub is_playing: bool,
    /// Whether overlay is shown only on hover.
    pub show_on_hover: bool,
}

impl PlayOverlay {
    /// Creates a new PlayOverlay component.
    ///
    /// # Arguments
    ///
    /// * `is_playing` - Initial playing state (true = show pause icon)
    /// * `show_on_hover` - Whether to show overlay only on hover
    ///
    /// # Returns
    ///
    /// A new `PlayOverlay` instance.
    pub fn new(is_playing: bool, show_on_hover: bool) -> Self {
        let icon_name = if is_playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        };

        let button = Button::builder()
            .icon_name(icon_name)
            .halign(Center)
            .valign(Center)
            .css_classes(vec!["play-overlay".to_string()])
            .tooltip_text(if is_playing { "Pause" } else { "Play" })
            .build();

        // Set ARIA attributes for accessibility
        button.set_accessible_role(libadwaita::gtk::AccessibleRole::Button);
        button.set_accessible_description(Some(
            if is_playing {
                "Pause playback"
            } else {
                "Start playback"
            },
        ));

        // Handle hover states if show_on_hover is enabled
        if show_on_hover {
            button.set_opacity(0.0);
            
            // Connect hover events
            button.connect_enter_notify_event(|button, _| {
                button.set_opacity(1.0);
                glib::Propagation::Proceed
            });
            
            button.connect_leave_notify_event(|button, _| {
                button.set_opacity(0.0);
                glib::Propagation::Proceed
            });
        }

        let widget = button.clone().upcast::<Widget>();

        Self {
            widget,
            button,
            is_playing,
            show_on_hover,
        }
    }

    /// Creates a PlayOverlay builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `PlayOverlayBuilder` instance.
    pub fn builder() -> PlayOverlayBuilder {
        PlayOverlayBuilder::default()
    }

    /// Updates the playing state of the overlay.
    ///
    /// # Arguments
    ///
    /// * `is_playing` - New playing state (true = show pause icon)
    pub fn set_playing(&mut self, is_playing: bool) {
        if self.is_playing != is_playing {
            self.is_playing = is_playing;
            
            let icon_name = if is_playing {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            };
            
            self.button.set_icon_name(icon_name);
            self.button.set_tooltip_text(Some(
                if is_playing { "Pause" } else { "Play" },
            ));
            self.button.set_accessible_description(Some(
                if is_playing {
                    "Pause playback"
                } else {
                    "Start playback"
                },
            ));
        }
    }

    /// Shows or hides the overlay based on hover state.
    ///
    /// # Arguments
    ///
    /// * `visible` - Whether the overlay should be visible
    pub fn set_visible(&self, visible: bool) {
        if self.show_on_hover {
            self.button.set_opacity(if visible { 1.0 } else { 0.0 });
        }
    }

    /// Gets the current playing state.
    ///
    /// # Returns
    ///
    /// The current playing state.
    pub fn is_playing(&self) -> bool {
        self.is_playing
    }
}

impl Default for PlayOverlay {
    fn default() -> Self {
        Self::new(false, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_play_overlay_builder() {
        let overlay = PlayOverlay::builder()
            .is_playing(true)
            .show_on_hover(false)
            .build();
        
        assert!(overlay.is_playing);
        assert!(!overlay.show_on_hover);
        assert_eq!(overlay.button.icon_name().as_deref(), Some("media-playback-pause-symbolic"));
    }

    #[test]
    fn test_play_overlay_default() {
        let overlay = PlayOverlay::default();
        assert!(!overlay.is_playing);
        assert!(overlay.show_on_hover);
        assert_eq!(overlay.button.icon_name().as_deref(), Some("media-playback-start-symbolic"));
    }

    #[test]
    fn test_play_overlay_set_playing() {
        let mut overlay = PlayOverlay::new(false, false);
        assert!(!overlay.is_playing);
        assert_eq!(overlay.button.icon_name().as_deref(), Some("media-playback-start-symbolic"));
        
        overlay.set_playing(true);
        assert!(overlay.is_playing);
        assert_eq!(overlay.button.icon_name().as_deref(), Some("media-playback-pause-symbolic"));
        
        // Test idempotent update
        overlay.set_playing(true);
        assert!(overlay.is_playing);
        assert_eq!(overlay.button.icon_name().as_deref(), Some("media-playback-pause-symbolic"));
    }
}