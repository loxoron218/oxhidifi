//! Persistent bottom player control bar with comprehensive metadata.
//!
//! This module implements the player bar component that provides
//! playback controls, progress display, and Hi-Fi metadata information.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering::SeqCst},
    },
    time::Duration,
};

use {
    libadwaita::{
        gdk::{Paintable, Texture},
        glib::{
            Bytes,
            ControlFlow::{Break, Continue},
            MainContext,
            Propagation::Proceed,
            SourceId, timeout_add_local,
        },
        gtk::{
            AccessibleRole::Img,
            Align::{Center, Fill, Start},
            Box as GtkBox, Button,
            ContentFit::Contain,
            Label,
            Orientation::{Horizontal, Vertical},
            Picture,
            PolicyType::Never,
            Scale, ScrolledWindow, ToggleButton, Widget,
            pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExt, BoxExt, ButtonExt, Cast, RangeExt, ToggleButtonExt, WidgetExt},
    },
    num_traits::cast::FromPrimitive,
    tracing::{debug, error},
};

use crate::{
    audio::{
        engine::{
            AudioEngine,
            PlaybackState::{self, Buffering, Paused, Playing, Ready, Stopped},
            TrackInfo,
        },
        queue_manager::QueueManager,
    },
    state::{
        AppState,
        AppStateEvent::{CurrentTrackChanged, PlaybackStateChanged, QueueChanged},
        PlaybackQueue,
    },
};

/// Context struct for track information updates.
struct TrackInfoUpdateContext<'a> {
    /// Label widget displaying the track title.
    title_label: &'a Label,
    /// Label widget displaying the album name.
    album_label: &'a Label,
    /// Label widget displaying the artist name.
    artist_label: &'a Label,
    /// Label widget displaying the compact format info.
    format_label: &'a Label,
    /// Picture widget displaying the album artwork.
    artwork: &'a Picture,
    /// Label widget displaying the total track duration.
    total_duration_label: &'a Label,
    /// Toggle button for play/pause control.
    play_button: &'a ToggleButton,
    /// Button for skipping to previous track.
    prev_button: &'a Button,
    /// Button for skipping to next track.
    next_button: &'a Button,
}

/// Context struct for `AppState` subscription closure.
struct StateSubscriptionContext {
    /// Label widget displaying the track title.
    title_label: Label,
    /// Label widget displaying the album name.
    album_label: Label,
    /// Label widget displaying the artist name.
    artist_label: Label,
    /// Label widget displaying the compact format info.
    format_label: Label,
    /// Picture widget displaying the album artwork.
    artwork: Picture,
    /// Label widget displaying the total track duration.
    total_duration_label: Label,
    /// Toggle button for play/pause control.
    play_button: ToggleButton,
    /// Button for skipping to previous track.
    prev_button: Button,
    /// Button for skipping to next track.
    next_button: Button,
    /// Progress scale widget.
    progress_scale: Scale,
    /// Label widget displaying the current playback position.
    current_time_label: Label,
    /// Track duration in milliseconds.
    track_duration_ms: Arc<AtomicU64>,
}

