//! Album card component with proper widget hierarchy and styling.
//!
//! This module implements the `AlbumCard` component that displays albums
//! with cover art, DR badges, play overlays, and metadata following the
//! exact specification from docs/4.\ album-cards.md.

use std::rc::Rc;

use libadwaita::{
    gtk::{
        AccessibleRole::Group,
        Align::{End, Fill, Start},
        Box, FlowBoxChild, GestureClick, Label,
        Orientation::{Horizontal, Vertical},
        Widget,
        pango::EllipsizeMode::End as EllipsizeEnd,
    },
    prelude::{AccessibleExt, BoxExt, ButtonExt, Cast, FlowBoxChildExt, WidgetExt},
};

use crate::{
    library::models::Album,
    ui::{
        components::{cover_art::CoverArt, dr_badge::DRBadge, play_overlay::PlayOverlay},
        utils::create_format_display,
    },
};

/// Builder pattern for configuring AlbumCard components.
#[derive(Default)]
pub struct AlbumCardBuilder {
    album: Option<Album>,
    artist_name: Option<String>,
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

    /// Sets the artist name for the card.
    ///
    /// # Arguments
    ///
    /// * `artist_name` - The artist name to display
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn artist_name(mut self, artist_name: String) -> Self {
        self.artist_name = Some(artist_name);
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
            self.artist_name
                .unwrap_or_else(|| "Unknown Artist".to_string()),
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
    /// Current artist name.
    pub artist_name: String,
}

impl AlbumCard {
    /// Creates a new AlbumCard component.
    ///
    /// # Arguments
    ///
    /// * `album` - The album data to display
    /// * `artist_name` - The artist name to display
    /// * `format` - Optional audio format information
    /// * `show_dr_badge` - Whether to show the DR badge overlay
    /// * `compact` - Whether to use compact layout
    /// * `on_play_clicked` - Optional callback for play button clicks
    /// * `on_card_clicked` - Optional callback for card clicks (outside play button)
    ///
    /// # Returns
    ///
    /// A new `AlbumCard` instance.
    pub fn new(
        album: Album,
        artist_name: String,
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
            .ellipsize(EllipsizeEnd)
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
            .label(&artist_name)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(EllipsizeEnd)
            .lines(1)
            .max_width_chars(((cover_width - 16) / 10).max(8)) // Dynamic calculation
            .tooltip_text(&artist_name)
            .css_classes(["album-artist-label"])
            .build();

        // Create format and year labels
        let format_info = format.unwrap_or_else(|| {
            // If no explicit format provided, try to create one from album metadata
            create_format_display(&album).unwrap_or_default()
        });
        let mut format_label_builder = Label::builder()
            .label(&format_info)
            .halign(Start)
            .xalign(0.0)
            .lines(1)
            .max_width_chars((((cover_width - 16) / 2) / 10).max(8)) // Dynamic calculation
            .css_classes(["album-format-label"]);

        if !format_info.is_empty() {
            format_label_builder = format_label_builder.tooltip_text(&format_info);
        }

        let format_label = format_label_builder.build();

        let year_info = album.year.map(|y| y.to_string()).unwrap_or_default();
        let year_label = Label::builder()
            .label(&year_info)
            .halign(End)
            .xalign(0.0)
            .lines(1)
            .max_width_chars(4) // Fixed 4 chars for a regular year number
            .tooltip_text(&year_info)
            .css_classes(["album-format-label"])
            .hexpand(true)
            .build();

        // Create horizontal metadata container
        let metadata_hbox = Box::builder()
            .orientation(Horizontal)
            .halign(Start)
            .width_request(cover_width) // Force full width to align year to right margin
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
            artist_name,
            album.year.unwrap_or(0)
        )));

        // Create FlowBoxChild wrapper
        let child = FlowBoxChild::new();
        child.set_child(Some(&album_tile));
        child.set_focusable(true);

        // Handle click events
        // Note: FlowBoxChild handles selection/activation, but we want custom behavior
        // We use a GestureClick controller on the child widget to capture clicks
        let click_controller = GestureClick::new();

        // Clone for closures
        let play_callback = on_play_clicked;
        let card_callback = on_card_clicked;

        let card_callback_clone = card_callback.clone();
        click_controller.connect_released(move |_gesture, _n_press, _x, _y| {
            // If we have a card callback, trigger it
            if let Some(ref callback) = card_callback_clone {
                callback();
            }
        });

        // Add controller to the main tile widget
        album_tile.add_controller(click_controller);

        // Support keyboard activation (Enter/Space)
        if let Some(card_callback) = card_callback {
            child.connect_activate(move |_| {
                card_callback();
            });
        }

        // Also connect the play button specifically if we have a callback
        if let Some(play_callback) = play_callback {
            play_overlay.button.connect_clicked(move |_| {
                play_callback();
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
            artist_name,
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
    /// * `artist_name` - New artist name to display
    /// * `format` - Optional new format information
    pub fn update_album(&mut self, album: Album, artist_name: String, format: Option<String>) {
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

        self.artist_label.set_label(&artist_name);
        self.artist_label.set_tooltip_text(Some(&artist_name));

        let format_info = format.unwrap_or_else(|| {
            // If no explicit format provided, try to create one from album metadata
            create_format_display(&album).unwrap_or_default()
        });
        self.format_label.set_label(&format_info);
        if format_info.is_empty() {
            self.format_label.set_tooltip_text(None);
        } else {
            self.format_label.set_tooltip_text(Some(&format_info));
        }

        let year_info = album.year.map(|y| y.to_string()).unwrap_or_default();
        self.year_label.set_label(&year_info);
        self.year_label.set_tooltip_text(Some(&year_info));

        // Update tooltip
        self.album_tile.set_tooltip_text(Some(&format!(
            "{} by {} ({})",
            album.title,
            artist_name,
            album.year.unwrap_or(0)
        )));

        // Update stored artist name
        self.artist_name = artist_name;
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
            format: Some("FLAC".to_string()),
            bits_per_sample: Some(24),
            sample_rate: Some(96000),
            compilation: false,
            path: "/default/path".to_string(),
            dr_value: Some("DR12".to_string()),
            artwork_path: None,
            created_at: None,
            updated_at: None,
        };

        Self::new(
            dummy_album,
            "Default Artist".to_string(),
            None,
            true,
            false,
            None,
            None,
        )
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
            format: Some("FLAC".to_string()),
            bits_per_sample: Some(24),
            sample_rate: Some(96000),
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

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_album_card_sample_rate_decimal_formatting() {
        // Test 44.1 kHz sample rate in album card
        let album_441 = Album {
            id: 1,
            artist_id: 1,
            title: "Test Album 44.1".to_string(),
            year: Some(2023),
            genre: Some("Classical".to_string()),
            format: Some("FLAC".to_string()),
            bits_per_sample: Some(24),
            sample_rate: Some(44100),
            compilation: false,
            path: "/path/to/album_441".to_string(),
            dr_value: Some("DR12".to_string()),
            artwork_path: None,
            created_at: None,
            updated_at: None,
        };

        let card_441 = AlbumCard::builder()
            .album(album_441)
            .artist_name("Test Artist".to_string())
            .show_dr_badge(true)
            .compact(false)
            .build();

        // The format label should contain "FLAC 24/44.1"
        let format_text = card_441.format_label.text().to_string();
        assert_eq!(
            format_text, "FLAC 24/44.1",
            "Expected 'FLAC 24/44.1' but got '{}'",
            format_text
        );

        // Test 88.2 kHz sample rate
        let album_882 = Album {
            sample_rate: Some(88200),
            ..Album::default()
        };

        let card_882 = AlbumCard::new(
            album_882,
            "Test Artist".to_string(),
            None,
            true,
            false,
            None,
            None,
        );

        let format_text_882 = card_882.format_label.text().to_string();
        assert_eq!(
            format_text_882, "FLAC 24/88.2",
            "Expected 'FLAC 24/88.2' but got '{}'",
            format_text_882
        );

        // Test 96 kHz (whole number) sample rate
        let album_96 = Album {
            sample_rate: Some(96000),
            ..Album::default()
        };

        let card_96 = AlbumCard::new(
            album_96,
            "Test Artist".to_string(),
            None,
            true,
            false,
            None,
            None,
        );

        let format_text_96 = card_96.format_label.text().to_string();
        assert_eq!(
            format_text_96, "FLAC 24/96",
            "Expected 'FLAC 24/96' but got '{}'",
            format_text_96
        );
    }
}
