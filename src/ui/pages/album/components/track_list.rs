use std::{collections::HashMap, sync::Arc};

use gtk4::{
    Align::{Center, End},
    Button, Label,
};
use libadwaita::{
    ActionRow, PreferencesGroup,
    glib::{MainContext, markup_escape_text},
    prelude::{ActionRowExt, ButtonExt, PreferencesGroupExt, WidgetExt},
};
use sqlx::SqlitePool;

use crate::{
    data::models::{Album, Artist, Track},
    ui::components::player_bar::PlayerBar,
    utils::formatting::{format_duration_mmss, format_sample_rate_khz},
};

/// Build a DR badge widget for an individual track's DR value.
///
/// Creates a UI component that displays the track's DR value as a color-coded badge
/// similar to the album DR badges.
///
/// # Parameters
/// - `dr_value`: The DR value of the track (if available)
///
/// # Returns
/// A GTK Label widget containing the DR badge UI element
fn build_track_dr_badge(dr_value: Option<u8>) -> Label {
    // Determine the display values based on whether DR is available
    let (dr_str, tooltip_text, css_class) = match dr_value {
        Some(value) => (
            // Format DR value as two-digit number (e.g., "08", "12")
            format!("{:02}", value),
            Some("Official Dynamic Range Value"),
            // CSS class for color coding based on DR value
            format!("dr-{:02}", value),
        ),
        None => (
            // Display "N/A" when DR value is not available
            "N/A".to_string(),
            Some("Dynamic Range Value not available"),
            // CSS class for "not available" state
            "dr-na".to_string(),
        ),
    };

    // Create the DR value label with styling
    let dr_label = Label::builder()
        .label(&dr_str)
        .halign(Center)
        .valign(Center)
        .build();

    // Set fixed size to ensure consistent layout
    dr_label.set_size_request(24, 24);

    // Apply base styling class and value-specific class
    dr_label.add_css_class("dr-badge-label");
    dr_label.add_css_class("dr-badge-label-list"); // Use existing class for list context
    dr_label.add_css_class(&css_class);

    // Set tooltip text for the DR badge
    dr_label.set_tooltip_text(tooltip_text);
    dr_label
}

