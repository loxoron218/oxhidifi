use std::{cell::Cell, rc::Rc};

use glib::Propagation;
use gtk4::{Box, Button, Entry, EventControllerFocus, EventControllerKey, GestureClick, Revealer, RevealerTransitionType};
use libadwaita::ApplicationWindow;
use libadwaita::prelude::{ButtonExt, EditableExt, WidgetExt};

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
                .transition_type(RevealerTransitionType::SlideLeft)
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
    pub fn setup_logic(
        &self,
        window: &ApplicationWindow,
        vbox_inner: &Box,
        _refresh_library_ui: Rc<dyn Fn(bool, bool)>,
        _sort_ascending: Rc<Cell<bool>>,
        _sort_ascending_artists: Rc<Cell<bool>>,
    ) {
        let search_revealer = self.revealer.clone();
        let search_entry = self.entry.clone();
        let search_button = self.button.clone();

        // Show search bar when button is clicked
        let search_revealer_c = search_revealer.clone();
        let search_button_c = search_button.clone();
        let search_entry_c = search_entry.clone();
        search_button.connect_clicked(move |_| {
            search_button_c.set_visible(false);
            search_revealer_c.set_reveal_child(true);
            search_entry_c.set_text("");
            search_entry_c.grab_focus();
            search_entry_c.set_position(-1);
        });

        // Hide search bar when clicking outside
        let gesture = GestureClick::new();
        let search_entry_for_click = search_entry.clone();
        let search_button_for_click = search_button.clone();
        let search_revealer_click = search_revealer.clone();
        let vbox_inner_for_click = vbox_inner.clone();
        gesture.connect_pressed(move |_, _, x, y| {
            if search_revealer_click.reveals_child() {
                let alloc = search_entry_for_click.allocation();
                let (sx, sy) = search_entry_for_click
                    .translate_coordinates(&vbox_inner_for_click, 0.0, 0.0)
                    .unwrap_or((alloc.x() as f64, alloc.y() as f64));
                let inside = x >= sx
                    && x <= sx + alloc.width() as f64
                    && y >= sy
                    && y <= sy + alloc.height() as f64;
                if !inside {
                    search_entry_for_click.set_text("");
                    search_revealer_click.set_reveal_child(false);
                    search_button_for_click.set_visible(true);
                }
            }
        });
        vbox_inner.add_controller(gesture);

        // Type-to-search: activate search bar on any printable key
        let search_revealer_type = search_revealer.clone();
        let search_entry_type = search_entry.clone();
        let search_button_type = search_button.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if let Some(ch) = keyval.to_unicode() {
                if !ch.is_control() && !ch.is_whitespace() {
                    if !search_revealer_type.reveals_child() {
                        search_button_type.set_visible(false);
                        search_revealer_type.set_reveal_child(true);
                        search_entry_type.set_text(&ch.to_string());
                        search_entry_type.grab_focus();
                        search_entry_type.set_position(-1);
                        return Propagation::Stop;
                    }
                }
            }
            Propagation::Proceed
        });
        window.add_controller(key_controller);

        // Hide entry and show button on focus out
        let search_button_clone = search_button.clone();
        let search_revealer_focus = search_revealer.clone();
        let search_entry_for_focus = search_entry.clone();
        let focus_controller = EventControllerFocus::new();
        focus_controller.connect_leave(move |_| {
            search_entry_for_focus.set_text("");

            // Optionally clear the grids here if you have access
            search_revealer_focus.set_reveal_child(false);
            search_button_clone.set_visible(true);
        });
        search_entry.add_controller(focus_controller);
    }
}

/// Attach focus-out logic to a SearchBar, hiding the revealer and showing the button when focus is lost.
pub fn connect_searchbar_focus_out(search_bar: &SearchBar) {
    let search_button_clone = search_bar.button.clone();
    let search_revealer_focus = search_bar.revealer.clone();
    let focus_controller = EventControllerFocus::new();
    focus_controller.connect_leave(move |_| {
        search_revealer_focus.set_reveal_child(false);
        search_button_clone.set_visible(true);
    });
    search_bar.entry.add_controller(focus_controller);
}

/// Sets up all search bar UI logic: gesture for closing search bar, and event logic for show/hide, focus, keys.
/// This should be called from the main window setup instead of duplicating search bar logic.
pub fn setup_searchbar_all(
    search_bar: &SearchBar,
    window: &ApplicationWindow,
    vbox_inner: &Box,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
) {

    // GestureClick for closing search bar
    let search_entry_for_click = search_bar.entry.clone();
    let search_button_for_click = search_bar.button.clone();
    let search_revealer_click = search_bar.revealer.clone();
    let vbox_inner_for_click = vbox_inner.clone();
    let gesture = GestureClick::new();
    gesture.connect_pressed(move |_, _, x, y| {
        if search_revealer_click.reveals_child() {
            let alloc = search_entry_for_click.allocation();
            let (sx, sy) = search_entry_for_click
                .translate_coordinates(&vbox_inner_for_click, 0.0, 0.0)
                .unwrap_or((alloc.x() as f64, alloc.y() as f64));
            let inside = x >= sx
                && x <= sx + alloc.width() as f64
                && y >= sy
                && y <= sy + alloc.height() as f64;
            if !inside {
                search_revealer_click.set_reveal_child(false);
                search_button_for_click.set_visible(true);
            }
        }
    });
    vbox_inner.add_controller(gesture);

    // Search bar logic
    search_bar.setup_logic(
        window,
        vbox_inner,
        refresh_library_ui,
        sort_ascending,
        sort_ascending_artists,
    );
}
