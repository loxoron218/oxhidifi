//! Music library management system.
//!
//! This module provides the foundation for managing a music library,
//! including database operations, data models, and schema management.

pub mod database;
pub mod models;
pub mod schema;

pub use database::LibraryDatabase;
pub use models::{Album, Artist, SearchResults, Track};
pub use schema::{create_connection_pool, get_database_url, SchemaManager, CURRENT_SCHEMA_VERSION};