//! Artist grid view with artist images and album counts.
//!
//! This module implements the `ArtistGridView` component that displays artists
//! in a responsive grid layout with artist images, names, and album counts,
//! supporting virtual scrolling for large datasets and real-time filtering.

use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
    sync::Arc,
};

use {
    libadwaita::{
        glib::{JoinHandle, MainContext},
        gtk::{
            AccessibleRole::{Grid, Group},
            Align::{Center, Fill, Start},
            Box, CheckButton, EventControllerMotion, FlowBox, FlowBoxChild, GestureClick, Label,
            Orientation::Vertical,
            Overlay,
            SelectionMode::None as SelectionNone,
            Widget,
            pango::EllipsizeMode::End,
        },
        prelude::{
            AccessibleExt, BoxExt, Cast, CheckButtonExt, FlowBoxChildExt, ObjectExt, WidgetExt,
        },
    },
    tracing::error,
};

use crate::{
    error::{
        domain::UiError::{self, BuilderError},
        numeric_conversion::{safe_i32_to_u32, safe_u32_to_i32},
    },
    library::models::Artist,
    state::{
        app_state::{
            AppState, AppStateEvent::SelectionChanged, LibraryState, LibraryTab::Artists,
            NavigationState::ArtistDetail,
        },
        zoom_manager::ZoomEvent::GridZoomChanged,
    },
    ui::{
        components::{
            cover_art::CoverArt,
            empty_state::{EmptyState, EmptyStateConfig},
            search_empty_state::SearchEmptyState,
        },
        views::filtering::Filterable,
    },
};

/// Maximum cover size in pixels to prevent UI rendering issues.
const MAX_COVER_SIZE: u32 = 4096;

/// Maximum cover size as i32 (derived from `MAX_COVER_SIZE`).
const MAX_COVER_SIZE_I32: i32 = MAX_COVER_SIZE.cast_signed();

/// Builder pattern for configuring `ArtistGridView` components.
#[derive(Debug, Default)]
pub struct ArtistGridViewBuilder {
    /// Optional application state reference for reactive updates.
    app_state: Option<Arc<AppState>>,
    /// Vector of artists to display in the grid.
    artists: Vec<Artist>,
    /// Whether to use compact layout with smaller cover sizes.
    compact: bool,
}

impl ArtistGridViewBuilder {
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

    /// Sets the initial artists to display.
    ///
    /// # Arguments
    ///
    /// * `artists` - Vector of artists to display
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn artists(mut self, artists: Vec<Artist>) -> Self {
        self.artists = artists;
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

    /// Builds the `ArtistGridView` component.
    ///
    /// # Returns
    ///
    /// A new `ArtistGridView` instance.
    #[must_use]
    pub fn build(self) -> ArtistGridView {
        ArtistGridView::new(self.app_state.as_ref(), self.artists, self.compact)
    }
}

/// Responsive grid view for displaying artists with images and album counts.
///
/// The `ArtistGridView` component displays artists in a responsive grid layout
/// that adapts from 360px to 4K+ displays, with support for virtual scrolling,
/// real-time filtering, and keyboard navigation.
pub struct ArtistGridView {
    /// The underlying GTK widget (`FlowBox`).
    pub widget: Widget,
    /// The flow box container.
    pub flow_box: FlowBox,
    /// The count label showing artist count.
    pub count_label: Label,
    /// Current application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Current artists being displayed.
    pub artists: Vec<Artist>,
    /// Full unfiltered list of all artists.
    pub all_artists: Vec<Artist>,
    /// Configuration flags.
    pub config: ArtistGridViewConfig,
    /// Empty state component for when no artists are available.
    pub empty_state: Option<EmptyState>,
    /// Search empty state component for when search returns no results.
    pub search_empty_state: SearchEmptyState,
    /// Current sort criteria.
    pub current_sort: ArtistSortCriteria,
    /// Shared reference to artist cards for zoom updates.
    pub artist_cards_ref: Rc<RefCell<Vec<Rc<ArtistCard>>>>,
    /// Flag to prevent feedback loops during selection sync.
    pub is_syncing_selection: Rc<Cell<bool>>,
    /// Zoom subscription handle for cleanup.
    zoom_subscription_handle: Option<JoinHandle<()>>,
    /// Selection subscription handle for cleanup.
    selection_subscription_handle: Option<JoinHandle<()>>,
}

/// Configuration for `ArtistGridView` display options.
#[derive(Debug, Clone)]
pub struct ArtistGridViewConfig {
    /// Whether to use compact layout.
    pub compact: bool,
}

impl ArtistGridView {
    /// Creates a new `ArtistGridView` component.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference for reactive updates
    /// * `artists` - Initial artists to display
    /// * `compact` - Whether to use compact layout
    ///
    /// # Returns
    ///
    /// A new `ArtistGridView` instance.
    #[must_use]
    pub fn new(app_state: Option<&Arc<AppState>>, artists: Vec<Artist>, compact: bool) -> Self {
        let config = ArtistGridViewConfig { compact };

        let flow_box = Self::create_flow_box();

        let main_container = Self::create_main_container();

        let count_label = Label::builder()
            .label("0 Artists")
            .halign(Center)
            .margin_top(12)
            .margin_bottom(6)
            .css_classes(["album-artist-label"])
            .build();

        main_container.append(&count_label.clone().upcast::<Widget>());
        main_container.append(&flow_box.clone().upcast::<Widget>());

        // Set ARIA attributes for accessibility
        flow_box.set_accessible_role(Grid);

        // Create empty state component
        let empty_state = Self::create_empty_state(app_state);

        // Add empty state to main container if it exists
        if let Some(empty_state) = &empty_state {
            main_container.append(&empty_state.widget);
        }

        // Create and add search empty state component
        let search_empty_state = SearchEmptyState::builder().is_album_view(false).build();
        main_container.append(search_empty_state.widget());
        search_empty_state.hide();

        let artist_cards_ref = Rc::new(RefCell::new(Vec::new()));
        let is_syncing_selection = Rc::new(Cell::new(false));

        let zoom_subscription_handle =
            Self::setup_zoom_subscription(app_state, &flow_box, &artist_cards_ref);

        let selection_subscription_handle =
            Self::setup_selection_subscription(app_state, &artist_cards_ref, &is_syncing_selection);

        let mut view = Self {
            widget: main_container.upcast_ref::<Widget>().clone(),
            flow_box,
            count_label,
            app_state: app_state.cloned(),
            artists: Vec::new(),
            all_artists: artists.clone(),
            config,
            empty_state,
            search_empty_state,
            current_sort: ArtistSortCriteria::Name,
            artist_cards_ref: Rc::clone(&artist_cards_ref),
            is_syncing_selection,
            zoom_subscription_handle,
            selection_subscription_handle,
        };

        view.set_artists(artists);

        view
    }