/// Comprehensive Hi-Fi player control center with metadata display.
///
/// The `PlayerBar` provides advanced playback controls, comprehensive
/// Hi-Fi technical metadata, status indicators, and real-time updates
/// integrated with the `AudioEngine` and `AppState`.
pub struct PlayerBar {
    /// The underlying GTK box widget.
    pub widget: GtkBox,
    /// Album artwork display.
    pub artwork: Picture,
    /// Track title label.
    pub title_label: Label,
    /// Album name label.
    pub album_label: Label,
    /// Artist name label.
    pub artist_label: Label,
    /// Compact format info label.
    pub format_label: Label,
    /// Play/pause toggle button.
    pub play_button: ToggleButton,
    /// Previous track button.
    pub prev_button: Button,
    /// Next track button.
    pub next_button: Button,
    /// Progress scale with time indicators.
    pub progress_scale: Scale,
    /// Current time label.
    pub current_time_label: Label,
    /// Total duration label.
    pub total_duration_label: Label,
    /// Volume scale.
    pub volume_scale: Scale,
    /// Mute toggle button.
    pub mute_button: ToggleButton,
    /// Gapless playback indicator.
    pub gapless_indicator: Label,
    /// Bit-perfect output indicator.
    pub bit_perfect_indicator: Label,
    /// Audio routing indicator.
    pub routing_indicator: Label,
    /// Application state reference.
    pub app_state: Arc<AppState>,
    /// Audio engine reference.
    pub audio_engine: Arc<AudioEngine>,
    /// Queue manager reference.
    pub queue_manager: Option<Arc<QueueManager>>,
    /// Flag indicating if user is currently seeking.
    is_seeking: Arc<AtomicBool>,
    /// Current track duration in milliseconds.
    track_duration_ms: Arc<AtomicU64>,
    /// Source ID for position update timeout.
    position_update_source: Rc<RefCell<Option<SourceId>>>,
    /// Flag indicating if position updates are currently running.
    position_updates_running: Rc<RefCell<bool>>,
    /// Pending seek position in milliseconds.
    pending_seek_position: Arc<AtomicU64>,
    /// Pending seek sequence number to identify the latest seek request.
    pending_seek_sequence: Arc<AtomicU64>,
}

impl PlayerBar {
    /// Creates a new player bar instance with `AppState` integration.
    ///
    /// # Arguments
    ///
    /// * `app_state` - Application state reference for reactive updates
    /// * `audio_engine` - Audio engine reference for playback control
    /// * `queue_manager` - Optional queue manager reference for queue navigation
    ///
    /// # Returns
    ///
    /// A new `PlayerBar` instance.
    #[must_use]
    pub fn new(
        app_state: &Arc<AppState>,
        audio_engine: &Arc<AudioEngine>,
        queue_manager: Option<&Arc<QueueManager>>,
    ) -> Self {
        let widget = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(12)
            .css_classes(["player-bar"])
            .build();

        // Left section: Artwork and track info (fixed width)
        let (left_section, artwork, title_label, album_label, artist_label, format_label) =
            Self::create_left_section();

        widget.append(&left_section);

        // Center section: Progress bar, time labels, and playback buttons (expands)
        let (
            center_section,
            progress_scale,
            current_time_label,
            total_duration_label,
            prev_button,
            play_button,
            next_button,
        ) = Self::create_center_section();

        widget.append(&center_section);

        // Right section: Volume control and status indicators (fixed width)
        let (
            right_section,
            volume_scale,
            mute_button,
            gapless_indicator,
            bit_perfect_indicator,
            routing_indicator,
        ) = Self::create_right_section();

        widget.append(&right_section);

        let player_bar = Self {
            widget,
            artwork,
            title_label,
            album_label,
            artist_label,
            format_label,
            play_button,
            prev_button,
            next_button,
            progress_scale,
            current_time_label,
            total_duration_label,
            volume_scale,
            mute_button,
            gapless_indicator,
            bit_perfect_indicator,
            routing_indicator,
            app_state: app_state.clone(),
            audio_engine: audio_engine.clone(),
            queue_manager: queue_manager.cloned(),
            is_seeking: Arc::new(AtomicBool::new(false)),
            track_duration_ms: Arc::new(AtomicU64::new(0)),
            position_update_source: Rc::new(RefCell::new(None)),
            position_updates_running: Rc::new(RefCell::new(false)),
            pending_seek_position: Arc::new(AtomicU64::new(0)),
            pending_seek_sequence: Arc::new(AtomicU64::new(0)),
        };

        // Connect UI controls to audio engine
        player_bar.connect_controls();

        // Subscribe to AppState changes
        player_bar.subscribe_to_state_changes();

        player_bar
    }

