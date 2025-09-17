use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
};

use gtk4::{
    Align::{End, Start},
    Box, Button, Image, Label,
    Orientation::{Horizontal, Vertical},
    Scale,
    gdk_pixbuf::Pixbuf,
    glib::SignalHandlerId,
    pango::EllipsizeMode,
    prelude::{BoxExt, ObjectExt, RangeExt, WidgetExt},
};

use crate::utils::formatting::format_sample_rate_value;

/// A UI component that displays currently playing track information at the bottom of the window.
///
/// The player bar is only visible when a track is playing. It shows:
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
/// When no track is playing, the player bar is hidden from view.
#[derive(Clone)]
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
    /// Progress bar showing current position in the track
    pub progress_bar: Scale,
    /// Label displaying current playback position (e.g., "1:23")
    pub time_label_start: Label,
    /// Label displaying total track duration (e.g., "4:56")
    pub time_label_end: Label,
    /// Volume control slider
    pub _volume_slider: Scale,
    /// Placeholder for bit perfect indicator
    pub _bit_perfect_indicator: Button,
    /// Placeholder for gapless indicator
    pub _gapless_indicator: Button,
    /// Previous track button
    pub _prev_button: Button,
    /// Play/pause button
    pub _play_button: Button,
    /// Next track button
    pub _next_button: Button,
    /// Reference to the main content area that needs padding adjustment
    main_content_area: Rc<RefCell<Option<Box>>>,
    /// Signal handler ID for visibility change notifications
    visibility_handler_id: Option<Rc<SignalHandlerId>>,
    /// Duration of the current track in seconds
    duration: Rc<Cell<f64>>,
}