    /// Creates and configures the `FlowBox` widget.
    ///
    /// # Returns
    ///
    /// Configured `FlowBox` widget.
    fn create_flow_box() -> FlowBox {
        FlowBox::builder()
            .halign(Fill)
            .valign(Start)
            .homogeneous(true)
            .max_children_per_line(100)
            .selection_mode(SelectionNone)
            .row_spacing(6)
            .column_spacing(6)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .hexpand(true)
            .vexpand(false)
            .css_classes(["artist-grid"])
            .build()
    }

    /// Creates the main container box.
    ///
    /// # Returns
    ///
    /// Main container Box widget.
    fn create_main_container() -> Box {
        Box::builder().orientation(Vertical).build()
    }

    /// Creates the empty state component.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference
    ///
    /// # Returns
    ///
    /// Option containing `EmptyState` if `app_state` is provided.
    fn create_empty_state(app_state: Option<&Arc<AppState>>) -> Option<EmptyState> {
        let state = app_state?;
        Some(EmptyState::new(
            Some(Arc::clone(state)),
            None,
            EmptyStateConfig {
                is_album_view: false,
            },
            None,
        ))
    }

    /// Sets up zoom subscription for real-time cover size updates.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference
    /// * `flow_box` - `FlowBox` widget to queue redraws
    /// * `artist_cards_ref` - Reference to artist cards for size updates
    ///
    /// # Returns
    ///
    /// Option containing subscription handle.
    fn setup_zoom_subscription(
        app_state: Option<&Arc<AppState>>,
        flow_box: &FlowBox,
        artist_cards_ref: &Rc<RefCell<Vec<Rc<ArtistCard>>>>,
    ) -> Option<JoinHandle<()>> {
        let state = app_state?;
        let state_clone = Arc::clone(state);
        let flow_box_clone = flow_box.clone();
        let artist_cards_ref_clone = Rc::clone(artist_cards_ref);
        Some(MainContext::default().spawn_local(async move {
            let rx = state_clone.zoom_manager.subscribe();
            while let Ok(event) = rx.recv().await {
                if let GridZoomChanged(_) = &*event {
                    let cover_size = state_clone.zoom_manager.get_grid_cover_dimensions().0;
                    let cover_size_u32 = safe_i32_to_u32(cover_size, 180, "cover_size");

                    let cards = artist_cards_ref_clone.borrow();
                    for card in cards.iter() {
                        if let Err(e) = card.update_cover_size(cover_size_u32) {
                            error!(error = %e, "Failed to update cover size");
                        }
                    }

                    flow_box_clone.queue_draw();
                }
            }
        }))
    }

