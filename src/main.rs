mod data;
mod playback;
mod ui;
mod utils;

use std::{env::var, fs::create_dir_all, path::PathBuf, sync::Arc, time::Duration};

use gtk4::{CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION, StyleContext};
use libadwaita::{gdk::Display, init};
use sqlx::sqlite::SqlitePoolOptions;
use tokio::main;

use crate::{data::db::schema::init_db, ui::App, utils::image::AsyncImageLoader};

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
    let home_dir = var("HOME").expect("HOME environment variable not set");
    let config_dir = PathBuf::from(home_dir).join(CONFIG_DIR);

    // Ensure the configuration directory exists
    create_dir_all(&config_dir).unwrap_or_else(|_| {
        panic!(
            "Failed to create config directory at {}: {}",
            config_dir.display(),
            "Error creating directory"
        )
    });

    // Construct the database path within the configuration directory
    let db_path = config_dir.join("music_library.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());

    // Set up DB pool with optimized settings
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(60))
        .max_lifetime(Duration::from_secs(300))
        .connect(&db_url)
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Failed to connect to DB at {}: {}",
                db_path.display(),
                "Error connecting to database"
            )
        });
    init_db(&pool).await.expect("Failed to initialize DB");
    let pool = Arc::new(pool);

    // Create a new application instance and run the application
    let image_loader = AsyncImageLoader::new().expect("Failed to create image loader");
    let app = App::new(pool.clone(), image_loader);
    app.run();
}
