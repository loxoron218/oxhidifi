use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk4::{
    Box,
    Orientation::{Horizontal, Vertical},
    Popover, Separator, Widget,
};
use libadwaita::{
    SplitButton, ViewStack,
    prelude::{BoxExt, Cast, PopoverExt, WidgetExt},
};

use crate::ui::components::{
    navigation::VIEW_STACK_ALBUMS,
    view_controls::{
        ZoomManager,
        sorting_controls::{create_sorting_control_row, types::SortOrder},
        view_mode::ViewMode::{self, GridView, ListView},
        zoom_controls::create_zoom_control_row,
    },
};

/// A split button widget that provides view control functionality
///
/// The `ViewControlButton` combines a main button for switching view modes with
/// a dropdown menu containing additional view-related options such as zoom controls,
/// sorting options, and preferences.
#[derive(Debug, Clone)]
pub struct ViewControlButton {
    /// The underlying libadwaita SplitButton widget
    split_button: SplitButton,
    /// The current view mode of the button
    view_mode: RefCell<ViewMode>,
    /// The zoom manager for handling zoom levels
    zoom_manager: RefCell<Option<Rc<ZoomManager>>>,
    /// The sorting control widget
    sorting_widget: RefCell<Option<Rc<Box>>>,
}

impl ViewControlButton {
    /// Creates a new `ViewControlButton` with a specified initial view mode
    ///
    /// Initializes the button with a popover menu containing various view-related
    /// options and sets the initial view mode to the provided value.
    ///
    /// # Arguments
    ///
    /// * `initial_view_mode` - The initial view mode for the button
    ///
    /// # Returns
    ///
    /// A new instance of `ViewControlButton`
    pub fn with_initial_view_mode(initial_view_mode: ViewMode) -> Self {
        // Create a container box for the popover content
        let popover_box = Box::builder().orientation(Vertical).spacing(6).build();

        // Add a separator for the next section (placeholder for zoom controls)
        let separator = Separator::new(Horizontal);
        popover_box.append(&separator);

        // Create a regular popover with our custom content
        let popover = Popover::builder().child(&popover_box).build();

        // Create the main split button with the popover
        let split_button = SplitButton::builder().popover(&popover).build();

        // Initialize the button with the specified initial view mode
        let button = Self {
            split_button,
            view_mode: RefCell::new(initial_view_mode),
            zoom_manager: RefCell::new(None),
            sorting_widget: RefCell::new(None),
        };

        // Update the main button to reflect the initial view mode
        button.update_main_button();
        button
    }

    /// Updates the main button's icon and tooltip based on the current view mode
    ///
    /// This method is called whenever the view mode changes to ensure the
    /// button's visual representation matches the current state.
    fn update_main_button(&self) {
        let view_mode = *self.view_mode.borrow();
        self.split_button.set_icon_name(view_mode.icon_name());
        self.split_button
            .set_tooltip_text(Some(view_mode.tooltip_text()));
    }

    /// Connects the view control button to the application's sorting system
    ///
    /// This method adds the sorting controls to the popover and connects them
    /// to the shared sorting references. It also sets up visibility logic
    /// to only show sorting criteria on the albums view, while keeping
    /// the sort direction button visible on both views.
    ///
    /// # Arguments
    ///
    /// * `sort_orders` - Shared reference to the current sort order preferences
    /// * `sort_ascending` - Shared reference to the album sort direction
    /// * `sort_ascending_artists` - Shared reference to the artist sort direction
    /// * `on_sort_changed` - Callback function to refresh the UI when sorting changes
    pub fn connect_sorting(
        &self,
        sort_orders: Rc<RefCell<Vec<SortOrder>>>,
        sort_ascending: Rc<Cell<bool>>,
        sort_ascending_artists: Rc<Cell<bool>>,
        on_sort_changed: Rc<dyn Fn(bool, bool)>,
        stack: Rc<ViewStack>,
    ) {
        // Get the popover content box
        if let Some(popover) = self.split_button.popover() {
            if let Some(popover_child) = popover.child() {
                if let Some(popover_box) = popover_child.downcast_ref::<Box>() {
                    // Create our custom sorting widget
                    let sorting_widget = create_sorting_control_row(
                        sort_orders,
                        sort_ascending,
                        sort_ascending_artists,
                        on_sort_changed,
                        stack.clone(),
                    );

                    // Store the sorting widget for visibility control
                    let sorting_widget_rc = Rc::new(sorting_widget);
                    self.sorting_widget
                        .borrow_mut()
                        .replace(sorting_widget_rc.clone());
                    popover_box.append(&*sorting_widget_rc);

                    // Get references to the child widgets we want to control
                    // The sorting widget is a vertical box with:
                    // 0: sorting_box (label + direction button) - always visible
                    // 1: criteria_label ("Drag to reorder criteria:") - hide on artist view
                    // 2: sort_listbox (sort criteria) - hide on artist view
                    let mut children = Vec::new();
                    let mut child = sorting_widget_rc.first_child();
                    while let Some(c) = child {
                        children.push(c.clone());
                        child = c.next_sibling();
                    }

                    // Get the criteria label and listbox (indices 1 and 2)
                    if children.len() >= 3 {
                        let criteria_label = children[1].clone();
                        let sort_listbox = children[2].clone();

                        // Set initial visibility based on current view
                        let current_view = stack
                            .visible_child_name()
                            .unwrap_or_else(|| VIEW_STACK_ALBUMS.into());
                        let is_albums_view = current_view.as_str() == VIEW_STACK_ALBUMS;
                        criteria_label.set_visible(is_albums_view);
                        sort_listbox.set_visible(is_albums_view);

                        // Connect to view changes to update sorting criteria visibility
                        let stack_clone = stack.clone();
                        let criteria_label_clone = criteria_label.clone();
                        let sort_listbox_clone = sort_listbox.clone();
                        stack.connect_visible_child_notify(move |_| {
                            let current_view = stack_clone
                                .visible_child_name()
                                .unwrap_or_else(|| VIEW_STACK_ALBUMS.into());
                            let is_albums_view = current_view.as_str() == VIEW_STACK_ALBUMS;
                            criteria_label_clone.set_visible(is_albums_view);
                            sort_listbox_clone.set_visible(is_albums_view);
                        });
                    }
                }
            }
        }
    }

