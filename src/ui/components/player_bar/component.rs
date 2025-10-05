use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    Box, Button, Image, Label, Scale,
    glib::{SignalHandlerId, WeakRef},
};
use libadwaita::prelude::{ObjectExt, WidgetExt};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::playback::controller::PlaybackController;

/// A UI component that displays currently playing song information at the bottom of the window.
///
/// The player bar is only visible when a song is playing. It shows:
/// - Album art (96x96 pixels)
/// - Song title (ellipsized when too long)
/// - Artist name (ellipsized when too long)
/// - Album title (ellipsized when too long)
/// - Bit depth and sample rate information (ellipsized when too long)
/// - Audio format (ellipsized when too long)
/// - Progress bar
/// - Time labels (start and end) with play controls between them
/// - Additional controls (volume, indicators) on the right
///
/// The text information is contained in a fixed-width box that maintains consistent sizing
/// regardless of text length. Text that exceeds the available space is automatically
/// ellipsized following GNOME Human Interface Guidelines.
///
/// When no song is playing, the player bar is hidden from view.
pub struct PlayerBar {
    /// The main container for all player bar elements, arranged horizontally
    pub container: Box,
    /// Display for album art, defaults to a placeholder icon when no art is available
    pub album_art: Image,
    /// Label displaying the currently playing song title
    pub song_title: Label,
    /// Label displaying the artist of the currently playing song
    pub song_artist: Label,
    /// Label displaying the album title
    pub album_title: Label,
    /// Label displaying bit depth and sample rate information
    pub bit_depth_sample_rate: Label,
    /// Label displaying the audio format (combined with bit depth/sample rate)
    pub format: Label,
    /// Progress bar showing current position in the song
    pub progress_bar: Scale,
    /// Label displaying current playback position (e.g., "1:23")
    pub time_label_start: Label,
    /// Label displaying total song duration (e.g., "4:56")
    pub time_label_end: Label,
    /// Volume control slider
    pub _volume_slider: Scale,
    /// Placeholder for bit perfect indicator
    pub _bit_perfect_indicator: Button,
    /// Placeholder for gapless indicator
    pub _gapless_indicator: Button,
    /// Previous song button
    pub _prev_button: Button,
    /// Play/pause button
    pub _play_button: Button,
    /// Next song button
    pub _next_button: Button,
    /// Reference to the main content area that needs padding adjustment
    pub main_content_area: Rc<RefCell<Option<Box>>>,
    /// Signal handler ID for visibility change notifications
    pub visibility_handler_id: Option<Rc<SignalHandlerId>>,
    /// Duration of the current song in seconds
    pub duration: Rc<Cell<f64>>,
    /// Playback controller for managing audio playback
    pub playback_controller: Option<Arc<Mutex<PlaybackController>>>,
    /// Whether navigation to the previous song is possible
    pub can_go_prev: Cell<bool>,
    /// Whether navigation to the next song is possible
    pub can_go_next: Cell<bool>,
    /// Cancellation token for stopping the event listening task
    pub cancellation_token: CancellationToken,
}

impl Clone for PlayerBar {
    /// Creates a new `PlayerBar` instance by cloning the existing one.
    ///
    /// This implementation performs a shallow clone of most fields, as they are
    /// `Rc` or `Arc` wrapped, meaning they share ownership of the underlying data.
    /// A new `CancellationToken` is created for the cloned instance to ensure
    /// independent cancellation behavior.
    fn clone(&self) -> Self {
        Self {
            container: self.container.clone(),
            album_art: self.album_art.clone(),
            song_title: self.song_title.clone(),
            song_artist: self.song_artist.clone(),
            album_title: self.album_title.clone(),
            bit_depth_sample_rate: self.bit_depth_sample_rate.clone(),
            format: self.format.clone(),
            progress_bar: self.progress_bar.clone(),
            time_label_start: self.time_label_start.clone(),
            time_label_end: self.time_label_end.clone(),
            _volume_slider: self._volume_slider.clone(),
            _bit_perfect_indicator: self._bit_perfect_indicator.clone(),
            _gapless_indicator: self._gapless_indicator.clone(),
            _prev_button: self._prev_button.clone(),
            _play_button: self._play_button.clone(),
            _next_button: self._next_button.clone(),
            main_content_area: self.main_content_area.clone(),
            visibility_handler_id: self.visibility_handler_id.clone(),
            duration: self.duration.clone(),
            playback_controller: self.playback_controller.clone(),
            can_go_prev: self.can_go_prev.clone(),
            can_go_next: self.can_go_next.clone(),
            cancellation_token: CancellationToken::new(),
        }
    }
}

