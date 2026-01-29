//! Album/artist cover display with DR badge overlay and accessibility support.
//!
//! This module implements the `CoverArt` component that displays album or artist
//! artwork with optional DR badge overlay, following GNOME HIG guidelines.

use std::path::Path;

use {
    libadwaita::{
        gio::File,
        gtk::{
            AccessibleRole::Img, Align::Center, ContentFit::Cover, Image, Overlay, Picture,
            PolicyType::Never, ScrolledWindow, Widget,
        },
        prelude::{AccessibleExt, Cast, FileExt, IsA, WidgetExt},
    },
    tracing::warn,
};

use crate::ui::components::dr_badge::{DRBadge, DRBadgeBuilder};

/// Builder pattern for configuring `CoverArt` components.
#[derive(Debug, Default)]
pub struct CoverArtBuilder {
    /// Path to the artwork image file.
    artwork_path: Option<String>,
    /// GTK icon name to display when no artwork is provided.
    icon_name: Option<String>,
    /// DR value string to display in the badge overlay (e.g., "DR12").
    dr_value: Option<String>,
    /// Whether to show the DR badge overlay on the cover.
    show_dr_badge: bool,
    /// Width of the cover art display in pixels.
    width: i32,
    /// Height of the cover art display in pixels.
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
    #[must_use]
    pub fn artwork_path<S: Into<String>>(mut self, artwork_path: impl Into<Option<S>>) -> Self {
        self.artwork_path = artwork_path.into().map(Into::into);
        self
    }

