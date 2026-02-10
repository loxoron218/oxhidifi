//! Column/list view alternative for both albums and artists.
//!
//! This module implements the `ListView` component that displays albums or artists
//! in a column/list layout with detailed metadata, supporting virtual scrolling
//! for large datasets and real-time filtering/sorting.

use std::{cell::RefCell, collections::HashSet, convert::TryFrom, rc::Rc, sync::Arc};

use libadwaita::{
    glib::{JoinHandle, MainContext},
    gtk::{
        AccessibleRole::List,
        Align::Start,
        Box, Label, ListBox, ListBoxRow,
        Orientation::{Horizontal, Vertical},
        SelectionMode::None as SelectionNone,
        Widget,
        pango::EllipsizeMode::End,
    },
    prelude::{AccessibleExt, BoxExt, Cast, ListBoxRowExt, WidgetExt},
};

use crate::{
    library::models::{Album, Artist},
    state::{
        AppState,
        NavigationState::{AlbumDetail, ArtistDetail},
        ZoomEvent::ListZoomChanged,
        app_state::AppStateEvent::SettingsChanged,
    },
    ui::{
        components::{cover_art::CoverArt, search_empty_state::SearchEmptyState},
        views::filtering::Filterable,
    },
};

/// Builder pattern for configuring `ListView` components.
#[derive(Debug, Default)]
pub struct ListViewBuilder {
    /// Optional application state reference for reactive updates.
    app_state: Option<Arc<AppState>>,
    /// The type of items to display (albums or artists).
    view_type: ListViewType,
    /// Whether to use compact layout.
    compact: bool,
}

impl ListViewBuilder {
    /// Sets the application state for reactive updates.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn app_state(mut self, app_state: Arc<AppState>) -> Self {
        self.app_state = Some(app_state);
        self
    }

    /// Sets the view type (albums or artists).
    ///
    /// # Arguments
    ///
    /// * `view_type` - The type of items to display
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn view_type(mut self, view_type: ListViewType) -> Self {
        self.view_type = view_type;
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

    /// Builds the `ListView` component.
    ///
    /// # Returns
    ///
    /// A new `ListView` instance.
    #[must_use]
    pub fn build(self) -> ListView {
        ListView::new(self.app_state.as_ref(), &self.view_type, self.compact)
    }
}

/// Type of items to display in the list view.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ListViewType {
    /// Display albums in list view
    #[default]
    Albums,
    /// Display artists in list view
    Artists,
}

/// Column/list view for displaying albums or artists with detailed metadata.
///
/// The `ListView` component displays items in a column layout that provides
/// more detailed information than grid view, with support for virtual
/// scrolling, real-time filtering, and keyboard navigation.
pub struct ListView {
    /// The underlying GTK widget (`ListBox`).
    pub widget: Widget,
    /// The list box container.
    pub list_box: ListBox,
    /// Current application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Type of items being displayed.
    pub view_type: ListViewType,
    /// Configuration flags.
    pub config: ListViewConfig,
    /// Search empty state component for when search returns no results.
    pub search_empty_state: SearchEmptyState,
    /// Zoom subscription handle for cleanup.
    _zoom_subscription_handle: Option<JoinHandle<()>>,
    /// Settings subscription handle for cleanup.
    _settings_subscription_handle: Option<JoinHandle<()>>,
    /// References to cover art components for dynamic updates.
    cover_arts: Rc<RefCell<Vec<CoverArt>>>,
    /// Current albums being displayed.
    pub albums: Vec<Album>,
    /// Current artists being displayed.
    pub artists: Vec<Artist>,
    /// Row widgets with their IDs for filtering.
    rows: Rc<RefCell<Vec<(Widget, i64)>>>,
}

/// Configuration for `ListView` display options.
#[derive(Debug, Clone)]
pub struct ListViewConfig {
    /// Whether to use compact layout.
    pub compact: bool,
}

