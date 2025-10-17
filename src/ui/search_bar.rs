use gtk4::{
    Box, Button, Entry, EventControllerFocus, EventControllerKey, GestureClick,
    Orientation::Horizontal,
    glib::Propagation::{Proceed, Stop},
    graphene::Point,
};
use libadwaita::{
    ApplicationWindow,
    prelude::{BoxExt, ButtonExt, EditableExt, WidgetExt},
};

// SearchBar Widget
#[derive(Clone)]
pub struct SearchBar {
    pub entry: Entry,
    pub container: Box,
    pub button: Button,
}

impl SearchBar {
    /// Construct a new SearchBar widget.
    pub fn new() -> Self {
        let entry = Entry::builder().placeholder_text("Search...").build();

        // Hidden by default
        entry.set_visible(false);

        // Create the search button with a symbolic icon
        let button = Button::builder()
            .icon_name("system-search-symbolic")
            .build();

        // Add tooltip to the search button
        button.set_tooltip_text(Some("Search Library"));

        // Create a container box to hold both button and entry
        let container = Box::builder().orientation(Horizontal).build();
        container.append(&button);
        container.append(&entry);

        // Initialize the SearchBar struct with its components
        Self {
            entry,
            container,
            button,
        }
    }

    /// Set up event logic for the SearchBar (show/hide, focus, key events).
    /// Sets up all UI event logic for the SearchBar.
    ///
    /// This includes:
    /// - Showing the search entry and hiding the search button when the search button is clicked.
    /// - Hiding the search entry and showing the search button when clicking outside the entry.
    /// - Activating the search bar (showing entry, hiding button) on any printable key press.
    /// - Hiding the search entry and showing the search button when the search entry loses focus.
    ///
    /// # Arguments
    /// * `window` - The application window, used for adding global key event controllers.
    /// * `vbox_inner` - The main vertical box containing the header and content, used for detecting clicks outside the search bar.
    pub fn setup_logic(&self, window: &ApplicationWindow, vbox_inner: &Box) {
        let search_entry = self.entry.clone();
        let search_button = self.button.clone();

        // --- Event: Show search bar when button is clicked ---
        let search_button_on_click = search_button.clone();
        let search_entry_on_click = search_entry.clone();
        search_button.connect_clicked(move |_| {
            search_button_on_click.set_visible(false);
            search_entry_on_click.set_visible(true);

            // Clear previous search text
            search_entry_on_click.set_text("");

            // Immediately focus the entry
            search_entry_on_click.grab_focus();

            // Move cursor to end of text
            search_entry_on_click.set_position(-1);
        });

        // --- Event: Hide search bar when clicking outside ---
        // This gesture is added to a parent container (vbox_inner) to detect clicks anywhere
        // within the window that are *not* on the search entry itself.
        let gesture_click_outside = GestureClick::new();
        let search_entry_for_gesture = search_entry.clone();
        let search_button_for_gesture = search_button.clone();
        let vbox_inner_for_gesture = vbox_inner.clone();
        gesture_click_outside.connect_pressed(move |_, _, x, y| {
            // Only act if the entry is currently visible
            if search_entry_for_gesture.is_visible() {
                // Get the bounds (position and size) of the search entry
                let bounds = search_entry_for_gesture
                    .compute_bounds(&search_entry_for_gesture)
                    .unwrap_or_default();
                let bounds_width = bounds.width();
                let bounds_height = bounds.height();

                // Compute the position of the search entry relative to the vbox_inner
                let point = Point::new(0.0, 0.0);
                let computed_point = search_entry_for_gesture
                    .compute_point(&vbox_inner_for_gesture, &point)
                    .unwrap_or_else(|| Point::new(bounds.x(), bounds.y()));
                let sx = computed_point.x() as f64;
                let sy = computed_point.y() as f64;

                // Check if the click coordinates (x, y) are *inside* the search entry's bounds
                let inside = x >= sx
                    && x <= sx + bounds_width as f64
                    && y >= sy
                    && y <= sy + bounds_height as f64;

                // If the click was outside the search entry, hide the search bar
                if !inside {
                    // Clear search text
                    search_entry_for_gesture.set_text("");

                    // Hide the entry
                    search_entry_for_gesture.set_visible(false);

                    // Show the search button again
                    search_button_for_gesture.set_visible(true);
                }
            }
        });

        // Attach the gesture to the main content box
        vbox_inner.add_controller(gesture_click_outside);

        // --- Event: Type-to-search (activate search bar on any printable key) ---
        // This controller is added to the main application window to capture global key presses.
        let search_entry_on_key = search_entry.clone();
        let search_button_on_key = search_button.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if let Some(ch) = keyval.to_unicode() {
                // Check if the character is printable and not a control character or whitespace
                if !ch.is_control() && !ch.is_whitespace() {
                    // If the search bar is currently hidden, activate it
                    if !search_entry_on_key.is_visible() {
                        // Hide the search button
                        search_button_on_key.set_visible(false);

                        // Show the search entry
                        search_entry_on_key.set_visible(true);

                        // Pre-fill with the typed character
                        search_entry_on_key.set_text(&ch.to_string());

                        // Give focus to the entry
                        search_entry_on_key.grab_focus();

                        // Move cursor to end
                        search_entry_on_key.set_position(-1);

                        // Stop propagation so the character is not processed twice
                        return Stop;
                    }
                }
            }

            // Continue propagating the event
            Proceed
        });

        // Attach the key controller to the window
        window.add_controller(key_controller);

        // --- Event: Hide entry and show button on focus out ---
        // This controller is specifically for when the search Entry widget loses focus.
        let search_button_on_focus_out = search_button.clone();
        let search_entry_on_focus_out = search_entry.clone();
        let focus_controller = EventControllerFocus::new();
        focus_controller.connect_leave(move |_| {
            // Clear the text when focus is lost, ensuring a clean state for the next search
            search_entry_on_focus_out.set_text("");

            // Hide the search entry
            search_entry_on_focus_out.set_visible(false);

            // Show the search button again
            search_button_on_focus_out.set_visible(true);
        });

        // Attach to the search entry itself
        search_entry.add_controller(focus_controller);
    }
}