    /// Sets the GTK icon name to display when no artwork is provided.
    ///
    /// # Arguments
    ///
    /// * `icon_name` - GTK icon name (e.g., "avatar-default-symbolic")
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn icon_name<S: Into<String>>(mut self, icon_name: impl Into<Option<S>>) -> Self {
        self.icon_name = icon_name.into().map(Into::into);
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
    #[must_use]
    pub fn dr_value<S: Into<String>>(mut self, dr_value: impl Into<Option<S>>) -> Self {
        self.dr_value = dr_value.into().map(Into::into);
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
    #[must_use]
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
    #[must_use]
    pub fn dimensions(mut self, width: i32, height: i32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Builds the `CoverArt` component.
    ///
    /// # Returns
    ///
    /// A new `CoverArt` instance.
    #[must_use]
    pub fn build(self) -> CoverArt {
        CoverArt::new(
            self.artwork_path.as_ref(),
            self.icon_name.as_ref(),
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
#[derive(Clone)]
pub struct CoverArt {
    /// The underlying GTK widget container.
    pub widget: Widget,
    /// The picture widget displaying the artwork (for images).
    pub picture: Option<Picture>,
    /// The image widget displaying the icon (for icons).
    pub image: Option<Image>,
    /// The DR badge overlay (if enabled).
    pub dr_badge: Option<DRBadge>,
    /// The current DR value for this cover art.
    pub dr_value: String,
}

impl CoverArt {
    fn build_overlay<W: IsA<Widget>>(child: Option<&W>, width: i32, height: i32) -> Overlay {
        let mut builder = Overlay::builder()
            .halign(Center)
            .valign(Center)
            .hexpand(false)
            .vexpand(false)
            .width_request(width)
            .height_request(height)
            .css_classes(["cover-art-container"]);

        if let Some(c) = child {
            builder = builder.child(c);
        }

        builder.build()
    }

    /// Creates a new `CoverArt` component.
    ///
    /// Display priority: `artwork_path` > `icon_name` > default placeholder
    ///
    /// # Arguments
    ///
    /// * `artwork_path` - Optional path to the artwork image file (highest priority; used if file exists)
    /// * `icon_name` - Optional GTK icon name to display when no artwork is provided (used if `artwork_path` is None or file doesn't exist)
    /// * `dr_value` - Optional DR value for the badge overlay
    /// * `show_dr_badge` - Whether to show the DR badge overlay
    /// * `width` - Width of the cover art display
    /// * `height` - Height of the cover art display
    ///
    /// # Returns
    ///
    /// A new `CoverArt` instance.
    #[must_use]
    pub fn new(
        artwork_path: Option<&String>,
        icon_name: Option<&String>,
        dr_value: Option<String>,
        show_dr_badge: bool,
        width: i32,
        height: i32,
    ) -> Self {
        let (image, picture) = if let Some(path) = &artwork_path
            && Path::new(path).exists()
        {
            let file = File::for_path(path);

            if file.peek_path().is_some() {
                let pic = Picture::builder()
                    .content_fit(Cover)
                    .css_classes(["cover-art-picture"])
                    .file(&file)
                    .build();
                pic.set_accessible_role(Img);
                pic.set_tooltip_text(Some(&format!("Album artwork for {path}")));
                (None, Some(pic))
            } else {
                warn!("Failed to load artwork from {path}: file path not accessible");
                (None, None)
            }
        } else if let Some(icon) = &icon_name {
            let img = Image::builder()
                .icon_name(icon.as_str())
                .pixel_size(height.max(width))
                .css_classes(["cover-art-picture"])
                .build();
            img.set_accessible_role(Img);
            img.set_halign(Center);
            img.set_valign(Center);
            (Some(img), None)
        } else {
            let pic = Picture::builder()
                .content_fit(Cover)
                .css_classes(["cover-art-picture"])
                .build();
            pic.set_accessible_role(Img);
            pic.set_tooltip_text(Some("Default album artwork"));
            (None, Some(pic))
        };

        let mut dr_badge = None;

        if show_dr_badge {
            // Create DR badge
            let badge = DRBadgeBuilder::default()
                .dr_value(dr_value.clone().unwrap_or_else(|| "N/A".to_string()))
                .show_label(false) // Don't show "DR" prefix in grid view
                .build();
            dr_badge = Some(badge);
        }

        let overlay = if let Some(pic) = &picture {
            // Create ScrolledWindow to enforce strict sizing and break request propagation
            // This acts as a clipping container for the Picture
            let scrolled_window = ScrolledWindow::builder()
                .hscrollbar_policy(Never)
                .vscrollbar_policy(Never)
                .width_request(width)
                .height_request(height)
                .propagate_natural_width(false)
                .propagate_natural_height(false)
                .has_frame(false)
                .min_content_width(width)
                .min_content_height(height)
                .child(pic)
                .build();

            // Create overlay container
            // Overlay holds the ScrolledWindow (which holds the Picture) and the Badge
            Self::build_overlay(Some(&scrolled_window), width, height)
        } else if let Some(img) = &image {
            // For icons, use Image directly in Overlay
            Self::build_overlay(Some(img), width, height)
        } else {
            // Fallback - should never happen
            Self::build_overlay(None::<&ScrolledWindow>, width, height)
        };

        if let Some(ref badge) = dr_badge {
            // Add DR badge as overlay - it will align to the Overlay's bounds
            overlay.add_overlay(&badge.widget);
        }

        let stored_dr_value = dr_value.unwrap_or_else(|| "N/A".to_string());

        Self {
            widget: overlay.upcast_ref::<Widget>().clone(),
            picture,
            image,
            dr_badge,
            dr_value: stored_dr_value,
        }
    }

    /// Creates a `CoverArt` builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `CoverArtBuilder` instance.
    #[must_use]
    pub fn builder() -> CoverArtBuilder {
        CoverArtBuilder::default()
    }

    /// Updates the artwork image displayed by this component.
    ///
    /// # Arguments
    ///
    /// * `artwork_path` - New path to the artwork image file
    ///
    /// # Note
    ///
    /// If called on a `CoverArt` in icon mode, this will transition to picture mode
    /// by rebuilding the widget structure. The transition is logged with a warning.
    ///
    /// # Panics
    ///
    /// Panics if the `CoverArt` widget is not an Overlay (should never happen with proper widget construction).
    pub fn update_artwork(&mut self, artwork_path: Option<String>) {
        if let Some(path) = artwork_path {
            if Path::new(&path).exists() {
                let file = File::for_path(&path);

                if file.peek_path().is_some() {
                    if let Some(ref pic) = self.picture {
                        pic.set_file(Some(&file));
                        pic.set_tooltip_text(Some(&format!("Album artwork for {path}")));
                    } else if self.image.is_some() {
                        warn!("Transitioning CoverArt from icon to picture mode");

                        let overlay = self
                            .widget
                            .downcast_ref::<Overlay>()
                            .expect("CoverArt widget should be an Overlay");

                        self.image.take();

                        overlay.set_child(None::<&Widget>);

                        let pic = Picture::builder()
                            .content_fit(Cover)
                            .css_classes(["cover-art-picture"])
                            .file(&file)
                            .build();
                        pic.set_accessible_role(Img);
                        pic.set_tooltip_text(Some(&format!("Album artwork for {path}")));

                        let width = overlay.width_request();
                        let height = overlay.height_request();

                        let scrolled_window = ScrolledWindow::builder()
                            .hscrollbar_policy(Never)
                            .vscrollbar_policy(Never)
                            .width_request(width)
                            .height_request(height)
                            .propagate_natural_width(false)
                            .propagate_natural_height(false)
                            .has_frame(false)
                            .min_content_width(width)
                            .min_content_height(height)
                            .child(&pic)
                            .build();

                        overlay.set_child(Some(&scrolled_window));

                        self.picture = Some(pic);
                    }
                } else {
                    warn!("Failed to load artwork from {path}: file path not accessible");
                    if let Some(ref pic) = self.picture {
                        pic.set_file(None::<&File>);
                        pic.set_tooltip_text(Some("Default album artwork"));
                    }
                }
            } else if let Some(ref pic) = self.picture {
                pic.set_file(None::<&File>);
                pic.set_tooltip_text(Some("Default album artwork"));
            }
        } else if let Some(ref pic) = self.picture {
            pic.set_file(None::<&File>);
            pic.set_tooltip_text(Some("Default album artwork"));
        }
    }

    /// Updates the DR value displayed in the badge overlay.
    ///
    /// # Arguments
    ///
    /// * `dr_value` - New DR value string (e.g., "DR12")
    pub fn update_dr_value(&mut self, dr_value: Option<String>) {
        let new_dr_value = dr_value.clone().unwrap_or_else(|| "N/A".to_string());
        self.dr_value = new_dr_value;

        if let Some(ref mut badge) = self.dr_badge {
            badge.update_dr_value(dr_value);
        }
    }

    /// Shows or hides the DR badge overlay.
    ///
    /// # Arguments
    ///
    /// * `show` - Whether to show the DR badge
    ///
    /// # Panics
    ///
    /// Panics if the `CoverArt` widget is not an Overlay (should never happen with proper widget construction).
    pub fn set_show_dr_badge(&mut self, show: bool) {
        let overlay = self
            .widget
            .downcast_ref::<Overlay>()
            .expect("CoverArt widget should be an Overlay");

        if show {
            if self.dr_badge.is_none() {
                // Create and add DR badge if it doesn't exist
                let badge = DRBadgeBuilder::default()
                    .dr_value(self.dr_value.clone())
                    .show_label(false) // Don't show "DR" prefix in grid view
                    .build();
                overlay.add_overlay(&badge.widget);
                self.dr_badge = Some(badge);
            } else {
                // Ensure existing badge is visible
                if let Some(ref badge) = self.dr_badge {
                    badge.widget.set_visible(true);
                }
            }
        } else if let Some(ref badge) = self.dr_badge {
            // Remove badge from overlay and clear reference
            overlay.remove_overlay(&badge.widget);
            self.dr_badge = None;
        }
    }

    /// Updates the dimensions of the cover art display.
    ///
    /// # Arguments
    ///
    /// * `width` - New width in pixels
    /// * `height` - New height in pixels
    ///
    /// # Panics
    ///
    /// Panics if the `CoverArt` widget is not an Overlay (should never happen with proper widget construction).
    pub fn update_dimensions(&self, width: i32, height: i32) {
        let overlay = self
            .widget
            .downcast_ref::<Overlay>()
            .expect("CoverArt widget should be an Overlay");

        overlay.set_width_request(width);
        overlay.set_height_request(height);

        // Update ScrolledWindow dimensions if using picture
        if self.picture.is_some()
            && let Some(child) = overlay.child()
            && let Ok(scrolled_window) = child.downcast::<ScrolledWindow>()
        {
            scrolled_window.set_width_request(width);
            scrolled_window.set_height_request(height);
            scrolled_window.set_min_content_width(width);
            scrolled_window.set_min_content_height(height);
        }

        // Update Image pixel size if using icon
        if let Some(ref img) = self.image {
            img.set_pixel_size(height.max(width));
        }
    }
}

impl Default for CoverArt {
    fn default() -> Self {
        Self::new(None, None, None, false, 200, 200)
    }
}

#[cfg(test)]
mod tests {
    use libadwaita::prelude::WidgetExt;

    use crate::ui::components::cover_art::CoverArt;

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_cover_art_builder() {
        let cover_art = CoverArt::builder()
            .artwork_path("/path/to/artwork.jpg")
            .dr_value("DR12")
            .show_dr_badge(true)
            .dimensions(150, 150)
            .build();

        assert!(cover_art.dr_badge.is_some());
        assert!(cover_art.picture.is_some());
        assert_eq!(cover_art.picture.as_ref().unwrap().width_request(), 150);
        assert_eq!(cover_art.picture.as_ref().unwrap().height_request(), 150);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_cover_art_default() {
        let cover_art = CoverArt::default();
        assert!(cover_art.dr_badge.is_none());
        assert!(cover_art.picture.is_some());
        assert_eq!(cover_art.picture.as_ref().unwrap().width_request(), 200);
        assert_eq!(cover_art.picture.as_ref().unwrap().height_request(), 200);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_cover_art_update_artwork() {
        let mut cover_art = CoverArt::new(None, None, None, false, 100, 100);

        // Test with non-existent path
        cover_art.update_artwork(Some("/non/existent/path.jpg".to_string()));

        // Should not panic and should clear the image

        // Test with None
        cover_art.update_artwork(None);

        // Should not panic and should clear the image
    }
}