impl ListView {
    /// Creates a new `ListView` component.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference for reactive updates
    /// * `view_type` - Type of items to display (albums or artists)
    /// * `compact` - Whether to use compact layout
    ///
    /// # Returns
    ///
    /// A new `ListView` instance.
    ///
    /// # Panics
    ///
    /// Panics if the cover dimensions from zoom manager are negative.
    #[must_use]
    pub fn new(app_state: Option<&Arc<AppState>>, view_type: &ListViewType, compact: bool) -> Self {
        let config = ListViewConfig { compact };

        let list_box = ListBox::builder()
            .selection_mode(SelectionNone)
            .css_classes(["list-view"])
            .build();

        // Set ARIA attributes for accessibility
        list_box.set_accessible_role(List);

        // set_accessible_description doesn't exist in GTK4, remove this line

        let cover_arts = Rc::new(RefCell::new(Vec::new()));

        // Create main container that can hold both list box and search empty state
        let main_container = Box::builder().orientation(Vertical).build();
        main_container.append(&list_box.clone().upcast::<Widget>());

        // Create and add search empty state component
        let search_empty_state = SearchEmptyState::builder()
            .is_album_view(matches!(view_type, ListViewType::Albums))
            .build();
        main_container.append(search_empty_state.widget());
        search_empty_state.hide();

        let mut view = Self {
            widget: main_container.upcast_ref::<Widget>().clone(),
            list_box: list_box.clone(),
            app_state: app_state.cloned(),
            view_type: view_type.clone(),
            config: config.clone(),
            search_empty_state,
            _zoom_subscription_handle: if let Some(state) = app_state {
                // Subscribe to zoom changes
                let state_clone: Arc<AppState> = state.clone();
                let list_box_clone = list_box.clone();
                let view_type_clone = view_type.clone();
                let config_clone = config.clone();
                let cover_arts_clone = cover_arts.clone();
                let handle = MainContext::default().spawn_local(async move {
                    let rx = state_clone.zoom_manager.subscribe();
                    while let Ok(event) = rx.recv().await {
                        if let ListZoomChanged(_) = event {
                            // Rebuild all list items with new zoom level
                            // Get current library state
                            let library_state = state_clone.get_library_state();

                            // Clear existing children
                            while let Some(child) = list_box_clone.first_child() {
                                list_box_clone.remove(&child);
                            }
                            cover_arts_clone.borrow_mut().clear();

                            // Rebuild list with updated dimensions
                            match view_type_clone {
                                ListViewType::Albums => {
                                    for album in &library_state.albums {
                                        let row = create_album_row_with_zoom(
                                            album,
                                            Some(&state_clone),
                                            &config_clone,
                                            u32::try_from(
                                                state_clone
                                                    .zoom_manager
                                                    .get_list_cover_dimensions()
                                                    .0,
                                            )
                                            .expect("cover_size should be within u32 range"),
                                            &cover_arts_clone,
                                        );
                                        list_box_clone.append(&row);
                                    }
                                }
                                ListViewType::Artists => {
                                    for artist in &library_state.artists {
                                        let row = create_artist_row_with_zoom(
                                            artist,
                                            Some(&state_clone),
                                            &config_clone,
                                            u32::try_from(
                                                state_clone
                                                    .zoom_manager
                                                    .get_list_cover_dimensions()
                                                    .0,
                                            )
                                            .expect("cover_size should be within u32 range"),
                                        );
                                        list_box_clone.append(&row);
                                    }
                                }
                            }
                        }
                    }
                });
                Some(handle)
            } else {
                None
            },
            _settings_subscription_handle: if let Some(state) = app_state {
                // Subscribe to settings changes
                let state_clone: Arc<AppState> = state.clone();
                let cover_arts_clone = cover_arts.clone();
                let view_type_clone = view_type.clone();
                let handle = MainContext::default().spawn_local(async move {
                    let rx = state_clone.subscribe();
                    while let Ok(event) = rx.recv().await {
                        if let SettingsChanged { show_dr_values } = event {
                            // Only update albums, not artists
                            if view_type_clone == ListViewType::Albums {
                                // Update all cover art components with new DR badge visibility
                                let mut cover_arts = cover_arts_clone.borrow_mut();
                                for cover_art in cover_arts.iter_mut() {
                                    cover_art.set_show_dr_badge(show_dr_values);
                                }
                            }
                        }
                    }
                });
                Some(handle)
            } else {
                None
            },
            cover_arts,
            albums: Vec::new(),
            artists: Vec::new(),
            rows: Rc::new(RefCell::new(Vec::new())),
        };

        // Initialize empty list
        view.clear_list();

        view
    }

    /// Creates a `ListView` builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `ListViewBuilder` instance.
    #[must_use]
    pub fn builder() -> ListViewBuilder {
        ListViewBuilder::default()
    }

    /// Clears the current list and prepares for new items.
    fn clear_list(&mut self) {
        // Clear all children from the list box
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        // Hide search empty state when clearing
        self.search_empty_state.hide();
    }

