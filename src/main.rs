mod ui;
mod data;
mod utils;

use std::{fs::OpenOptions, path::PathBuf, sync::Arc};
use std::env::{current_exe, var_os};

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

    // Find a project-local path for the DB
    let exe_dir = current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let mut db_path = exe_dir.join("music_library.db");

    // If not writable, use home directory
    let test = OpenOptions::new().write(true).create(true).open(&db_path);
    if test.is_err() {
        let home_dir = var_os("HOME").or_else(|| var_os("USERPROFILE"));
        if let Some(home) = home_dir {
            db_path = PathBuf::from(home).join("music_library.db");
        }
    }
    let db_url = format!("sqlite://{}", db_path.to_string_lossy());

    // Set up DB pool and run migrations
    let pool = SqlitePool::connect(&db_url)
        .await
        .expect("Failed to connect to DB");
    init_db(&pool).await.expect("Failed to initialize DB");
    let pool = Arc::new(pool);

    // Create a new application instance and run the application
    let app = MyApp::new(pool.clone());
    app.run();
}
