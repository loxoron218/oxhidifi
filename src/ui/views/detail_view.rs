//! Album/artist detail pages with comprehensive metadata and track listings.
//!
//! This module implements the `DetailView` component that displays detailed
//! information for albums or artists, including comprehensive metadata,
//! track listings with technical specifications, and playback controls.

use std::sync::Arc;

use {
    async_trait::async_trait,
    libadwaita::{
        gtk::{
            AccessibleRole::Article,
            Align::{Fill, Start},
            Box as GtkBox, Button, Label, ListBox, ListBoxRow,
            Orientation::{Horizontal, Vertical},
            ScrolledWindow,
            SelectionMode::None as SelectionNone,
            Widget,
            pango::EllipsizeMode::End as EllipsizeEnd,
        },
        prelude::{AccessibleExt, BoxExt, Cast, ListBoxRowExt, ListModelExt, WidgetExt},
    },
};

use crate::{
    library::models::{Album, Artist, Track},
    state::{
        AppState,
        AppStateEvent::{self, LibraryStateChanged},
        LibraryState, StateObserver,
    },
    ui::components::{cover_art::CoverArt, hifi_metadata::HiFiMetadata, play_overlay::PlayOverlay},
};

/// Builder pattern for configuring DetailView components.
#[derive(Debug, Default)]
pub struct DetailViewBuilder {
    app_state: Option<Arc<AppState>>,
    detail_type: DetailType,
    compact: bool,
}

impl DetailViewBuilder {
    /// Sets the application state for reactive updates.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn app_state(mut self, app_state: Arc<AppState>) -> Self {
        self.app_state = Some(app_state);
        self
    }

    /// Sets the detail type (album or artist).
    ///
    /// # Arguments
    ///
    /// * `detail_type` - The type of detail to display
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn detail_type(mut self, detail_type: DetailType) -> Self {
        self.detail_type = detail_type;
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

    /// Builds the DetailView component.
    ///
    /// # Returns
    ///
    /// A new `DetailView` instance.
    pub fn build(self) -> DetailView {
        DetailView::new(self.app_state, self.detail_type, self.compact)
    }
}

/// Type of detail to display.
#[derive(Debug, Clone, PartialEq)]
pub enum DetailType {
    /// Display album detail
    Album(Album),
    /// Display artist detail
    Artist(Artist),
}

impl Default for DetailType {
    fn default() -> Self {
        // Default to empty album (this should be handled by the application logic)
        // Since we need a default, we'll use a placeholder album
        DetailType::Album(Album::default())
    }
}

/// Comprehensive detail view for albums or artists.
///
/// The `DetailView` component displays detailed information for a single
/// album or artist, including artwork, metadata, track listings, and
/// playback controls, with smooth transitions and proper navigation.
pub struct DetailView {
    /// The underlying GTK widget (main container).
    pub widget: Widget,
    /// Main container box.
    pub main_container: GtkBox,
    /// Current application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Current detail type being displayed.
    pub detail_type: Option<DetailType>,
    /// Configuration flags.
    pub config: DetailViewConfig,
}

/// Configuration for DetailView display options.
#[derive(Debug, Clone)]
pub struct DetailViewConfig {
    /// Whether to use compact layout.
    pub compact: bool,
}

impl DetailView {
    /// Creates a new DetailView component.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference for reactive updates
    /// * `detail_type` - Initial detail type to display
    /// * `compact` - Whether to use compact layout
    ///
    /// # Returns
    ///
    /// A new `DetailView` instance.
    pub fn new(app_state: Option<Arc<AppState>>, detail_type: DetailType, compact: bool) -> Self {
        let config = DetailViewConfig { compact };

        let main_container = GtkBox::builder()
            .orientation(Vertical)
            .halign(Fill)
            .valign(Fill)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .css_classes(["detail-view"])
            .build();

        // Set ARIA attributes for accessibility
        main_container.set_accessible_role(Article);

        let mut view = Self {
            widget: main_container.clone().upcast_ref::<Widget>().clone(),
            main_container,
            app_state,
            detail_type: None,
            config,
        };

        // Set initial detail
        view.set_detail(detail_type);

        view
    }

    /// Creates a DetailView builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `DetailViewBuilder` instance.
    pub fn builder() -> DetailViewBuilder {
        DetailViewBuilder::default()
    }