    /// Sets the albums to display in the list.
    ///
    /// # Arguments
    ///
    /// * `albums` - New vector of albums to display
    pub fn set_albums(&mut self, albums: Vec<Album>) {
        if self.view_type != ListViewType::Albums {
            return;
        }

        // Check if albums are actually different to avoid unnecessary widget recreation
        let albums_unchanged = self.albums.len() == albums.len()
            && self
                .albums
                .iter()
                .zip(albums.iter())
                .all(|(a, b)| a.id == b.id);

        if albums_unchanged {
            return;
        }

        self.clear_list();
        self.cover_arts.borrow_mut().clear();
        self.rows.borrow_mut().clear();

        for album in &albums {
            let row = self.create_album_row(album);
            self.list_box.append(&row);
            self.rows.borrow_mut().push((row.clone(), album.id));
        }

        self.albums = albums;

        // Hide search empty state when showing albums
        self.search_empty_state.hide();
    }

    /// Sets the artists to display in the list.
    ///
    /// # Arguments
    ///
    /// * `artists` - New vector of artists to display
    pub fn set_artists(&mut self, artists: Vec<Artist>) {
        if self.view_type != ListViewType::Artists {
            return;
        }

        // Check if artists are actually different to avoid unnecessary widget recreation
        let artists_unchanged = self.artists.len() == artists.len()
            && self
                .artists
                .iter()
                .zip(artists.iter())
                .all(|(a, b)| a.id == b.id);

        if artists_unchanged {
            return;
        }

        self.clear_list();
        self.rows.borrow_mut().clear();

        for artist in &artists {
            let row = self.create_artist_row(artist);
            self.list_box.append(&row);
            self.rows.borrow_mut().push((row.clone(), artist.id));
        }

        self.artists = artists;

        // Hide search empty state when showing artists
        self.search_empty_state.hide();
    }

    /// Creates a single album row widget for the list.
    ///
    /// # Arguments
    ///
    /// * `album` - The album to create a row for
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the album row.
    fn create_album_row(&self, album: &Album) -> Widget {
        // Get cover size from zoom manager if available
        let cover_size = if let Some(app_state) = &self.app_state {
            app_state.zoom_manager.get_list_cover_dimensions().0
        } else {
            48 // Default cover size
        };

        create_album_row_with_zoom(
            album,
            self.app_state.as_ref(),
            &self.config,
            u32::try_from(cover_size).expect("cover_size should be within u32 range"),
            &self.cover_arts,
        )
    }

    /// Creates a single artist row widget for the list.
    ///
    /// # Arguments
    ///
    /// * `artist` - The artist to create a row for
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the artist row.
    fn create_artist_row(&self, artist: &Artist) -> Widget {
        // Get cover size from zoom manager if available
        let cover_size = if let Some(app_state) = &self.app_state {
            app_state.zoom_manager.get_list_cover_dimensions().0
        } else {
            48 // Default cover size
        };

        create_artist_row_with_zoom(
            artist,
            self.app_state.as_ref(),
            &self.config,
            u32::try_from(cover_size).expect("cover_size should be within u32 range"),
        )
    }

    /// Updates the display configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - New display configuration
    pub fn update_config(&mut self, config: ListViewConfig) {
        self.config = config;

        // Rebuild the list with new configuration
        if let Some(ref app_state) = self.app_state {
            let library_state = app_state.get_library_state();
            match self.view_type {
                ListViewType::Albums => self.set_albums(library_state.albums),
                ListViewType::Artists => self.set_artists(library_state.artists),
            }
        }
    }

    /// Updates the search empty state visibility based on filter results.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query string
    /// * `has_results` - Whether the filter returned any results
    fn update_empty_state(&mut self, query: &str, has_results: bool) {
        if has_results {
            self.search_empty_state.hide();
        } else {
            self.search_empty_state.update_search_query(query);
            self.search_empty_state.show();
        }
    }

    /// Filters items based on a search query.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    pub fn filter_view_items(&mut self, query: &str) {
        if let Some(ref app_state) = self.app_state {
            let library_state = app_state.get_library_state();

            match self.view_type {
                ListViewType::Albums => {
                    let albums = library_state.albums.clone();
                    let has_results = self.filter_items(query, &albums, |album, q| {
                        album.title.to_lowercase().contains(q)
                            || album.artist_id.to_string().to_lowercase().contains(q)
                    });
                    self.update_empty_state(query, has_results);
                }
                ListViewType::Artists => {
                    let artists = library_state.artists.clone();
                    let has_results = self.filter_items(query, &artists, |artist, q| {
                        artist.name.to_lowercase().contains(q)
                    });
                    self.update_empty_state(query, has_results);
                }
            }
        }
    }

    /// Clears the view by hiding all items.
    ///
    /// This is used when switching tabs with an active search to prevent
    /// the unfiltered view from appearing during the transition.
    pub fn clear_view(&self) {
        Filterable::<Album>::clear_view(self);
    }
}

