//! Responsive grid and list views for albums and artists.
//!
//! This module provides the main view components for displaying albums and artists
//! in both grid and list layouts, with support for responsive design, virtual
//! scrolling, and real-time filtering/sorting.

#[cfg(test)]
mod tests;

pub mod album_columns;
pub mod album_columns_macro;
pub mod album_columns_text;
pub mod album_detail_renderer;
pub mod album_grid;
pub mod artist_columns;
pub mod artist_detail_renderer;
pub mod artist_grid;
pub mod column_sorting;
pub mod column_view;
pub mod column_view_builder;
pub mod column_view_subscriptions;
pub mod column_view_types;
pub mod column_view_updates;
pub mod detail_playback;
pub mod detail_types;
pub mod detail_view;
pub mod filtering;
pub mod search_results_view;
pub mod search_results_view_builder;
pub mod search_results_view_methods;
pub mod search_results_view_populate_albums;
pub mod search_results_view_populate_artists;
pub mod search_results_view_populate_songs;
pub mod search_results_view_population;
pub mod search_results_view_subscriptions;
pub mod search_song_columns;
pub mod ui_macros;
