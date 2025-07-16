use std::sync::Arc;

use libadwaita::Application;
use libadwaita::prelude::ApplicationExt;
use sqlx::SqlitePool;

use super::window::build_main_window;

/// Application entry point struct.
pub struct MyApp;

impl MyApp {

    /// Create a new Libadwaita Application and connect activation to main window builder.
    pub fn new(db_pool: Arc<SqlitePool>) -> Application {
        let app = Application::builder()
            .application_id("org.loxoron218.oxhidifi")
            .build();
        app.connect_activate(move |app| {
            build_main_window(app, db_pool.clone());
        });
        app
    }
}
