use std::sync::Arc;

use libadwaita::{Application, prelude::ApplicationExt};
use sqlx::SqlitePool;

use super::window::build_main_window;

/// `App` struct represents the main application entry point and configuration.
///
/// This struct is responsible for creating and managing the `libadwaita::Application`
/// instance, which is the core of the GTK/Libadwaita application. It acts as the
/// initial bootstrap for the UI, connecting the application's lifecycle to the
/// main window construction.
pub struct App;

impl App {
    /// Creates a new `libadwaita::Application` instance and connects its `activate` signal.
    ///
    /// The `activate` signal is emitted when the application is launched or activated.
    /// When activated, the `build_main_window` function is called to construct and
    /// display the main application window, passing the database connection pool.
    ///
    /// # Arguments
    ///
    /// * `db_pool` - An `Arc<SqlitePool>` representing the shared database connection pool.
    ///   This pool is cloned and moved into the `activate` signal handler closure,
    ///   allowing the main window to interact with the database.
    ///
    /// # Returns
    ///
    /// A `libadwaita::Application` instance, ready to be run.
    pub fn new(db_pool: Arc<SqlitePool>) -> Application {
        // Create a new Libadwaita Application with a unique application ID.
        // The application ID is crucial for desktop integration (e.g., .desktop files).
        let app = Application::builder()
            .application_id("org.loxoron218.oxhidifi")
            .build();

        // Connect the 'activate' signal to the main window builder.
        // The `move` keyword ensures that `app` and `db_pool` are moved into the closure,
        // allowing them to be used within the asynchronous context of the signal handler.
        app.connect_activate(move |app| {
            // Build and present the main application window.
            // A clone of `db_pool` is passed to `build_main_window` to ensure
            // the main window has access to the database.
            build_main_window(app, db_pool.clone());
        });

        // Return the configured application instance.
        app
    }
}
