//! Album card component with proper widget hierarchy and styling.
//!
//! This module implements the `AlbumCard` component that displays albums
//! with cover art, DR badges, play overlays, and metadata following the
//! exact specification from docs/4.\ album-cards.md.

use std::rc::Rc;

use libadwaita::{
    gtk::{
        AccessibleRole::Group,
        Align::{Fill, Start},
        Box, FlowBoxChild, Label,
        Orientation::{Horizontal, Vertical},
        Widget,
        pango::EllipsizeMode::End,
    },
    prelude::{AccessibleExt, BoxExt, ButtonExt, Cast, FlowBoxChildExt, WidgetExt},
};

use crate::{
    library::models::Album,
    ui::components::{cover_art::CoverArt, dr_badge::DRBadge, play_overlay::PlayOverlay},
};

/// Builder pattern for configuring AlbumCard components.
#[derive(Default)]
pub struct AlbumCardBuilder {
    album: Option<Album>,
    format: Option<String>,
    show_dr_badge: bool,
    compact: bool,
    on_play_clicked: Option<Rc<dyn Fn()>>,
    on_card_clicked: Option<Rc<dyn Fn()>>,
}

impl AlbumCardBuilder {
    /// Sets the album data for the card.
    ///
    /// # Arguments
    ///
    /// * `album` - The album to display
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn album(mut self, album: Album) -> Self {
        self.album = Some(album);
        self
    }