    /// Sets the detail to display.
    ///
    /// # Arguments
    ///
    /// * `detail_type` - New detail type to display
    pub fn set_detail(&mut self, detail_type: DetailType) {
        // Clear existing content
        let children = self.main_container.observe_children();
        let n_items = children.n_items();
        for i in 0..n_items {
            if let Some(child) = children.item(i)
                && let Ok(widget) = child.downcast::<Widget>()
            {
                self.main_container.remove(&widget);
            }
        }

        self.detail_type = Some(detail_type.clone());

        match detail_type {
            DetailType::Album(album) => self.display_album_detail(album),
            DetailType::Artist(artist) => self.display_artist_detail(artist),
        }
    }

    /// Displays detailed album information.
    ///
    /// # Arguments
    ///
    /// * `album` - The album to display details for
    fn display_album_detail(&mut self, album: Album) {
        // Create header section with cover art and metadata
        let header_container = self.create_album_header(&album);
        self.main_container.append(&header_container);

        // Create track listing section
        if let Some(ref app_state) = self.app_state {
            let library_state = app_state.get_library_state();
            let tracks: Vec<Track> = library_state
                .current_tracks
                .into_iter()
                .filter(|track| track.album_id == album.id)
                .collect();

            if !tracks.is_empty() {
                let track_list = self.create_track_list(tracks);
                self.main_container.append(&track_list);
            }
        }

        // set_accessible_description doesn't exist in GTK4, remove this line
    }

    /// Creates the album header section with cover art and metadata.
    ///
    /// # Arguments
    ///
    /// * `album` - The album to create header for
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the album header.
    fn create_album_header(&self, album: &Album) -> Widget {
        let header_container = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(24)
            .build();

        // Large cover art with play overlay
        let cover_art = CoverArt::builder()
            .artwork_path(album.artwork_path.as_deref().unwrap_or(&album.path))
            .dr_value(album.dr_value.clone().unwrap_or_else(|| "N/A".to_string()))
            .show_dr_badge(true)
            .dimensions(300, 300)
            .build();

        let play_overlay = PlayOverlay::builder()
            .is_playing(false)
            .show_on_hover(true)
            .build();

        let cover_container = GtkBox::builder()
            .orientation(Vertical)
            .halign(Start)
            .valign(Start)
            .build();

        cover_container.append(&cover_art.widget);
        cover_container.append(&play_overlay.widget);

        // Metadata container
        let metadata_container = GtkBox::builder()
            .orientation(Vertical)
            .hexpand(true)
            .spacing(8)
            .build();

        // Title
        let title_label = Label::builder()
            .label(&album.title)
            .halign(Start)
            .xalign(0.0)
            .css_classes(["title-1"])
            .ellipsize(EllipsizeEnd)
            .tooltip_text(&album.title)
            .build();
        metadata_container.append(title_label.upcast_ref::<Widget>());

        // Artist
        let artist_label = Label::builder()
            .label(format!("Artist ID: {}", album.artist_id))
            .halign(Start)
            .xalign(0.0)
            .css_classes(["title-2"])
            .ellipsize(EllipsizeEnd)
            .tooltip_text(format!("Artist ID: {}", album.artist_id))
            .build();
        metadata_container.append(artist_label.upcast_ref::<Widget>());

        // Year and genre
        if let Some(year) = album.year {
            let year_label = Label::builder()
                .label(year.to_string())
                .halign(Start)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .build();
            metadata_container.append(year_label.upcast_ref::<Widget>());
        }

        if let Some(ref genre) = album.genre {
            let genre_label = Label::builder()
                .label(genre)
                .halign(Start)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .ellipsize(EllipsizeEnd)
                .tooltip_text(genre)
                .build();
            metadata_container.append(genre_label.upcast_ref::<Widget>());
        }

        // Compilation indicator
        if album.compilation {
            let compilation_label = Label::builder()
                .label("Compilation")
                .halign(Start)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .build();
            metadata_container.append(compilation_label.upcast_ref::<Widget>());
        }

        // Play all button
        let play_all_button = Button::builder().label("Play All").halign(Start).build();
        metadata_container.append(play_all_button.upcast_ref::<Widget>());

        header_container.append(cover_container.upcast_ref::<Widget>());
        header_container.append(metadata_container.upcast_ref::<Widget>());

        header_container.upcast_ref::<Widget>().clone()
    }

