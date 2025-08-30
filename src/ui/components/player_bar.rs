use std::{cell::RefCell, path::Path, rc::Rc};

use gdk_pixbuf::Pixbuf;
use glib::SignalHandlerId;
use gtk4::{
    Align::Start,
    Box, Button, Image, Label,
    Orientation::{Horizontal, Vertical},
    Scale,
    prelude::{BoxExt, ObjectExt, RangeExt, WidgetExt},
};

/// A UI component that displays currently playing track information at the bottom of the window.
///
/// The player bar is only visible when a track is playing. It shows:
/// - Album art (48x48 pixels)
/// - Song title
/// - Artist name
/// - Media controls (previous, play, next)
///
/// When no track is playing, the player bar is hidden from view.
#[derive(Clone)]
pub struct PlayerBar {
    /// The main container for all player bar elements, arranged horizontally
    pub container: Box,
    /// Display for album art, defaults to a placeholder icon when no art is available
    pub album_art: Image,
    /// Label displaying the album title
    pub album_title: Label,
    /// Label displaying the currently playing song title
    pub song_title: Label,
    /// Label displaying the artist of the currently playing song
    pub song_artist: Label,
    /// Label displaying technical information (bit depth, frequency, format)
    pub technical_info: Label,
    /// Progress bar showing current position in the track
    pub progress_bar: Scale,
    /// Label displaying current time and total duration (e.g., "1:23 / 4:56")
    pub time_label: Label,
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
}

impl PlayerBar {
    /// Creates a new PlayerBar instance with all UI elements initialized.
    ///
    /// The player bar is initially hidden and will only become visible when
    /// `update_with_metadata` is called with track information.
    ///
    /// # UI Structure
    /// The player bar layout consists of:
    /// 1. Album art (48x48 pixels) on the left
    /// 2. Track information (title and artist) in the center
    /// 3. Media controls (prev, play, next) aligned to the right
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

        // Initialize album art display with a default placeholder icon
        let album_art = Image::builder()
            .width_request(96)
            .height_request(96)
            .icon_name("image-missing")
            .build();
        container.append(&album_art);

        // Create a vertical box to hold track information
        let info_box = Box::builder().orientation(Vertical).build();
        container.append(&info_box);

        // Initialize album title label with placeholder text
        let album_title = Label::builder().label("Album Title").halign(Start).build();
        album_title.add_css_class("album-title");
        info_box.append(&album_title);

        // Initialize track title label with placeholder text
        let song_title = Label::builder().label("Song Title").halign(Start).build();
        info_box.append(&song_title);

        // Initialize artist label with placeholder text
        let song_artist = Label::builder().label("Artist").halign(Start).build();
        song_artist.add_css_class("artist-name");
        info_box.append(&song_artist);

        // Initialize technical info label with placeholder text
        let technical_info = Label::builder()
            .label("24-Bit/96 kHz FLAC")
            .halign(Start)
            .build();
        technical_info.add_css_class("technical-info");
        info_box.append(&technical_info);

        // Create a container for progress bar and time label
        let progress_box = Box::builder().orientation(Vertical).hexpand(true).build();
        container.append(&progress_box);

        // Create progress bar
        let progress_bar = Scale::builder()
            .orientation(Horizontal)
            .draw_value(false)
            .hexpand(true)
            .build();
        progress_bar.set_range(0.0, 100.0);
        progress_box.append(&progress_bar);

        // Create time label
        let time_label = Label::builder().label("0:0 / 0:00").halign(Start).build();
        time_label.add_css_class("time-label");
        progress_box.append(&time_label);

        // Create a container for media control buttons
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

        // Add previous track button with standard media icon
        let prev_button = Button::from_icon_name("media-skip-backward");
        prev_button.add_css_class("media-button");
        controls_box.append(&prev_button);

        // Add play button with standard media icon
        let play_button = Button::from_icon_name("media-playback-start");
        play_button.add_css_class("media-button");
        controls_box.append(&play_button);

        // Add next track button with standard media icon
        let next_button = Button::from_icon_name("media-skip-forward");
        next_button.add_css_class("media-button");
        controls_box.append(&next_button);

        // Initially hide the player bar until a track is played
        container.set_visible(false);

        // Construct and return the PlayerBar instance with all initialized components
        // This struct initialization makes all the UI components accessible to the caller
        Self {
            container,
            album_art,
            album_title,
            song_title,
            song_artist,
            technical_info,
            progress_bar,
            time_label,
            _volume_slider: volume_slider,
            _bit_perfect_indicator: bit_perfect_indicator,
            _gapless_indicator: gapless_indicator,
            _prev_button: prev_button,
            _play_button: play_button,
            _next_button: next_button,
            main_content_area: Rc::new(RefCell::new(None)),
            visibility_handler_id: None,
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
    /// in the player bar at the bottom of the window.
    ///
    /// # Parameters
    /// - `album_title`: The title of the album
    /// - `song_title`: The title of the currently playing track
    /// - `song_artist`: The artist of the currently playing track
    /// - `cover_art_path`: Optional path to the album art image file
    /// - `bit_depth`: Optional bit depth of the audio file
    /// - `frequency`: Optional frequency of the audio file
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
        frequency: Option<u32>,
        format: Option<&str>,
        duration: Option<u32>,
    ) {
        // Update the album title label
        self.album_title.set_label(album_title);

        // Update the song title label with the provided track title
        self.song_title.set_label(song_title);

        // Update the artist label with the provided artist name
        self.song_artist.set_label(song_artist);

        // Format and update technical information
        let technical_text = match (bit_depth, frequency, format) {
            (Some(bit), Some(freq), Some(fmt)) => {
                format!("{}-Bit/{} kHz {}", bit, freq / 1000, fmt.to_uppercase())
            }
            (Some(bit), None, Some(fmt)) => format!("{}-Bit {}", bit, fmt.to_uppercase()),
            (None, Some(freq), Some(fmt)) => {
                format!("{} kHz {}", freq / 1000, fmt.to_uppercase())
            }
            (None, None, Some(fmt)) => fmt.to_uppercase(),
            _ => String::new(),
        };
        self.technical_info.set_label(&technical_text);

        // Determine the label and progress range based on the duration
        let (label, range_end) = if let Some(duration_secs) = duration {
            let minutes = duration_secs / 60;
            let seconds = duration_secs % 60;
            // Note the {:02} to correctly pad seconds (e.g., 1:07)
            (
                format!("0:00 / {}:{:02}", minutes, seconds),
                duration_secs as f64,
            )
        } else {
            ("0:00 / 0:0".to_string(), 100.0)
        };

        // Now, perform the UI updates once
        self.time_label.set_label(&label);
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
    }
}