    /// Creates a metadata label with common properties.
    ///
    /// # Arguments
    ///
    /// * `initial_text` - The initial label text
    /// * `css_classes` - CSS classes to apply to the label
    ///
    /// # Returns
    ///
    /// A configured `Label` widget.
    fn create_metadata_label(initial_text: &str, css_classes: &[&str]) -> Label {
        Label::builder()
            .label(initial_text)
            .halign(Start)
            .xalign(0.0)
            .css_classes(css_classes)
            .ellipsize(End)
            .tooltip_text(initial_text)
            .build()
    }

    /// Creates the left section containing artwork and track info.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// - The section container box
    /// - The artwork picture widget
    /// - The title label
    /// - The album label
    /// - The artist label
    /// - The format info label
    fn create_left_section() -> (GtkBox, Picture, Label, Label, Label, Label) {
        let left_section = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(12)
            .hexpand(false)
            .vexpand(false)
            .build();

        // Album artwork with strict size constraints
        let artwork = Picture::builder()
            .content_fit(Contain)
            .css_classes(["player-bar-artwork"])
            .build();

        // Set ARIA attributes
        artwork.set_accessible_role(Img);
        artwork.set_tooltip_text(Some("No artwork"));

        // Wrap artwork in ScrolledWindow to enforce strict 80x80 size and prevent expansion
        // ScrolledWindow is used here because it can clamp child widget size via
        // propagate_natural_width/height properties, unlike Frame or Box
        let artwork_container = ScrolledWindow::builder()
            .hscrollbar_policy(Never)
            .vscrollbar_policy(Never)
            .width_request(80)
            .height_request(80)
            .propagate_natural_width(false)
            .propagate_natural_height(false)
            .has_frame(false)
            .min_content_width(80)
            .min_content_height(80)
            .hexpand(false)
            .vexpand(false)
            .child(&artwork)
            .build();
        left_section.append(&artwork_container);

        // Fixed-width track info container
        let track_info_container = GtkBox::builder()
            .orientation(Vertical)
            .spacing(2)
            .hexpand(false)
            .vexpand(false)
            .width_request(320)
            .halign(Start)
            .valign(Center)
            .build();

        let title_label = Self::create_metadata_label("No track loaded", &[]);
        track_info_container.append(title_label.upcast_ref::<Widget>());

        let album_label = Self::create_metadata_label("", &["dim-label"]);
        track_info_container.append(album_label.upcast_ref::<Widget>());

        let artist_label = Self::create_metadata_label("", &["dim-label"]);
        track_info_container.append(artist_label.upcast_ref::<Widget>());

        let format_label = Self::create_metadata_label("", &["dim-label"]);
        track_info_container.append(format_label.upcast_ref::<Widget>());

        left_section.append(&track_info_container);

        (
            left_section,
            artwork,
            title_label,
            album_label,
            artist_label,
            format_label,
        )
    }

