//! Album/artist detail pages with comprehensive metadata and track listings.

use std::{
    fmt::{Debug, Formatter, Result as StdResult},
    sync::Arc,
};

use libadwaita::{
    ToastOverlay,
    gtk::{AccessibleRole::Article, Align::Fill, Box, Orientation::Vertical, Widget},
    prelude::{AccessibleExt, BoxExt, Cast, ListModelExt, WidgetExt},
};

use crate::{
    audio::{
        engine::{AudioEngine, PlaybackState, TrackInfo},
        queue_manager::QueueManager,
    },
    library::{
        database::LibraryDatabase,
        models::{Album, Artist},
    },
    state::AppState,
    ui::views::{
        album_detail_renderer::AlbumDetailRenderer,
        artist_detail_renderer::ArtistDetailRenderer,
        detail_playback::PlaybackHandler,
        detail_types::{DetailType, DetailViewBuilder, DetailViewConfig},
    },
};

/// Comprehensive detail view for albums or artists.
pub struct DetailView {
    /// The underlying GTK widget (main container).
    pub widget: Widget,
    /// Main container box.
    pub main_container: Box,
    /// Toast overlay for displaying feedback messages.
    pub toast_overlay: ToastOverlay,
    /// Current application state reference.
    pub app_state: Option<Arc<AppState>>,
    /// Library database reference for fetching tracks.
    pub library_db: Option<Arc<LibraryDatabase>>,
    /// Audio engine reference for playback.
    pub audio_engine: Option<Arc<AudioEngine>>,
    /// Queue manager reference for queue operations.
    pub queue_manager: Option<Arc<QueueManager>>,
    /// Current detail type being displayed.
    pub detail_type: Option<DetailType>,
    /// Configuration flags.
    pub config: DetailViewConfig,
}

impl Debug for DetailView {
    fn fmt(&self, f: &mut Formatter<'_>) -> StdResult {
        f.debug_struct("DetailView")
            .field("widget", &self.widget)
            .field("main_container", &self.main_container)
            .field("toast_overlay", &self.toast_overlay)
            .field(
                "app_state",
                &self.app_state.as_ref().map(|_| "Arc<AppState>"),
            )
            .field(
                "library_db",
                &self.library_db.as_ref().map(|_| "Arc<LibraryDatabase>"),
            )
            .field(
                "audio_engine",
                &self.audio_engine.as_ref().map(|_| "Arc<AudioEngine>"),
            )
            .field(
                "queue_manager",
                &self.queue_manager.as_ref().map(|_| "Arc<QueueManager>"),
            )
            .field("detail_type", &self.detail_type)
            .field("config", &self.config)
            .finish()
    }
}

impl DetailView {
    /// Creates a new `DetailView` component.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Optional application state reference for reactive updates
    /// * `library_db` - Optional library database reference for fetching tracks
    /// * `audio_engine` - Optional audio engine reference for playback
    /// * `queue_manager` - Optional queue manager reference for queue operations
    /// * `detail_type` - Initial detail type to display
    /// * `compact` - Whether to use compact layout
    ///
    /// # Returns
    ///
    /// A new `DetailView` instance.
    #[must_use]
    pub fn new(
        app_state: Option<Arc<AppState>>,
        library_db: Option<Arc<LibraryDatabase>>,
        audio_engine: Option<Arc<AudioEngine>>,
        queue_manager: Option<Arc<QueueManager>>,
        detail_type: DetailType,
        compact: bool,
    ) -> Self {
        let config = DetailViewConfig { compact };

        let main_container = Box::builder()
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

        let toast_overlay = ToastOverlay::new();
        toast_overlay.set_child(Some(&main_container));

        let mut view = Self {
            widget: toast_overlay.clone().upcast_ref::<Widget>().clone(),
            main_container,
            toast_overlay,
            app_state,
            library_db,
            audio_engine,
            queue_manager,
            detail_type: None,
            config,
        };

        // Set initial detail
        view.set_detail(detail_type);

        view
    }

    /// Creates a `DetailView` builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `DetailViewBuilder` instance.
    #[must_use]
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
            DetailType::Album(album) => self.display_album_detail(&album),
            DetailType::Artist(artist) => self.display_artist_detail(&artist),
        }
    }

    /// Displays detailed album information.
    ///
    /// # Arguments
    ///
    /// * `album` - Reference to the album to display
    fn display_album_detail(&self, album: &Album) {
        let playback_handler = Some(PlaybackHandler::new(
            self.audio_engine.clone(),
            self.queue_manager.clone(),
        ));

        let renderer = AlbumDetailRenderer::new(
            self.app_state.clone(),
            self.library_db.clone(),
            playback_handler,
        );

        renderer.render(&self.main_container, album, &self.toast_overlay);
    }

    /// Displays detailed artist information.
    ///
    /// # Arguments
    ///
    /// * `artist` - Reference to the artist to display
    fn display_artist_detail(&self, artist: &Artist) {
        ArtistDetailRenderer::render(&self.main_container, artist);
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

    /// Gets the current track info from audio engine for state updates.
    ///
    /// # Returns
    ///
    /// The current `TrackInfo` if available, otherwise `None`.
    #[must_use]
    pub fn get_current_track_info(&self) -> Option<TrackInfo> {
        self.audio_engine.as_ref()?.current_track_info()
    }

    /// Updates playback state and current track in application state.
    ///
    /// # Arguments
    ///
    /// * `state` - New playback state
    /// * `track_info` - Track info to update
    pub fn update_playback_state(&self, state: PlaybackState, track_info: TrackInfo) {
        if let Some(app_state) = &self.app_state {
            app_state.update_playback_state(state);
            app_state.update_current_track(Some(track_info));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        library::models::{Album, Artist},
        ui::views::{
            detail_types::{DetailType, DetailViewBuildError::MissingDetailType},
            detail_view::DetailView,
        },
    };

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_detail_view_builder() {
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            ..Artist::default()
        };

        let detail_view = DetailView::builder()
            .detail_type(Some(DetailType::Artist(artist)))
            .compact(true)
            .build()
            .unwrap();

        match &detail_view.detail_type {
            Some(DetailType::Artist(_)) => {}
            _ => unreachable!(),
        }
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_detail_view_builder_missing_detail_type() {
        let error = DetailView::builder()
            .compact(true)
            .build()
            .expect_err("Should fail without detail type");

        assert!(matches!(error, MissingDetailType));
    }

    #[test]
    fn test_detail_types() {
        let album = Album::default();
        let artist = Artist::default();

        assert_eq!(
            format!("{:?}", DetailType::Album(album)),
            "Album(Album { id: 0, artist_id: 0, title: \"\", year: None, genre: None, format: None, bits_per_sample: None, sample_rate: None, compilation: false, path: \"\", dr_value: None, artwork_path: None, created_at: None, updated_at: None })"
        );
        assert_eq!(
            format!("{:?}", DetailType::Artist(artist)),
            "Artist(Artist { id: 0, name: \"\", album_count: 0, created_at: None, updated_at: None })"
        );
    }
}