impl PlayerBar {
    /// Creates a new PlayerBar instance with all UI elements initialized.
    ///
    /// The player bar is initially hidden and will only become visible when
    /// `update_with_metadata` is called with track information.
    ///
    /// # UI Structure
    /// The player bar layout consists of:
    /// 1. Album art (96x96 pixels) on the left
    /// 2. Track information (song title, artist, album title, bit depth/sample rate, format) in a fixed-width container in the center
    /// 3. Progress bar
    /// 4. Time labels (start and end) with play controls between them
    /// 5. Additional controls (volume, indicators) aligned to the right
    ///
    /// The track information container has a fixed width of 300 pixels and automatically ellipsizes
    /// text that exceeds the available space, following GNOME Human Interface Guidelines.
    ///
    /// # Returns
    /// A new `PlayerBar` instance with all widgets created but not yet visible
    pub fn new() -> Self {
        // Create the main horizontal container with spacing and CSS styling
        let container = Box::builder()
            .orientation(Horizontal)
            .spacing(12)
            .css_classes(vec!["player-bar"])
            .build();

        // Ensure the container properly distributes space among children
        container.set_homogeneous(false);

        // Initialize album art display with a default placeholder icon
        let album_art = Image::builder()
            .width_request(96)
            .height_request(96)
            .icon_name("image-missing")
            .build();
        container.append(&album_art);

        // Create a vertical box to hold track information
        let info_box = Box::builder().orientation(Vertical).build();

        // Set a fixed width for the info box to ensure consistent sizing
        info_box.set_size_request(300, -1);

        // Prevent the info box from expanding horizontally
        info_box.set_hexpand(false);
        info_box.set_halign(Start);
        info_box.set_valign(Start);

        // Ensure the info box maintains its size
        info_box.set_vexpand(false);
        container.append(&info_box);

        // Initialize track title label with placeholder text
        let song_title = Label::builder().label("Song Title").halign(Start).build();
        song_title.add_css_class("song-title");

        // Apply ellipsizing to prevent text overflow
        song_title.set_ellipsize(EllipsizeMode::End);
        song_title.set_xalign(0.0);

        // Prevent horizontal expansion and set max width chars for ellipsizing
        song_title.set_hexpand(false);
        song_title.set_max_width_chars(25);
        info_box.append(&song_title);

        // Initialize artist label with placeholder text
        let song_artist = Label::builder().label("Artist").halign(Start).build();
        song_artist.add_css_class("artist-name");

        // Apply ellipsizing to prevent text overflow
        song_artist.set_ellipsize(EllipsizeMode::End);
        song_artist.set_xalign(0.0);

        // Prevent horizontal expansion and set max width chars for ellipsizing
        song_artist.set_hexpand(false);
        song_artist.set_max_width_chars(25);
        info_box.append(&song_artist);

        // Initialize album title label with placeholder text
        let album_title = Label::builder().label("Album Title").halign(Start).build();
        album_title.add_css_class("album-title");

        // Apply ellipsizing to prevent text overflow
        album_title.set_ellipsize(EllipsizeMode::End);
        album_title.set_xalign(0.0);

        // Prevent horizontal expansion and set max width chars for ellipsizing
        album_title.set_hexpand(false);
        album_title.set_max_width_chars(25);
        info_box.append(&album_title);

        // Initialize bit depth/sample rate label with placeholder text
        let bit_depth_sample_rate = Label::builder()
            .label("24-Bit/96 kHz")
            .halign(Start)
            .build();
        bit_depth_sample_rate.add_css_class("bit-depth-sample-rate");

        // Apply ellipsizing to prevent text overflow
        bit_depth_sample_rate.set_ellipsize(EllipsizeMode::End);
        bit_depth_sample_rate.set_xalign(0.0);

        // Prevent horizontal expansion and set max width chars for ellipsizing
        bit_depth_sample_rate.set_hexpand(false);
        bit_depth_sample_rate.set_max_width_chars(25);
        info_box.append(&bit_depth_sample_rate);

        // Initialize format label with placeholder text (combined display with bit depth/sample rate)
        let format = Label::builder().label("").halign(Start).build();
        format.add_css_class("format");

        // Apply ellipsizing to prevent text overflow
        format.set_ellipsize(EllipsizeMode::End);
        format.set_xalign(0.0);

        // Prevent horizontal expansion and set max width chars for ellipsizing
        format.set_hexpand(false);
        format.set_max_width_chars(25);
        info_box.append(&format);

        // Create a container for progress bar and time label
        let progress_box = Box::builder().orientation(Vertical).hexpand(true).build();

        // Removed halign(Start) to allow the progress box to expand and fill available space
        container.append(&progress_box);

        // Create progress bar
        let progress_bar = Scale::builder()
            .orientation(Horizontal)
            .draw_value(false)
            .hexpand(true)
            .build();
        progress_bar.add_css_class("player-progress-bar");
        progress_bar.set_range(0.0, 100.0);
        progress_box.append(&progress_bar);

        // Create a container for the bottom row (time labels and play controls)
        let bottom_row_box = Box::builder().orientation(Horizontal).hexpand(true).build();
        bottom_row_box.add_css_class("player-bottom-row");
        bottom_row_box.set_spacing(0);
        progress_box.append(&bottom_row_box);

        // Create start time label (current position)
        let time_label_start = Label::builder().label("0:00").halign(Start).build();
        time_label_start.add_css_class("time-label");
        time_label_start.add_css_class("time-label-start");
        time_label_start.set_hexpand(false);
        time_label_start.set_halign(Start);
        time_label_start.set_width_chars(5);
        time_label_start.set_xalign(0.0);
        bottom_row_box.append(&time_label_start);

        // Add a spacer to push the play controls to the center with reduced size
        let spacer_left = Box::builder().hexpand(true).build();
        spacer_left.set_size_request(-1, -1);
        bottom_row_box.append(&spacer_left);

        // Create a container for play control buttons
        let play_controls_box = Box::builder().orientation(Horizontal).spacing(2).build();
        play_controls_box.add_css_class("play-controls-box");
        play_controls_box.set_halign(Start);
        bottom_row_box.append(&play_controls_box);

        // Add previous track button with standard media icon
        let prev_button = Button::from_icon_name("media-skip-backward");
        prev_button.add_css_class("media-button");
        play_controls_box.append(&prev_button);

        // Add play button with standard media icon
        let play_button = Button::from_icon_name("media-playback-start");
        play_button.add_css_class("media-button");
        play_controls_box.append(&play_button);

        // Add next track button with standard media icon
        let next_button = Button::from_icon_name("media-skip-forward");
        next_button.add_css_class("media-button");
        play_controls_box.append(&next_button);

        // Add a spacer to push the end time label to the right with reduced size
        let spacer_right = Box::builder().hexpand(true).build();
        spacer_right.set_size_request(-1, -1);
        bottom_row_box.append(&spacer_right);

        // Create end time label (total duration)
        let time_label_end = Label::builder().label("0:00").halign(End).build();
        time_label_end.add_css_class("time-label");
        time_label_end.add_css_class("time-label-end");
        time_label_end.set_hexpand(false);
        time_label_end.set_halign(End);
        time_label_end.set_width_chars(5);
        time_label_end.set_xalign(1.0);
        bottom_row_box.append(&time_label_end);

        // Create a container for additional control buttons (volume, indicators)
        let controls_box = Box::builder().orientation(Horizontal).spacing(8).build();
        container.append(&controls_box);

        // Add volume slider
        let volume_slider = Scale::builder()
            .orientation(Horizontal)
            .draw_value(false)
            .width_request(80)
            .build();
        volume_slider.set_range(0.0, 100.0);
        volume_slider.set_value(100.0);
        volume_slider.add_css_class("volume-slider");
        controls_box.append(&volume_slider);

        // Add bit perfect indicator
        let bit_perfect_indicator = Button::builder()
            .label("BP")
            .css_classes(vec!["indicator", "bit-perfect-indicator"])
            .build();
        controls_box.append(&bit_perfect_indicator);

        // Add gapless indicator
        let gapless_indicator = Button::builder()
            .label("G")
            .css_classes(vec!["indicator", "gapless-indicator"])
            .build();
        controls_box.append(&gapless_indicator);

        // Initially hide the player bar until a track is played
        container.set_visible(false);

        // Construct and return the PlayerBar instance with all initialized components
        // This struct initialization makes all the UI components accessible to the caller
        Self {
            container,
            album_art,
            song_title,
            song_artist,
            album_title,
            bit_depth_sample_rate,
            format,
            progress_bar,
            time_label_start,
            time_label_end,
            _volume_slider: volume_slider,
            _bit_perfect_indicator: bit_perfect_indicator,
            _gapless_indicator: gapless_indicator,
            _prev_button: prev_button,
            _play_button: play_button,
            _next_button: next_button,
            main_content_area: Rc::new(RefCell::new(None)),
            visibility_handler_id: None,
            duration: Rc::new(Cell::new(0.0)),
        }
    }