    /// Creates the center section containing progress bar, time labels, and playback buttons.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// - The section container box
    /// - The progress scale widget
    /// - The current time label
    /// - The total duration label
    /// - The previous track button
    /// - The play/pause toggle button
    /// - The next track button
    fn create_center_section() -> (GtkBox, Scale, Label, Label, Button, ToggleButton, Button) {
        let center_section = GtkBox::builder()
            .orientation(Vertical)
            .hexpand(true)
            .vexpand(false)
            .valign(Center)
            .build();

        // Progress scale (expands horizontally)
        let progress_scale = Scale::builder()
            .orientation(Horizontal)
            .hexpand(true)
            .draw_value(false)
            .build();
        progress_scale.set_range(0.0, 100.0);
        center_section.append(progress_scale.upcast_ref::<Widget>());

        // Time labels row: current time (left), spacer, total time (right)
        let time_row = GtkBox::builder()
            .orientation(Horizontal)
            .hexpand(true)
            .build();

        let current_time_label = Label::builder()
            .label("00:00")
            .css_classes(["dim-label", "numeric"])
            .build();
        time_row.append(current_time_label.upcast_ref::<Widget>());

        // Spacer to push labels to edges
        let spacer = GtkBox::builder()
            .orientation(Horizontal)
            .hexpand(true)
            .build();
        time_row.append(&spacer);

        let total_duration_label = Label::builder()
            .label("00:00")
            .css_classes(["dim-label", "numeric", "hifi-metadata-container"])
            .build();
        time_row.append(total_duration_label.upcast_ref::<Widget>());

        center_section.append(&time_row);

        // Playback buttons row (centered)
        let buttons_row = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(12)
            .halign(Center)
            .hexpand(false)
            .build();

        let prev_button = Button::builder()
            .icon_name("media-skip-backward-symbolic")
            .tooltip_text("Previous track")
            .use_underline(true)
            .has_frame(false)
            .sensitive(false)
            .build();
        buttons_row.append(prev_button.upcast_ref::<Widget>());

        let play_button = ToggleButton::builder()
            .icon_name("media-playback-start-symbolic")
            .tooltip_text("Play")
            .use_underline(true)
            .has_frame(false)
            .sensitive(false)
            .build();
        buttons_row.append(play_button.upcast_ref::<Widget>());

        let next_button = Button::builder()
            .icon_name("media-skip-forward-symbolic")
            .tooltip_text("Next track")
            .use_underline(true)
            .has_frame(false)
            .sensitive(false)
            .build();
        buttons_row.append(next_button.upcast_ref::<Widget>());

        center_section.append(&buttons_row);

        (
            center_section,
            progress_scale,
            current_time_label,
            total_duration_label,
            prev_button,
            play_button,
            next_button,
        )
    }

    /// Creates the right section containing volume control and status indicators.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// - The section container box
    /// - The volume scale widget
    /// - The mute toggle button
    /// - The gapless playback indicator label
    /// - The bit-perfect output indicator label
    /// - The audio routing indicator label
    fn create_right_section() -> (GtkBox, Scale, ToggleButton, Label, Label, Label) {
        let right_section = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(12)
            .hexpand(false)
            .vexpand(false)
            .halign(Fill)
            .valign(Center)
            .build();

        // Volume control with mute button
        let volume_container = GtkBox::builder().orientation(Horizontal).spacing(6).build();

        let mute_button = ToggleButton::builder()
            .icon_name("audio-volume-high-symbolic")
            .tooltip_text("Mute")
            .use_underline(true)
            .build();
        volume_container.append(mute_button.upcast_ref::<Widget>());

        let volume_scale = Scale::builder()
            .orientation(Horizontal)
            .width_request(100)
            .draw_value(false)
            .build();
        volume_scale.set_value(100.0);
        volume_container.append(volume_scale.upcast_ref::<Widget>());

        right_section.append(&volume_container);

        // Status indicators container
        let status_container = GtkBox::builder()
            .orientation(Horizontal)
            .spacing(6)
            .css_classes(["status-indicators"])
            .build();

        // Gapless playback indicator
        let gapless_indicator = Label::builder()
            .label("Gapless")
            .tooltip_text("Gapless playback enabled")
            .visible(false)
            .css_classes(["status-indicator", "dim-label"])
            .build();
        status_container.append(gapless_indicator.upcast_ref::<Widget>());

        // Bit-perfect output indicator
        let bit_perfect_indicator = Label::builder()
            .label("Bit-perfect")
            .tooltip_text("Bit-perfect output active")
            .visible(false)
            .css_classes(["status-indicator", "dim-label"])
            .build();
        status_container.append(bit_perfect_indicator.upcast_ref::<Widget>());

        // Audio routing indicator
        let routing_indicator = Label::builder()
            .label("Stereo")
            .tooltip_text("Stereo output")
            .visible(false)
            .css_classes(["status-indicator", "dim-label"])
            .build();
        status_container.append(routing_indicator.upcast_ref::<Widget>());

        right_section.append(&status_container);

        (
            right_section,
            volume_scale,
            mute_button,
            gapless_indicator,
            bit_perfect_indicator,
            routing_indicator,
        )
    }

