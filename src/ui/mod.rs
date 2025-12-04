//! GNOME HIG-compliant user interface built entirely with Libadwaita.
//!
//! This module provides the foundation for the Oxhidifi user interface,
//! including the main application window, header bar, and player controls.

pub mod application;
pub mod header_bar;
pub mod player_bar;

pub use {application::OxhidifiApplication, header_bar::HeaderBar, player_bar::PlayerBar};