    /// Sets up selection subscription for real-time selection updates.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference
    /// * `artist_cards_ref` - Reference to artist cards for selection updates
    ///
    /// # Returns
    ///
    /// Option containing subscription handle.
    fn setup_selection_subscription(
        app_state: Option<&Arc<AppState>>,
        artist_cards_ref: &Rc<RefCell<Vec<Rc<ArtistCard>>>>,
        is_syncing: &Rc<Cell<bool>>,
    ) -> Option<JoinHandle<()>> {
        let state = app_state?;
        let state_clone = Arc::clone(state);
        let artist_cards_ref_clone = Rc::clone(artist_cards_ref);
        let is_syncing_clone = Rc::clone(is_syncing);
        Some(MainContext::default().spawn_local(async move {
            let rx = state_clone.subscribe();
            while let Ok(event) = rx.recv().await {
                if let SelectionChanged { tab, selected_ids } = event.as_ref()
                    && matches!(tab, Artists)
                {
                    is_syncing_clone.set(true);
                    let has_selection = !selected_ids.is_empty();
                    let cards = artist_cards_ref_clone.borrow();
                    for card in cards.iter() {
                        let is_selected = selected_ids.contains(&card.artist_id);
                        card.selection_checkbox.set_visible(has_selection);
                        card.selection_checkbox.set_can_target(has_selection);
                        if card.selection_checkbox.is_active() != is_selected {
                            card.set_selection_state(is_selected);
                        }
                    }
                    is_syncing_clone.set(false);
                }
            }
        }))
    }

    /// Creates an `ArtistGridView` builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `ArtistGridViewBuilder` instance.
    #[must_use]
    pub fn builder() -> ArtistGridViewBuilder {
        ArtistGridViewBuilder::default()
    }

    /// Sets the artists to display in the grid.
    ///
    /// # Arguments
    ///
    /// * `artists` - New vector of artists to display
    pub fn set_artists(&mut self, artists: Vec<Artist>) {
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

        // Clear existing children
        while let Some(child) = self.flow_box.first_child() {
            self.flow_box.remove(&child);
        }

        // Clear existing artist cards
        self.artist_cards_ref.borrow_mut().clear();

        self.artists = artists;

        // Apply current sort
        self.apply_sort();

        // Update empty state visibility only when artists change
        if let Some(empty_state) = &self.empty_state {
            let library_state = if let Some(app_state) = &self.app_state {
                app_state.get_library_state()
            } else {
                LibraryState {
                    artists: self.artists.clone(),
                    ..Default::default()
                }
            };
            empty_state.update_from_library_state(&library_state);
        }

        let cover_size = self.get_cover_size();

        // Add new artist items
        for artist in &self.artists {
            let artist_card = match self.create_artist_card(artist, cover_size) {
                Ok(card) => card,
                Err(e) => {
                    error!(artist_name = %artist.name, error = %e, "Failed to create artist card");
                    continue;
                }
            };
            let card_arc = Rc::new(artist_card);
            self.flow_box.insert(&card_arc.widget, -1);
            self.artist_cards_ref.borrow_mut().push(card_arc);
        }

        // Update count label
        self.update_count_label();

        // Hide search empty state when showing artists
        self.search_empty_state.hide();
    }

    /// Updates the count label with the current artist count.
    fn update_count_label(&self) {
        let artist_count = self.artists.len();
        let song_count: i64 = self.artists.iter().map(|a| a.album_count).sum();

        let label_text = if song_count > 0 {
            if song_count == 1 {
                format!("{artist_count} Artist (1 Album)")
            } else {
                format!("{artist_count} Artists ({song_count} Albums)")
            }
        } else if artist_count == 1 {
            "1 Artist".to_string()
        } else {
            format!("{artist_count} Artists")
        };

        self.count_label.set_text(&label_text);
    }

    /// Updates the full unfiltered artists list.
    ///
    /// This should be called when library data changes.
    ///
    /// # Arguments
    ///
    /// * `all_artists` - Complete list of all artists from database
    pub fn update_all_artists(&mut self, all_artists: Vec<Artist>) {
        self.all_artists = all_artists;

        // If there's no active search filter, show all artists
        let library_state = self.app_state.as_ref().map(|s| s.get_library_state());
        if library_state
            .as_ref()
            .and_then(|s| s.search_filter.as_ref())
            .is_none_or(String::is_empty)
        {
            self.set_artists(self.all_artists.clone());
        }
    }