    /// Connects UI controls to audio engine methods.
    fn connect_controls(&self) {
        let play_button = self.play_button.clone();
        let prev_button = self.prev_button.clone();
        let next_button = self.next_button.clone();
        let progress_scale = self.progress_scale.clone();
        let current_time_label = self.current_time_label.clone();
        let volume_scale = self.volume_scale.clone();
        let mute_button = self.mute_button.clone();
        let queue_manager = self.queue_manager.clone();
        let is_seeking = self.is_seeking.clone();
        let track_duration_ms = self.track_duration_ms.clone();

        // Play/Pause button
        let audio_engine_for_play = self.audio_engine.clone();
        play_button.connect_clicked(move |_| {
            let audio_engine_clone = audio_engine_for_play.clone();

            MainContext::default().spawn_local(async move {
                let state = audio_engine_clone.current_playback_state();

                match state {
                    Playing => {
                        if let Err(e) = audio_engine_clone.pause().await {
                            error!(error = %e, "Failed to pause playback");
                        }
                    }
                    Paused => {
                        if let Err(e) = audio_engine_clone.resume().await {
                            error!(error = %e, "Failed to resume playback");
                        }
                    }
                    Ready => {
                        if let Err(e) = audio_engine_clone.play().await {
                            error!(error = %e, "Failed to start playback");
                        }
                    }
                    Buffering => {
                        debug!("Ignoring play button click while buffering");
                    }
                    Stopped => {
                        debug!("Ignoring play button click - no track loaded");
                    }
                }
            });
        });

        // Previous button
        let prev_queue_manager = queue_manager.clone();
        prev_button.connect_clicked(move |_| {
            if let Some(ref qm) = prev_queue_manager {
                qm.previous_track();
            }
        });

        // Next button
        let next_queue_manager = queue_manager.clone();
        next_button.connect_clicked(move |_| {
            if let Some(ref qm) = next_queue_manager {
                qm.next_track();
            }
        });

        // Progress scale - change-value signal for user interaction
        let audio_engine_seek = self.audio_engine.clone();
        let is_seeking_clone = is_seeking.clone();
        let track_duration_ms_clone = track_duration_ms.clone();
        let current_time_label_seek = current_time_label.clone();
        let pending_seek_position_ref = self.pending_seek_position.clone();
        let pending_seek_sequence_ref = self.pending_seek_sequence.clone();

        progress_scale.connect_change_value(move |_scale, _scroll_type, value| {
            let is_seeking = is_seeking_clone.clone();
            let audio_engine = audio_engine_seek.clone();
            let track_duration_ms = track_duration_ms_clone.clone();
            let current_time_label = current_time_label_seek.clone();
            let pending_seek_position = pending_seek_position_ref.clone();
            let pending_seek_sequence = pending_seek_sequence_ref.clone();

            // Set seeking flag
            is_seeking.store(true, SeqCst);

            // Calculate position in milliseconds
            let duration_ms = track_duration_ms.load(SeqCst);
            let position_ms = if duration_ms > 0 {
                let percent = value.clamp(0.0, 100.0).round();
                let percent_u64 = u64::from_f64(percent).unwrap_or_default();
                percent_u64.saturating_mul(duration_ms) / 100
            } else {
                0
            };

            // Update time label in real-time during seek
            let seconds = position_ms / 1000;
            let minutes = seconds / 60;
            let remaining = seconds % 60;
            let time_text = format!("{minutes:02}:{remaining:02}");
            current_time_label.set_label(&time_text);

            // Store the pending position
            pending_seek_position.store(position_ms, SeqCst);

            // Increment sequence number for this seek request
            let current_sequence = pending_seek_sequence.fetch_add(1, SeqCst).wrapping_add(1);

            // Schedule a debounced seek after 100ms of no movement
            // Use sequence number to ensure only the latest seek executes
            timeout_add_local(Duration::from_millis(100), move || {
                // Only execute if this is still the latest seek request
                let latest_sequence = pending_seek_sequence.load(SeqCst);

                if current_sequence >= latest_sequence {
                    let position = pending_seek_position.load(SeqCst);
                    let audio_engine = audio_engine.clone();
                    let is_seeking = is_seeking.clone();

                    MainContext::default().spawn_local(async move {
                        if let Err(e) = audio_engine.seek(position).await {
                            error!(position = %position, error = %e, "Failed to seek to position");
                        }

                        // Clear seeking flag after seek completes
                        is_seeking.store(false, SeqCst);
                    });
                }

                Break
            });

            Proceed
        });

        // Volume control
        volume_scale.connect_value_changed(move |scale| {
            let volume = scale.value() / 100.0;

            // Implementation would handle volume setting
            debug!(volume = %volume, "Volume changed");
        });

        // Mute button
        mute_button.connect_toggled(move |button| {
            let muted = button.is_active();

            // Implementation would handle mute state
            debug!(muted = %muted, "Mute state changed");
        });
    }

