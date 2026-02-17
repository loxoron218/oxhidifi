//! Album card component with proper widget hierarchy and styling.
//!
//! This module implements the `AlbumCard` component that displays albums
//! with cover art, DR badges, play overlays, and metadata following the
//! exact specification from docs/4.\ album-cards.md.

use std::{convert::TryFrom, rc::Rc};

use libadwaita::{
    gtk::{
        AccessibleRole::Group,
        Align::{End, Fill, Start},
        Box, FlowBoxChild, GestureClick, Label,
        Orientation::{Horizontal, Vertical},
        Overlay, Widget,
        pango::EllipsizeMode::End as EllipsizeEnd,
    },
    prelude::{AccessibleExt, BoxExt, ButtonExt, Cast, FlowBoxChildExt, WidgetExt},
};

use crate::{
    error::domain::UiError::{self, BuilderError, InvalidCoverWidth, WidgetError},
    library::models::Album,
    ui::{
        components::{cover_art::CoverArt, dr_badge::DRBadge, play_overlay::PlayOverlay},
        formatting::create_format_display,
    },
};

/// Builder pattern for configuring `AlbumCard` components.
#[derive(Default)]
pub struct AlbumCardBuilder {
    /// The album data to display on the card.
    album: Option<Album>,
    /// The artist name to display on the card.
    artist_name: Option<String>,
    /// The audio format to display (e.g., "FLAC", "MP3").
    format: Option<String>,
    /// Whether to show the DR badge overlay on the cover.
    show_dr_badge: bool,
    /// Whether to use compact layout with smaller cover size.
    compact: bool,
    /// Optional cover size override in pixels (width and height).
    cover_size: Option<u32>,
    /// Optional callback invoked when the play button is clicked.
    on_play_clicked: Option<Rc<dyn Fn()>>,
    /// Optional callback invoked when the card (outside play button) is clicked.
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Sets the cover size for the album card.
    ///
    /// # Arguments
    ///
    /// * `cover_size` - The size of the cover art in pixels (width and height)
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn cover_size(mut self, cover_size: u32) -> Self {
        self.cover_size = Some(cover_size);
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
    #[must_use]
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
    #[must_use]
    pub fn on_card_clicked<F>(mut self, callback: F) -> Self
    where
        F: Fn() + 'static,
    {
        self.on_card_clicked = Some(Rc::new(callback));
        self
    }

    /// Builds the `AlbumCard` component.
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `AlbumCard` instance or an error.
    ///
    /// # Errors
    ///
    /// Returns `UiError::BuilderError` if the album has not been set before building.
    pub fn build(self) -> Result<AlbumCard, UiError> {
        AlbumCard::new(AlbumCardConfig {
            album: self
                .album
                .ok_or_else(|| BuilderError("Album must be set".to_string()))?,
            artist_name: self
                .artist_name
                .unwrap_or_else(|| "Unknown Artist".to_string()),
            format: self.format,
            show_dr_badge: self.show_dr_badge,
            compact: self.compact,
            cover_size: self.cover_size,
            on_play_clicked: self.on_play_clicked,
            on_card_clicked: self.on_card_clicked,
        })
    }
}

/// Configuration for `AlbumCard` creation.
pub struct AlbumCardConfig {
    /// The album data to display
    pub album: Album,
    /// The artist name to display
    pub artist_name: String,
    /// Optional audio format information
    pub format: Option<String>,
    /// Whether to show the DR badge overlay
    pub show_dr_badge: bool,
    /// Whether to use compact layout
    pub compact: bool,
    /// Optional cover size override (if None, uses compact-based default)
    pub cover_size: Option<u32>,
    /// Optional callback for play button clicks
    pub on_play_clicked: Option<Rc<dyn Fn()>>,
    /// Optional callback for card clicks (outside play button)
    pub on_card_clicked: Option<Rc<dyn Fn()>>,
}

/// Album card component with proper widget hierarchy and styling.
///
/// The `AlbumCard` component implements the exact widget structure specified
/// in docs/4.\ album-cards.md with proper spacing, CSS classes, and interaction patterns.
#[derive(Clone)]
pub struct AlbumCard {
    /// The underlying `FlowBoxChild` widget.
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
    /// Title area container (contains title label).
    pub title_area: Box,
    /// Metadata container (contains format and year labels).
    pub metadata_container: Box,
    /// Album ID for tracking during filtering.
    pub album_id: i64,
}

impl AlbumCard {
    /// Creates a new `AlbumCard` component.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the album card
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `AlbumCard` instance or an error.
    ///
    /// # Errors
    ///
    /// Returns `UiError::InvalidCoverWidth` if cover dimensions exceed i32 bounds.
    pub fn new(config: AlbumCardConfig) -> Result<Self, UiError> {
        let AlbumCardConfig {
            album,
            artist_name,
            format,
            show_dr_badge,
            compact,
            cover_size,
            on_play_clicked,
            on_card_clicked,
        } = config;

        let (cover_width, cover_height) = Self::calculate_cover_dimensions(compact, cover_size)?;

        let cover_art = Self::create_cover_art(&album, show_dr_badge, cover_width, cover_height);

        let play_overlay = Self::create_play_overlay(&cover_art)?;

        let (title_label, title_area) = Self::create_title_section(&album.title, cover_width);

        let artist_label = Self::create_artist_label(&artist_name, cover_width);

        let (format_label, year_label, metadata_container) =
            Self::create_metadata_section(&album, format, cover_width);

        let album_tile = Self::create_album_tile(
            &cover_art.widget,
            &title_area,
            &artist_label,
            &metadata_container,
            &album.title,
            &artist_name,
            album.year,
        );

        let child = Self::create_flow_box_child(
            &album_tile,
            on_play_clicked,
            on_card_clicked,
            &play_overlay,
        );

        Ok(Self {
            widget: child.upcast_ref::<Widget>().clone(),
            album_tile,
            cover_art,
            play_overlay,
            dr_badge: None,
            title_label,
            artist_label,
            format_label,
            year_label,
            artist_name,
            title_area,
            metadata_container,
            album_id: album.id,
        })
    }