    /// Gets the cover size for artist cards based on current configuration.
    ///
    /// # Returns
    ///
    /// The cover size in pixels.
    fn get_cover_size(&self) -> i32 {
        self.app_state
            .as_ref()
            .map_or(if self.config.compact { 120 } else { 180 }, |app_state| {
                app_state.zoom_manager.get_grid_cover_dimensions().0
            })
    }

    /// Creates a single artist card for the grid.
    ///
    /// # Arguments
    ///
    /// * `artist` - The artist to create a card for
    /// * `cover_size` - The size of cover art in pixels
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `ArtistCard` instance or a `UiError` if
    /// card creation fails.
    ///
    /// # Errors
    ///
    /// Returns a `UiError` if the cover size cannot be converted to a valid
    /// unsigned integer or if the artist card cannot be built.
    fn create_artist_card(&self, artist: &Artist, cover_size: i32) -> Result<ArtistCard, UiError> {
        let artist_clone = artist.clone();
        let app_state = self.app_state.clone();
        let artist_id = artist.id;

        let is_selected = app_state
            .as_ref()
            .is_some_and(|state| state.is_artist_selected(artist_id));

        let any_selected = app_state
            .as_ref()
            .is_some_and(|state| state.has_selected_artists());

        let cover_size_u32 = safe_i32_to_u32(cover_size, 180, "cover_size");
        let app_state_for_click = app_state.clone();
        let app_state_for_selection = app_state;
        let artist_cards_clone = Rc::clone(&self.artist_cards_ref);
        let is_syncing_for_toggle = Rc::clone(&self.is_syncing_selection);
        let artist_card = ArtistCard::builder()
            .artist(artist.clone())
            .cover_size(cover_size_u32)
            .selected(is_selected)
            .on_card_clicked(move || {
                if let Some(state) = &app_state_for_click {
                    state.update_navigation(ArtistDetail(artist_clone.clone()));
                }
            })
            .on_selection_toggled(move |selected| {
                if is_syncing_for_toggle.get() {
                    return;
                }

                if let Some(state) = &app_state_for_selection {
                    if selected {
                        state.select_artist(artist_id);
                    } else {
                        state.deselect_artist(artist_id);
                    }
                    let has_selection = state.has_selected_artists();
                    for card in artist_cards_clone.borrow().iter() {
                        card.selection_checkbox.set_visible(has_selection);
                        card.selection_checkbox.set_can_target(has_selection);
                    }
                }
            })
            .build()?;

        if any_selected {
            artist_card.selection_checkbox.set_visible(true);
            artist_card.selection_checkbox.set_can_target(true);
        }

        Ok(artist_card)
    }

    /// Updates the display configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - New display configuration
    pub fn update_config(&mut self, config: ArtistGridViewConfig) {
        self.config = config;

        // Rebuild all artist items with new configuration
        self.set_artists(self.all_artists.clone());
    }

    /// Filters artists based on a search query.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    pub fn filter_artists(&mut self, query: &str) {
        let all_artists = self.all_artists.clone();

        // Don't show search empty state if library is empty (main empty state should be shown)
        if all_artists.is_empty() {
            self.search_empty_state.hide();
            return;
        }

        // Call filter_items to update the grid and get result status
        let has_results = self.filter_items(query, &all_artists, |artist, q| {
            artist.name.to_lowercase().contains(q)
        });

        // Update search empty state visibility
        if has_results {
            self.search_empty_state.hide();
        } else {
            self.search_empty_state.update_search_query(query);
            self.search_empty_state.show();
        }
    }

    /// Clears the view by hiding all items.
    ///
    /// This is used when switching tabs with an active search to prevent
    /// the unfiltered view from appearing during the transition.
    pub fn clear_view(&self) {
        Filterable::<Artist>::clear_view(self);
    }
}

impl Filterable<Artist> for ArtistGridView {
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

    /// Sets the visibility of artist cards based on filtered IDs.
    ///
    /// # Arguments
    ///
    /// * `visible_ids` - Set of artist IDs that should be visible
    fn set_visibility(&self, visible_ids: &HashSet<i64>) {
        let _freeze_guard = self.flow_box.freeze_notify();

        let cards = self.artist_cards_ref.borrow();
        for card in cards.iter() {
            let card_visible = visible_ids.contains(&card.artist_id);
            card.widget.set_visible(card_visible);
        }
    }
}

impl ArtistGridView {
    /// Sorts artists by the specified criteria.
    ///
    /// # Arguments
    ///
    /// * `sort_by` - Sorting criteria
    pub fn sort_artists(&mut self, sort_by: ArtistSortCriteria) {
        self.current_sort = sort_by;

        // Apply sort to current artists and refresh display
        self.apply_sort();

        // Re-display sorted artists
        self.set_artists(self.artists.clone());
    }