    /// Creates the track listing section.
    ///
    /// # Arguments
    ///
    /// * `tracks` - Vector of tracks to display
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the track list.
    fn create_track_list(&self, tracks: Vec<Track>) -> Widget {
        let list_container = GtkBox::builder().orientation(Vertical).spacing(8).build();

        let title_label = Label::builder()
            .label("Tracks")
            .halign(Start)
            .xalign(0.0)
            .css_classes(["title-2"])
            .build();
        list_container.append(title_label.upcast_ref::<Widget>());

        let scrolled_window = ScrolledWindow::builder()
            .vexpand(true)
            .min_content_height(300)
            .build();

        let track_list = ListBox::builder()
            .selection_mode(SelectionNone)
            .css_classes(["track-list"])
            .build();

        for (index, track) in tracks.iter().enumerate() {
            let row = self.create_track_row(track, index + 1);
            track_list.append(&row);
        }

        scrolled_window.set_child(Some(&track_list));
        list_container.append(scrolled_window.upcast_ref::<Widget>());

        list_container.upcast_ref::<Widget>().clone()
    }

    /// Creates a single track row widget.
    ///
    /// # Arguments
    ///
    /// * `track` - The track to create a row for
    /// * `track_number` - Display track number
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the track row.
    fn create_track_row(&self, track: &Track, track_number: usize) -> Widget {
        let row_container = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_top(4)
            .margin_bottom(4)
            .margin_start(8)
            .margin_end(8)
            .build();

        // Track number
        let number_label = Label::builder()
            .label(track_number.to_string())
            .width_chars(3)
            .xalign(1.0)
            .css_classes(["dim-label"])
            .build();
        row_container.append(number_label.upcast_ref::<Widget>());

        // Title
        let title_label = Label::builder()
            .label(&track.title)
            .halign(Start)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(EllipsizeEnd)
            .tooltip_text(&track.title)
            .build();
        row_container.append(title_label.upcast_ref::<Widget>());

        // Duration
        let duration_seconds = track.duration_ms / 1000;
        let duration_minutes = duration_seconds / 60;
        let duration_remaining = duration_seconds % 60;
        let duration_text = format!("{:02}:{:02}", duration_minutes, duration_remaining);
        let duration_label = Label::builder()
            .label(&duration_text)
            .halign(Start)
            .xalign(1.0)
            .css_classes(["dim-label"])
            .build();
        row_container.append(duration_label.upcast_ref::<Widget>());

        // Hi-Fi metadata
        let hifi_metadata = HiFiMetadata::builder()
            .track(track.clone())
            .show_format(true)
            .show_sample_rate(true)
            .show_bit_depth(true)
            .show_channels(false) // Save space in track list
            .compact(true)
            .build();
        row_container.append(&hifi_metadata.widget);

        // Create ListBoxRow wrapper
        let row = ListBoxRow::new();
        row.set_child(Some(&row_container));
        row.set_activatable(true);
        row.set_selectable(true);

        // set_accessible_description doesn't exist in GTK4, remove this block

        row.upcast_ref::<Widget>().clone()
    }

    /// Displays detailed artist information.
    ///
    /// # Arguments
    ///
    /// * `artist` - The artist to display details for
    fn display_artist_detail(&mut self, artist: Artist) {
        // Create header section with artist image and metadata
        let header_container = self.create_artist_header(&artist);
        self.main_container.append(&header_container);

        // Create album listing section (placeholder - would need album data)
        let album_list_placeholder = self.create_album_list_placeholder();
        self.main_container.append(&album_list_placeholder);

        // set_accessible_description doesn't exist in GTK4, remove this line
    }

