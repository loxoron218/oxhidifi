use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use glib::MainContext;
use gtk4::Label;
use libadwaita::{ViewStack, prelude::WidgetExt};
use tokio::sync::mpsc::UnboundedReceiver;

/// Creates a new `gtk4::Label` widget configured for displaying scanning feedback.
///
/// This label is initially invisible and styled with a specific CSS class.
/// It's intended to be shown during library scanning operations.
///
/// # Returns
/// A new `gtk4::Label` instance.
pub fn create_scanning_label() -> Label {
    Label::builder()
        .label("Scanning...")
        .visible(false)
        .css_classes(["album-artist-label"])
        .build()
}

/// Spawns a local asynchronous task to listen for scan completion signals
/// and update the UI accordingly.
///
/// When a signal is received, it hides the appropriate scanning label (albums or artists)
/// and triggers a UI refresh for the library view. This helps provide visual feedback
/// to the user that a scan has finished.
///
/// # Arguments
/// * `receiver` - A `Rc<RefCell<UnboundedReceiver<()>>>` used to receive completion signals.
/// * `scanning_label_albums` - A `Rc<Label>` for the albums scanning label.
/// * `scanning_label_artists` - A `Rc<Label>` for the artists scanning label.
/// * `stack` - A `libadwaita::ViewStack` to determine the currently visible page.
/// * `refresh_library_ui` - A `Rc<dyn Fn(bool, bool)>` closure to refresh the library UI.
/// * `sort_ascending` - A `Rc<Cell<bool>>` indicating the current album sort order.
/// * `sort_ascending_artists` - A `Rc<Cell<bool>>` indicating the current artist sort order.
pub fn spawn_scanning_label_refresh_task(
    receiver: Rc<RefCell<UnboundedReceiver<()>>>,
    scanning_label_albums: Rc<Label>,
    scanning_label_artists: Rc<Label>,
    stack: ViewStack,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) {
    let refresh_library_ui_clone = refresh_library_ui.clone();
    let sort_ascending_for_refresh = sort_ascending.clone();
    let sort_ascending_artists_for_refresh = sort_ascending_artists.clone();
    let stack_clone = stack.clone();

    MainContext::default().spawn_local(async move {
        let mut receiver = receiver.borrow_mut();

        // Loop indefinitely, waiting for scan completion signals.
        while receiver.recv().await.is_some() {
            let page = stack_clone.visible_child_name().unwrap_or_default();

            // Hide the appropriate scanning label based on the currently visible page.
            scanning_label_albums.set_visible(page == "albums");
            scanning_label_artists.set_visible(page == "artists");

            // Refresh the library UI to reflect any changes from the scan.
            refresh_library_ui_clone(
                sort_ascending_for_refresh.get(),
                sort_ascending_artists_for_refresh.get(),
            );
        }
    });
}