    /// Applies the current sort criteria to the artists vector.
    fn apply_sort(&mut self) {
        match self.current_sort {
            ArtistSortCriteria::Name => {
                self.artists.sort_by(|a, b| a.name.cmp(&b.name));
            }
            ArtistSortCriteria::AlbumCount => {
                // TODO: Implement album count sorting - requires querying database or having album
                // counts in state
                self.artists.sort_by(|a, b| a.name.cmp(&b.name));
            }
        }
    }

    /// Stops the zoom subscription and cleans up resources.
    pub fn cleanup(&mut self) {
        if let Some(handle) = self.zoom_subscription_handle.take() {
            handle.abort();
        }
        if let Some(handle) = self.selection_subscription_handle.take() {
            handle.abort();
        }
    }
}

impl Drop for ArtistGridView {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Artist card component with cover art and metadata.
///
/// The `ArtistCard` component displays artists with cover art, names, and
/// album counts, matching the album card styling.
#[derive(Clone)]
pub struct ArtistCard {
    /// The underlying `FlowBoxChild` widget.
    pub widget: Widget,
    /// The main artist tile container.
    pub artist_tile: Box,
    /// The cover art component.
    pub cover_art: CoverArt,
    /// Artist name label.
    pub name_label: Label,
    /// Album count label.
    pub album_count_label: Label,
    /// Artist ID for tracking during filtering.
    pub artist_id: i64,
    /// Selection checkbox button.
    pub selection_checkbox: CheckButton,
    /// Flag to prevent callback during programmatic updates.
    updating_checkbox: Cell<bool>,
}

impl ArtistCard {
    /// Creates a new `ArtistCard` component.
    ///
    /// # Arguments
    ///
    /// * `artist` - The artist to display
    /// * `cover_size` - The size of the cover art in pixels
    /// * `on_card_clicked` - Optional callback for card clicks
    /// * `on_selection_toggled` - Optional callback for selection toggled
    /// * `selected` - Whether the card is initially selected
    ///
    /// # Returns
    ///
    /// A new `ArtistCard` instance.
    ///
    /// # Panics
    ///
    /// Panics if `cover_size` or the calculated `max_width_chars` values
    /// cannot be converted to i32. This indicates a programming error as
    /// the calculation logic should always produce valid values.
    #[must_use]
    pub fn new(
        artist: &Artist,
        cover_size: u32,
        on_card_clicked: Option<Rc<dyn Fn()>>,
        on_selection_toggled: Option<&Rc<dyn Fn(bool)>>,
        selected: bool,
    ) -> Self {
        let clamped_cover_size = cover_size.min(MAX_COVER_SIZE);
        let cover_size_i32 = safe_u32_to_i32(
            clamped_cover_size,
            MAX_COVER_SIZE,
            MAX_COVER_SIZE_I32,
            "cover_size",
        );
        let (cover_width, cover_height) = (cover_size_i32, cover_size_i32);

        let cover_art = CoverArt::builder()
            .icon_name("avatar-default-symbolic")
            .show_dr_badge(false)
            .dimensions(cover_width, cover_height)
            .build();

        let (selection_checkbox, updating_flag) =
            Self::create_selection_checkbox(selected, on_selection_toggled);

        if let Some(cover_overlay) = cover_art.widget.downcast_ref::<Overlay>() {
            cover_overlay.add_overlay(&selection_checkbox);
        }

        let (name_label, album_count_label) =
            Self::create_artist_labels(artist, clamped_cover_size);

        let artist_tile =
            Self::create_artist_tile(&cover_art, &name_label, &album_count_label, artist);

        let child = FlowBoxChild::new();
        child.set_child(Some(&artist_tile));
        child.set_focusable(true);

        Self::setup_motion_controller(&child, &selection_checkbox);
        Self::setup_click_controller(&child, on_card_clicked);

        Self {
            widget: child.upcast_ref::<Widget>().clone(),
            artist_tile,
            cover_art,
            name_label,
            album_count_label,
            artist_id: artist.id,
            selection_checkbox,
            updating_checkbox: updating_flag,
        }
    }

