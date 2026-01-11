//! Play/pause button overlays with hover states and accessibility support.
//!
//! This module implements the `PlayOverlay` component that displays play/pause
//! buttons over cover art or other media elements with proper hover effects
//! and keyboard navigation support.

use libadwaita::{
    gtk::{
        AccessibleRole::Button as AccessibleButton, Align::Center, Button, EventControllerMotion,
        Widget,
    },
    prelude::{AccessibleExt, ButtonExt, Cast, WidgetExt},
};

/// Builder pattern for configuring `PlayOverlay` components.
#[derive(Debug, Default)]
pub struct PlayOverlayBuilder {
    /// Whether the overlay should show pause icon initially.
    is_playing: bool,
    /// Whether to show overlay only on hover.
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
    #[must_use]
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
    #[must_use]
    pub fn show_on_hover(mut self, show_on_hover: bool) -> Self {
        self.show_on_hover = show_on_hover;
        self
    }

    /// Builds the `PlayOverlay` component.
    ///
    /// # Returns
    ///
    /// A new `PlayOverlay` instance.
    #[must_use]
    pub fn build(self) -> PlayOverlay {
        PlayOverlay::new(self.is_playing, self.show_on_hover)
    }
}

/// Play/pause button overlay with hover states and accessibility support.
///
/// The `PlayOverlay` component displays a play or pause button that can be
/// overlaid on media elements like cover art, with proper hover effects,
/// keyboard navigation, and screen reader support.
#[derive(Clone)]
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
    /// Creates a new `PlayOverlay` component.
    ///
    /// # Arguments
    ///
    /// * `is_playing` - Initial playing state (true = show pause icon)
    /// * `show_on_hover` - Whether to show overlay only on hover
    ///
    /// # Returns
    ///
    /// A new `PlayOverlay` instance.
    #[must_use]
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
        button.set_accessible_role(AccessibleButton);

        // set_accessible_description doesn't exist in GTK4
        // Accessibility is handled through other means

        // Handle hover states if show_on_hover is enabled
        if show_on_hover {
            button.set_opacity(0.0);

            // Connect hover events
            // Use EventControllerMotion for hover events
            let motion_controller = EventControllerMotion::new();
            let button_clone1 = button.clone();
            motion_controller.connect_enter(move |_, _, _| {
                button_clone1.set_opacity(1.0);
            });
            let button_clone2 = button.clone();
            motion_controller.connect_leave(move |_| {
                button_clone2.set_opacity(0.0);
            });
            button.add_controller(motion_controller);
        }

        let binding = button.clone();
        let widget = binding.upcast_ref::<Widget>();

        Self {
            widget: widget.clone(),
            button,
            is_playing,
            show_on_hover,
        }
    }

    /// Creates a `PlayOverlay` builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `PlayOverlayBuilder` instance.
    #[must_use]
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
            self.button
                .set_tooltip_text(Some(if is_playing { "Pause" } else { "Play" }));

            // set_accessible_description doesn't exist in GTK4
            // Accessibility is handled through other means
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
    #[must_use]
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
    use libadwaita::prelude::ButtonExt;

    use crate::ui::components::play_overlay::PlayOverlay;

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_play_overlay_builder() {
        let overlay = PlayOverlay::builder()
            .is_playing(true)
            .show_on_hover(false)
            .build();

        assert!(overlay.is_playing);
        assert!(!overlay.show_on_hover);
        assert_eq!(
            overlay.button.icon_name().as_deref(),
            Some("media-playback-pause-symbolic")
        );
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_play_overlay_default() {
        let overlay = PlayOverlay::default();
        assert!(!overlay.is_playing);
        assert!(overlay.show_on_hover);
        assert_eq!(
            overlay.button.icon_name().as_deref(),
            Some("media-playback-start-symbolic")
        );
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_play_overlay_set_playing() {
        let mut overlay = PlayOverlay::new(false, false);
        assert!(!overlay.is_playing);
        assert_eq!(
            overlay.button.icon_name().as_deref(),
            Some("media-playback-start-symbolic")
        );

        overlay.set_playing(true);
        assert!(overlay.is_playing);
        assert_eq!(
            overlay.button.icon_name().as_deref(),
            Some("media-playback-pause-symbolic")
        );

        // Test idempotent update
        overlay.set_playing(true);
        assert!(overlay.is_playing);
        assert_eq!(
            overlay.button.icon_name().as_deref(),
            Some("media-playback-pause-symbolic")
        );
    }
}
