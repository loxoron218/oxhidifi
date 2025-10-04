use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk4::{
    Align::{End, Start},
    Box, Button, Image, Label,
    Orientation::{Horizontal, Vertical},
    Scale,
    glib::{MainContext, Propagation::Proceed, object::Cast},
    pango::EllipsizeMode,
};
use libadwaita::prelude::{BoxExt, ObjectExt, RangeExt, WidgetExt};
use tokio_util::sync::CancellationToken;

use super::{PlayerBar, PlayerBarWeak};

impl PlayerBar {
    /// Creates a new PlayerBar instance with all UI elements initialized.
    ///
    /// The player bar is initially hidden and will only become visible when
    /// `update_with_metadata` is called with song information.
    ///
    /// # UI Structure
    /// The player bar layout consists of:
    /// 1. Album art (96x96 pixels) on the left
    /// 2. Song information (song title, artist, album title, bit depth/sample rate, format) in a fixed-width container in the center
    /// 3. Progress bar
    /// 4. Time labels (start and end) with play controls between them
    /// 5. Additional controls (volume, indicators) aligned to the right
    ///
    /// The song information container has a fixed width of 300 pixels and automatically ellipsizes
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

        // Create a vertical box to hold song information
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

        // Initialize song title label with placeholder text
        let song_title = Label::builder().label("Song Title").halign(Start).build();
        song_title.add_css_class("song-title");

        // Apply ellipsizing to prevent text overflow
        song_title.set_ellipsize(EllipsizeMode::End);
        song_title.set_xalign(0.0);

        // Prevent horizontal expansion and set max width chars for ellipsizing
        song_title.set_hexpand(false);
        song_title.set_max_width_chars(25);

        // Set up tooltip to show full text when ellipsized
        setup_ellipsized_tooltip(&song_title);
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

        // Set up tooltip to show full text when ellipsized
        setup_ellipsized_tooltip(&song_artist);
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

        // Set up tooltip to show full text when ellipsized
        setup_ellipsized_tooltip(&album_title);
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

        // Set up tooltip to show full text when ellipsized
        setup_ellipsized_tooltip(&bit_depth_sample_rate);
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

        // Set up tooltip to show full text when ellipsized
        setup_ellipsized_tooltip(&format);
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
        progress_bar.set_range(0.0, 10.0);
        progress_box.append(&progress_bar);

        // Connect change event to seek functionality
        // Create a cell to store a weak reference to self
        let player_bar_ref: Rc<RefCell<Option<PlayerBarWeak>>> = Rc::new(RefCell::new(None));
        let player_bar_ref_clone = player_bar_ref.clone();
        progress_bar.connect_change_value(move |_scale, _, value| {
            // Convert value to nanoseconds for seeking
            let position_ns = (value * 1_000_000.0) as u64;

            // Get the player bar reference and seek
            if let Some(player_bar_weak) = &*player_bar_ref_clone.borrow()
                && let Some(player_bar) = player_bar_weak.upgrade()
                && let Some(controller) = &player_bar.playback_controller
            {
                let controller_clone = controller.clone();
                MainContext::default().spawn_local(async move {
                    let mut controller = controller_clone.lock().await;
                    if let Err(e) = controller.seek(position_ns) {
                        eprintln!("Error seeking to position: {}", e);
                    }
                });
            }
            Proceed
        });

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

        // Add previous song button with standard media icon
        let prev_button = Button::from_icon_name("media-skip-backward");
        prev_button.add_css_class("media-button");
        play_controls_box.append(&prev_button);

        // Add play button with standard media icon
        let play_button = Button::from_icon_name("media-playback-start");
        play_button.add_css_class("media-button");
        play_controls_box.append(&play_button);

        // Add next song button with standard media icon
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
        volume_slider.set_range(0.0, 10.0);
        volume_slider.set_value(10.0);
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

        // Initially hide the player bar until a song is played
        container.set_visible(false);

        // Construct and return the PlayerBar instance with all initialized components
        // This struct initialization makes all the UI components accessible to the caller
        let player_bar = Self {
            container,
            album_art,
            song_title,
            song_artist,
            album_title,
            bit_depth_sample_rate,
            format,
            progress_bar: progress_bar.clone(),
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
            playback_controller: None,
            can_go_prev: Cell::new(false),
            can_go_next: Cell::new(false),
            cancellation_token: CancellationToken::new(),
        };

        // Store a weak reference to self in the progress bar handler
        *player_bar_ref.borrow_mut() = Some(player_bar.downgrade());
        player_bar
    }
}

/// Sets up a tooltip for a label that shows the full text on hover
///
/// This function connects to the label's notify::label signal to update the tooltip
/// text when the label content changes. The tooltip will show the full text
/// when the user hovers over the label, which is useful when the text is ellipsized.
fn setup_ellipsized_tooltip(label: &Label) {
    // Initially set the tooltip to the label's text
    let initial_text = label.text();
    label.set_tooltip_text(Some(&initial_text));

    // Connect to the label text change to update the tooltip
    label.connect_notify_local(Some("label"), {
        move |label_obj, _| {
            // Get the actual label widget from the object
            if let Some(label_widget) = label_obj.downcast_ref::<Label>() {
                let text = label_widget.text();

                // Set the tooltip to the current text
                label_widget.set_tooltip_text(Some(&text));
            }
        }
    });
}