impl Filterable<Album> for ListView {
    /// Returns the unique identifier for an album item.
    ///
    /// # Arguments
    ///
    /// * `item` - The album to get the ID from
    ///
    /// # Returns
    ///
    /// The album's unique identifier.
    fn get_widget_id(&self, item: &Album) -> i64 {
        item.id
    }

    /// Returns a copy of the currently displayed albums.
    ///
    /// # Returns
    ///
    /// A vector of albums currently displayed in the view.
    fn get_current_items(&self) -> Vec<Album> {
        self.albums.clone()
    }

    /// Updates the albums currently displayed in the view.
    ///
    /// # Arguments
    ///
    /// * `items` - New vector of albums to display
    fn set_current_items(&mut self, items: Vec<Album>) {
        self.albums = items;
    }

    /// Sets the visibility of album rows based on filtered IDs.
    ///
    /// # Arguments
    ///
    /// * `visible_ids` - Set of album IDs that should be visible
    fn set_visibility(&self, visible_ids: &HashSet<i64>) {
        let rows = self.rows.borrow();
        for (row_widget, album_id) in rows.iter() {
            let row_visible = visible_ids.contains(album_id);
            row_widget.set_visible(row_visible);
        }
    }
}

impl Filterable<Artist> for ListView {
    /// Returns the unique identifier for an artist item.
    ///
    /// # Arguments
    ///
    /// * `item` - The artist to get the ID from
    ///
    /// # Returns
    ///
    /// The artist's unique identifier.
    fn get_widget_id(&self, item: &Artist) -> i64 {
        item.id
    }

    /// Returns a copy of the currently displayed artists.
    ///
    /// # Returns
    ///
    /// A vector of artists currently displayed in the view.
    fn get_current_items(&self) -> Vec<Artist> {
        self.artists.clone()
    }

    /// Updates the artists currently displayed in the view.
    ///
    /// # Arguments
    ///
    /// * `items` - New vector of artists to display
    fn set_current_items(&mut self, items: Vec<Artist>) {
        self.artists = items;
    }

    /// Sets the visibility of artist rows based on filtered IDs.
    ///
    /// # Arguments
    ///
    /// * `visible_ids` - Set of artist IDs that should be visible
    fn set_visibility(&self, visible_ids: &HashSet<i64>) {
        let rows = self.rows.borrow();
        for (row_widget, artist_id) in rows.iter() {
            let row_visible = visible_ids.contains(artist_id);
            row_widget.set_visible(row_visible);
        }
    }
}

