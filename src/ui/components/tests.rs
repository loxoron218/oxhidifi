//! Unit tests for UI components.
//!
//! This module contains comprehensive unit tests for all UI components
//! to ensure proper creation, property setting, signal emission, and
//! state update handling.

#[cfg(test)]
mod component_tests {
    use libadwaita::{
        gtk::AccessibleRole::None as AccessibleNone,
        prelude::{AccessibleExt, ButtonExt, WidgetExt},
    };

    use crate::{
        library::models::Track,
        ui::components::{
            cover_art::CoverArt, dr_badge::DRBadge, hifi_metadata::HiFiMetadata,
            play_overlay::PlayOverlay,
        },
    };

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_dr_badge_creation_and_properties() {
        let badge = DRBadge::new(Some("DR12".to_string()), true);
        assert_eq!(badge.label.text().as_str(), "DR12");
        assert_eq!(badge.quality.css_class(), "dr-badge-good");

        // Test builder pattern
        let badge_builder = DRBadge::builder().dr_value("DR8").show_label(false).build();
        assert_eq!(badge_builder.label.text().as_str(), "8");
        assert_eq!(badge_builder.quality.css_class(), "dr-badge-poor");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_cover_art_creation_and_properties() {
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
    #[ignore = "Requires GTK display for UI testing"]
    fn test_play_overlay_creation_and_properties() {
        let overlay = PlayOverlay::builder()
            .is_playing(true)
            .show_on_hover(false)
            .build();

        assert!(overlay.is_playing);
        assert_eq!(
            overlay.button.icon_name().as_deref(),
            Some("media-playback-pause-symbolic")
        );
        assert!(!overlay.show_on_hover);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_hifi_metadata_creation_and_properties() {
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
    #[ignore = "Requires GTK display for UI testing"]
    fn test_component_accessibility_attributes() {
        // Test DRBadge accessibility
        let badge = DRBadge::new(Some("DR12".to_string()), true);
        assert!(badge.label.accessible_role() != AccessibleNone);

        // Test CoverArt accessibility
        let cover_art = CoverArt::new(Option::None, Option::None, false, 50, 50);
        assert!(cover_art.picture.accessible_role() != AccessibleNone);

        // Test PlayOverlay accessibility
        let overlay = PlayOverlay::new(false, false);
        assert!(overlay.button.accessible_role() != AccessibleNone);
    }
}
