use gtk4::gdk::{Display, Monitor};
use libadwaita::prelude::{Cast, DisplayExt, ListModelExt, MonitorExt};

/// Returns (screen_width, screen_height) of the primary monitor
pub fn get_primary_screen_size() -> (i32, i32) {
    let display = Display::default().expect("No display found");

    // Use the first monitor in the ListModel as the primary
    let monitors = display.monitors();
    let monitor = monitors.item(0).and_then(|obj| obj.downcast::<Monitor>().ok()).expect("No monitor found");
    let geometry = monitor.geometry();
    (geometry.width(), geometry.height())
}

/// Compute cover size and tile size based on screen width.
/// - cover_size is clamped between 96 and 384
/// - tile_size is cover_size + fixed margin for text
pub fn compute_cover_and_tile_size(screen_width: i32) -> (i32, i32) {
    let min_cover = 96;
    let max_cover = 384;
    let reference_width = 1920.0;
    let base_cover = 192.0;
    let cover_size = ((screen_width as f32) / reference_width * base_cover)
        .clamp(min_cover as f32, max_cover as f32)
        .round() as i32;
    let tile_size = cover_size;
    (cover_size, tile_size)
}