    /// Calculates cover dimensions based on configuration.
    ///
    /// # Arguments
    ///
    /// * `compact` - Whether to use compact layout
    /// * `cover_size` - Optional cover size override
    ///
    /// # Returns
    ///
    /// A tuple of (width, height) as i32 values.
    ///
    /// # Errors
    ///
    /// Returns `UiError::InvalidCoverWidth` if dimensions exceed i32 bounds.
    fn calculate_cover_dimensions(
        compact: bool,
        cover_size: Option<u32>,
    ) -> Result<(i32, i32), UiError> {
        let base_cover_size = cover_size.unwrap_or(if compact { 120 } else { 180 });
        let cover_width = base_cover_size;
        let cover_height = base_cover_size;

        let cover_width_i32 =
            i32::try_from(cover_width).map_err(|_| InvalidCoverWidth { cover_width })?;
        let cover_height_i32 = i32::try_from(cover_height).map_err(|_| InvalidCoverWidth {
            cover_width: cover_height,
        })?;

        Ok((cover_width_i32, cover_height_i32))
    }

    /// Creates the cover art widget for the album.
    ///
    /// # Arguments
    ///
    /// * `album` - The album data
    /// * `show_dr_badge` - Whether to show DR badge
    /// * `cover_width` - Cover width in pixels
    /// * `cover_height` - Cover height in pixels
    ///
    /// # Returns
    ///
    /// The created `CoverArt` component.
    fn create_cover_art(
        album: &Album,
        show_dr_badge: bool,
        cover_width: i32,
        cover_height: i32,
    ) -> CoverArt {
        CoverArt::builder()
            .artwork_path(album.artwork_path.as_deref().unwrap_or(&album.path))
            .dr_value(album.dr_value.clone().unwrap_or_else(|| "N/A".to_string()))
            .show_dr_badge(show_dr_badge)
            .dimensions(cover_width, cover_height)
            .build()
    }

    /// Creates and configures the play overlay.
    ///
    /// # Arguments
    ///
    /// * `cover_art` - The cover art component to overlay the play button on
    ///
    /// # Returns
    ///
    /// The configured `PlayOverlay` component.
    ///
    /// # Errors
    ///
    /// Returns `UiError::WidgetError` if cover art widget is not an Overlay.
    fn create_play_overlay(cover_art: &CoverArt) -> Result<PlayOverlay, UiError> {
        let play_overlay = PlayOverlay::builder()
            .is_playing(false)
            .show_on_hover(false)
            .build();

        // Add CSS class for CSS-based hover effect
        play_overlay.widget.add_css_class("cover-play-button");

        // Set explicit size for the play button
        play_overlay.widget.set_size_request(48, 48);

        // Add play overlay to the cover art overlay
        let cover_art_overlay = cover_art
            .widget
            .downcast_ref::<Overlay>()
            .ok_or_else(|| WidgetError("CoverArt widget should be an Overlay".to_string()))?;
        cover_art_overlay.add_overlay(&play_overlay.widget);

        Ok(play_overlay)
    }

