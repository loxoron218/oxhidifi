//! Album/artist cover display with DR badge overlay and accessibility support.
//!
//! This module implements the `CoverArt` component that displays album or artist
//! artwork with optional DR badge overlay, following GNOME HIG guidelines.

use std::path::Path;

use libadwaita::{
    gio::File,
    gtk::{
        AccessibleRole::Img,
        Align::{Center, End},
        Box as GtkBox,
        ContentFit::Cover,
        Orientation::Vertical,
        Picture, Widget,
    },
    prelude::{AccessibleExt, BoxExt, Cast, WidgetExt},
};

use crate::ui::components::dr_badge::{DRBadge, DRBadgeBuilder};

/// Builder pattern for configuring CoverArt components.
#[derive(Debug, Default)]
pub struct CoverArtBuilder {
    artwork_path: Option<String>,
    dr_value: Option<String>,
    show_dr_badge: bool,
    width: i32,
    height: i32,
}

impl CoverArtBuilder {
    /// Sets the path to the artwork image file.
    ///
    /// # Arguments
    ///
    /// * `artwork_path` - Path to the image file
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn artwork_path(mut self, artwork_path: impl Into<String>) -> Self {
        self.artwork_path = Some(artwork_path.into());
        self
    }

    /// Sets the DR value to display in the badge overlay.
    ///
    /// # Arguments
    ///
    /// * `dr_value` - The DR value string (e.g., "DR12")
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn dr_value(mut self, dr_value: impl Into<String>) -> Self {
        self.dr_value = Some(dr_value.into());
        self
    }

    /// Configures whether to show the DR badge overlay.
    ///
    /// # Arguments
    ///
    /// * `show_dr_badge` - Whether to show the DR badge
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn show_dr_badge(mut self, show_dr_badge: bool) -> Self {
        self.show_dr_badge = show_dr_badge;
        self
    }

    /// Sets the dimensions of the cover art display.
    ///
    /// # Arguments
    ///
    /// * `width` - Width in pixels
    /// * `height` - Height in pixels
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn dimensions(mut self, width: i32, height: i32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Builds the CoverArt component.
    ///
    /// # Returns
    ///
    /// A new `CoverArt` instance.
    pub fn build(self) -> CoverArt {
        CoverArt::new(
            self.artwork_path,
            self.dr_value,
            self.show_dr_badge,
            self.width,
            self.height,
        )
    }
}

/// Container for album/artist cover art with optional DR badge overlay.
///
/// The `CoverArt` component displays artwork images with proper aspect ratio
/// handling and optional DR quality badge overlay in the bottom-right corner.
pub struct CoverArt {
    /// The underlying GTK widget container.
    pub widget: Widget,
    /// The picture widget displaying the artwork.
    pub picture: Picture,
    /// The DR badge overlay (if enabled).
    pub dr_badge: Option<DRBadge>,
    /// The container box for the badge overlay.
    pub badge_container: Option<GtkBox>,
}

impl CoverArt {
    /// Creates a new CoverArt component.
    ///
    /// # Arguments
    ///
    /// * `artwork_path` - Optional path to the artwork image file
    /// * `dr_value` - Optional DR value for the badge overlay
    /// * `show_dr_badge` - Whether to show the DR badge overlay
    /// * `width` - Width of the cover art display
    /// * `height` - Height of the cover art display
    ///
    /// # Returns
    ///
    /// A new `CoverArt` instance.
    pub fn new(
        artwork_path: Option<String>,
        dr_value: Option<String>,
        show_dr_badge: bool,
        width: i32,
        height: i32,
    ) -> Self {
        // Create the main picture widget
        let mut picture_builder = Picture::builder()
            .halign(Center)
            .valign(Center)
            .width_request(width)
            .height_request(height)
            .content_fit(Cover);

        if let Some(path) = &artwork_path
            && Path::new(path).exists()
        {
            let file = File::for_path(path);
            picture_builder = picture_builder.file(&file);
        }

        let picture = picture_builder.build();

        // Set ARIA attributes for accessibility
        picture.set_accessible_role(Img);
        if let Some(path) = &artwork_path {
            // set_accessible_description doesn't exist in GTK4, use alternative accessibility methods
            picture.set_tooltip_text(Some(&format!("Album artwork for {}", path)));
        } else {
            picture.set_tooltip_text(Some("Default album artwork"));
        }

        let mut dr_badge = None;
        let mut badge_container = None;

        if show_dr_badge {
            // Create DR badge
            let badge = DRBadgeBuilder::default()
                .dr_value(dr_value.unwrap_or_else(|| "N/A".to_string()))
                .show_label(true)
                .build();
            dr_badge = Some(badge);

            // Create container for badge positioning
            let container = GtkBox::builder()
                .orientation(Vertical)
                .halign(End)
                .valign(End)
                .margin_end(4)
                .margin_bottom(4)
                .build();

            container.append(&dr_badge.as_ref().unwrap().widget);
            badge_container = Some(container);
        }

        // Create main container
        let main_container = GtkBox::builder()
            .orientation(Vertical)
            .halign(Center)
            .valign(Center)
            .build();

        main_container.append(picture.upcast_ref::<Widget>());

        if let Some(ref container) = badge_container {
            main_container.append(container);
        }

        Self {
            widget: main_container.upcast_ref::<Widget>().clone(),
            picture,
            dr_badge,
            badge_container,
        }
    }

