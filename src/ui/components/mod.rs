//! Reusable UI components following GNOME HIG guidelines.
//!
//! This module provides composable, accessible UI components that can be
//! used throughout the application to maintain consistency and reduce code duplication.

#[cfg(test)]
mod tests;

pub mod cover_art;
pub mod dr_badge;
pub mod empty_state;
pub mod hifi_metadata;
pub mod play_overlay;

pub use {
    cover_art::CoverArt, dr_badge::DRBadge, empty_state::EmptyState, hifi_metadata::HiFiMetadata,
    play_overlay::PlayOverlay,
};
