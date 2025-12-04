//! Oxhidifi - High-Fidelity Music Player
//!
//! This is the main entry point for the Oxhidifi music player application.
//! It initializes GTK/Libadwaita and starts the main application loop.

use std::error::Error;

use {libadwaita::init, tokio::main};

use oxhidifi::ui::OxhidifiApplication;

/// Main entry point for the Oxhidifi application.
///
/// This function initializes the GTK and Libadwaita libraries,
/// creates the main application instance, and starts the event loop.
#[main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize GTK and Libadwaita
    init()?;

    // Create and run the application
    let app = OxhidifiApplication::new().await?;
    app.run();

    Ok(())
}
