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
            codec: "FLAC".to_string(),
            sample_rate: 96000,
            bits_per_sample: 24,
            channels: 2,
            is_lossless: true,
            is_high_resolution: true,
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
    fn test_hifi_metadata_sample_rate_decimal_formatting() {
        // Test 44.1 kHz sample rate
        let track_441 = Track {
            id: 1,
            album_id: 1,
            title: "Test Track 44.1".to_string(),
            track_number: Some(1),
            disc_number: 1,
            duration_ms: 300000,
            path: "/path/to/track_441.flac".to_string(),
            file_size: 1024,
            format: "FLAC".to_string(),
            codec: "FLAC".to_string(),
            sample_rate: 44100,
            bits_per_sample: 24,
            channels: 2,
            is_lossless: true,
            is_high_resolution: true,
            created_at: None,
            updated_at: None,
        };

        let metadata_441 = HiFiMetadata::builder()
            .track(track_441)
            .show_sample_rate(true)
            .compact(true)
            .build();

        // The label should contain "44.1 kHz"
        assert_eq!(metadata_441.labels.len(), 1);
        let label_text = metadata_441.labels[0].text().to_string();
        assert!(
            label_text.contains("44.1 kHz"),
            "Expected '44.1 kHz' but got '{}'",
            label_text
        );

        // Test 88.2 kHz sample rate
        let track_882 = Track {
            sample_rate: 88200,
            ..Track::default()
        };

        let metadata_882 = HiFiMetadata::new(
            Some(track_882),
            false, // show_format
            true,  // show_sample_rate
            false, // show_bit_depth
            false, // show_channels
            true,  // compact
        );

        let label_text_882 = metadata_882.labels[0].text().to_string();
        assert!(
            label_text_882.contains("88.2 kHz"),
            "Expected '88.2 kHz' but got '{}'",
            label_text_882
        );

        // Test 96 kHz (whole number) sample rate
        let track_96 = Track {
            sample_rate: 96000,
            ..Track::default()
        };

        let metadata_96 = HiFiMetadata::new(Some(track_96), false, true, false, false, true);

        let label_text_96 = metadata_96.labels[0].text().to_string();
        assert!(
            label_text_96.contains("96 kHz"),
            "Expected '96 kHz' but got '{}'",
            label_text_96
        );
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
