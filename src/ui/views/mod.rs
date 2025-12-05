//! Responsive grid and list views for albums and artists.
//!
//! This module provides the main view components for displaying albums and artists
//! in both grid and list layouts, with support for responsive design, virtual
//! scrolling, and real-time filtering/sorting.

#[cfg(test)]
mod tests;

pub mod album_grid;
pub mod artist_grid;
pub mod detail_view;
pub mod list_view;

pub use {
    album_grid::AlbumGridView, artist_grid::ArtistGridView, detail_view::DetailView,
    list_view::ListView,
};
