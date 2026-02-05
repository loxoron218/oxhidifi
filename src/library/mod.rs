//! Music library management system.
//!
//! This module provides the foundation for managing a music library,
//! including database operations and data models.

pub mod connection;
pub mod database;
pub mod dr_parser;
pub mod file_watcher;
pub mod incremental_updater;
pub mod models;
pub mod scanner;

pub use {
    connection::{create_connection_pool, get_database_url},
    database::LibraryDatabase,
    models::{Album, Artist, SearchResults, Track},
};
