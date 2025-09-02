use std::{cell::RefCell, rc::Rc};

use glib::MainContext;
use gtk4::{
    Align::{Center, Start},
    Box, CheckButton, EventControllerMotion, Label,
    Orientation::Horizontal,
    Overlay, Stack, StackTransitionType,
};
use libadwaita::prelude::{BoxExt, CheckButtonExt, WidgetExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    data::models::{Album, Artist, Folder},
    utils::best_dr_persistence::{AlbumKey, DrValueStore},
};

/// Build the DR badge widget for dynamic range value.
///
/// Creates a UI component that displays the album's DR value and allows users
/// to mark it as completed. The badge includes:
/// - A visual indicator of the DR value with color coding
/// - A hover effect that shows a checkbox for marking completion
/// - Persistence of the completion status in both database and local storage
///
/// # Parameters
/// - `album_id`: The unique identifier of the album in the database
/// - `dr`: The DR value of the album (if available)
/// - `dr_completed`: Whether the DR value has been marked as completed by the user
/// - `db_pool`: Database connection pool for persisting completion status
/// - `sender`: Channel sender for notifying UI updates
/// - `album`: Album data for persistence operations
/// - `artist`: Artist data for persistence operations
/// - `folder`: Folder data for persistence operations
///
/// # Returns
/// A GTK Box widget containing the DR badge UI elements
pub fn build_dr_badge(
    album_id: i64,
    dr: Option<u8>,
    dr_completed: bool,
    db_pool: std::sync::Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    album: Rc<Album>,
    artist: Rc<Artist>,
    folder: Rc<Folder>,
) -> Box {
    // Create the main container box for the DR badge
    let dr_box = Box::builder().orientation(Horizontal).spacing(4).build();

    // Determine the display values based on whether DR is available
    let (dr_str, tooltip_text, css_class) = match dr {
        Some(value) => (
            // Format DR value as two-digit number (e.g., "08", "12")
            format!("{:02}", value),
            Some("Official Dynamic Range Value"),
            // CSS class for color coding based on DR value
            format!("dr-{:02}", value),
        ),
        None => (
            // Display "N/A" when DR value is not available
            "N/A".to_string(),
            Some("Dynamic Range Value not available"),
            // CSS class for "not available" state
            "dr-na".to_string(),
        ),
    };

    // Create the main DR value label with styling
    let dr_label = Label::builder()
        .label(&dr_str)
        .halign(Center)
        .valign(Center)
        .build();

    // Set fixed size to ensure consistent layout
    dr_label.set_size_request(44, 44);

    // Apply base styling class and value-specific class
    dr_label.add_css_class("dr-badge-label");
    dr_label.add_css_class(&css_class);

    // Add dr-completed class if the DR value is marked as completed
    if dr_completed {
        dr_label.add_css_class("dr-completed");
    }

    // Create the text label that describes the DR value
    let dr_text_label = Label::builder()
        .label("Official DR Value")
        .halign(Start)
        .build();
    dr_text_label.add_css_class("album-technical-label");

    // Set a fixed width to prevent UI movement when text changes
    dr_text_label.set_width_chars(18);

    // Create the checkbox for marking DR value as completed
    let checkbox = CheckButton::builder()
        .active(dr_completed)
        .halign(Center)
        .valign(Center)
        .css_classes(vec!["dr-completion-checkbox"])
        .build();

    // Create a stack to switch between DR label and checkbox on hover
    let stack = Stack::builder()
        .transition_type(StackTransitionType::None)
        .build();
    stack.add_named(&dr_label, Some("dr_label"));
    stack.add_named(&checkbox, Some("checkbox"));

    // Initially show the DR label (checkbox shown on hover)
    stack.set_visible_child_name("dr_label");

    // Create overlay to contain the stack and enable tooltips
    let overlay = Overlay::new();

    // Set the stack as the child of the overlay
    overlay.set_child(Some(&stack));
    overlay.set_tooltip_text(tooltip_text);

    // Add UI elements to the main container
    dr_box.append(&overlay);
    dr_box.append(&dr_text_label);

    // Create weak references for use in event handlers
    let checkbox_weak = Rc::new(RefCell::new(checkbox));
    let dr_text_label_weak = Rc::new(RefCell::new(dr_text_label));
    let stack_weak = Rc::new(RefCell::new(stack));

    // Event controller for hover effects
    let motion_controller = EventControllerMotion::new();

    // Handle mouse enter event - show checkbox and update text
    motion_controller.connect_enter({
        let dr_text_label_weak = dr_text_label_weak.clone();
        let stack_weak = stack_weak.clone();
        move |_, _, _| {
            // Switch to checkbox view
            stack_weak.borrow().set_visible_child_name("checkbox");

            // Update text to indicate purpose of checkbox
            dr_text_label_weak.borrow().set_label("Best DR Value");
        }
    });

    // Handle mouse leave event - revert to DR label and original text
    motion_controller.connect_leave({
        let dr_text_label_weak = dr_text_label_weak.clone();
        let stack_weak = stack_weak.clone();
        move |_| {
            // Switch back to DR label view
            stack_weak.borrow().set_visible_child_name("dr_label");

            // Revert to original text on leave
            dr_text_label_weak.borrow().set_label("Official DR Value");
        }
    });

    // Attach the motion controller to the overlay
    overlay.add_controller(motion_controller);

    // Connect checkbox toggled signal to handle completion status changes
    checkbox_weak.borrow().connect_toggled(move |btn| {
        // Clone values for async context
        let db_pool = db_pool.clone();
        let sender = sender.clone();
        let is_completed = btn.is_active();
        let current_db_pool = db_pool.clone();
        let sender = sender.clone();
        let album_rc = album.clone();
        let artist_rc = artist.clone();
        let folder_rc = folder.clone();

        // Spawn async task to handle database and persistence updates
        MainContext::default().spawn_local(async move {
            // Update the album's DR completion status in the database
            if let Err(_e) = crate::data::db::crud::update_album_dr_completed(
                &*current_db_pool,
                album_id,
                is_completed,
            )
            .await
            {}

            // Notify UI of changes through the channel
            if let Err(_e) = sender.send(()) {
                // Handle error if sending fails, e.g., receiver dropped
            }

            // Update DrValueStore for persistent storage across sessions
            let mut dr_store = DrValueStore::load();
            let album_key = AlbumKey {
                title: album_rc.title.clone(),
                artist: artist_rc.name.clone(),
                folder_path: folder_rc.path.clone(),
            };

            // Add or remove the DR value from the store based on completion status
            if is_completed {
                dr_store.add_dr_value(album_key, dr.unwrap_or(0));
            } else {
                dr_store.remove_dr_value(&album_key);
            }

            // Save the updated DR store to persistent storage
            if let Err(e) = dr_store.save() {
                // Log the error instead of ignoring it.
                // Replace with your actual logging framework (e.g., log::error!)
                eprintln!("Failed to save DR store: {}", e);
            }
        });
    });

    // Return the constructed DR badge widget
    dr_box
}