    /// Creates the artist header section with image and metadata.
    ///
    /// # Arguments
    ///
    /// * `artist` - The artist to create header for
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the artist header.
    fn create_artist_header(&self, artist: &Artist) -> Widget {
        let header_container = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(24)
            .build();

        // Artist image (default avatar)
        let cover_art = CoverArt::builder()
            .artwork_path("")
            .show_dr_badge(false)
            .dimensions(300, 300)
            .build();

        let cover_container = GtkBox::builder()
            .orientation(Vertical)
            .halign(Start)
            .valign(Start)
            .build();

        cover_container.append(&cover_art.widget);

        // Metadata container
        let metadata_container = GtkBox::builder()
            .orientation(Vertical)
            .hexpand(true)
            .spacing(8)
            .build();

        // Name
        let name_label = Label::builder()
            .label(&artist.name)
            .halign(Start)
            .xalign(0.0)
            .css_classes(["title-1"])
            .ellipsize(EllipsizeEnd)
            .tooltip_text(&artist.name)
            .build();
        metadata_container.append(name_label.upcast_ref::<Widget>());

        // Bio placeholder
        let bio_label = Label::builder()
            .label("Artist biography would appear here when available.")
            .halign(Start)
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(80)
            .css_classes(["dim-label"])
            .build();
        metadata_container.append(bio_label.upcast_ref::<Widget>());

        // Play all button
        let play_all_button = Button::builder()
            .label("Play All Artist Tracks")
            .halign(Start)
            .build();
        metadata_container.append(play_all_button.upcast_ref::<Widget>());

        header_container.append(cover_container.upcast_ref::<Widget>());
        header_container.append(metadata_container.upcast_ref::<Widget>());

        header_container.upcast_ref::<Widget>().clone()
    }

    /// Creates a placeholder for the album listing section.
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the album list placeholder.
    fn create_album_list_placeholder(&self) -> Widget {
        let list_container = GtkBox::builder().orientation(Vertical).spacing(8).build();

        let title_label = Label::builder()
            .label("Albums")
            .halign(Start)
            .xalign(0.0)
            .css_classes(["title-2"])
            .build();
        list_container.append(title_label.upcast_ref::<Widget>());

        let placeholder_label = Label::builder()
            .label("Album listing would appear here.")
            .halign(Start)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        list_container.append(placeholder_label.upcast_ref::<Widget>());

        list_container.upcast_ref::<Widget>().clone()
    }

    /// Updates the display configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - New display configuration
    pub fn update_config(&mut self, config: DetailViewConfig) {
        self.config = config;

        // Rebuild the detail view with new configuration
        if let Some(detail_type) = self.detail_type.clone() {
            self.set_detail(detail_type);
        }
    }
}

#[async_trait(?Send)]
impl StateObserver for DetailView {
    async fn handle_state_change(&mut self, event: AppStateEvent) {
        if let LibraryStateChanged(state) = event {
            self.handle_library_state_change(state).await;
        }
    }
}

impl DetailView {
    async fn handle_library_state_change(&mut self, state: LibraryState) {
        // Update track listings if we're showing an album detail
        if let Some(DetailType::Album(ref album)) = self.detail_type {
            let tracks: Vec<Track> = state
                .current_tracks
                .into_iter()
                .filter(|track| track.album_id == album.id)
                .collect();

            // Find the track list section and update it
            // This is a simplified approach - in practice, we'd need to track the track list widget
            if !tracks.is_empty() {
                // For now, just rebuild the entire view
                if let Some(detail_type) = self.detail_type.clone() {
                    self.set_detail(detail_type);
                }
            }
        }
    }
}

impl Default for DetailView {
    fn default() -> Self {
        Self::new(None, DetailType::Album(Album::default()), false)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        library::models::{Album, Artist},
        ui::views::detail_view::{DetailType, DetailView},
    };

    #[test]
    fn test_detail_view_builder() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            ..Artist::default()
        };

        let detail_view = DetailView::builder()
            .detail_type(DetailType::Artist(artist))
            .compact(true)
            .build();

        match &detail_view.detail_type {
            Some(DetailType::Artist(_)) => assert!(true),
            _ => assert!(false),
        }
        assert!(detail_view.config.compact);
    }

    #[test]
    fn test_detail_view_default() {
        let detail_view = DetailView::default();
        match &detail_view.detail_type {
            Some(DetailType::Album(_)) => assert!(true),
            _ => assert!(false),
        }
        assert!(!detail_view.config.compact);
    }

    #[test]
    fn test_detail_types() {
        let album = Album::default();
        let artist = Artist::default();

        assert_eq!(
            format!("{:?}", DetailType::Album(album)),
            "Album(Album { id: 0, artist_id: 0, title: \"\", year: None, genre: None, compilation: false, path: \"\", dr_value: None, created_at: None, updated_at: None })"
        );
        assert_eq!(
            format!("{:?}", DetailType::Artist(artist)),
            "Artist(Artist { id: 0, name: \"\", created_at: None, updated_at: None })"
        );
    }
}