    /// Creates the title label and title area container.
    ///
    /// # Arguments
    ///
    /// * `title` - The album title
    /// * `cover_width` - Cover width for calculating max width chars
    ///
    /// # Returns
    ///
    /// A tuple of (`title_label`, `title_area_box`).
    fn create_title_section(title: &str, cover_width: i32) -> (Label, Box) {
        let title_max = Self::calculate_title_max_width(cover_width);

        let title_label = Label::builder()
            .label(title)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(EllipsizeEnd)
            .lines(2)
            .max_width_chars(title_max)
            .tooltip_text(title)
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

        (title_label, title_area)
    }

    /// Calculates the maximum width characters for title labels.
    ///
    /// # Arguments
    ///
    /// * `cover_width` - Cover width in pixels
    ///
    /// # Returns
    ///
    /// The calculated max width characters value.
    fn calculate_title_max_width(cover_width: i32) -> i32 {
        ((cover_width - 16) / 10).max(8)
    }

    /// Creates the artist label.
    ///
    /// # Arguments
    ///
    /// * `artist_name` - The artist name
    /// * `cover_width` - Cover width for calculating max width chars
    ///
    /// # Returns
    ///
    /// The created artist label.
    fn create_artist_label(artist_name: &str, cover_width: i32) -> Label {
        let artist_max = ((cover_width - 16) / 10).max(8);

        Label::builder()
            .label(artist_name)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(EllipsizeEnd)
            .lines(1)
            .max_width_chars(artist_max)
            .tooltip_text(artist_name)
            .css_classes(["album-artist-label"])
            .build()
    }

    /// Creates the metadata section with format and year labels.
    ///
    /// # Arguments
    ///
    /// * `album` - The album data
    /// * `format` - Optional explicit format string
    /// * `cover_width` - Cover width for calculating max width chars
    ///
    /// # Returns
    ///
    /// A tuple of (`format_label`, `year_label`, `metadata_container`).
    fn create_metadata_section(
        album: &Album,
        format: Option<String>,
        cover_width: i32,
    ) -> (Label, Label, Box) {
        let format_info =
            format.unwrap_or_else(|| create_format_display(album).unwrap_or_default());

        let format_max = (((cover_width - 16) / 2) / 10).max(8);

        let mut format_label_builder = Label::builder()
            .label(&format_info)
            .halign(Start)
            .xalign(0.0)
            .lines(1)
            .max_width_chars(format_max)
            .css_classes(["album-format-label"]);

        if !format_info.is_empty() {
            format_label_builder = format_label_builder.tooltip_text(&format_info);
        }

        let format_label = format_label_builder.build();

        let year_info = album.year.map(|y| y.to_string()).unwrap_or_default();
        let year_label = Label::builder()
            .label(&year_info)
            .halign(End)
            .lines(1)
            .tooltip_text(&year_info)
            .css_classes(["album-format-label"])
            .hexpand(true)
            .build();

        // Create horizontal metadata container
        let metadata_hbox = Box::builder()
            .orientation(Horizontal)
            .halign(Fill)
            .spacing(6)
            .build();

        metadata_hbox.append(format_label.upcast_ref::<Widget>());
        metadata_hbox.append(year_label.upcast_ref::<Widget>());

        // Create metadata container
        let metadata_container = Box::builder().orientation(Vertical).halign(Fill).build();

        metadata_container.append(metadata_hbox.upcast_ref::<Widget>());

        (format_label, year_label, metadata_container)
    }

