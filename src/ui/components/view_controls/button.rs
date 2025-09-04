use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk4::{
    Box,
    Orientation::{Horizontal, Vertical},
    Popover, Separator,
};
use libadwaita::{
    SplitButton,
    prelude::{BoxExt, Cast, PopoverExt, WidgetExt},
};

use crate::ui::components::{
    sorting_types::SortOrder,
    view_controls::{
        sorting_controls::create_sorting_control_row, view_mode::ViewMode,
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
    view_mode: ViewMode,
}

impl ViewControlButton {
    /// Creates a new `ViewControlButton` with default settings
    ///
    /// Initializes the button with a popover menu containing various view-related
    /// options and sets the initial view mode to `GridView`.
    ///
    /// # Returns
    ///
    /// A new instance of `ViewControlButton`
    pub fn new() -> Self {
        // Create a container box for the popover content
        let popover_box = Box::builder().orientation(Vertical).spacing(6).build();

        // Create our custom zoom widget
        let zoom_widget = create_zoom_control_row();
        popover_box.append(&zoom_widget);

        // Add a separator for the next section
        let separator = Separator::new(Horizontal);
        popover_box.append(&separator);

        // Create a regular popover with our custom content
        let popover = Popover::builder().child(&popover_box).build();

        // Create the main split button with the popover
        let split_button = SplitButton::builder().popover(&popover).build();

        // Initialize the button with default view mode
        let button = Self {
            split_button,
            view_mode: ViewMode::GridView,
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
        self.split_button.set_icon_name(self.view_mode.icon_name());
        self.split_button
            .set_tooltip_text(Some(self.view_mode.tooltip_text()));
    }

    /// Connects the view control button to the application's sorting system
    ///
    /// This method adds the sorting controls to the popover and connects them
    /// to the shared sorting references.
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
                    );
                    popover_box.append(&sorting_widget);
                }
            }
        }
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
}

/// Implements the Default trait for ViewControlButton
///
/// This allows creating a default instance of ViewControlButton using ViewControlButton::default(),
/// which is equivalent to calling ViewControlButton::new(). This implementation follows Rust's
/// convention for providing default values for types and enables ViewControlButton to be used
/// with constructs that require the Default trait.
impl Default for ViewControlButton {
    fn default() -> Self {
        Self::new()
    }
}
