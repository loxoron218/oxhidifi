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
    data::models::{Album, Artist, Song},
    ui::components::player_bar::PlayerBar,
    utils::formatting::{format_duration_mmss, format_sample_rate_khz},
};

/// Build a DR badge widget for an individual song's DR value.
///
/// Creates a UI component that displays the song's DR value as a color-coded badge
/// similar to the album DR badges.
///
/// # Parameters
/// - `dr_value`: The DR value of the song (if available)
///
/// # Returns
/// A GTK Label widget containing the DR badge UI element
fn build_song_dr_badge(dr_value: Option<u8>) -> Label {
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

/// Builds a single song row for display in the album song list.
///
/// This function creates an `ActionRow` widget that represents a single song
/// in the album view. Each row contains:
/// - Song number (disc and song number)
/// - Song title
/// - Song metadata (artist, format, bit depth, sample rate)
/// - Song duration
/// - Play button
/// - Song DR value (if show_dr_badges is enabled)
///
/// The row is designed to be added to a `PreferencesGroup` to create a complete
/// song list for an album.
///
/// # Arguments
///
/// * `t` - Reference to the `Song` model containing song information
/// * `album_artist_id` - The ID of the album's primary artist
/// * `song_artists` - A map of artist IDs to artist names for songs with different artists
/// * `is_various_artists_album` - Whether this is a "Various Artists" compilation album
/// * `player_bar` - Reference to the application's player bar for playback control
/// * `album` - Reference to the `Album` model containing album information
/// * `artist` - Reference to the `Artist` model containing artist information
/// * `db_pool` - Database connection pool for fetching song information
/// * `show_dr_badges` - Whether to display DR badges for songs
///
/// # Returns
///
/// Returns a configured `ActionRow` widget representing the song
pub fn build_song_row(
    t: &Song,
    album_artist_id: i64,
    song_artists: &HashMap<i64, String>,
    is_various_artists_album: bool,
    player_bar: &PlayerBar,
    album: &Album,
    artist: &Artist,
    _db_pool: Arc<SqlitePool>,
    show_dr_badges: bool,
) -> ActionRow {
    // Prepare subtitle fields with song metadata
    let mut subtitle_fields = Vec::with_capacity(4);

    // Add song artist if different from album artist OR if it's a "Various Artists" album
    if (t.artist_id != album_artist_id || is_various_artists_album)
        && let Some(artist_name) = song_artists.get(&t.artist_id)
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

    // Create the main action row with song title and metadata
    let row = ActionRow::builder()
        .title(markup_escape_text(&t.title))
        .subtitle(markup_escape_text(&subtitle))
        .build();

    // Create and configure the song number label (disc-song format)
    let disc = t.disc_no.unwrap_or(1);
    let song = t.song_no.unwrap_or(0);
    let number_label = Label::builder()
        .label(format!("{}-{:02}", disc, song))
        .css_classes(["dim-label"])
        .xalign(0.0)
        .width_chars(5)
        .build();
    number_label.set_margin_end(16);
    row.add_prefix(&number_label);

    // Add song duration if available
    if let Some(length) = t.duration.map(format_duration_mmss) {
        let length_label = Label::builder()
            .label(&length)
            .css_classes(["dim-label"])
            .xalign(1.0)
            .build();
        length_label.set_margin_end(8);
        row.add_suffix(&length_label);
    }

    // Create the play button for this song
    let play_pause_button = Button::builder()
        .icon_name("media-playback-start")
        .css_classes(["flat"])
        .halign(End)
        .build();

    // Clone necessary values for the closure
    let player_bar_clone = player_bar.clone();
    let album_id = album.id;
    let song_id = t.id;

    // Clone song and album data to ensure it's owned by the closure
    let song_title = t.title.clone();
    let album_title = album.title.clone();
    let artist_name = artist.name.clone();
    let cover_art_path = album.cover_art.clone();
    let bit_depth = t.bit_depth;
    let sample_rate = t.sample_rate;
    let format = t.format.clone();
    let duration = t.duration;

    // Connect the play button to queue the song and subsequent songs
    play_pause_button.connect_clicked(move |_| {
        // Clone the player bar again for the async context
        let player_bar_async = player_bar_clone.clone();

        // Clone all the metadata needed for the direct update
        let album_title_clone = album_title.clone();
        let song_title_clone = song_title.clone();
        let artist_name_clone = artist_name.clone();
        let cover_art_path_clone = cover_art_path.clone();
        let bit_depth_clone = bit_depth;
        let sample_rate_clone = sample_rate;
        let format_clone = format.clone();
        let duration_clone = duration;

        // Update the player bar with metadata directly to ensure it's updated before visibility
        player_bar_clone.update_with_metadata(
            &album_title_clone,
            &song_title_clone,
            &artist_name_clone,
            cover_art_path_clone.as_deref(),
            bit_depth_clone,
            sample_rate_clone,
            format_clone.as_deref(),
            duration_clone,
        );

        // Spawn async task to queue the songs
        MainContext::default().spawn_local(async move {
            // If we have a playback controller, use it to queue the songs
            if let Some(controller) = player_bar_async.get_playback_controller() {
                let mut controller = controller.lock().await;

                // Queue songs from the selected song onwards
                if let Err(e) = controller.queue_songs_from(album_id, song_id).await {
                    eprintln!("Error queuing songs: {}", e);
                }

                // Update navigation button states after queue initialization
                player_bar_async.update_navigation_button_states();

                // Ensure the player bar is visible when playback starts
                player_bar_async.ensure_visible();
            } else {
                eprintln!("No playback controller available");
            }
        });
    });

    // Add the play button to the row
    row.add_suffix(&play_pause_button);

    // Add song DR value if available and show_dr_badges is enabled
    if show_dr_badges && let Some(dr_value) = t.dr_value {
        let dr_badge = build_song_dr_badge(Some(dr_value));
        dr_badge.set_margin_end(8);
        row.add_suffix(&dr_badge);
    }
    row
}

/// Builds a complete song list group for display on an album page.
///
/// This function creates a `PreferencesGroup` containing all songs for an album,
/// with each song represented as an `ActionRow` created by `build_song_row`.
///
/// # Arguments
///
/// * `songs` - Slice of `Song` models to display in the list
/// * `album` - Reference to the `Album` model containing album information
/// * `artist` - Reference to the `Artist` model containing artist information
/// * `song_artists` - A map of artist IDs to artist names for songs with different artists
/// * `is_various_artists_album` - Whether this is a "Various Artists" compilation album
/// * `player_bar` - Reference to the application's player bar for playback control
/// * `db_pool` - Database connection pool for fetching song information
/// * `show_dr_badges` - Whether to display DR badges for songs
///
/// # Returns
///
/// Returns a configured `PreferencesGroup` widget containing all song rows
pub fn build_song_list(
    songs: &[Song],
    album: &Album,
    artist: &Artist,
    song_artists: &HashMap<i64, String>,
    is_various_artists_album: bool,
    player_bar: &PlayerBar,
    db_pool: Arc<SqlitePool>,
    show_dr_badges: bool,
) -> PreferencesGroup {
    // Create the main container for the song list
    let group = PreferencesGroup::builder().build();

    // Add each song as a row to the group
    for t in songs {
        group.add(&build_song_row(
            t,
            album.artist_id,
            song_artists,
            is_various_artists_album,
            player_bar,
            album,
            artist,
            db_pool.clone(),
            show_dr_badges,
        ));
    }

    // Return the constructed song list group
    group
}