    /// Creates and configures the selection checkbox.
    ///
    /// # Arguments
    ///
    /// * `selected` - Whether the checkbox should be initially selected
    /// * `on_selection_toggled` - Optional callback for selection changes
    ///
    /// # Returns
    ///
    /// Tuple of (checkbox, `updating_flag`).
    fn create_selection_checkbox(
        selected: bool,
        on_selection_toggled: Option<&Rc<dyn Fn(bool)>>,
    ) -> (CheckButton, Cell<bool>) {
        let selection_checkbox = CheckButton::builder()
            .tooltip_text("Select for batch operations")
            .halign(Start)
            .valign(Start)
            .visible(false)
            .build();
        selection_checkbox.set_can_target(false);
        selection_checkbox.set_active(selected);

        let updating_flag = Cell::new(false);
        if let Some(callback) = on_selection_toggled {
            let callback_clone = Rc::clone(callback);
            let flag_clone = updating_flag.clone();
            selection_checkbox.connect_toggled(move |checkbox| {
                if !flag_clone.get() {
                    callback_clone(checkbox.is_active());
                }
            });
        }

        (selection_checkbox, updating_flag)
    }

    /// Creates the name and album count labels.
    ///
    /// # Arguments
    ///
    /// * `artist` - Artist data for label content
    /// * `clamped_cover_size` - Cover size for label width calculation
    ///
    /// # Returns
    ///
    /// Tuple of (`name_label`, `album_count_label`).
    fn create_artist_labels(artist: &Artist, clamped_cover_size: u32) -> (Label, Label) {
        let name_max_width = ((clamped_cover_size - 16) / 10).max(8);
        let name_max_width_i32 = safe_u32_to_i32(name_max_width, 408, 408, "name_max_width");
        let name_label = Label::builder()
            .label(&artist.name)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(End)
            .lines(2)
            .max_width_chars(name_max_width_i32)
            .tooltip_text(&artist.name)
            .css_classes(["album-title-label"])
            .build();

        let album_count_text = if artist.album_count == 1 {
            "1 Album".to_string()
        } else {
            format!("{} Albums", artist.album_count)
        };
        let album_count_max_width = ((clamped_cover_size - 16) / 10).max(8);
        let album_count_max_width_i32 =
            safe_u32_to_i32(album_count_max_width, 408, 408, "album_count_max_width");
        let album_count_label = Label::builder()
            .label(&album_count_text)
            .halign(Start)
            .xalign(0.0)
            .ellipsize(End)
            .lines(1)
            .max_width_chars(album_count_max_width_i32)
            .tooltip_text(&album_count_text)
            .css_classes(["album-artist-label"])
            .build();

        (name_label, album_count_label)
    }

    /// Creates the artist tile box with cover, name, and album count.
    ///
    /// # Arguments
    ///
    /// * `cover_art` - Cover art widget
    /// * `name_label` - Artist name label
    /// * `album_count_label` - Album count label
    /// * `artist` - Artist data
    ///
    /// # Returns
    ///
    /// Configured artist tile Box widget.
    fn create_artist_tile(
        cover_art: &CoverArt,
        name_label: &Label,
        album_count_label: &Label,
        artist: &Artist,
    ) -> Box {
        let artist_tile = Box::builder()
            .orientation(Vertical)
            .halign(Start)
            .valign(Start)
            .hexpand(false)
            .vexpand(false)
            .spacing(6)
            .css_classes(["album-tile"])
            .build();

        artist_tile.append(&cover_art.widget);
        artist_tile.append(name_label.upcast_ref::<Widget>());
        artist_tile.append(album_count_label.upcast_ref::<Widget>());

        artist_tile.set_accessible_role(Group);
        artist_tile.set_tooltip_text(Some(&artist.name));

        artist_tile
    }

    /// Sets up motion controller for hover effects.
    ///
    /// # Arguments
    ///
    /// * `child` - `FlowBoxChild` to add controller to
    /// * `selection_checkbox` - Checkbox to show/hide on hover
    fn setup_motion_controller(child: &FlowBoxChild, selection_checkbox: &CheckButton) {
        let motion_controller = EventControllerMotion::new();
        let checkbox_clone = selection_checkbox.clone();
        motion_controller.connect_enter(move |_controller, _x, _y| {
            checkbox_clone.set_can_target(true);
            checkbox_clone.set_visible(true);
        });

        let checkbox_clone2 = selection_checkbox.clone();
        motion_controller.connect_leave(move |_controller| {
            if !checkbox_clone2.is_active() {
                checkbox_clone2.set_can_target(false);
                checkbox_clone2.set_visible(false);
            }
        });

        child.add_controller(motion_controller);
    }