    /// Creates the main album tile container.
    ///
    /// # Arguments
    ///
    /// * `cover_container` - The cover art widget
    /// * `title_area` - The title area container
    /// * `artist_label` - The artist label
    /// * `metadata_container` - The metadata container
    /// * `title` - The album title
    /// * `artist_name` - The artist name
    /// * `album_year` - Optional album year
    ///
    /// # Returns
    ///
    /// The configured album tile box widget.
    fn create_album_tile(
        cover_container: &Widget,
        title_area: &Box,
        artist_label: &Label,
        metadata_container: &Box,
        title: &str,
        artist_name: &str,
        album_year: Option<i64>,
    ) -> Box {
        let album_tile = Box::builder()
            .orientation(Vertical)
            .halign(Start)
            .valign(Start)
            .hexpand(false)
            .vexpand(false)
            .width_request(64)
            .spacing(6)
            .css_classes(["album-tile", "card"])
            .build();

        album_tile.append(cover_container);
        album_tile.append(title_area);
        album_tile.append(artist_label.upcast_ref::<Widget>());
        album_tile.append(metadata_container);

        // Set ARIA attributes for accessibility
        album_tile.set_accessible_role(Group);
        album_tile.set_tooltip_text(Some(&format!(
            "{title} by {artist_name} ({})",
            album_year.unwrap_or(0)
        )));

        album_tile
    }

    /// Creates the `FlowBoxChild` wrapper with click handlers.
    ///
    /// # Arguments
    ///
    /// * `album_tile` - The album tile widget
    /// * `on_play_clicked` - Optional play button callback
    /// * `on_card_clicked` - Optional card click callback
    /// * `play_overlay` - The play overlay component
    ///
    /// # Returns
    ///
    /// The configured `FlowBoxChild` widget.
    fn create_flow_box_child(
        album_tile: &Box,
        on_play_clicked: Option<Rc<dyn Fn()>>,
        on_card_clicked: Option<Rc<dyn Fn()>>,
        play_overlay: &PlayOverlay,
    ) -> FlowBoxChild {
        let child = FlowBoxChild::new();
        child.set_child(Some(album_tile));
        child.set_focusable(true);

        Self::setup_click_handlers(
            album_tile,
            &child,
            on_play_clicked,
            on_card_clicked,
            play_overlay,
        );

        child
    }

    /// Sets up click handlers for the album card.
    ///
    /// # Arguments
    ///
    /// * `album_tile` - The album tile widget
    /// * `child` - The `FlowBoxChild` wrapper
    /// * `on_play_clicked` - Optional play button callback
    /// * `on_card_clicked` - Optional card click callback
    /// * `play_overlay` - The play overlay component
    fn setup_click_handlers(
        album_tile: &Box,
        child: &FlowBoxChild,
        on_play_clicked: Option<Rc<dyn Fn()>>,
        on_card_clicked: Option<Rc<dyn Fn()>>,
        play_overlay: &PlayOverlay,
    ) {
        // Handle click events
        // Note: FlowBoxChild handles selection/activation, but we want custom behavior
        // We use a GestureClick controller on the child widget to capture clicks
        let click_controller = GestureClick::new();

        // Clone for closures
        let card_callback_clone = on_card_clicked.clone();
        click_controller.connect_released(move |_gesture, _n_press, _x, _y| {
            // If we have a card callback, trigger it
            if let Some(ref callback) = card_callback_clone {
                callback();
            }
        });

        // Add controller to the main tile widget
        album_tile.add_controller(click_controller);

        // Support keyboard activation (Enter/Space)
        if let Some(card_callback) = on_card_clicked {
            child.connect_activate(move |_| {
                card_callback();
            });
        }

        // Also connect the play button specifically if we have a callback
        if let Some(play_callback) = on_play_clicked {
            play_overlay.button.connect_clicked(move |_| {
                play_callback();
            });
        }
    }

