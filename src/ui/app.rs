use std::sync::Arc;

use libadwaita::{
    Application,
    prelude::{ApplicationExt, ApplicationExtManual},
};
use sqlx::SqlitePool;

use super::main_window::builder::build_main_window;
use crate::utils::image::AsyncImageLoader;

/// `App` struct represents the main application entry point and configuration.
///
/// This struct is responsible for creating and managing the `libadwaita::Application`
/// instance, which is the core of the GTK/Libadwaita application. It acts as the
/// initial bootstrap for the UI, connecting the application's lifecycle to the
/// main window construction.
pub struct App {
    pub application: Application,
}

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
    /// * `image_loader` - An `AsyncImageLoader` instance for shared image caching.
    ///
    /// # Returns
    ///
    /// An `App` instance containing the configured `libadwaita::Application`.
    pub fn new(db_pool: Arc<SqlitePool>, image_loader: AsyncImageLoader) -> Self {
        // Create a new Libadwaita Application with a unique application ID.
        // The application ID is crucial for desktop integration (e.g., .desktop files).
        let app = Application::builder()
            .application_id("org.loxoron218.oxhidifi")
            .build();

        // Connect the 'activate' signal to the main window builder.
        // The `move` keyword ensures that `app`, `db_pool`, and `image_loader` are moved into the closure,
        // allowing them to be used within the asynchronous context of the signal handler.
        app.connect_activate(move |app| {
            // Build and present the main application window.
            // Clones of `db_pool` and `image_loader` are passed to `build_main_window` to ensure
            // the main window has access to the database and shared image cache.
            build_main_window(app, db_pool.clone(), image_loader.clone());
        });

        // Return the configured application instance wrapped in the App struct.
        App { application: app }
    }

    /// Runs the application
    pub fn run(&self) {
        self.application.run();
    }
}