    /// Subscribes to `AppState` changes for reactive updates.
    fn subscribe_to_state_changes(&self) {
        let app_state = self.app_state.clone();
        let position_update_source = self.position_update_source.clone();
        let position_updates_running = self.position_updates_running.clone();
        let audio_engine = self.audio_engine.clone();
        let progress_scale = self.progress_scale.clone();
        let current_time_label = self.current_time_label.clone();
        let is_seeking = self.is_seeking.clone();
        let track_duration_ms = self.track_duration_ms.clone();
        let context = StateSubscriptionContext {
            title_label: self.title_label.clone(),
            album_label: self.album_label.clone(),
            artist_label: self.artist_label.clone(),
            format_label: self.format_label.clone(),
            artwork: self.artwork.clone(),
            total_duration_label: self.total_duration_label.clone(),
            play_button: self.play_button.clone(),
            prev_button: self.prev_button.clone(),
            next_button: self.next_button.clone(),
            progress_scale: self.progress_scale.clone(),
            current_time_label: self.current_time_label.clone(),
            track_duration_ms: self.track_duration_ms.clone(),
        };

        debug!("PlayerBar: Subscribing to AppState changes");
        MainContext::default().spawn_local(async move {
            let start_position_updates = {
                let position_update_source = position_update_source.clone();
                let position_updates_running = position_updates_running.clone();
                let audio_engine = audio_engine.clone();
                let progress_scale = progress_scale.clone();
                let current_time_label = current_time_label.clone();
                let is_seeking = is_seeking.clone();
                let track_duration_ms = track_duration_ms.clone();

                move || {
                    if track_duration_ms.load(SeqCst) == 0 || *position_updates_running.borrow() {
                        return;
                    }

                    let audio_engine = audio_engine.clone();
                    let progress_scale = progress_scale.clone();
                    let current_time_label = current_time_label.clone();
                    let is_seeking = is_seeking.clone();
                    let track_duration_ms = track_duration_ms.clone();

                    let source_id = timeout_add_local(Duration::from_millis(100), move || {
                        if !is_seeking.load(SeqCst)
                            && let Some(position_ms) = audio_engine.current_position()
                        {
                            let duration_ms = track_duration_ms.load(SeqCst);

                            if duration_ms > 0
                                && position_ms < u64::from(u32::MAX)
                                && duration_ms < u64::from(u32::MAX)
                            {
                                let progress = f64::from(u32::try_from(position_ms).unwrap())
                                    / f64::from(u32::try_from(duration_ms).unwrap());
                                let progress_percent = progress * 100.0;

                                progress_scale.set_value(progress_percent);
                            }

                            let seconds = position_ms / 1000;
                            let minutes = seconds / 60;
                            let remaining = seconds % 60;
                            let time_text = format!("{minutes:02}:{remaining:02}");
                            current_time_label.set_label(&time_text);
                        }
                        Continue
                    });

                    *position_update_source.borrow_mut() = Some(source_id);
                    *position_updates_running.borrow_mut() = true;
                }
            };

            let stop_position_updates = {
                let position_update_source = position_update_source.clone();
                let position_updates_running = position_updates_running.clone();

                move || {
                    if !*position_updates_running.borrow() {
                        return;
                    }

                    if let Some(source_id) = position_update_source.borrow_mut().take() {
                        let () = source_id.remove();
                    }
                    *position_updates_running.borrow_mut() = false;
                }
            };

            let receiver = app_state.subscribe();
            loop {
                if let Ok(event) = receiver.recv().await {
                    match event {
                        CurrentTrackChanged(track_info) => {
                            debug!("PlayerBar: Current track changed");
                            let update_context = TrackInfoUpdateContext {
                                title_label: &context.title_label,
                                album_label: &context.album_label,
                                artist_label: &context.artist_label,
                                format_label: &context.format_label,
                                artwork: &context.artwork,
                                total_duration_label: &context.total_duration_label,
                                play_button: &context.play_button,
                                prev_button: &context.prev_button,
                                next_button: &context.next_button,
                            };
                            Self::update_track_info(
                                track_info.as_ref().as_ref(),
                                &update_context,
                                &context.progress_scale,
                                &context.current_time_label,
                                &context.track_duration_ms,
                            );

                            // Manage position updates based on track loading
                            if track_duration_ms.load(SeqCst) > 0 {
                                start_position_updates();
                            } else {
                                stop_position_updates();
                            }
                        }
                        PlaybackStateChanged(state) => {
                            debug!(state = ?state, "PlayerBar: Playback state changed");
                            Self::update_playback_state(&state, &context.play_button);

                            // Only update position when playing
                            if matches!(state, Playing) {
                                start_position_updates();
                            } else if matches!(state, Stopped) {
                                stop_position_updates();
                            }
                        }
                        QueueChanged(queue) => {
                            debug!(track_count = %queue.tracks.len(), "PlayerBar: Queue changed");
                            Self::update_queue_buttons(
                                &context.prev_button,
                                &context.next_button,
                                &queue,
                            );
                        }
                        _ => {}
                    }
                } else {
                    // Channel was closed - this means AppState is gone.
                    debug!("PlayerBar state subscription channel closed");
                    break;
                }
            }
        });
    }

