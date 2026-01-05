//! Oxhidifi - High-Fidelity Music Player
//!
//! This is the main entry point for the Oxhidifi music player application.
//! It initializes GTK/Libadwaita and starts the main application loop.

use std::error::Error;

use {
    libadwaita::init,
    tokio::main,
    tracing_subscriber::{EnvFilter, fmt::Subscriber},
};

use oxhidifi::ui::OxhidifiApplication;

/// Main entry point for the Oxhidifi application.
///
/// This function initializes the GTK and Libadwaita libraries,
/// creates the main application instance, and starts the event loop.
#[main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing for observability
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    Subscriber::builder().with_env_filter(filter).init();

    // Initialize GTK and Libadwaita
    init()?;

    // Create and run the application
    let app = OxhidifiApplication::new().await?;
    app.run();

    Ok(())
}
