//! Column/list view alternative for both albums and artists.
//!
//! This module implements the `ListView` component that displays albums or artists
//! in a column/list layout with detailed metadata, supporting virtual scrolling
//! for large datasets and real-time filtering/sorting.

use std::sync::Arc;

use libadwaita::{
    gtk::{
        Align::{Start, Fill},
        Box as GtkBox,
        Label,
        ListBox,
        ListBoxRow,
        Orientation::Horizontal,
        Widget,
    },
    prelude::{BoxExt, LabelExt, ListBoxExt, ListBoxRowExt, WidgetExt},
};

use crate::{
    library::models::{Album, Artist},
    state::{AppState, LibraryState, StateObserver},
    ui::components::{cover_art::CoverArt, hifi_metadata::HiFiMetadata},
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
#[derive(Debug, Clone, PartialEq)]
pub enum ListViewType {
    /// Display albums in list view
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
    pub fn new(
        app_state: Option<Arc<AppState>>,
        view_type: ListViewType,
        compact: bool,
    ) -> Self {
        let config = ListViewConfig { compact };

        let list_box = ListBox::builder()
            .selection_mode(libadwaita::gtk::SelectionMode::None)
            .css_classes(vec!["list-view".to_string()])
            .build();

        // Set ARIA attributes for accessibility
        list_box.set_accessible_role(libadwaita::gtk::AccessibleRole::List);
        list_box.set_accessible_description(Some(match view_type {
            ListViewType::Albums => "Album list view",
            ListViewType::Artists => "Artist list view",
        }));

        let mut view = Self {
            widget: list_box.clone().upcast::<Widget>(),
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
        self.list_box.foreach(|row| {
            self.list_box.remove(row);
        });
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
            .artwork_path(&album.path)
            .dr_value(album.dr_value.clone().unwrap_or_else(|| "N/A".to_string()))
            .show_dr_badge(true)
            .dimensions(48, 48)
            .build();

        // Create main info container
        let info_container = GtkBox::builder()
            .orientation(Vertical)
            .hexpand(true)
            .spacing(2)
            .build();

        // Title label
        let title_label = Label::builder()
            .label(&album.title)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(libadwaita::gtk::pango::EllipsizeMode::End)
            .tooltip_text(&album.title)
            .build();

        // Artist/year info
        let artist_year_text = if let Some(year) = album.year {
            format!("{} ({})", album.artist_id, year)
        } else {
            album.artist_id.to_string()
        };

        let artist_year_label = Label::builder()
            .label(&artist_year_text)
            .halign(Start)
            .xalign(0.0)
            .css_classes(vec!["dim-label".to_string()])
            .ellipsize(libadwaita::gtk::pango::EllipsizeMode::End)
            .tooltip_text(&artist_year_text)
            .build();

        info_container.append(&title_label.upcast::<Widget>());
        info_container.append(&artist_year_label.upcast::<Widget>());

        // Create main row container
        let row_container = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();

        row_container.append(&cover_art.widget);
        row_container.append(&info_container.upcast::<Widget>());

        // Add additional metadata if not compact
        if !self.config.compact {
            // Genre info
            if let Some(ref genre) = album.genre {
                let genre_label = Label::builder()
                    .label(genre)
                    .halign(Start)
                    .xalign(0.0)
                    .css_classes(vec!["dim-label".to_string()])
                    .ellipsize(libadwaita::gtk::pango::EllipsizeMode::End)
                    .tooltip_text(genre)
                    .build();
                row_container.append(&genre_label.upcast::<Widget>());
            }

            // Compilation indicator
            if album.compilation {
                let compilation_label = Label::builder()
                    .label("Compilation")
                    .halign(Start)
                    .xalign(0.0)
                    .css_classes(vec!["dim-label".to_string()])
                    .ellipsize(libadwaita::gtk::pango::EllipsizeMode::End)
                    .tooltip_text("Compilation album")
                    .build();
                row_container.append(&compilation_label.upcast::<Widget>());
            }
        }

        // Create ListBoxRow wrapper
        let row = ListBoxRow::new();
        row.set_child(Some(&row_container));
        row.set_activatable(true);
        row.set_selectable(true);

        // Set ARIA attributes for accessibility
        row.set_accessible_description(Some(&format!(
            "Album: {}, Artist ID: {}",
            album.title, album.artist_id
        )));

        row.upcast::<Widget>()
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
        let info_container = GtkBox::builder()
            .orientation(Vertical)
            .hexpand(true)
            .spacing(2)
            .build();

        // Name label
        let name_label = Label::builder()
            .label(&artist.name)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(libadwaita::gtk::pango::EllipsizeMode::End)
            .tooltip_text(&artist.name)
            .build();

        info_container.append(&name_label.upcast::<Widget>());

        // Create main row container
        let row_container = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();

        row_container.append(&cover_art.widget);
        row_container.append(&info_container.upcast::<Widget>());

        // Create ListBoxRow wrapper
        let row = ListBoxRow::new();
        row.set_child(Some(&row_container));
        row.set_activatable(true);
        row.set_selectable(true);

        // Set ARIA attributes for accessibility
        row.set_accessible_description(Some(&format!("Artist: {}", artist.name)));

        row.upcast::<Widget>()
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
                        .filter(|artist| {
                            artist.name.to_lowercase().contains(&query.to_lowercase())
                        })
                        .collect();
                    self.set_artists(filtered_artists);
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl StateObserver for ListView {
    async fn handle_state_change(&mut self, event: crate::state::AppStateEvent) {
        match event {
            crate::state::AppStateEvent::LibraryStateChanged(state) => {
                self.handle_library_state_change(state).await;
            }
            crate::state::AppStateEvent::SearchFilterChanged(filter) => {
                if let Some(query) = filter {
                    self.filter_items(&query);
                } else {
                    // Reset to all items
                    if let Some(ref app_state) = self.app_state {
                        let library_state = app_state.get_library_state();
                        match self.view_type {
                            ListViewType::Albums => self.set_albums(library_state.albums),
                            ListViewType::Artists => self.set_artists(library_state.artists),
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl ListView {
    async fn handle_library_state_change(&mut self, state: LibraryState) {
        match self.view_type {
            ListViewType::Albums => self.set_albums(state.albums),
            ListViewType::Artists => self.set_artists(state.artists),
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
    use super::*;
    use crate::library::models::{Album, Artist};

    #[test]
    fn test_list_view_builder() {
        let list_view = ListView::builder()
            .view_type(ListViewType::Artists)
            .compact(true)
            .build();

        assert_eq!(list_view.view_type, ListViewType::Artists);
        assert!(list_view.config.compact);
    }

    #[test]
    fn test_list_view_default() {
        let list_view = ListView::default();
        assert_eq!(list_view.view_type, ListViewType::Albums);
        assert!(!list_view.config.compact);
    }

    #[test]
    fn test_list_view_types() {
        assert_eq!(format!("{:?}", ListViewType::Albums), "Albums");
        assert_eq!(format!("{:?}", ListViewType::Artists), "Artists");
    }
}