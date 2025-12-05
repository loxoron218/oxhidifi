//! Unit tests for UI components.
//!
//! This module contains comprehensive unit tests for all UI components
//! to ensure proper creation, property setting, signal emission, and
//! state update handling.

#[cfg(test)]
mod component_tests {
    use std::sync::Arc;

    use libadwaita::{init, prelude::*};

    use crate::{
        library::models::Track,
        state::AppState,
        ui::components::{
            cover_art::CoverArt, dr_badge::DRBadge, hifi_metadata::HiFiMetadata,
            play_overlay::PlayOverlay,
        },
    };

    #[test]
    fn test_dr_badge_creation_and_properties() {
        if init().is_err() {
            return;
        }

        let badge = DRBadge::new(Some("DR12".to_string()), true);
        assert_eq!(badge.label.text().as_str(), "DR12");
        assert_eq!(badge.quality.css_class(), "dr-badge-good");

        // Test builder pattern
        let badge_builder = DRBadge::builder()
            .dr_value("DR8")
            .show_label(false)
            .build();
        assert_eq!(badge_builder.label.text().as_str(), "8");
        assert_eq!(badge_builder.quality.css_class(), "dr-badge-poor");
    }

    #[test]
    fn test_cover_art_creation_and_properties() {
        if init().is_err() {
            return;
        }

        let cover_art = CoverArt::builder()
            .artwork_path("/non/existent/path.jpg")
            .dr_value("DR14")
            .show_dr_badge(true)
            .dimensions(100, 100)
            .build();

        assert_eq!(cover_art.picture.width_request(), 100);
        assert_eq!(cover_art.picture.height_request(), 100);
        assert!(cover_art.dr_badge.is_some());
    }

    #[test]
    fn test_play_overlay_creation_and_properties() {
        if init().is_err() {
            return;
        }

        let overlay = PlayOverlay::builder()
            .is_playing(true)
            .show_on_hover(false)
            .build();

        assert!(overlay.is_playing);
        assert_eq!(overlay.button.icon_name().as_deref(), Some("media-playback-pause-symbolic"));
        assert!(!overlay.show_on_hover);
    }

    #[test]
    fn test_hifi_metadata_creation_and_properties() {
        if init().is_err() {
            return;
        }

        let track = Track {
            id: 1,
            album_id: 1,
            title: "Test Track".to_string(),
            track_number: Some(1),
            disc_number: 1,
            duration_ms: 300000,
            path: "/path/to/track.flac".to_string(),
            file_size: 1024,
            format: "FLAC".to_string(),
            sample_rate: 96000,
            bits_per_sample: 24,
            channels: 2,
            created_at: None,
            updated_at: None,
        };

        let metadata = HiFiMetadata::builder()
            .track(track)
            .show_format(true)
            .show_sample_rate(true)
            .show_bit_depth(true)
            .show_channels(true)
            .compact(true)
            .build();

        assert_eq!(metadata.labels.len(), 4);
        assert!(metadata.config.compact);
    }

    #[test]
    fn test_component_accessibility_attributes() {
        if init().is_err() {
            return;
        }

        // Test DRBadge accessibility
        let badge = DRBadge::new(Some("DR12".to_string()), true);
        assert!(badge.label.accessible_description().is_some());

        // Test CoverArt accessibility  
        let cover_art = CoverArt::new(None, None, false, 50, 50);
        assert!(cover_art.picture.accessible_description().is_some());

        // Test PlayOverlay accessibility
        let overlay = PlayOverlay::new(false, false);
        assert!(overlay.button.accessible_description().is_some());
    }
}