/// Builds a single track row for display in the album track list.
///
/// This function creates an `ActionRow` widget that represents a single track
/// in the album view. Each row contains:
/// - Track number (disc and track number)
/// - Track title
/// - Track metadata (artist, format, bit depth, sample rate)
/// - Track duration
/// - Play button
/// - Track DR value (if show_dr_badges is enabled)
///
/// The row is designed to be added to a `PreferencesGroup` to create a complete
/// track list for an album.
///
/// # Arguments
///
/// * `t` - Reference to the `Track` model containing track information
/// * `album_artist_id` - The ID of the album's primary artist
/// * `track_artists` - A map of artist IDs to artist names for tracks with different artists
/// * `is_various_artists_album` - Whether this is a "Various Artists" compilation album
/// * `player_bar` - Reference to the application's player bar for playback control
/// * `album` - Reference to the `Album` model containing album information
/// * `artist` - Reference to the `Artist` model containing artist information
/// * `db_pool` - Database connection pool for fetching track information
/// * `show_dr_badges` - Whether to display DR badges for tracks
///
/// # Returns
///
/// Returns a configured `ActionRow` widget representing the track
pub fn build_track_row(
    t: &Track,
    album_artist_id: i64,
    track_artists: &HashMap<i64, String>,
    is_various_artists_album: bool,
    player_bar: &PlayerBar,
    album: &Album,
    _artist: &Artist,
    _db_pool: Arc<SqlitePool>,
    show_dr_badges: bool,
) -> ActionRow {
    // Prepare subtitle fields with track metadata
    let mut subtitle_fields = Vec::with_capacity(4);

    // Add track artist if different from album artist OR if it's a "Various Artists" album
    if (t.artist_id != album_artist_id || is_various_artists_album)
        && let Some(artist_name) = track_artists.get(&t.artist_id)
    {
        subtitle_fields.push(artist_name.clone());
    }

    // Add audio format if available
    if let Some(fmt) = &t.format {
        subtitle_fields.push(fmt.to_uppercase());
    }

    // Add bit depth if available
    if let Some(bit) = t.bit_depth {
        subtitle_fields.push(format!("{}-Bit", bit));
    }

    // Add sample rate if available
    if let Some(freq) = t.sample_rate {
        subtitle_fields.push(format_sample_rate_khz(freq));
    }

    // Join all subtitle fields with a separator
    let subtitle = subtitle_fields.join(" · ");

    // Create the main action row with track title and metadata
    let row = ActionRow::builder()
        .title(markup_escape_text(&t.title))
        .subtitle(markup_escape_text(&subtitle))
        .build();

    // Create and configure the track number label (disc-track format)
    let disc = t.disc_no.unwrap_or(1);
    let track = t.track_no.unwrap_or(0);
    let number_label = Label::builder()
        .label(format!("{}-{:02}", disc, track))
        .css_classes(["dim-label"])
        .xalign(0.0)
        .width_chars(5)
        .build();
    number_label.set_margin_end(16);
    row.add_prefix(&number_label);

    // Add track duration if available
    if let Some(length) = t.duration.map(format_duration_mmss) {
        let length_label = Label::builder()
            .label(&length)
            .css_classes(["dim-label"])
            .xalign(1.0)
            .build();
        length_label.set_margin_end(8);
        row.add_suffix(&length_label);
    }

    // Create the play button for this track
    let play_pause_button = Button::builder()
        .icon_name("media-playback-start")
        .css_classes(["flat"])
        .halign(End)
        .build();

    // Clone necessary values for the closure
    let player_bar_clone = player_bar.clone();
    let album_id = album.id;
    let track_id = t.id;

    // Connect the play button to queue the track and subsequent tracks
    play_pause_button.connect_clicked(move |_| {
        // Clone the player bar again for the async context
        let player_bar_async = player_bar_clone.clone();
        // Spawn async task to queue the tracks
        MainContext::default().spawn_local(async move {
            // If we have a playback controller, use it to queue the tracks
            if let Some(controller) = player_bar_async.get_playback_controller() {
                match controller.lock() {
                    Ok(mut controller) => {
                        // Queue tracks from the selected track onwards
                        if let Err(e) = controller.queue_tracks_from(album_id, track_id).await {
                            eprintln!("Error queuing tracks: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to acquire lock on playback controller: {}", e);
                    }
                }

                // Update navigation button states after queue initialization
                player_bar_async.update_navigation_button_states();
            } else {
                eprintln!("No playback controller available");
            }
        });
    });

    // Add the play button to the row
    row.add_suffix(&play_pause_button);

    // Add track DR value if available and show_dr_badges is enabled
    if show_dr_badges && let Some(dr_value) = t.dr_value {
        let dr_badge = build_track_dr_badge(Some(dr_value));
        dr_badge.set_margin_end(8);
        row.add_suffix(&dr_badge);
    }
    row
}

/// Builds a complete track list group for display on an album page.
///
/// This function creates a `PreferencesGroup` containing all tracks for an album,
/// with each track represented as an `ActionRow` created by `build_track_row`.
///
/// # Arguments
///
/// * `tracks` - Slice of `Track` models to display in the list
/// * `album` - Reference to the `Album` model containing album information
/// * `artist` - Reference to the `Artist` model containing artist information
/// * `track_artists` - A map of artist IDs to artist names for tracks with different artists
/// * `is_various_artists_album` - Whether this is a "Various Artists" compilation album
/// * `player_bar` - Reference to the application's player bar for playback control
/// * `db_pool` - Database connection pool for fetching track information
/// * `show_dr_badges` - Whether to display DR badges for tracks
///
/// # Returns
///
/// Returns a configured `PreferencesGroup` widget containing all track rows
pub fn build_track_list(
    tracks: &[Track],
    album: &Album,
    artist: &Artist,
    track_artists: &HashMap<i64, String>,
    is_various_artists_album: bool,
    player_bar: &PlayerBar,
    db_pool: Arc<SqlitePool>,
    show_dr_badges: bool,
) -> PreferencesGroup {
    // Create the main container for the track list
    let group = PreferencesGroup::builder().build();

    // Add each track as a row to the group
    for t in tracks {
        group.add(&build_track_row(
            t,
            album.artist_id,
            track_artists,
            is_various_artists_album,
            player_bar,
            album,
            artist,
            db_pool.clone(),
            show_dr_badges,
        ));
    }

    // Return the constructed track list group
    group
}
