//! User-facing settings data model persisted as JSON.

use serde::{Deserialize, Serialize};

use crate::{
    playback::devices::OutputMode::{self, Resampled},
    storage::{
        settings::{ActiveTab, ViewMode},
        sort_rules::{AlbumSortItem, ArtistSortItem, default_albums_sort, default_artists_sort},
    },
    zoom::{DEFAULT_GRID_ZOOM, DEFAULT_LIST_ZOOM},
};

/// Persistent user settings stored as JSON at XDG config path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserSettings {
    /// Preferred audio output device name (None = default).
    pub audio_device: Option<String>,
    /// Playback volume (0.0–1.0).
    pub volume: f64,
    /// Current view mode preference.
    pub view_mode: ViewMode,
    /// Last active tab.
    pub active_tab: ActiveTab,
    /// Stored window width.
    pub window_width: i32,
    /// Stored window height.
    pub window_height: i32,
    /// Whether window is maximized.
    pub window_maximized: bool,
    /// Whether gapless playback is enabled.
    pub gapless_enabled: bool,
    /// Whether to show album title/artist/format labels under cover art.
    pub show_album_labels: bool,
    /// Output mode: resampled (software volume) or bit-perfect (hardware volume).
    pub output_mode: OutputMode,
    /// Track IDs from the last playback session (for queue restoration).
    pub last_queue: Vec<i64>,
    /// Index into `last_queue` for the track that was playing.
    pub last_queue_index: Option<usize>,
    /// Track ID that was playing when the session ended.
    pub last_track_id: Option<i64>,
    /// Elapsed seconds in the last track.
    pub last_position: f64,
    /// Duration of the last track (for validation).
    pub last_duration: f64,
    /// Sort criteria for albums grid view.
    pub albums_sort: Vec<AlbumSortItem>,
    /// Sort criteria for artists grid view.
    pub artists_sort: Vec<ArtistSortItem>,
    /// Grid view zoom level (0–4).
    pub grid_zoom_level: u8,
    /// List view zoom level (0–2).
    pub list_zoom_level: u8,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            audio_device: None,
            volume: 1.0,
            view_mode: ViewMode::Grid,
            active_tab: ActiveTab::Albums,
            window_width: 1200,
            window_height: 800,
            window_maximized: false,
            gapless_enabled: true,
            show_album_labels: true,
            output_mode: Resampled,
            last_queue: Vec::new(),
            last_queue_index: None,
            last_track_id: None,
            last_position: 0.0,
            last_duration: 0.0,
            albums_sort: default_albums_sort(),
            artists_sort: default_artists_sort(),
            grid_zoom_level: DEFAULT_GRID_ZOOM,
            list_zoom_level: DEFAULT_LIST_ZOOM,
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        serde_json::{from_str, to_string_pretty},
    };

    use crate::{
        playback::devices::OutputMode::BitPerfect,
        storage::{
            settings::{ActiveTab::Artists, ViewMode::Column},
            sort_rules::{
                AlbumSortCriteria::{BitDepth, Title},
                AlbumSortItem,
                ArtistSortCriteria::Name,
                ArtistSortItem,
                SortOrder::{Ascending, Descending},
                default_albums_sort,
            },
            user_settings::{UserSettings, default_artists_sort},
        },
        zoom::{DEFAULT_GRID_ZOOM, DEFAULT_LIST_ZOOM},
    };

    fn custom_settings() -> UserSettings {
        UserSettings {
            audio_device: Some("hw:0".to_string()),
            volume: 0.25,
            view_mode: Column,
            active_tab: Artists,
            window_width: 900,
            window_height: 600,
            window_maximized: true,
            gapless_enabled: false,
            show_album_labels: false,
            output_mode: BitPerfect,
            last_queue: vec![1, 2, 3],
            last_queue_index: Some(1),
            last_track_id: Some(2),
            last_position: 42.5,
            last_duration: 200.0,
            albums_sort: vec![
                AlbumSortItem {
                    criteria: Title,
                    order: Descending,
                },
                AlbumSortItem {
                    criteria: BitDepth,
                    order: Ascending,
                },
            ],
            artists_sort: vec![ArtistSortItem {
                criteria: Name,
                order: Descending,
            }],
            grid_zoom_level: 3,
            list_zoom_level: 2,
        }
    }

    #[test]
    fn full_round_trip_preserves_every_field() -> Result<()> {
        let original = custom_settings();
        let json = to_string_pretty(&original)?;
        let restored: UserSettings = from_str(&json)?;

        ensure!(restored.audio_device == original.audio_device);
        ensure!((restored.volume - original.volume).abs() < f64::EPSILON);
        ensure!(restored.view_mode == original.view_mode);
        ensure!(restored.active_tab == original.active_tab);
        ensure!(restored.window_width == original.window_width);
        ensure!(restored.window_height == original.window_height);
        ensure!(restored.window_maximized == original.window_maximized);
        ensure!(restored.gapless_enabled == original.gapless_enabled);
        ensure!(restored.show_album_labels == original.show_album_labels);
        ensure!(restored.output_mode == original.output_mode);
        ensure!(restored.last_queue == original.last_queue);
        ensure!(restored.last_queue_index == original.last_queue_index);
        ensure!(restored.last_track_id == original.last_track_id);
        ensure!((restored.last_position - original.last_position).abs() < f64::EPSILON);
        ensure!((restored.last_duration - original.last_duration).abs() < f64::EPSILON);
        ensure!(restored.albums_sort == original.albums_sort);
        ensure!(restored.artists_sort == original.artists_sort);
        ensure!(restored.grid_zoom_level == original.grid_zoom_level);
        ensure!(restored.list_zoom_level == original.list_zoom_level);
        Ok(())
    }

    #[test]
    fn empty_document_parses_to_defaults() -> Result<()> {
        let settings: UserSettings = from_str("{}")?;
        ensure!(
            settings.albums_sort == default_albums_sort(),
            "missing albums_sort must fall back to defaults"
        );
        ensure!(
            settings.artists_sort == default_artists_sort(),
            "missing artists_sort must fall back to defaults"
        );
        ensure!(settings.grid_zoom_level == DEFAULT_GRID_ZOOM);
        ensure!(settings.list_zoom_level == DEFAULT_LIST_ZOOM);
        ensure!((settings.volume - 1.0).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn legacy_document_without_new_fields_parses_to_defaults() -> Result<()> {
        let legacy = r#"{
            "audio_device": "default",
            "volume": 0.5,
            "view_mode": "Grid",
            "active_tab": "Albums",
            "window_width": 1200,
            "window_height": 800,
            "window_maximized": false,
            "gapless_enabled": true,
            "show_album_labels": true,
            "output_mode": "resampled"
        }"#;
        let settings: UserSettings = from_str(legacy)?;
        ensure!(settings.audio_device.as_deref() == Some("default"));
        ensure!(
            settings.albums_sort == default_albums_sort(),
            "a legacy file without sort fields must use the default sort"
        );
        ensure!(settings.grid_zoom_level == DEFAULT_GRID_ZOOM);
        Ok(())
    }
}