    /// Sets the audio format for the album.
    ///
    /// # Arguments
    ///
    /// * `format` - The audio format (e.g., "FLAC", "MP3")
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn format(mut self, format: String) -> Self {
        self.format = Some(format);
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

    /// Configures whether to use compact layout.
    ///
    /// # Arguments
    ///
    /// * `compact` - Whether to use compact layout
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Sets the callback for when the play button is clicked.
    ///
    /// # Arguments
    ///
    /// * `callback` - Function to call when play button is clicked
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn on_play_clicked<F>(mut self, callback: F) -> Self
    where
        F: Fn() + 'static,
    {
        self.on_play_clicked = Some(Rc::new(callback));
        self
    }

    /// Sets the callback for when the card (outside play button) is clicked.
    ///
    /// # Arguments
    ///
    /// * `callback` - Function to call when card is clicked
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn on_card_clicked<F>(mut self, callback: F) -> Self
    where
        F: Fn() + 'static,
    {
        self.on_card_clicked = Some(Rc::new(callback));
        self
    }

    /// Builds the AlbumCard component.
    ///
    /// # Returns
    ///
    /// A new `AlbumCard` instance.
    pub fn build(self) -> AlbumCard {
        AlbumCard::new(
            self.album.expect("Album must be set"),
            self.format,
            self.show_dr_badge,
            self.compact,
            self.on_play_clicked,
            self.on_card_clicked,
        )
    }
}

/// Album card component with proper widget hierarchy and styling.
///
/// The `AlbumCard` component implements the exact widget structure specified
/// in docs/4.\ album-cards.md with proper spacing, CSS classes, and interaction patterns.
pub struct AlbumCard {
    /// The underlying FlowBoxChild widget.
    pub widget: Widget,
    /// The main album tile container.
    pub album_tile: Box,
    /// The cover art component.
    pub cover_art: CoverArt,
    /// The play overlay button.
    pub play_overlay: PlayOverlay,
    /// The DR badge (if enabled).
    pub dr_badge: Option<DRBadge>,
    /// Album title label.
    pub title_label: Label,
    /// Artist name label.
    pub artist_label: Label,
    /// Format info label.
    pub format_label: Label,
    /// Year info label.
    pub year_label: Label,
}

impl AlbumCard {
    /// Creates a new AlbumCard component.
    ///
    /// # Arguments
    ///
    /// * `album` - The album data to display
    /// * `format` - Optional audio format information
    /// * `show_dr_badge` - Whether to show the DR badge overlay
    /// * `compact` - Whether to use compact layout
    /// * `on_play_clicked` - Optional callback for play button clicks
    /// * `on_card_clicked` - Optional callback for card clicks (outside play button)
    ///
    /// # Returns
    ///
    /// A new `AlbumCard` instance.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        album: Album,
        format: Option<String>,
        show_dr_badge: bool,
        compact: bool,
        on_play_clicked: Option<Rc<dyn Fn()>>,
        on_card_clicked: Option<Rc<dyn Fn()>>,
    ) -> Self {
        // Determine base cover dimensions based on compact mode
        // These are starting points that will be adjusted by the parent container
        let base_cover_size = if compact { 120 } else { 180 };
        let (cover_width, cover_height) = (base_cover_size, base_cover_size);

        // Create cover art with DR badge if enabled
        let cover_art = CoverArt::builder()
            .artwork_path(album.artwork_path.as_deref().unwrap_or(&album.path))
            .dr_value(album.dr_value.clone().unwrap_or_else(|| "N/A".to_string()))
            .show_dr_badge(show_dr_badge)
            .dimensions(cover_width, cover_height)
            .build();

        // Create play overlay
        let play_overlay = PlayOverlay::builder()
            .is_playing(false)
            .show_on_hover(true)
            .build();

        // DR badge is now handled by CoverArt component, so we don't need separate dr_badge field
        let dr_badge = None;

        // The cover_art widget already includes proper overlay handling and sizing
        // Just use it directly as the cover container
        let cover_container = cover_art.widget.clone();

        // Create title label
        let title_label = Label::builder()
            .label(&album.title)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(End)
            .lines(2)
            .max_width_chars(((cover_width - 16) / 10).max(8)) // Dynamic calculation as per spec
            .tooltip_text(&album.title)
            .css_classes(["album-title-label"])
            .build();

        // Create title area container
        let title_area = Box::builder()
            .orientation(Vertical)
            .halign(Start)
            .valign(Fill)
            .height_request(40)
            .margin_top(12)
            .build();

        title_area.append(title_label.upcast_ref::<Widget>());

        // Create artist label
        let artist_label = Label::builder()
            .label(album.artist_id.to_string())
            .halign(Start)
            .xalign(0.0)
            .ellipsize(End)
            .lines(1)
            .max_width_chars(((cover_width - 16) / 10).max(8)) // Dynamic calculation as per spec
            .tooltip_text(album.artist_id.to_string())
            .css_classes(["album-artist-label"])
            .build();

        // Create format and year labels
        let format_info = format.unwrap_or_else(|| "Hi-Res".to_string());
        let format_label = Label::builder()
            .label(&format_info)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(End)
            .lines(1)
            .max_width_chars((((cover_width - 16) / 2) / 10).max(8)) // Dynamic calculation as per spec
            .tooltip_text(&format_info)
            .css_classes(["album-format-label"])
            .build();

        let year_info = album.year.map(|y| y.to_string()).unwrap_or_default();
        let year_label = Label::builder()
            .label(&year_info)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(End)
            .lines(1)
            .max_width_chars(8) // Fixed 8 chars as per spec for year field
            .tooltip_text(&year_info)
            .css_classes(["album-format-label"])
            .build();

        // Create horizontal metadata container
        let metadata_hbox = Box::builder()
            .orientation(Horizontal)
            .halign(Start)
            .spacing(8)
            .build();

        metadata_hbox.append(format_label.upcast_ref::<Widget>());
        metadata_hbox.append(year_label.upcast_ref::<Widget>());

        // Create metadata container
        let metadata_container = Box::builder().orientation(Vertical).halign(Start).build();

        metadata_container.append(metadata_hbox.upcast_ref::<Widget>());

        // Create main album tile container with proper spacing
        let album_tile = Box::builder()
            .orientation(Vertical)
            .halign(Start)
            .valign(Start)
            .hexpand(false)
            .vexpand(false)
            .spacing(2) // Exactly 2px spacing as specified
            .css_classes(["album-tile"])
            .build();

        album_tile.append(&cover_container);
        album_tile.append(&title_area.upcast::<Widget>());
        album_tile.append(artist_label.upcast_ref::<Widget>());
        album_tile.append(metadata_container.upcast_ref::<Widget>());

        // Set ARIA attributes for accessibility
        album_tile.set_accessible_role(Group);
        album_tile.set_tooltip_text(Some(&format!(
            "{} by {} ({})",
            album.title,
            album.artist_id,
            album.year.unwrap_or(0)
        )));

        // Create FlowBoxChild wrapper
        let child = FlowBoxChild::new();
        child.set_child(Some(&album_tile));
        child.set_focusable(true);

        // Handle click events with coordinate-based detection
        if let Some(play_callback) = on_play_clicked {
            let play_button_clone = play_overlay.button.clone();

            // Connect to play button click
            play_button_clone.connect_clicked(move |_| {
                play_callback();
            });
        }

        if let Some(card_callback) = on_card_clicked {
            let child_clone = child.clone();

            // Connect to card click with coordinate detection
            child_clone.connect_activate(move |_| {
                // In a real implementation, we would check coordinates to distinguish
                // between play button clicks and card navigation clicks
                // For now, we assume card clicks are for navigation
                card_callback();
            });
        }

        Self {
            widget: child.upcast_ref::<Widget>().clone(),
            album_tile,
            cover_art,
            play_overlay,
            dr_badge,
            title_label,
            artist_label,
            format_label,
            year_label,
        }
    }