impl PlayerBar {
    /// Creates a weak reference to this PlayerBar instance
    pub fn downgrade(&self) -> PlayerBarWeak {
        PlayerBarWeak {
            container: self.container.downgrade(),
            album_art: self.album_art.downgrade(),
            song_title: self.song_title.downgrade(),
            song_artist: self.song_artist.downgrade(),
            album_title: self.album_title.downgrade(),
            bit_depth_sample_rate: self.bit_depth_sample_rate.downgrade(),
            format: self.format.downgrade(),
            progress_bar: self.progress_bar.downgrade(),
            time_label_start: self.time_label_start.downgrade(),
            time_label_end: self.time_label_end.downgrade(),
            _volume_slider: self._volume_slider.downgrade(),
            _bit_perfect_indicator: self._bit_perfect_indicator.downgrade(),
            _gapless_indicator: self._gapless_indicator.downgrade(),
            _prev_button: self._prev_button.downgrade(),
            _play_button: self._play_button.downgrade(),
            _next_button: self._next_button.downgrade(),
            main_content_area: self.main_content_area.clone(),
            visibility_handler_id: self.visibility_handler_id.clone(),
            duration: self.duration.clone(),
            playback_controller: self.playback_controller.clone(),
            can_go_prev: self.can_go_prev.clone(),
            can_go_next: self.can_go_next.clone(),
            cancellation_token: self.cancellation_token.clone(),
        }
    }

    /// Gets a reference to the playback controller
    ///
    /// This method returns a clone of the playback controller reference,
    /// which can be used to control playback and access queue functionality.
    ///
    /// # Returns
    /// * `Option<Arc<Mutex<PlaybackController>>>` - The playback controller reference, if available
    pub fn get_playback_controller(&self) -> Option<Arc<Mutex<PlaybackController>>> {
        self.playback_controller.clone()
    }

    /// Ensures the player bar is visible, typically called when playback starts
    ///
    /// This method can be called directly to make sure the player bar is visible
    /// when playback begins, providing an additional guarantee beyond event handling.
    pub fn ensure_visible(&self) {
        if !self.container.is_visible() {
            self.container.set_visible(true);
            if let Some(content_area) = self.main_content_area.borrow().as_ref() {
                content_area.set_margin_bottom(120);
            }
        }

        // Update play button state to reflect current playback state
        self.update_play_button_state();
    }
}

/// Weak reference to a PlayerBar instance
#[derive(Clone)]
pub struct PlayerBarWeak {
    container: WeakRef<Box>,
    album_art: WeakRef<Image>,
    song_title: WeakRef<Label>,
    song_artist: WeakRef<Label>,
    album_title: WeakRef<Label>,
    bit_depth_sample_rate: WeakRef<Label>,
    format: WeakRef<Label>,
    progress_bar: WeakRef<Scale>,
    time_label_start: WeakRef<Label>,
    time_label_end: WeakRef<Label>,
    _volume_slider: WeakRef<Scale>,
    _bit_perfect_indicator: WeakRef<Button>,
    _gapless_indicator: WeakRef<Button>,
    _prev_button: WeakRef<Button>,
    _play_button: WeakRef<Button>,
    _next_button: WeakRef<Button>,
    main_content_area: Rc<RefCell<Option<Box>>>,
    visibility_handler_id: Option<Rc<SignalHandlerId>>,
    duration: Rc<Cell<f64>>,
    playback_controller: Option<Arc<Mutex<PlaybackController>>>,
    can_go_prev: Cell<bool>,
    can_go_next: Cell<bool>,
    cancellation_token: CancellationToken,
}

impl PlayerBarWeak {
    /// Attempts to upgrade the weak reference to a PlayerBar instance
    pub fn upgrade(&self) -> Option<PlayerBar> {
        Some(PlayerBar {
            container: self.container.upgrade()?,
            album_art: self.album_art.upgrade()?,
            song_title: self.song_title.upgrade()?,
            song_artist: self.song_artist.upgrade()?,
            album_title: self.album_title.upgrade()?,
            bit_depth_sample_rate: self.bit_depth_sample_rate.upgrade()?,
            format: self.format.upgrade()?,
            progress_bar: self.progress_bar.upgrade()?,
            time_label_start: self.time_label_start.upgrade()?,
            time_label_end: self.time_label_end.upgrade()?,
            _volume_slider: self._volume_slider.upgrade()?,
            _bit_perfect_indicator: self._bit_perfect_indicator.upgrade()?,
            _gapless_indicator: self._gapless_indicator.upgrade()?,
            _prev_button: self._prev_button.upgrade()?,
            _play_button: self._play_button.upgrade()?,
            _next_button: self._next_button.upgrade()?,
            main_content_area: self.main_content_area.clone(),
            visibility_handler_id: self.visibility_handler_id.clone(),
            duration: self.duration.clone(),
            playback_controller: self.playback_controller.clone(),
            can_go_prev: self.can_go_prev.clone(),
            can_go_next: self.can_go_next.clone(),
            cancellation_token: self.cancellation_token.clone(),
        })
    }
}

impl Drop for PlayerBar {
    fn drop(&mut self) {
        // Cancel any running tasks when the PlayerBar is dropped
        self.cancellation_token.cancel();
    }
}
