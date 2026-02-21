//! Album detail rendering logic.

use std::sync::Arc;

use {
    libadwaita::{
        ToastOverlay,
        glib::MainContext,
        gtk::{
            Align::Start,
            Box, Label, ListBox, ListBoxRow,
            Orientation::{Horizontal, Vertical},
            ScrolledWindow,
            SelectionMode::None as SelectionNone,
            Widget,
            pango::EllipsizeMode::End,
        },
        prelude::{BoxExt, Cast, ListBoxRowExt},
    },
    tracing::{error, warn},
};

use crate::{
    error::domain::UiError::ArtistNotFound,
    library::{
        database::LibraryDatabase,
        models::{Album, Track},
    },
    state::AppState,
    ui::{
        components::{
            cover_art::CoverArt,
            hifi_metadata::{
                BitDepthDisplay::Show as ShowBitDepth, ChannelsDisplay::Hide as HideChannels,
                FormatDisplay::Show as ShowFormat, HiFiMetadata, LayoutMode::Compact,
                SampleRateDisplay::Show as ShowSampleRate,
            },
            play_overlay::PlayOverlay,
        },
        views::detail_playback::PlaybackHandler,
    },
};

/// Renderer for album detail views.
pub struct AlbumDetailRenderer {
    /// Application state reference for reactive updates.
    app_state: Option<Arc<AppState>>,
    /// Library database reference for fetching tracks.
    library_db: Option<Arc<LibraryDatabase>>,
    /// Playback handler for track playback operations.
    playback_handler: Option<PlaybackHandler>,
}

impl AlbumDetailRenderer {
    /// Creates a new album detail renderer.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference
    /// * `library_db` - Library database reference
    /// * `playback_handler` - Playback handler reference
    ///
    /// # Returns
    ///
    /// A new `AlbumDetailRenderer` instance.
    #[must_use]
    pub fn new(
        app_state: Option<Arc<AppState>>,
        library_db: Option<Arc<LibraryDatabase>>,
        playback_handler: Option<PlaybackHandler>,
    ) -> Self {
        Self {
            app_state,
            library_db,
            playback_handler,
        }
    }

    /// Renders album detail to a container.
    ///
    /// # Arguments
    ///
    /// * `container` - Container widget to append content to
    /// * `album` - Album to display
    /// * `toast_overlay` - Toast overlay for displaying feedback messages
    pub fn render(&self, container: &Box, album: &Album, toast_overlay: &ToastOverlay) {
        let artist_name = self.fetch_artist_name(album);
        let header = Self::create_album_header(album, &artist_name);
        container.append(&header);

        self.load_tracks_async(album.id, container, toast_overlay);
    }

    /// Fetches artist name from library state.
    ///
    /// # Arguments
    ///
    /// * `album` - Album to fetch artist name for
    ///
    /// # Returns
    ///
    /// Artist name as a string, or "Unknown Artist" if not found.
    fn fetch_artist_name(&self, album: &Album) -> String {
        self.app_state.as_ref().map_or_else(
            || "Unknown Artist".to_string(),
            |state| {
                let library_state = state.get_library_state();
                library_state
                    .artists
                    .iter()
                    .find(|artist| artist.id == album.artist_id)
                    .map_or_else(
                        || {
                            warn!(error = ?ArtistNotFound {
                                artist_id: album.artist_id,
                                album_id: album.id,
                                album_title: album.title.clone(),
                            });
                            "Unknown Artist".to_string()
                        },
                        |artist| artist.name.clone(),
                    )
            },
        )
    }

    /// Loads tracks asynchronously and renders track list.
    ///
    /// # Arguments
    ///
    /// * `album_id` - Album ID to fetch tracks for
    /// * `container` - Container widget to append track list to
    /// * `toast_overlay` - Toast overlay for displaying feedback messages
    fn load_tracks_async(&self, album_id: i64, container: &Box, toast_overlay: &ToastOverlay) {
        if let Some(library_db) = &self.library_db {
            let library_db = Arc::clone(library_db);
            let playback_handler = self.playback_handler.clone();
            let container = container.clone();
            let toast_overlay = toast_overlay.clone();

            MainContext::default().spawn_local(async move {
                match library_db.get_tracks_by_album(album_id).await {
                    Ok(tracks) if !tracks.is_empty() => {
                        if let Some(handler) = &playback_handler {
                            let on_track_clicked =
                                handler.create_track_click_handler(tracks.clone(), toast_overlay);
                            let track_list = Self::create_track_list(&tracks, on_track_clicked);
                            container.append(&track_list);
                        }
                    }
                    Ok(_) => {
                        warn!("No tracks found for album {}", album_id);
                    }
                    Err(e) => {
                        error!("Failed to load tracks for album {}: {}", album_id, e);
                    }
                }
            });
        }
    }

