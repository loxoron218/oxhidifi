//! Integration tests for UI views and navigation.
//!
//! This module contains integration tests for view transitions, navigation,
//! real-time filtering, sorting, and player bar control functionality.

#[cfg(test)]
mod view_integration_tests {
    use std::sync::Arc;

    use {
        libadwaita::{
            gtk::AccessibleRole::{Grid, List},
            prelude::AccessibleExt,
        },
        parking_lot::RwLock,
    };

    use crate::{
        AppState, AudioEngine,
        config::SettingsManager,
        library::models::{Album, Artist},
        ui::{
            components::cover_art::CoverArt,
            views::{
                AlbumGridView, ArtistGridView, DetailView, ListView,
                album_grid::AlbumSortCriteria::{Title, Year},
                detail_view::DetailType,
                list_view::ListViewType::{Albums, Artists},
            },
        },
    };

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_view_transitions_and_navigation() {
        let engine = AudioEngine::new().unwrap();
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new().unwrap();
        let app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        // Test album grid view creation
        let _album_grid = AlbumGridView::builder()
            .app_state(Arc::new(app_state.clone()))
            .albums(Vec::new())
            .show_dr_badges(true)
            .compact(false)
            .build();

        // Test artist grid view creation
        let _artist_grid = ArtistGridView::builder()
            .app_state(Arc::new(app_state.clone()))
            .artists(Vec::new())
            .compact(false)
            .build();

        // Test list view creation for albums
        let _album_list = ListView::builder()
            .app_state(Arc::new(app_state.clone()))
            .view_type(Albums)
            .compact(false)
            .build();

        // Test list view creation for artists
        let _artist_list = ListView::builder()
            .app_state(Arc::new(app_state.clone()))
            .view_type(Artists)
            .compact(false)
            .build();

        // Test detail view creation for album
        let album = Album::default();
        let _album_detail = DetailView::builder()
            .app_state(Arc::new(app_state.clone()))
            .detail_type(DetailType::Album(album))
            .compact(false)
            .build();

        // Test detail view creation for artist
        let artist = Artist::default();
        let _artist_detail = DetailView::builder()
            .app_state(Arc::new(app_state.clone()))
            .detail_type(DetailType::Artist(artist))
            .compact(false)
            .build();
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_real_time_filtering_and_sorting() {
        let engine = AudioEngine::new().unwrap();
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new().unwrap();
        let app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        let albums = vec![
            Album {
                id: 1,
                artist_id: 1,
                title: "Test Album A".to_string(),
                year: Some(2023),
                ..Album::default()
            },
            Album {
                id: 2,
                artist_id: 2,
                title: "Test Album B".to_string(),
                year: Some(2022),
                ..Album::default()
            },
        ];

        let mut album_grid = AlbumGridView::builder()
            .app_state(Arc::new(app_state.clone()))
            .albums(albums.clone())
            .show_dr_badges(true)
            .compact(false)
            .build();

        // Test filtering
        album_grid.filter_albums("A");

        // In real implementation, this would verify the filtered results

        // Test sorting by title
        album_grid.sort_albums(Title);

        // Test sorting by year
        album_grid.sort_albums(Year);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_keyboard_navigation_support() {
        let engine = AudioEngine::new().unwrap();
        let engine_weak = Arc::downgrade(&Arc::new(engine));
        let settings_manager = SettingsManager::new().unwrap();
        let app_state = AppState::new(engine_weak, None, Arc::new(RwLock::new(settings_manager)));

        // Test that views support keyboard navigation
        let app_state_arc = Arc::new(app_state.clone());
        let _album_grid = AlbumGridView::new(
            Some(&app_state_arc),
            None,
            None,
            None,
            Vec::new(),
            true,
            false,
        );

        let artist_grid = ArtistGridView::new(Some(Arc::new(app_state.clone())), Vec::new(), false);
        assert_eq!(artist_grid.flow_box.accessible_role(), Grid);

        let album_list = ListView::new(Some(&app_state.into()), &Albums, false);
        assert_eq!(album_list.list_box.accessible_role(), List);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_cover_art_dr_badge_methods() {
        // Create a CoverArt instance
        let mut cover_art = CoverArt::new(
            Some(&"/path/to/artwork.jpg".to_string()),
            None,
            Some("DR12".to_string()),
            true, // Initially show DR badge
            200,
            200,
        );

        // Test that the method can be called without panic
        cover_art.set_show_dr_badge(false);
        cover_art.set_show_dr_badge(true);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_cover_art_edge_cases() {
        // Test with no DR value
        let mut cover_art = CoverArt::new(None, None, None, false, 100, 100);

        // Should not panic when updating DR value
        cover_art.update_dr_value(Some("DR8".to_string()));
        cover_art.update_dr_value(None);

        // Should not panic when toggling visibility
        cover_art.set_show_dr_badge(true);
        cover_art.set_show_dr_badge(false);
    }
}