    /// Sets the view mode of the button and updates its visual representation
    ///
    /// This method allows external code to update the button's view mode,
    /// ensuring that the button's state matches the actual view mode.
    ///
    /// # Arguments
    ///
    /// * `view_mode` - The new view mode to set
    pub fn set_view_mode(&self, view_mode: ViewMode) {
        *self.view_mode.borrow_mut() = view_mode;
        self.update_main_button();
    }

    /// Returns a reference to the underlying SplitButton widget
    ///
    /// This method allows access to the raw GTK widget for further customization
    /// or integration with other components.
    ///
    /// # Returns
    ///
    /// A reference to the internal `SplitButton` widget
    pub fn widget(&self) -> &SplitButton {
        &self.split_button
    }

    /// Connects a callback function to handle view mode changes
    ///
    /// This method sets up a click handler on the main button that cycles through
    /// available view modes when clicked.
    ///
    /// # Arguments
    ///
    /// * `on_view_mode_changed` - A callback function that will be called when the view mode changes
    pub fn connect_view_mode_changed<F>(&self, on_view_mode_changed: F)
    where
        F: Fn(ViewMode) + 'static,
    {
        let button_ref = self.clone();
        self.split_button.connect_clicked(move |_| {
            // Cycle through view modes
            let new_view_mode = match *button_ref.view_mode.borrow() {
                GridView => ListView,
                ListView => GridView,
            };

            // Update the view mode
            *button_ref.view_mode.borrow_mut() = new_view_mode;
            button_ref.update_main_button();

            // The callback will handle updating the view mode and UI
            on_view_mode_changed(new_view_mode);
        });
    }
}

/// Implements the Default trait for ViewControlButton
///
/// This allows creating a default instance of ViewControlButton using ViewControlButton::default(),
/// which initializes the button with GridView mode. This implementation follows Rust's
/// convention for providing default values for types and enables ViewControlButton to be used
/// with constructs that require the Default trait.
impl Default for ViewControlButton {
    fn default() -> Self {
        Self::with_initial_view_mode(GridView)
    }
}

impl ViewControlButton {
    /// Sets the zoom manager for the button
    ///
    /// This method allows connecting a zoom manager to the button, which
    /// enables the zoom controls in the popover.
    ///
    /// # Arguments
    ///
    /// * `zoom_manager` - The zoom manager to connect
    pub fn set_zoom_manager(&self, zoom_manager: Rc<ZoomManager>) {
        *self.zoom_manager.borrow_mut() = Some(zoom_manager);

        // Update the popover with the zoom controls
        if let Some(popover) = self.split_button.popover() {
            if let Some(popover_child) = popover.child() {
                if let Some(popover_box) = popover_child.downcast_ref::<Box>() {
                    // Create the zoom controls widget
                    let zoom_widget = create_zoom_control_row(
                        &self.zoom_manager.borrow().as_ref().unwrap().clone(),
                    );

                    // Add a separator after the zoom controls
                    let separator = Separator::new(Horizontal);

                    // Insert the zoom controls at the beginning of the popover
                    popover_box.insert_child_after(&zoom_widget, None::<&Widget>);
                    popover_box.insert_child_after(&separator, Some(&zoom_widget));
                }
            }
        }
    }
}