    /// Sets up click controller for double-click and activation.
    ///
    /// # Arguments
    ///
    /// * `child` - `FlowBoxChild` to add controller to
    /// * `on_card_clicked` - Optional callback for card activation
    fn setup_click_controller(child: &FlowBoxChild, on_card_clicked: Option<Rc<dyn Fn()>>) {
        let click_controller = GestureClick::new();

        if let Some(callback) = on_card_clicked {
            let callback_for_click = Rc::clone(&callback);
            let callback_for_activate = Rc::clone(&callback);

            click_controller.connect_released(move |_gesture, n_press, _x, _y| {
                if n_press == 2 {
                    callback_for_click();
                }
            });

            child.connect_activate(move |_| {
                callback_for_activate();
            });
        }

        if let Some(child_widget) = child.child()
            && let Some(artist_tile) = child_widget.downcast_ref::<Box>()
        {
            artist_tile.add_controller(click_controller);
        }
    }

    /// Creates a builder for configuring artist cards.
    #[must_use]
    pub fn builder() -> ArtistCardBuilder {
        ArtistCardBuilder::default()
    }

    /// Sets the checkbox state without triggering the selection callback.
    ///
    /// # Arguments
    ///
    /// * `selected` - Whether the checkbox should be selected
    pub fn set_selection_state(&self, selected: bool) {
        self.updating_checkbox.set(true);
        self.selection_checkbox.set_active(selected);
        self.updating_checkbox.set(false);
    }

    /// Updates the cover size for this artist card.
    ///
    /// # Arguments
    ///
    /// * `cover_size` - New cover size in pixels
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or a `UiError` if the size is invalid.
    ///
    /// # Errors
    ///
    /// Returns a `UiError::BuilderError` if the cover size or calculated
    /// `max_width_chars` cannot be converted to i32.
    pub fn update_cover_size(&self, cover_size: u32) -> Result<(), UiError> {
        let clamped_cover_size = cover_size.min(MAX_COVER_SIZE);
        let cover_size_i32 = i32::try_from(clamped_cover_size)
            .map_err(|_err| BuilderError(format!("Invalid cover size: {cover_size}")))?;

        self.cover_art
            .update_dimensions(cover_size_i32, cover_size_i32);

        let max_width = ((clamped_cover_size - 16) / 10).max(8);
        let max_width_i32 = i32::try_from(max_width)
            .map_err(|_err| BuilderError(format!("Invalid max_width_chars: {max_width}")))?;
        self.name_label.set_max_width_chars(max_width_i32);
        self.album_count_label.set_max_width_chars(max_width_i32);
        Ok(())
    }
}

/// Builder pattern for configuring `ArtistCard` components.
#[derive(Default)]
pub struct ArtistCardBuilder {
    /// The artist data to display on the card.
    artist: Option<Artist>,
    /// Optional cover size override in pixels.
    cover_size: Option<u32>,
    /// Optional callback invoked when the card is clicked.
    on_card_clicked: Option<Rc<dyn Fn()>>,
    /// Callback invoked when the selection checkbox is toggled.
    on_selection_toggled: Option<Rc<dyn Fn(bool)>>,
    /// Whether the card is initially selected.
    selected: bool,
}

impl ArtistCardBuilder {
    /// Sets the artist data for the card.
    ///
    /// # Arguments
    ///
    /// * `artist` - The artist to display
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn artist(mut self, artist: Artist) -> Self {
        self.artist = Some(artist);
        self
    }

    /// Sets the cover size for the artist card.
    ///
    /// # Arguments
    ///
    /// * `cover_size` - The size of the cover art in pixels
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn cover_size(mut self, cover_size: u32) -> Self {
        self.cover_size = Some(cover_size);
        self
    }

    /// Sets the callback for when the card is clicked.
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

    /// Sets the callback for when the selection checkbox is toggled.
    ///
    /// # Arguments
    ///
    /// * `callback` - Function to call with the new selection state (true = selected)
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn on_selection_toggled<F>(mut self, callback: F) -> Self
    where
        F: Fn(bool) + 'static,
    {
        self.on_selection_toggled = Some(Rc::new(callback));
        self
    }

    /// Sets whether the card is initially selected.
    ///
    /// # Arguments
    ///
    /// * `selected` - Whether the card is selected
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Builds the `ArtistCard` component.
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `ArtistCard` instance or a `UiError` if
    /// required fields are missing.
    ///
    /// # Errors
    ///
    /// Returns a `UiError::BuilderError` if the artist field has not been set.
    pub fn build(self) -> Result<ArtistCard, UiError> {
        let artist = self
            .artist
            .ok_or_else(|| BuilderError("Artist must be set".to_string()))?;
        let cover_size = self.cover_size.unwrap_or(180);
        Ok(ArtistCard::new(
            &artist,
            cover_size,
            self.on_card_clicked,
            self.on_selection_toggled.as_ref(),
            self.selected,
        ))
    }
}

/// Sorting criteria for artists.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtistSortCriteria {
    /// Sort by artist name
    Name,
    /// Sort by album count (requires additional data)
    AlbumCount,
}