    /// Sets the main content area that needs padding adjustment when player bar visibility changes.
    ///
    /// This method should be called once during initialization to provide a reference
    /// to the main content area (typically vbox_inner from the main window builder).
    ///
    /// # Parameters
    /// * `content_area` - A reference to the main content area Box widget
    pub fn set_main_content_area(&mut self, content_area: Box) {
        *self.main_content_area.borrow_mut() = Some(content_area);
    }

    /// Connects to visibility change notifications for the player bar container.
    ///
    /// This method sets up a signal handler that monitors the "visible" property
    /// of the player bar container. When visibility changes, it adjusts the
    /// bottom margin of the main content area to prevent overlap.
    ///
    /// This method should be called after the player bar has been added to the overlay
    /// and the main content area has been set.
    pub fn connect_visibility_changes(&mut self) {
        // If we already have a handler, disconnect it first
        // Note: We can't disconnect the handler because SignalHandlerId doesn't implement Clone
        // This means we might have multiple handlers if this method is called multiple times
        // In practice, this should only be called once during initialization

        // If we have a content area, connect the visibility change handler
        if let Some(content_area) = self.main_content_area.borrow().as_ref() {
            let content_area_weak = ObjectExt::downgrade(content_area);
            let container_weak = ObjectExt::downgrade(&self.container);
            let handler_id =
                self.container
                    .connect_notify_local(Some("visible"), move |_container, _| {
                        if let (Some(content_area), Some(container_strong)) =
                            (content_area_weak.upgrade(), container_weak.upgrade())
                        {
                            if container_strong.is_visible() {
                                // When player bar becomes visible, add bottom margin to content area
                                // Get the player bar height and use it as margin
                                let allocation = container_strong.allocation();
                                let height = allocation.height();
                                content_area.set_margin_bottom(height);
                            } else {
                                // When player bar becomes hidden, remove bottom margin from content area
                                content_area.set_margin_bottom(0);
                            }
                        }
                    });
            self.visibility_handler_id = Some(Rc::new(handler_id));
        }
    }

