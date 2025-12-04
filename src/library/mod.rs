//! Music library management system.
//!
//! This module provides the foundation for managing a music library,
//! including database operations, data models, and schema management.

pub mod database;
pub mod dr_parser;
pub mod file_watcher;
pub mod incremental_updater;
pub mod models;
pub mod scanner;
pub mod schema;

pub use {
    database::LibraryDatabase,
    models::{Album, Artist, SearchResults, Track},
    schema::{CURRENT_SCHEMA_VERSION, SchemaManager, create_connection_pool, get_database_url},
};