impl Default for ArtistGridView {
    fn default() -> Self {
        Self::new(None, Vec::new(), false)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering::SeqCst},
    };

    use anyhow::{Result, bail};

    use crate::{
        error::domain::UiError::BuilderError,
        library::models::Artist,
        ui::views::artist_grid::{ArtistCard, ArtistGridView},
    };

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_creation() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            album_count: 5,
            created_at: None,
            updated_at: None,
        };

        let card = ArtistCard::new(&artist, 180, None, None, false);

        assert_eq!(card.name_label.label(), "Test Artist");
        assert_eq!(card.album_count_label.label(), "5 Albums");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_builder() -> Result<()> {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            album_count: 3,
            created_at: None,
            updated_at: None,
        };
        let clicked = AtomicBool::new(false);
        let clicked = Arc::new(clicked);

        let card = ArtistCard::builder()
            .artist(artist)
            .cover_size(200)
            .on_card_clicked(move || {
                clicked.store(true, SeqCst);
            })
            .build();

        let card = card?;

        if card.name_label.label() != "Test Artist" {
            bail!("Name label should be 'Test Artist'");
        }
        if card.album_count_label.label() != "3 Albums" {
            bail!("Album count label should be '3 Albums'");
        }
        Ok(())
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_builder_missing_artist() {
        let result = ArtistCard::builder().cover_size(200).build();
        assert!(matches!(result, Err(BuilderError(_))));
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_default_cover_size() -> Result<()> {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            album_count: 0,
            ..Artist::default()
        };

        let card = ArtistCard::builder().artist(artist).build()?;

        if card.name_label.label() != "Test Artist" {
            bail!("Name label should be 'Test Artist'");
        }
        if card.album_count_label.label() != "0 Albums" {
            bail!("Album count label should be '0 Albums'");
        }
        Ok(())
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_update_cover_size() -> Result<()> {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            album_count: 1,
            ..Artist::default()
        };

        let card = ArtistCard::new(&artist, 180, None, None, false);

        card.update_cover_size(250)?;

        if card.name_label.label() != "Test Artist" {
            bail!("Name label should be 'Test Artist'");
        }
        if card.album_count_label.label() != "1 Album" {
            bail!("Album count label should be '1 Album'");
        }
        Ok(())
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_small_cover_size() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            album_count: 2,
            ..Artist::default()
        };

        let card = ArtistCard::new(&artist, 120, None, None, false);

        assert_eq!(card.name_label.label(), "Test Artist");
        assert_eq!(card.album_count_label.label(), "2 Albums");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_large_cover_size() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            album_count: 10,
            ..Artist::default()
        };

        let card = ArtistCard::new(&artist, 400, None, None, false);

        assert_eq!(card.name_label.label(), "Test Artist");
        assert_eq!(card.album_count_label.label(), "10 Albums");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_card_long_name() {
        let artist = Artist {
            id: 1,
            name: "A Very Long Artist Name That Should Be Elided".to_string(),
            album_count: 1,
            ..Artist::default()
        };

        let card = ArtistCard::new(&artist, 180, None, None, false);

        assert_eq!(
            card.name_label.label(),
            "A Very Long Artist Name That Should Be Elided"
        );
        assert_eq!(card.album_count_label.label(), "1 Album");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_grid_view_builder() {
        let artists = vec![
            Artist {
                id: 1,
                name: "Test Artist 1".to_string(),
                album_count: 3,
                created_at: None,
                updated_at: None,
            },
            Artist {
                id: 2,
                name: "Test Artist 2".to_string(),
                album_count: 1,
                created_at: None,
                updated_at: None,
            },
        ];

        let grid_view = ArtistGridView::builder()
            .artists(artists)
            .compact(false)
            .build();

        assert_eq!(grid_view.artists.len(), 2);
        assert!(!grid_view.config.compact);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_artist_grid_view_default() {
        let grid_view = ArtistGridView::default();
        assert_eq!(grid_view.artists.len(), 0);
        assert!(!grid_view.config.compact);
    }

    #[test]
    fn test_artist_sort_criteria() {
        // This test doesn't require GTK, so no skip needed
        let mut artists = [
            Artist {
                id: 1,
                name: "B Artist".to_string(),
                album_count: 0,
                ..Artist::default()
            },
            Artist {
                id: 2,
                name: "A Artist".to_string(),
                album_count: 0,
                ..Artist::default()
            },
        ];

        // Test name sorting
        artists.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(artists[0].name, "A Artist");
    }
}