    /// Updates the player bar with track metadata and makes it visible.
    ///
    /// This method is called when a track starts playing to display its information
    /// in the player bar at the bottom of the window. The information is displayed
    /// in the following order to follow GNOME Human Interface Guidelines:
    /// 1. Song title
    /// 2. Artist name
    /// 3. Album title
    /// 4. Bit depth and sample rate
    /// 5. Audio format
    ///
    /// # Parameters
    /// - `album_title`: The title of the album
    /// - `song_title`: The title of the currently playing track
    /// - `song_artist`: The artist of the currently playing track
    /// - `cover_art_path`: Optional path to the album art image file
    /// - `bit_depth`: Optional bit depth of the audio file
    /// - `sample_rate`: Optional sample rate of the audio file
    /// - `format`: Optional format of the audio file
    /// - `duration`: Optional duration of the track in seconds
    ///
    /// # Behavior
    /// - Updates all labels with the provided metadata
    /// - Attempts to load album art from the provided path, falling back to a
    ///   placeholder icon if loading fails or no path is provided
    /// - Makes the player bar visible
    pub fn update_with_metadata(
        &self,
        album_title: &str,
        song_title: &str,
        song_artist: &str,
        cover_art_path: Option<&Path>,
        bit_depth: Option<u32>,
        sample_rate: Option<u32>,
        format: Option<&str>,
        duration: Option<u32>,
    ) {
        // Update the song title label with the provided track title
        self.song_title.set_label(song_title);

        // Update the artist label with the provided artist name
        self.song_artist.set_label(song_artist);

        // Update the album title label
        self.album_title.set_label(album_title);

        // Format and update bit depth/sample rate information
        let bit_depth_sample_rate_text = match (bit_depth, sample_rate) {
            (Some(bit), Some(freq)) => {
                format!("{}-Bit/{} kHz", bit, format_sample_rate_value(freq))
            }
            (Some(bit), None) => format!("{}-Bit", bit),
            (None, Some(freq)) => format!("{} kHz", format_sample_rate_value(freq)),
            (None, None) => String::new(),
        };

        // Format information
        let format_text = format.map(|f| f.to_uppercase()).unwrap_or_default();

        // Combine bit depth/sample rate and format information with a separator
        let combined_text = if !bit_depth_sample_rate_text.is_empty() && !format_text.is_empty() {
            format!("{} · {}", bit_depth_sample_rate_text, format_text)
        } else if !bit_depth_sample_rate_text.is_empty() {
            bit_depth_sample_rate_text
        } else if !format_text.is_empty() {
            format_text
        } else {
            String::new()
        };

        // Update the bit depth/sample rate label with the combined information
        self.bit_depth_sample_rate.set_label(&combined_text);

        // Hide the separate format label since we're now combining the information
        self.format.set_visible(false);

        // Determine the label and progress range based on the duration
        let range_end = if let Some(duration_secs) = duration {
            let minutes = duration_secs / 60;
            let seconds = duration_secs % 60;

            // Note the {:02} to correctly pad seconds (e.g., 1:07)
            let duration_text = format!("{}:{:02}", minutes, seconds);
            self.time_label_end.set_label(&duration_text);
            duration_secs as f64
        } else {
            self.time_label_end.set_label("0:00");
            100.0
        };

        // Set the initial start time to 0:00
        self.time_label_start.set_label("0:00");

        // Set the progress bar range
        self.progress_bar.set_range(0.0, range_end);

        // Chain the operations: start with an optional path, then try to load from it.
        // .and_then() is perfect for this. .ok() converts the Result into an Option.
        let pixbuf =
            cover_art_path.and_then(|path| Pixbuf::from_file_at_scale(path, 96, 96, true).ok());

        // Now we have an Option<Pixbuf>. We can act on it in one place.
        if let Some(p) = pixbuf.as_ref() {
            self.album_art.set_from_pixbuf(Some(p));
        } else {
            // This single else block now handles both "no path" and "failed to load" cases.
            self.album_art.set_icon_name(Some("image-missing"));
        }

        // Make the player bar visible now that it has track information
        self.container.set_visible(true);

        // Adjust the main content area padding to prevent overlap
        if let Some(content_area) = self.main_content_area.borrow().as_ref() {
            // Use a fixed height for the margin based on the player bar's design
            // The player bar has a fixed height request of 96 pixels for the album art,
            // plus some padding from the CSS (12px top/bottom), so we'll use 120 pixels
            content_area.set_margin_bottom(120);
        }

        // Store the duration for progress calculations
        self.duration.set(range_end);
    }

    /// Updates the progress bar position and time label during playback.
    ///
    /// This method should be called periodically during playback to update
    /// the progress bar position and time display according to GNOME HIG.
    /// In a complete implementation with audio playback, this would be called
    /// by the playback system as the track progresses.
    ///
    /// # Parameters
    /// - `position`: Current playback position in seconds
    #[allow(dead_code)]
    pub fn update_progress(&self, position: f64) {
        // Update the progress bar position
        self.progress_bar.set_value(position);

        // Format the current position
        let position_minutes = (position / 60.0) as u32;
        let position_seconds = (position % 60.0) as u32;

        // Create the time label text for current position
        let position_text = format!("{}:{:02}", position_minutes, position_seconds);

        // Update the start time label
        self.time_label_start.set_label(&position_text);
    }
}
