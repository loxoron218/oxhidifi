mod ui;
mod data;
mod utils;

use std::{fs::create_dir_all, path::PathBuf, sync::Arc};
use std::env::var;

use gtk4::{CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, StyleContext};
use gtk4::gdk::Display;
use libadwaita::init;
use libadwaita::prelude::ApplicationExtManual;
use sqlx::SqlitePool;
use tokio::main;

use crate::ui::MyApp;
use crate::data::db::init_db;

/// Entry point: initializes the app, sets up CSS, and launches the main event loop.
#[main]
async fn main() {

    // Initialize GTK and Libadwaita
    init().expect("Failed to initialize libadwaita");

    // Load custom CSS
    let provider = CssProvider::new();
    provider.load_from_path("style.css");
    #[allow(deprecated)]
    StyleContext::add_provider_for_display(
        &Display::default().expect("No display found"),
        &provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Determine the configuration directory
    const CONFIG_DIR: &str = ".config/oxhidifi";
    let home_dir = var("HOME")
        .expect("HOME environment variable not set");
    let config_dir = PathBuf::from(home_dir).join(CONFIG_DIR);

    // Ensure the configuration directory exists
    create_dir_all(&config_dir).expect(&format!("Failed to create config directory at {}: {}", config_dir.display(), "Error creating directory"));

    // Construct the database path within the configuration directory
    let db_path = config_dir.join("music_library.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());

    // Set up DB pool and run migrations
    let pool = SqlitePool::connect(&db_url)
        .await
        .expect(&format!("Failed to connect to DB at {}: {}", db_path.display(), "Error connecting to database"));
    init_db(&pool).await.expect("Failed to initialize DB");
    let pool = Arc::new(pool);

    // Create a new application instance and run the application
    let app = MyApp::new(pool.clone());
    app.run();
}
