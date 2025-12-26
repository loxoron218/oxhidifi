//! GNOME HIG-compliant user interface built entirely with Libadwaita.
//!
//! This module provides the foundation for the Oxhidifi user interface,
//! including the main application window, header bar, and player controls.

#[cfg(test)]
mod tests;

pub mod application;
pub mod components;
pub mod header_bar;
pub mod player_bar;
pub mod utils;
pub mod views;

pub use {
    application::OxhidifiApplication,
    components::{CoverArt, DRBadge, HiFiMetadata, PlayOverlay},
    header_bar::HeaderBar,
    player_bar::PlayerBar,
    views::{AlbumGridView, ArtistGridView, DetailView, ListView},
};