/// Helper function to create an album row with specific cover size.
fn create_album_row_with_zoom(
    album: &Album,
    app_state: Option<&Arc<AppState>>,
    config: &ListViewConfig,
    cover_size: u32,
    cover_arts: &Rc<RefCell<Vec<CoverArt>>>,
) -> Widget {
    // Create cover art
    let show_dr_badge = if let Some(app_state_ref) = app_state {
        app_state_ref
            .get_settings_manager()
            .read()
            .get_settings()
            .show_dr_values
    } else {
        true // Default to showing DR badges
    };

    let cover_art = CoverArt::builder()
        .artwork_path(album.artwork_path.as_deref().unwrap_or(&album.path))
        .dr_value(album.dr_value.clone().unwrap_or_else(|| "N/A".to_string()))
        .show_dr_badge(show_dr_badge)
        .dimensions(
            i32::try_from(cover_size).expect(
                "ListView album cover_size (u32) should fit in i32 for GTK widget dimensions",
            ),
            i32::try_from(cover_size).expect(
                "ListView album cover_size (u32) should fit in i32 for GTK widget dimensions",
            ),
        )
        .build();

    // Store cover art for dynamic updates
    cover_arts.borrow_mut().push(cover_art.clone());

    // Create main info container
    let info_container = Box::builder()
        .orientation(Vertical)
        .hexpand(true)
        .spacing(6)
        .build();

    // Title label
    let title_label = Label::builder()
        .label(&album.title)
        .halign(Start)
        .xalign(0.0)
        .ellipsize(End)
        .tooltip_text(&album.title)
        .build();

    // Look up artist name from app state
    let artist_name = if let Some(app_state_ref) = app_state {
        let library_state = app_state_ref.get_library_state();
        library_state
            .artists
            .iter()
            .find(|artist| artist.id == album.artist_id)
            .map_or_else(
                || "Unknown Artist".to_string(),
                |artist| artist.name.clone(),
            )
    } else {
        "Unknown Artist".to_string()
    };

    // Artist/year info
    let artist_year_text = if let Some(year) = album.year {
        format!("{artist_name} ({year})")
    } else {
        artist_name
    };

    let artist_year_label = Label::builder()
        .label(&artist_year_text)
        .halign(Start)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .ellipsize(End)
        .tooltip_text(&artist_year_text)
        .build();

    info_container.append(title_label.upcast_ref::<Widget>());
    info_container.append(artist_year_label.upcast_ref::<Widget>());

    // Create main row container
    let row_container = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    row_container.append(&cover_art.widget);
    row_container.append(info_container.upcast_ref::<Widget>());

    // Add additional metadata if not compact
    if !config.compact {
        // Genre info
        if let Some(ref genre) = album.genre {
            let genre_label = Label::builder()
                .label(genre)
                .halign(Start)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .ellipsize(End)
                .tooltip_text(genre)
                .build();
            row_container.append(genre_label.upcast_ref::<Widget>());
        }

        // Compilation indicator
        if album.compilation {
            let compilation_label = Label::builder()
                .label("Compilation")
                .halign(Start)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .ellipsize(End)
                .tooltip_text("Compilation album")
                .build();
            row_container.append(compilation_label.upcast_ref::<Widget>());
        }
    }

    // Create ListBoxRow wrapper
    let row = ListBoxRow::new();
    row.set_child(Some(&row_container));
    row.set_activatable(true);
    row.set_selectable(true);

    // Handle row activation for navigation
    let album_clone = album.clone();
    let app_state_clone = app_state.cloned();
    row.connect_activate(move |_| {
        if let Some(ref state) = app_state_clone {
            state.update_navigation(AlbumDetail(album_clone.clone()));
        }
    });

    row.upcast_ref::<Widget>().clone()
}

/// Helper function to create an artist row with specific cover size.
fn create_artist_row_with_zoom(
    artist: &Artist,
    app_state: Option<&Arc<AppState>>,
    _config: &ListViewConfig,
    cover_size: u32,
) -> Widget {
    // Create cover art (default image)
    let cover_art = CoverArt::builder()
        .artwork_path("")
        .show_dr_badge(false)
        .dimensions(
            i32::try_from(cover_size).expect(
                "ListView artist cover_size (u32) should fit in i32 for GTK widget dimensions",
            ),
            i32::try_from(cover_size).expect(
                "ListView artist cover_size (u32) should fit in i32 for GTK widget dimensions",
            ),
        )
        .build();

    // Create main info container
    let info_container = Box::builder()
        .orientation(Vertical)
        .hexpand(true)
        .spacing(6)
        .build();

    // Name label
    let name_label = Label::builder()
        .label(&artist.name)
        .halign(Start)
        .xalign(0.0)
        .ellipsize(End)
        .tooltip_text(&artist.name)
        .build();

    info_container.append(name_label.upcast_ref::<Widget>());

    // Create main row container
    let row_container = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    row_container.append(&cover_art.widget);
    row_container.append(info_container.upcast_ref::<Widget>());

    // Create ListBoxRow wrapper
    let row = ListBoxRow::new();
    row.set_child(Some(&row_container));
    row.set_activatable(true);
    row.set_selectable(true);

    // Handle row activation for navigation
    let artist_clone = artist.clone();
    let app_state_clone = app_state.cloned();
    row.connect_activate(move |_| {
        if let Some(ref state) = app_state_clone {
            state.update_navigation(ArtistDetail(artist_clone.clone()));
        }
    });

    row.upcast_ref::<Widget>().clone()
}

impl Default for ListView {
    fn default() -> Self {
        Self::new(None, &ListViewType::Albums, false)
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::views::list_view::{
        ListView,
        ListViewType::{Albums, Artists},
    };

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_list_view_builder() {
        let list_view = ListView::builder().view_type(Artists).compact(true).build();

        assert_eq!(list_view.view_type, Artists);
        assert!(list_view.config.compact);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_list_view_default() {
        let list_view = ListView::default();
        assert_eq!(list_view.view_type, Albums);
        assert!(!list_view.config.compact);
    }

    #[test]
    fn test_list_view_types() {
        // This test doesn't require GTK, so no skip needed
        assert_eq!(format!("{Albums:?}"), "Albums");
        assert_eq!(format!("{Artists:?}"), "Artists");
    }
}
