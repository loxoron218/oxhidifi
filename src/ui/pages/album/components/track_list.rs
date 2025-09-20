use std::{collections::HashMap, path::Path};

use gtk4::{Align::End, Button, Label};
use libadwaita::{
    ActionRow, PreferencesGroup,
    glib::markup_escape_text,
    prelude::{ActionRowExt, ButtonExt, PreferencesGroupExt, WidgetExt},
};

use crate::{
    data::models::{Album, Artist, Track},
    ui::components::player_bar::PlayerBar,
    utils::formatting::{format_duration_mmss, format_sample_rate_khz},
};

/// Builds a single track row for display in the album track list.
///
/// This function creates an `ActionRow` widget that represents a single track
/// in the album view. Each row contains:
/// - Track number (disc and track number)
/// - Track title
/// - Track metadata (artist, format, bit depth, sample rate)
/// - Track duration
/// - Play button
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
    artist: &Artist,
) -> ActionRow {
    // Prepare subtitle fields with track metadata
    let mut subtitle_fields = Vec::with_capacity(4);

    // Add track artist if different from album artist OR if it's a "Various Artists" album
    if t.artist_id != album_artist_id || is_various_artists_album {
        if let Some(artist_name) = track_artists.get(&t.artist_id) {
            subtitle_fields.push(artist_name.clone());
        }
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
        .label(&format!("{}-{:02}", disc, track))
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
    let album_clone = album.clone();
    let track_title = t.title.clone();

    // Determine the artist name to display for this track
    // Start with the album artist's name as the default
    let mut artist_name = artist.name.clone();

    // If this is a "various artists" situation, try to find a more specific artist
    if t.artist_id != album_artist_id || is_various_artists_album {
        if let Some(specific_artist_name) = track_artists.get(&t.artist_id) {
            // If found, override the default with the specific track artist
            artist_name = specific_artist_name.clone();
        }
    }

    // Clone track values to move into closure
    let track_bit_depth = t.bit_depth;
    let track_sample_rate = t.sample_rate;
    let track_format = t.format.clone();
    let track_duration = t.duration;
    let track_path = t.path.clone();

    // Connect the play button to load and play the track
    play_pause_button.connect_clicked(move |_| {
        // Update the player bar with track metadata
        player_bar_clone.update_with_metadata(
            &album_clone.title,
            &track_title,
            &artist_name,
            album_clone.cover_art.as_deref(),
            track_bit_depth,
            track_sample_rate,
            track_format.as_deref(),
            track_duration,
        );

        // Load and play the track
        player_bar_clone.load_and_play_track(Path::new(&track_path));
    });

    // Add the play button to the row
    row.add_suffix(&play_pause_button);
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
        ));
    }

    // Return the constructed track list group
    group
}
