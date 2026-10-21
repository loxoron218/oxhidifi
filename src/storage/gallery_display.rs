//! Gallery display flags for user settings.

use serde::{Deserialize, Serialize};

/// Gallery display flags.
///
/// Flattened into [`UserSettings`](crate::storage::settings::UserSettings) so
/// `settings.json` keeps its flat shape (`show_album_labels`,
/// `artist_albums_collapsed`). Grouping the gallery bools here keeps each
/// settings struct under the `struct_excessive_bools` limit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct GalleryDisplay {
    /// Whether to show album title/artist/format labels under cover art.
    pub show_album_labels: bool,
    /// Whether album sections on artist detail pages start collapsed.
    pub artist_albums_collapsed: bool,
}

impl Default for GalleryDisplay {
    fn default() -> Self {
        Self {
            show_album_labels: true,
            artist_albums_collapsed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        serde_json::{from_str, to_string_pretty},
    };

    use crate::storage::gallery_display::GalleryDisplay;

    #[test]
    fn gallery_display_defaults_to_labeled_expanded() -> Result<()> {
        let state = GalleryDisplay::default();
        ensure!(
            state.show_album_labels && !state.artist_albums_collapsed,
            "default gallery must show labels with expanded sections"
        );
        Ok(())
    }

    #[test]
    fn gallery_display_serializes_with_legacy_flat_keys() -> Result<()> {
        let original = GalleryDisplay {
            show_album_labels: false,
            artist_albums_collapsed: true,
        };
        let json = to_string_pretty(&original)?;
        ensure!(
            json.contains("\"show_album_labels\"") && json.contains("\"artist_albums_collapsed\""),
            "gallery display must keep the legacy flat key names"
        );
        ensure!(
            !json.contains("\"display\":"),
            "gallery display must not nest under a display object"
        );
        let restored: GalleryDisplay = from_str(&json)?;
        ensure!(
            !restored.show_album_labels && restored.artist_albums_collapsed,
            "gallery display must round-trip through JSON"
        );
        Ok(())
    }
}
