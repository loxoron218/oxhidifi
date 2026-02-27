//! Column view builder for configuring `ColumnListView` components.

use std::sync::Arc;

use crate::{
    audio::{engine::AudioEngine, queue_manager::QueueManager},
    library::database::LibraryDatabase,
    state::app_state::AppState,
    ui::views::{column_view::ColumnListView, column_view_types::ColumnListViewType},
};

/// Builder pattern for configuring `ColumnListView` components.
pub struct ColumnListViewBuilder {
    /// Optional application state reference for reactive updates.
    app_state: Option<Arc<AppState>>,
    /// Optional library database reference for fetching tracks.
    library_db: Option<Arc<LibraryDatabase>>,
    /// Optional audio engine reference for playback.
    audio_engine: Option<Arc<AudioEngine>>,
    /// Optional queue manager reference for queue operations.
    queue_manager: Option<Arc<QueueManager>>,
    /// Type of items to display (albums or artists).
    view_type: ColumnListViewType,
    /// Whether to show DR badges.
    show_dr_badges: bool,
    /// Whether to use compact layout.
    compact: bool,
}

impl Default for ColumnListViewBuilder {
    fn default() -> Self {
        Self {
            app_state: None,
            library_db: None,
            audio_engine: None,
            queue_manager: None,
            view_type: ColumnListViewType::default(),
            show_dr_badges: true,
            compact: false,
        }
    }
}

impl ColumnListViewBuilder {
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

    /// Sets the library database for fetching tracks.
    ///
    /// # Arguments
    ///
    /// * `library_db` - Library database reference
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn library_db(mut self, library_db: Arc<LibraryDatabase>) -> Self {
        self.library_db = Some(library_db);
        self
    }

    /// Sets the audio engine for playback.
    ///
    /// # Arguments
    ///
    /// * `audio_engine` - Audio engine reference
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn audio_engine(mut self, audio_engine: Arc<AudioEngine>) -> Self {
        self.audio_engine = Some(audio_engine);
        self
    }

    /// Sets the queue manager reference for queue operations.
    ///
    /// # Arguments
    ///
    /// * `queue_manager` - Queue manager reference
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn queue_manager(mut self, queue_manager: Arc<QueueManager>) -> Self {
        self.queue_manager = Some(queue_manager);
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
    pub fn view_type(mut self, view_type: ColumnListViewType) -> Self {
        self.view_type = view_type;
        self
    }

    /// Configures whether to show DR badges.
    ///
    /// # Arguments
    ///
    /// * `show_dr_badges` - Whether to show DR badges
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn show_dr_badges(mut self, show_dr_badges: bool) -> Self {
        self.show_dr_badges = show_dr_badges;
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

    /// Builds the `ColumnListView` component.
    ///
    /// # Returns
    ///
    /// A new `ColumnListView` instance.
    #[must_use]
    pub fn build(self) -> ColumnListView {
        ColumnListView::new(
            self.app_state.as_ref(),
            self.library_db.as_ref(),
            self.audio_engine.as_ref(),
            self.queue_manager.as_ref(),
            &self.view_type,
            self.show_dr_badges,
            self.compact,
        )
    }
}