    /// Creates the album header section with cover art and metadata.
    ///
    /// # Arguments
    ///
    /// * `album` - Album to create header for
    /// * `artist_name` - The artist name to display
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the album header.
    fn create_album_header(album: &Album, artist_name: &str) -> Widget {
        let header_container = Box::builder().orientation(Horizontal).spacing(24).build();

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

        let cover_container = Box::builder()
            .orientation(Vertical)
            .halign(Start)
            .valign(Start)
            .build();

        cover_container.append(&cover_art.widget);
        cover_container.append(&play_overlay.widget);

        let metadata_container = Box::builder()
            .orientation(Vertical)
            .hexpand(true)
            .spacing(6)
            .build();

        let title_label = Label::builder()
            .label(&album.title)
            .halign(Start)
            .xalign(0.0)
            .css_classes(["title-1"])
            .ellipsize(End)
            .tooltip_text(&album.title)
            .build();
        metadata_container.append(title_label.upcast_ref::<Widget>());

        let artist_label = Label::builder()
            .label(artist_name)
            .halign(Start)
            .xalign(0.0)
            .css_classes(["title-2"])
            .ellipsize(End)
            .tooltip_text(artist_name)
            .build();
        metadata_container.append(artist_label.upcast_ref::<Widget>());

        if let Some(year) = album.year {
            let year_label = Label::builder()
                .label(year.to_string())
                .halign(Start)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .build();
            metadata_container.append(year_label.upcast_ref::<Widget>());
        }

        if let Some(genre) = &album.genre {
            let genre_label = Label::builder()
                .label(genre)
                .halign(Start)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .ellipsize(End)
                .tooltip_text(genre)
                .build();
            metadata_container.append(genre_label.upcast_ref::<Widget>());
        }

        if album.compilation {
            let compilation_label = Label::builder()
                .label("Compilation")
                .halign(Start)
                .xalign(0.0)
                .css_classes(["dim-label"])
                .build();
            metadata_container.append(compilation_label.upcast_ref::<Widget>());
        }

        header_container.append(cover_container.upcast_ref::<Widget>());
        header_container.append(metadata_container.upcast_ref::<Widget>());

        header_container.upcast_ref::<Widget>().clone()
    }

    /// Creates the track listing section.
    ///
    /// # Arguments
    ///
    /// * `tracks` - Vector of tracks to display
    /// * `on_track_clicked` - Callback when a track is clicked
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the track list.
    fn create_track_list<F>(tracks: &[Track], on_track_clicked: F) -> Widget
    where
        F: Fn(Track) + Clone + 'static,
    {
        let list_container = Box::builder().orientation(Vertical).spacing(6).build();

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
            let row = Self::create_track_row(track, index + 1, on_track_clicked.clone());
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
    /// * `on_clicked` - Callback when track row is clicked
    ///
    /// # Returns
    ///
    /// A new `Widget` representing the track row.
    fn create_track_row<F>(track: &Track, track_number: usize, on_clicked: F) -> Widget
    where
        F: Fn(Track) + Clone + 'static,
    {
        let track = track.clone();

        let row_container = Box::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .build();

        let number_label = Label::builder()
            .label(track_number.to_string())
            .width_chars(3)
            .xalign(1.0)
            .css_classes(["dim-label"])
            .build();
        row_container.append(number_label.upcast_ref::<Widget>());

        let title_label = Label::builder()
            .label(&track.title)
            .halign(Start)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(End)
            .tooltip_text(&track.title)
            .build();
        row_container.append(title_label.upcast_ref::<Widget>());

        let duration_seconds = track.duration_ms / 1000;
        let duration_minutes = duration_seconds / 60;
        let duration_remaining = duration_seconds % 60;
        let duration_text = format!("{duration_minutes:02}:{duration_remaining:02}");
        let duration_label = Label::builder()
            .label(&duration_text)
            .halign(Start)
            .xalign(1.0)
            .css_classes(["dim-label"])
            .build();
        row_container.append(duration_label.upcast_ref::<Widget>());

        let hifi_metadata = HiFiMetadata::builder()
            .track(track.clone())
            .show_format(ShowFormat)
            .show_sample_rate(ShowSampleRate)
            .show_bit_depth(ShowBitDepth)
            .show_channels(HideChannels)
            .layout(Compact)
            .build();
        row_container.append(&hifi_metadata.widget);

        let row = ListBoxRow::new();
        row.set_child(Some(&row_container));
        row.set_activatable(true);
        row.set_selectable(true);

        row.connect_activate(move |_| {
            on_clicked(track.clone());
        });

        row.upcast_ref::<Widget>().clone()
    }
}
