use std::sync::Arc;

use sqlx::{Result, SqlitePool};
use tokio::spawn;

use crate::{
    data::db::{dr_sync::synchronize_dr_completed_background, query::fetch_album_display_info},
    ui::grids::album_grid_state::AlbumGridItem,
    utils::best_dr_persistence::DrValueStore,
};

/// Fetches and processes album data from the database.
///
/// This function fetches album display information from the database,
/// synchronizes DR completed status, and returns the processed data.
///
/// # Arguments
/// * `db_pool` - An `Arc<SqlitePool>` for database access.
///
/// # Returns
/// A `Result` containing a vector of `AlbumGridItem` on success, or an `sqlx::Error` on failure.
pub async fn fetch_and_process_album_data(db_pool: &Arc<SqlitePool>) -> Result<Vec<AlbumGridItem>> {
    // Synchronize DR completed status from the persistence store in the background.
    // This ensures that any manual changes to best_dr_values.json or updates from other
    // parts of the application are reflected in the database without blocking the UI.
    let db_pool_clone = Arc::clone(db_pool);

    // We spawn this in the background but don't wait for it to complete
    spawn(async move {
        if let Err(e) = synchronize_dr_completed_background(db_pool_clone, None).await {
            eprintln!(
                "Error synchronizing DR completed status in background: {}",
                e
            );
        }
    });

    // For immediate population, we'll use the existing DR status in the database
    let _dr_store = DrValueStore::load(); // Load the DR store once for efficiency

    // Fetch album display information from the database
    fetch_album_display_info(db_pool).await
}