    /// Creates a CoverArt builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `CoverArtBuilder` instance.
    pub fn builder() -> CoverArtBuilder {
        CoverArtBuilder::default()
    }

    /// Updates the artwork image displayed by this component.
    ///
    /// # Arguments
    ///
    /// * `artwork_path` - New path to the artwork image file
    pub fn update_artwork(&mut self, artwork_path: Option<String>) {
        if let Some(path) = artwork_path {
            if Path::new(&path).exists() {
                self.picture.set_file(Some(&File::for_path(&path)));
                self.picture
                    .set_tooltip_text(Some(&format!("Album artwork for {}", path)));
            } else {
                // Clear the image if path doesn't exist
                self.picture.set_file(None::<&File>);
                self.picture.set_tooltip_text(Some("Default album artwork"));
            }
        } else {
            // Clear the image
            self.picture.set_file(None::<&File>);
            self.picture.set_tooltip_text(Some("Default album artwork"));
        }
    }

    /// Updates the DR value displayed in the badge overlay.
    ///
    /// # Arguments
    ///
    /// * `dr_value` - New DR value string (e.g., "DR12")
    pub fn update_dr_value(&mut self, dr_value: Option<String>) {
        if let Some(ref mut badge) = self.dr_badge {
            badge.update_dr_value(dr_value);
        }
    }

    /// Shows or hides the DR badge overlay.
    ///
    /// # Arguments
    ///
    /// * `show` - Whether to show the DR badge
    pub fn set_show_dr_badge(&mut self, show: bool) {
        if show && self.dr_badge.is_none() {
            // Create badge if it doesn't exist
            let badge = DRBadgeBuilder::default()
                .dr_value("N/A".to_string())
                .show_label(true)
                .build();
            self.dr_badge = Some(badge);

            if let Some(ref container) = self.badge_container {
                container.append(&self.dr_badge.as_ref().unwrap().widget);
            }
        } else if !show && self.dr_badge.is_some() {
            // Remove badge from container
            if let Some(ref container) = self.badge_container
                && let Some(ref badge) = self.dr_badge
            {
                container.remove(&badge.widget);
            }
            self.dr_badge = None;
        }
    }
}

impl Default for CoverArt {
    fn default() -> Self {
        Self::new(None, None, false, 200, 200)
    }
}

#[cfg(test)]
mod tests {
    use libadwaita::prelude::WidgetExt;

    use crate::ui::components::cover_art::CoverArt;

    #[test]
    fn test_cover_art_builder() {
        let cover_art = CoverArt::builder()
            .artwork_path("/path/to/artwork.jpg")
            .dr_value("DR12")
            .show_dr_badge(true)
            .dimensions(150, 150)
            .build();

        assert!(cover_art.dr_badge.is_some());
        assert_eq!(cover_art.picture.width_request(), 150);
        assert_eq!(cover_art.picture.height_request(), 150);
    }

    #[test]
    fn test_cover_art_default() {
        let cover_art = CoverArt::default();
        assert!(cover_art.dr_badge.is_none());
        assert_eq!(cover_art.picture.width_request(), 200);
        assert_eq!(cover_art.picture.height_request(), 200);
    }

    #[test]
    fn test_cover_art_update_artwork() {
        let mut cover_art = CoverArt::new(None, None, false, 100, 100);

        // Test with non-existent path
        cover_art.update_artwork(Some("/non/existent/path.jpg".to_string()));

        // Should not panic and should clear the image

        // Test with None
        cover_art.update_artwork(None);

        // Should not panic and should clear the image
    }
}
