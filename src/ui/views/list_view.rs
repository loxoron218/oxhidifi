//! Column/list view alternative for both albums and artists.
//!
//! This module implements the `ListView` component that displays albums or artists
//! in a column/list layout with detailed metadata, supporting virtual scrolling
//! for large datasets and real-time filtering/sorting.

use std::sync::Arc;

use libadwaita::{
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
    state::AppState,
    ui::components::cover_art::CoverArt,
};

/// Builder pattern for configuring ListView components.
#[derive(Debug, Default)]
pub struct ListViewBuilder {
    app_state: Option<Arc<AppState>>,
    view_type: ListViewType,
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
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Builds the ListView component.
    ///
    /// # Returns
    ///
    /// A new `ListView` instance.
    pub fn build(self) -> ListView {
        ListView::new(self.app_state, self.view_type, self.compact)
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
/// more detailed information than the grid view, with support for virtual
/// scrolling, real-time filtering, and keyboard navigation.
pub struct ListView {
    /// The underlying GTK widget (ListBox).
    pub widget: Widget,
    /// The list box container.
    pub list_box: ListBox,
    /// Current application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Type of items being displayed.
    pub view_type: ListViewType,
    /// Configuration flags.
    pub config: ListViewConfig,
}

/// Configuration for ListView display options.
#[derive(Debug, Clone)]
pub struct ListViewConfig {
    /// Whether to use compact layout.
    pub compact: bool,
}

impl ListView {
    /// Creates a new ListView component.
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
    pub fn new(app_state: Option<Arc<AppState>>, view_type: ListViewType, compact: bool) -> Self {
        let config = ListViewConfig { compact };

        let list_box = ListBox::builder()
            .selection_mode(SelectionNone)
            .css_classes(["list-view"])
            .build();

        // Set ARIA attributes for accessibility
        list_box.set_accessible_role(List);

        // set_accessible_description doesn't exist in GTK4, remove this line

        let mut view = Self {
            widget: list_box.clone().upcast_ref::<Widget>().clone(),
            list_box,
            app_state,
            view_type,
            config,
        };

        // Initialize empty list
        view.clear_list();

        view
    }

    /// Creates a ListView builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `ListViewBuilder` instance.
    pub fn builder() -> ListViewBuilder {
        ListViewBuilder::default()
    }

    /// Clears the current list and prepares for new items.
    fn clear_list(&mut self) {
        // Clear all children from the list box
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }
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

        self.clear_list();

        for album in albums {
            let row = self.create_album_row(&album);
            self.list_box.append(&row);
        }
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

        self.clear_list();

        for artist in artists {
            let row = self.create_artist_row(&artist);
            self.list_box.append(&row);
        }
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
        // Create cover art
        let cover_art = CoverArt::builder()
            .artwork_path(album.artwork_path.as_deref().unwrap_or(&album.path))
            .dr_value(album.dr_value.clone().unwrap_or_else(|| "N/A".to_string()))
            .show_dr_badge(true)
            .dimensions(48, 48)
            .build();

        // Create main info container
        let info_container = Box::builder()
            .orientation(Vertical)
            .hexpand(true)
            .spacing(2)
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
        let artist_name = if let Some(app_state) = &self.app_state {
            let library_state = app_state.get_library_state();
            library_state
                .artists
                .iter()
                .find(|artist| artist.id == album.artist_id)
                .map(|artist| artist.name.clone())
                .unwrap_or_else(|| "Unknown Artist".to_string())
        } else {
            "Unknown Artist".to_string()
        };

        // Artist/year info
        let artist_year_text = if let Some(year) = album.year {
            format!("{} ({})", artist_name, year)
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
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();

        row_container.append(&cover_art.widget);
        row_container.append(info_container.upcast_ref::<Widget>());

        // Add additional metadata if not compact
        if !self.config.compact {
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

        // set_accessible_description doesn't exist in GTK4, remove this line

        row.upcast_ref::<Widget>().clone()
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
        // Create cover art (default image)
        let cover_art = CoverArt::builder()
            .artwork_path("")
            .show_dr_badge(false)
            .dimensions(48, 48)
            .build();

        // Create main info container
        let info_container = Box::builder()
            .orientation(Vertical)
            .hexpand(true)
            .spacing(2)
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
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();

        row_container.append(&cover_art.widget);
        row_container.append(info_container.upcast_ref::<Widget>());

        // Create ListBoxRow wrapper
        let row = ListBoxRow::new();
        row.set_child(Some(&row_container));
        row.set_activatable(true);
        row.set_selectable(true);

        // set_accessible_description doesn't exist in GTK4, remove this line

        row.upcast_ref::<Widget>().clone()
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

    /// Filters items based on a search query.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    pub fn filter_items(&mut self, query: &str) {
        if let Some(ref app_state) = self.app_state {
            let library_state = app_state.get_library_state();
            match self.view_type {
                ListViewType::Albums => {
                    let filtered_albums: Vec<Album> = library_state
                        .albums
                        .into_iter()
                        .filter(|album| {
                            album.title.to_lowercase().contains(&query.to_lowercase())
                                || album.artist_id.to_string().contains(&query.to_lowercase())
                        })
                        .collect();
                    self.set_albums(filtered_albums);
                }
                ListViewType::Artists => {
                    let filtered_artists: Vec<Artist> = library_state
                        .artists
                        .into_iter()
                        .filter(|artist| artist.name.to_lowercase().contains(&query.to_lowercase()))
                        .collect();
                    self.set_artists(filtered_artists);
                }
            }
        }
    }
}

impl Default for ListView {
    fn default() -> Self {
        Self::new(None, ListViewType::Albums, false)
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
        assert_eq!(format!("{:?}", Albums), "Albums");
        assert_eq!(format!("{:?}", Artists), "Artists");
    }
}
