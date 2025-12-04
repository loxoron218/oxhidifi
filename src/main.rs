//! Oxhidifi - High-Fidelity Music Player
//!
//! This is the main entry point for the Oxhidifi music player application.
//! It initializes GTK/Libadwaita and starts the main application loop.

use oxhidifi::ui::OxhidifiApplication;

/// Main entry point for the Oxhidifi application.
///
/// This function initializes the GTK and Libadwaita libraries,
/// creates the main application instance, and starts the event loop.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize GTK and Libadwaita
    libadwaita::gtk::init()?;
    let _ = libadwaita::init();

    // Create and run the application
    let app = OxhidifiApplication::new().await?;
    app.run();

    Ok(())
}
