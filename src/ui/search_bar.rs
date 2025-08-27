use std::rc::Rc;

use glib::Propagation::{Proceed, Stop};
use gtk4::{
    Box, Button, Entry, EventControllerFocus, EventControllerKey, GestureClick, Revealer,
    RevealerTransitionType::SlideLeft,
};
use libadwaita::{
    ApplicationWindow,
    prelude::{ButtonExt, EditableExt, WidgetExt},
};

// SearchBar Widget
#[derive(Clone)]
pub struct SearchBar {
    pub entry: Entry,
    pub revealer: Rc<Revealer>,
    pub button: Button,
}

impl SearchBar {
    /// Construct a new SearchBar widget.
    pub fn new() -> Self {
        let entry = Entry::builder()
            .placeholder_text("Search...")
            .hexpand(true)
            .build();
        let revealer = Rc::new(
            Revealer::builder()
                .transition_type(SlideLeft)
                .reveal_child(false)
                .build(),
        );
        revealer.set_child(Some(&entry));
        let button = Button::builder()
            .icon_name("system-search-symbolic")
            .build();
        Self {
            entry,
            revealer,
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
        let search_revealer = self.revealer.clone();
        let search_entry = self.entry.clone();
        let search_button = self.button.clone();

        // --- Event: Show search bar when button is clicked ---
        let search_revealer_on_click = search_revealer.clone();
        let search_button_on_click = search_button.clone();
        let search_entry_on_click = search_entry.clone();
        search_button.connect_clicked(move |_| {
            search_button_on_click.set_visible(false);
            search_revealer_on_click.set_reveal_child(true);

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
        let search_revealer_for_gesture = search_revealer.clone();
        let vbox_inner_for_gesture = vbox_inner.clone();
        gesture_click_outside.connect_pressed(move |_, _, x, y| {
            // Only act if the revealer is currently showing the search entry
            if search_revealer_for_gesture.reveals_child() {
                // Get the allocation (position and size) of the search entry
                let alloc = search_entry_for_gesture.allocation();

                // Translate the search entry's coordinates relative to the vbox_inner
                let (sx, sy) = search_entry_for_gesture
                    .translate_coordinates(&vbox_inner_for_gesture, 0.0, 0.0)
                    .unwrap_or((alloc.x() as f64, alloc.y() as f64));

                // Check if the click coordinates (x, y) are *inside* the search entry's bounds
                let inside = x >= sx
                    && x <= sx + alloc.width() as f64
                    && y >= sy
                    && y <= sy + alloc.height() as f64;

                // If the click was outside the search entry, hide the search bar
                if !inside {
                    // Clear search text
                    search_entry_for_gesture.set_text("");

                    // Hide the entry
                    search_revealer_for_gesture.set_reveal_child(false);

                    // Show the search button again
                    search_button_for_gesture.set_visible(true);
                }
            }
        });

        // Attach the gesture to the main content box
        vbox_inner.add_controller(gesture_click_outside);

        // --- Event: Type-to-search (activate search bar on any printable key) ---
        // This controller is added to the main application window to capture global key presses.
        let search_revealer_on_key = search_revealer.clone();
        let search_entry_on_key = search_entry.clone();
        let search_button_on_key = search_button.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if let Some(ch) = keyval.to_unicode() {
                // Check if the character is printable and not a control character or whitespace
                if !ch.is_control() && !ch.is_whitespace() {
                    // If the search bar is currently hidden, activate it
                    if !search_revealer_on_key.reveals_child() {
                        // Hide the search button
                        search_button_on_key.set_visible(false);

                        // Show the search entry
                        search_revealer_on_key.set_reveal_child(true);

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
        let search_revealer_on_focus_out = search_revealer.clone();
        let search_entry_on_focus_out = search_entry.clone();
        let focus_controller = EventControllerFocus::new();
        focus_controller.connect_leave(move |_| {
            // Clear the text when focus is lost, ensuring a clean state for the next search
            search_entry_on_focus_out.set_text("");

            // Hide the search entry
            search_revealer_on_focus_out.set_reveal_child(false);

            // Show the search button again
            search_button_on_focus_out.set_visible(true);
        });

        // Attach to the search entry itself
        search_entry.add_controller(focus_controller);
    }
}
