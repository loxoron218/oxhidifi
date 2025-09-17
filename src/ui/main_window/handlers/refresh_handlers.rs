use std::{cell::RefCell, rc::Rc, sync::Arc};

use sqlx::SqlitePool;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    data::watcher::start_watching_library,
    ui::components::{
        refresh::{RefreshService, setup_live_monitor_refresh},
        scan_feedback::spawn_scanning_label_refresh_task,
    },
};

use super::super::{state::WindowSharedState, widgets::WindowWidgets};

/// Sets up a periodic refresh mechanism that monitors screen size changes and adapts the UI accordingly.
///
/// This function initializes a background task that periodically checks for changes in screen dimensions.
/// When a change is detected, it recalculates cover and tile sizes to ensure the UI remains aesthetically
/// pleasing and functional across different display configurations without requiring an application restart.
///
/// The refresh is only performed when the settings dialog is not open to prevent visual glitches during user interactions.
///
/// # Arguments
///
/// * `shared_state` - A reference to the `WindowSharedState` containing shared application state,
///                   including screen information and settings dialog status.
/// * `refresh_service` - An `Rc<RefreshService>` instance that provides the refresh functionality.
pub fn setup_live_monitor_refresh_handler(
    shared_state: &WindowSharedState,
    refresh_service: Rc<RefreshService>,
) {
    setup_live_monitor_refresh(
        refresh_service.clone(),
        shared_state.screen_info.clone(),
        shared_state.is_settings_open.clone(),
        Some(shared_state.current_zoom_level.clone()),
    );
}

/// Starts the library watcher for real-time file system changes.
///
/// This function initializes a background task that monitors the music library folders for new,
/// modified, or deleted files. When changes are detected, it automatically triggers UI updates
/// to keep the library synchronized with the file system.
///
/// The watcher uses a debouncing mechanism to prevent excessive scanning during periods of
/// high file activity, and it periodically polls the database for changes in watched folders.
///
/// # Arguments
///
/// * `db_pool` - An `Arc<SqlitePool>` providing access to the application's database.
/// * `sender` - An `UnboundedSender<()>` used to signal the UI when a refresh is needed.
pub fn start_library_watcher(db_pool: Arc<SqlitePool>, sender: UnboundedSender<()>) {
    start_watching_library(db_pool.clone(), sender.clone());
}

/// Spawns a task to refresh scanning labels after a library scan completes.
///
/// This function sets up an asynchronous task that listens for signals from the library scanner.
/// When a scan finishes, it hides the "Scanning..." labels and triggers a UI refresh to display
/// the updated library content, providing clear visual feedback to the user.
///
/// The receiver is wrapped in `Rc<RefCell>` to allow shared mutable access across the application.
///
/// # Arguments
///
/// * `widgets` - A reference to `WindowWidgets` containing the UI components including scanning labels.
/// * `shared_state` - A reference to `WindowSharedState` containing shared application state.
/// * `receiver` - An `UnboundedReceiver<()>` that receives signals when a scan completes.
/// * `refresh_library_ui` - An `Rc<dyn Fn(bool, bool)>` closure that refreshes the library UI.
pub fn spawn_scanning_label_refresh_task_handler(
    widgets: &WindowWidgets,
    shared_state: &WindowSharedState,
    receiver: UnboundedReceiver<()>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
) {
    // Wrap receiver in Rc<RefCell> for shared mutable access
    let receiver = Rc::new(RefCell::new(receiver));
    spawn_scanning_label_refresh_task(
        receiver,
        Rc::new(widgets.scanning_label_albums.clone()),
        Rc::new(widgets.scanning_label_artists.clone()),
        widgets.stack.clone(),
        refresh_library_ui.clone(),
        shared_state.sort_ascending.clone(),
        shared_state.sort_ascending_artists.clone(),
        shared_state.initial_scan_ongoing.clone(),
        shared_state.current_view_mode.clone(),
    );
}