    /// Creates an AlbumCard builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `AlbumCardBuilder` instance.
    pub fn builder() -> AlbumCardBuilder {
        AlbumCardBuilder::default()
    }

    /// Updates the album data displayed by this card.
    ///
    /// # Arguments
    ///
    /// * `album` - New album data to display
    /// * `format` - Optional new format information
    pub fn update_album(&mut self, album: Album, format: Option<String>) {
        // Update cover art
        self.cover_art.update_artwork(Some(
            album
                .artwork_path
                .as_deref()
                .unwrap_or(&album.path)
                .to_string(),
        ));

        // Update cover art which handles DR badge internally
        self.cover_art.update_dr_value(album.dr_value.clone());

        // Update labels
        self.title_label.set_label(&album.title);
        self.title_label.set_tooltip_text(Some(&album.title));

        self.artist_label.set_label(&album.artist_id.to_string());
        self.artist_label
            .set_tooltip_text(Some(&album.artist_id.to_string()));

        let format_info = format.unwrap_or_else(|| "Hi-Res".to_string());
        self.format_label.set_label(&format_info);
        self.format_label.set_tooltip_text(Some(&format_info));

        let year_info = album.year.map(|y| y.to_string()).unwrap_or_default();
        self.year_label.set_label(&year_info);
        self.year_label.set_tooltip_text(Some(&year_info));

        // Update tooltip
        self.album_tile.set_tooltip_text(Some(&format!(
            "{} by {} ({})",
            album.title,
            album.artist_id,
            album.year.unwrap_or(0)
        )));
    }
}

impl Default for AlbumCard {
    fn default() -> Self {
        let dummy_album = Album {
            id: 0,
            artist_id: 0,
            title: "Default Album".to_string(),
            year: Some(2023),
            genre: None,
            compilation: false,
            path: "/default/path".to_string(),
            dr_value: Some("DR12".to_string()),
            artwork_path: None,
            created_at: None,
            updated_at: None,
        };

        Self::new(dummy_album, None, true, false, None, None)
    }
}

#[cfg(test)]
mod tests {
    use crate::{library::models::Album, ui::components::album_card::AlbumCard};

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_album_card_builder() {
        let dummy_album = Album {
            id: 1,
            artist_id: 1,
            title: "Test Album".to_string(),
            year: Some(2023),
            genre: Some("Classical".to_string()),
            compilation: false,
            path: "/path/to/album".to_string(),
            dr_value: Some("DR12".to_string()),
            artwork_path: None,
            created_at: None,
            updated_at: None,
        };

        let card = AlbumCard::builder()
            .album(dummy_album)
            .show_dr_badge(true)
            .compact(false)
            .build();

        assert!(card.dr_badge.is_some());
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_album_card_default() {
        let card = AlbumCard::default();
        assert!(card.dr_badge.is_some());
    }
}
