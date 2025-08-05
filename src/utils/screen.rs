use libadwaita::{
    gdk::{Display, Monitor},
    prelude::{Cast, DisplayExt, ListModelExt, MonitorExt},
};

/// Represents information about the primary display screen, including its dimensions
/// and calculated UI element sizes (cover and tile sizes) derived from the screen width.
///
/// This struct encapsulates screen-related data, making it easier to pass around
/// and manage display properties and their derived UI sizing parameters.
#[derive(Debug, Clone, Copy)]
pub struct ScreenInfo {
    /// The width of the primary monitor in pixels.
    pub width: i32,
    /// The calculated optimal size for cover art (e.g., album covers).
    /// This value is clamped between `MIN_COVER_SIZE` and `MAX_COVER_SIZE`
    /// to ensure reasonable scaling across various screen resolutions.
    pub cover_size: i32,
    /// The calculated optimal size for UI tiles containing cover art and text.
    /// Currently, this is set to be the same as `cover_size`, but it is kept
    /// as a separate field for potential future adjustments (e.g., adding padding).
    pub tile_size: i32,
}

impl ScreenInfo {
    /// Minimum allowed cover size in pixels.
    const MIN_COVER_SIZE: i32 = 96;
    /// Maximum allowed cover size in pixels.
    const MAX_COVER_SIZE: i32 = 384;
    /// Reference screen width (e.g., 1080p) used for scaling calculations.
    const REFERENCE_WIDTH: f32 = 1920.0;
    /// Base cover size corresponding to the `REFERENCE_WIDTH`.
    const BASE_COVER_SIZE: f32 = 192.0;

    /// Creates a new `ScreenInfo` instance by querying the primary display's properties.
    ///
    /// This function retrieves the default display and its primary monitor to determine
    /// the screen dimensions. It then calculates the `cover_size` and `tile_size`
    /// based on the screen width using a scaling algorithm.
    ///
    /// # Panics
    /// Panics if no default display or primary monitor can be found, as these are
    /// critical components for a graphical application.
    ///
    /// # Returns
    /// A `ScreenInfo` struct containing the primary screen's dimensions and
    /// calculated UI element sizes.
    pub fn new() -> Self {
        // Retrieve the default display. This is essential for any GTK application
        // as it represents the user's graphical environment.
        let display = Display::default().expect(
            "Failed to get default display. Is the application running in a graphical environment?",
        );

        // Attempt to get the primary monitor from the display.
        // `display.monitors()` returns a `ListModel` of available monitors.
        // We expect at least one monitor to be present for a functional display.
        let monitor = display
            .monitors()
            .item(0) // Get the first monitor, typically considered the primary.
            .and_then(|obj| obj.downcast::<Monitor>().ok()) // Downcast to `Monitor` type.
            .expect("No monitor found on the default display. Ensure a display device is connected and configured.");

        // Get the geometry (position and dimensions) of the monitor.
        let geometry = monitor.geometry();
        let screen_width = geometry.width();
        // Calculate cover and tile sizes based on the screen width.
        // This scaling ensures that UI elements adapt dynamically to different screen resolutions.
        let cover_size = ((screen_width as f32) / Self::REFERENCE_WIDTH * Self::BASE_COVER_SIZE)
            .clamp(Self::MIN_COVER_SIZE as f32, Self::MAX_COVER_SIZE as f32)
            .round() as i32;

        // Currently, tile size is the same as cover size. This can be adjusted in the future
        // if additional spacing or text area needs to be accounted for within a tile.
        let tile_size = cover_size;

        Self {
            width: screen_width,
            cover_size,
            tile_size,
        }
    }

    /// Returns the calculated cover art size.
    ///
    /// # Returns
    /// The cover size in pixels.
    pub fn get_cover_size(&self) -> i32 {
        self.cover_size
    }

    /// Returns the calculated tile size.
    ///
    /// # Returns
    /// The tile size in pixels.
    pub fn get_tile_size(&self) -> i32 {
        self.tile_size
    }
}
