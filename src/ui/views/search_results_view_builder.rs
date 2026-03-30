//! Builder pattern for `SearchResultsView`.

use std::sync::Arc;

use crate::{
    audio::{engine::AudioEngine, queue_manager::QueueManager},
    library::database::LibraryDatabase,
    state::app_state::AppState,
    ui::views::search_results_view::SearchResultsView,
};

/// Builder pattern for configuring `SearchResultsView` components.
#[derive(Default)]
pub struct SearchResultsViewBuilder {
    /// Optional application state reference.
    app_state: Option<Arc<AppState>>,
    /// Optional library database reference.
    library_db: Option<Arc<LibraryDatabase>>,
    /// Optional audio engine reference.
    audio_engine: Option<Arc<AudioEngine>>,
    /// Optional queue manager reference.
    queue_manager: Option<Arc<QueueManager>>,
}

impl SearchResultsViewBuilder {
    /// Sets the application state for the view.
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

    /// Sets the library database for the view.
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

    /// Sets the audio engine for the view.
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

    /// Sets the queue manager for the view.
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

    /// Builds the `SearchResultsView` component.
    ///
    /// # Returns
    ///
    /// A new `SearchResultsView` instance.
    #[must_use]
    pub fn build(self) -> SearchResultsView {
        SearchResultsView::new(
            self.app_state,
            self.library_db,
            self.audio_engine,
            self.queue_manager,
        )
    }
}
