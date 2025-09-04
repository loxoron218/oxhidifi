use std::{cell::Cell, rc::Rc};

use gtk4::Button;
use libadwaita::{ViewStack, prelude::ButtonExt};

use crate::ui::components::config::{load_settings, save_settings};

/// Connects the sort direction button handlers to update sort direction
///
/// This function sets up the click handler for the sort direction button and
/// the visible child notify handler for the stack to update the button icon
/// when switching between views.
///
/// # Arguments
///
/// * `sort_direction_button` - The button to connect handlers to
/// * `sort_ascending` - Shared reference to the album sort direction
/// * `sort_ascending_artists` - Shared reference to the artist sort direction
/// * `on_sort_changed` - Callback function to refresh the UI when sorting changes
/// * `stack` - The ViewStack to monitor for visible child changes
pub fn connect_sort_direction_handlers(
    sort_direction_button: &Button,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    on_sort_changed: Rc<dyn Fn(bool, bool)>,
    stack: Rc<ViewStack>,
) {
    // Clone references for the callbacks
    let sort_ascending_clone = sort_ascending.clone();
    let sort_ascending_artists_clone = sort_ascending_artists.clone();
    let on_sort_changed_clone = on_sort_changed.clone();
    let sort_direction_button_clone = sort_direction_button.clone();
    let stack_clone = stack.clone();

    // Connect the sort direction button to update sort direction
    let sort_ascending_clone_button = sort_ascending_clone.clone();
    let sort_ascending_artists_clone_button = sort_ascending_artists_clone.clone();
    let on_sort_changed_clone_button = on_sort_changed_clone.clone();
    sort_direction_button.connect_clicked(move |_| {
        // Determine the current view and toggle the appropriate sort direction
        let current_tab = stack_clone
            .visible_child_name()
            .unwrap_or_else(|| "albums".into());
        let is_currently_albums = current_tab.as_str() == "albums";
        if is_currently_albums {
            // Toggle the album sort direction
            let is_ascending = !sort_ascending_clone_button.get();

            // Update the shared state
            sort_ascending_clone_button.set(is_ascending);

            // Update the button icon
            let icon_name = if is_ascending {
                // For ascending order
                "view-sort-descending-symbolic"
            } else {
                // For descending order
                "view-sort-ascending-symbolic"
            };
            sort_direction_button_clone.set_icon_name(icon_name);

            // Save to settings
            let mut settings = load_settings();
            settings.sort_ascending_albums = is_ascending;
            let _ = save_settings(&settings);

            // Trigger UI refresh
            on_sort_changed_clone_button(
                sort_ascending_clone_button.get(),
                sort_ascending_artists_clone_button.get(),
            );
        } else {
            // Toggle the artist sort direction
            let is_ascending = !sort_ascending_artists_clone_button.get();

            // Update the shared state
            sort_ascending_artists_clone_button.set(is_ascending);

            // Update the button icon
            let icon_name = if is_ascending {
                // For ascending order
                "view-sort-descending-symbolic"
            } else {
                // For descending order
                "view-sort-ascending-symbolic"
            };
            sort_direction_button_clone.set_icon_name(icon_name);

            // Save to settings
            let mut settings = load_settings();
            settings.sort_ascending_artists = is_ascending;
            let _ = save_settings(&settings);

            // Trigger UI refresh
            on_sort_changed_clone_button(
                sort_ascending_clone_button.get(),
                sort_ascending_artists_clone_button.get(),
            );
        }
    });

    // Connect to view changes to update the sorting button icon
    let sort_direction_button_clone_for_listener = sort_direction_button.clone();
    let sort_ascending_clone_for_listener = sort_ascending.clone();
    let sort_ascending_artists_clone_for_listener = sort_ascending_artists.clone();
    let stack_clone_for_listener = stack.clone();
    stack_clone_for_listener.connect_visible_child_notify(move |stack| {
        // Update the button icon based on the current view and its sort direction
        let current_tab = stack
            .visible_child_name()
            .unwrap_or_else(|| "albums".into());
        let is_currently_albums = current_tab.as_str() == "albums";
        let is_ascending = if is_currently_albums {
            sort_ascending_clone_for_listener.get()
        } else {
            sort_ascending_artists_clone_for_listener.get()
        };
        let icon_name = if is_ascending {
            // For ascending order
            "view-sort-descending-symbolic"
        } else {
            // For descending order
            "view-sort-ascending-symbolic"
        };
        sort_direction_button_clone_for_listener.set_icon_name(icon_name);
    });
}