    /// Creates an `AlbumCard` builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `AlbumCardBuilder` instance.
    #[must_use]
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
    pub fn update_album(&mut self, album: &Album, artist_name: String, format: Option<String>) {
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
            create_format_display(album).unwrap_or_default()
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
            "{} by {artist_name} ({})",
            album.title,
            album.year.unwrap_or(0)
        )));

        // Update stored artist name
        self.artist_name = artist_name;
    }

    /// Updates the DR badge visibility for this album card.
    ///
    /// # Arguments
    ///
    /// * `show_dr_badge` - Whether to show the DR badge
    pub fn update_dr_badge_visibility(&mut self, show_dr_badge: bool) {
        self.cover_art.set_show_dr_badge(show_dr_badge);
    }

    /// Updates the metadata overlay visibility for this album card.
    ///
    /// # Arguments
    ///
    /// * `show_overlays` - Whether to show metadata overlays (title, artist, format, year)
    pub fn update_metadata_overlay_visibility(&mut self, show_overlays: bool) {
        // Show or hide the title, artist, format, and year labels
        self.title_label.set_visible(show_overlays);
        self.artist_label.set_visible(show_overlays);
        self.format_label.set_visible(show_overlays);
        self.year_label.set_visible(show_overlays);

        // Also hide the containers to make the card shrink vertically
        self.title_area.set_visible(show_overlays);
        self.metadata_container.set_visible(show_overlays);
    }

    /// Updates the `max_width_chars` for labels based on new cover size.
    ///
    /// # Arguments
    ///
    /// * `cover_width` - New cover width in pixels
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure with conversion error.
    ///
    /// # Errors
    ///
    /// Returns `UiError::InvalidCoverWidth` if the cover width exceeds i32 bounds.
    pub fn update_label_max_width_chars(&self, cover_width: u32) -> Result<(), UiError> {
        let title_max = i32::try_from(((cover_width - 16) / 10).max(8))
            .map_err(|_| InvalidCoverWidth { cover_width })?;
        let artist_max = i32::try_from(((cover_width - 16) / 10).max(8))
            .map_err(|_| InvalidCoverWidth { cover_width })?;
        let format_max = i32::try_from((((cover_width - 16) / 2) / 10).max(8))
            .map_err(|_| InvalidCoverWidth { cover_width })?;

        self.title_label.set_max_width_chars(title_max);
        self.artist_label.set_max_width_chars(artist_max);
        self.format_label.set_max_width_chars(format_max);
        Ok(())
    }
}

impl AlbumCard {
    /// Creates a default `AlbumCard` for testing purposes.
    ///
    /// # Returns
    ///
    /// A `Result` containing the default `AlbumCard` instance or an error.
    ///
    /// # Errors
    ///
    /// Returns `UiError::InvalidCoverWidth` if cover dimensions exceed i32 bounds.
    pub fn create_default() -> Result<Self, UiError> {
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

        Self::new(AlbumCardConfig {
            album: dummy_album,
            artist_name: "Default Artist".to_string(),
            format: None,
            show_dr_badge: true,
            compact: false,
            cover_size: None,
            on_play_clicked: None,
            on_card_clicked: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::{
        library::models::Album,
        ui::components::album_card::{AlbumCard, AlbumCardConfig},
    };

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_album_card_builder() -> Result<(), Box<dyn Error>> {
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
            .build()?;

        assert!(card.dr_badge.is_some());
        Ok(())
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_album_card_create_default() -> Result<(), Box<dyn Error>> {
        let card = AlbumCard::create_default()?;
        assert!(card.dr_badge.is_some());
        Ok(())
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_album_card_sample_rate_decimal_formatting() -> Result<(), Box<dyn Error>> {
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
            .build()?;

        // The format label should contain "FLAC 24/44.1"
        let format_text = card_441.format_label.text().to_string();
        assert_eq!(
            format_text, "FLAC 24/44.1",
            "Expected 'FLAC 24/44.1' but got '{format_text}'"
        );

        // Test 88.2 kHz sample rate
        let album_882 = Album {
            sample_rate: Some(88200),
            ..Album::default()
        };

        let card_882 = AlbumCard::new(AlbumCardConfig {
            album: album_882,
            artist_name: "Test Artist".to_string(),
            format: None,
            show_dr_badge: true,
            compact: false,
            cover_size: None,
            on_play_clicked: None,
            on_card_clicked: None,
        })?;

        let format_text_882 = card_882.format_label.text().to_string();
        assert_eq!(
            format_text_882, "FLAC 24/88.2",
            "Expected 'FLAC 24/88.2' but got '{format_text_882}'"
        );

        // Test 96 kHz (whole number) sample rate
        let album_96 = Album {
            sample_rate: Some(96000),
            ..Album::default()
        };

        let card_96 = AlbumCard::new(AlbumCardConfig {
            album: album_96,
            artist_name: "Test Artist".to_string(),
            format: None,
            show_dr_badge: true,
            compact: false,
            cover_size: None,
            on_play_clicked: None,
            on_card_clicked: None,
        })?;

        let format_text_96 = card_96.format_label.text().to_string();
        assert_eq!(
            format_text_96, "FLAC 24/96",
            "Expected 'FLAC 24/96' but got '{format_text_96}'"
        );
        Ok(())
    }
}