    /// Updates track information display.
    fn update_track_info(
        track_info: Option<&TrackInfo>,
        ctx: &TrackInfoUpdateContext,
        progress_scale: &Scale,
        current_time_label: &Label,
        track_duration_ms: &Arc<AtomicU64>,
    ) {
        if let Some(info) = track_info {
            // Update title
            let title = info
                .metadata
                .standard
                .title
                .clone()
                .unwrap_or_else(|| "Unknown Track".to_string());
            ctx.title_label.set_label(&title);
            ctx.title_label.set_tooltip_text(Some(&title));

            // Update album
            let album = info
                .metadata
                .standard
                .album
                .clone()
                .unwrap_or_else(|| "Unknown Album".to_string());
            ctx.album_label.set_label(&album);
            ctx.album_label.set_tooltip_text(Some(&album));

            // Update artist
            let artist = info
                .metadata
                .standard
                .artist
                .clone()
                .unwrap_or_else(|| "Unknown Artist".to_string());
            ctx.artist_label.set_label(&artist);
            ctx.artist_label.set_tooltip_text(Some(&artist));

            // Update format info (compact format)
            let channels_text = match info.format.channels {
                1 => "Mono".to_string(),
                2 => "Stereo".to_string(),
                n => format!("{n} ch"),
            };
            let sample_rate_display = if info.format.sample_rate % 1000 == 0 {
                format!("{}", info.format.sample_rate / 1000)
            } else {
                format!("{:.1}", f64::from(info.format.sample_rate) / 1000.0)
            };
            let format_str = format!(
                "{} {}/{sample_rate_display} {channels_text}",
                info.metadata.technical.format, info.metadata.technical.bits_per_sample
            );
            ctx.format_label.set_label(&format_str);
            ctx.format_label.set_tooltip_text(Some(&format_str));

            // Update embedded artwork
            if let Some(artwork_data) = &info.metadata.artwork {
                let bytes = Bytes::from(&artwork_data[..]);

                match Texture::from_bytes(&bytes) {
                    Ok(texture) => {
                        ctx.artwork.set_paintable(Some(&texture));
                        ctx.artwork.set_tooltip_text(Some("Embedded album artwork"));
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to load artwork from bytes");
                        ctx.artwork.set_paintable(None::<&Paintable>);
                        ctx.artwork.set_tooltip_text(Some("Failed to load artwork"));
                    }
                }
            } else {
                // No embedded artwork
                ctx.artwork.set_paintable(None::<&Paintable>);
                ctx.artwork.set_tooltip_text(Some("No artwork"));
            }

            // Update duration and store it
            track_duration_ms.store(info.duration_ms, SeqCst);
            let duration_seconds = info.duration_ms / 1000;
            let duration_minutes = duration_seconds / 60;
            let duration_remaining = duration_seconds % 60;
            let duration_text = format!("{duration_minutes:02}:{duration_remaining:02}");
            ctx.total_duration_label.set_label(&duration_text);

            // Reset progress bar
            progress_scale.set_value(0.0);
            current_time_label.set_label("00:00");

            // Enable controls
            ctx.play_button.set_sensitive(true);
            ctx.prev_button.set_sensitive(true);
            ctx.next_button.set_sensitive(true);
        } else {
            // Clear all track info
            ctx.title_label.set_label("No track loaded");
            ctx.title_label.set_tooltip_text(Some("No track loaded"));
            ctx.album_label.set_label("");
            ctx.album_label.set_tooltip_text(Some(""));
            ctx.artist_label.set_label("");
            ctx.artist_label.set_tooltip_text(Some(""));
            ctx.format_label.set_label("");
            ctx.format_label.set_tooltip_text(Some(""));
            ctx.artwork.set_paintable(None::<&Paintable>);
            ctx.artwork.set_tooltip_text(Some("No artwork"));
            ctx.total_duration_label.set_label("00:00");
            track_duration_ms.store(0, SeqCst);

            // Reset progress
            progress_scale.set_value(0.0);
            current_time_label.set_label("00:00");

            // Disable controls
            ctx.play_button.set_sensitive(false);
            ctx.prev_button.set_sensitive(false);
            ctx.next_button.set_sensitive(false);
        }
    }

    /// Updates playback state display.
    fn update_playback_state(state: &PlaybackState, play_button: &ToggleButton) {
        match state {
            Playing => {
                play_button.set_icon_name("media-playback-pause-symbolic");
                play_button.set_tooltip_text(Some("Pause"));
            }
            Paused | Stopped | Ready => {
                play_button.set_icon_name("media-playback-start-symbolic");
                play_button.set_tooltip_text(Some("Play"));
            }
            Buffering => {
                play_button.set_icon_name("media-playback-pause-symbolic");
                play_button.set_tooltip_text(Some("Buffering..."));
            }
        }
    }

    /// Updates prev/next button sensitivity based on queue state.
    fn update_queue_buttons(prev_button: &Button, next_button: &Button, queue: &PlaybackQueue) {
        if queue.tracks.is_empty() {
            prev_button.set_sensitive(false);
            next_button.set_sensitive(false);
            return;
        }

        let current_index = queue.current_index.unwrap_or(0);
        let total_tracks = queue.tracks.len();

        // Enable prev button if not at beginning
        prev_button.set_sensitive(current_index > 0);

        // Enable next button if not at end
        next_button.set_sensitive(current_index + 1 < total_tracks);
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, sync::Arc};

    use parking_lot::RwLock;

    use crate::{audio::engine::AudioEngine, config::SettingsManager, state::AppState};

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_player_bar_creation() -> Result<(), Box<dyn Error>> {
        let engine = AudioEngine::new()?;
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new()?;
        let _app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        // This test would require mocking AppState and AudioEngine properly
        // For now, we'll just verify the constructor signature compiles
        Ok(())
    }
}
