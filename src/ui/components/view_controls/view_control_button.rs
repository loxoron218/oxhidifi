use gtk4::{
    PopoverMenu,
    gio::{Menu, MenuItem},
};
use libadwaita::{
    SplitButton,
    prelude::{ToVariant, WidgetExt},
};

/// Represents the different view modes available in the application
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    /// Grid view layout for displaying content in a grid format
    GridView,
    /// List view layout for displaying content in a list format
    ListView,
}

impl ViewMode {
    /// Returns the icon name associated with the view mode
    ///
    /// # Returns
    ///
    /// A static string slice representing the icon name for the view mode
    pub fn icon_name(&self) -> &'static str {
        match self {
            ViewMode::GridView => "view-grid-symbolic",
            ViewMode::ListView => "view-list-symbolic",
        }
    }

    /// Returns the tooltip text associated with the view mode
    ///
    /// # Returns
    ///
    /// A static string slice representing the tooltip text for the view mode
    pub fn tooltip_text(&self) -> &'static str {
        match self {
            ViewMode::GridView => "Grid View",
            ViewMode::ListView => "List View",
        }
    }
}

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
        // Create the main menu model that will contain all menu sections
        let menu_model = Menu::new();

        // Populate the menu with different sections
        Self::add_zoom_controls_section(&menu_model);
        Self::add_sorting_options_section(&menu_model);

        // Create the popover menu using the menu model
        let popover_menu = PopoverMenu::builder().menu_model(&menu_model).build();

        // Create the main split button with the popover menu
        let split_button = SplitButton::builder().popover(&popover_menu).build();

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

    /// Adds zoom controls to the menu
    ///
    /// Creates a section with zoom in and zoom out options for adjusting
    /// the view's magnification level.
    ///
    /// # Parameters
    ///
    /// * `menu` - A reference to the main menu model to which the section will be added
    fn add_zoom_controls_section(menu: &Menu) {
        // Create a new section for zoom controls
        let section = Menu::new();

        // Create zoom in menu item with icon
        let zoom_in_item = MenuItem::new(Some("Zoom In"), Some("view.zoom-in"));
        zoom_in_item.set_attribute_value("verb-icon", Some(&"zoom-in-symbolic".to_variant()));
        section.append_item(&zoom_in_item);

        // Create zoom out menu item with icon
        let zoom_out_item = MenuItem::new(Some("Zoom Out"), Some("view.zoom-out"));
        zoom_out_item.set_attribute_value("verb-icon", Some(&"zoom-out-symbolic".to_variant()));
        section.append_item(&zoom_out_item);

        // Add the zoom controls section to the main menu
        menu.append_section(Some("Zoom"), &section);
    }

    /// Adds sorting options to the menu
    ///
    /// Creates a section with various sorting options that allow users to
    /// change how content is organized in the view.
    ///
    /// # Parameters
    ///
    /// * `menu` - A reference to the main menu model to which the section will be added
    fn add_sorting_options_section(menu: &Menu) {
        // Create a new section for sorting options
        let section = Menu::new();

        // Add different sorting options to the section
        section.append(Some("Name"), Some("view.sort-name"));
        section.append(Some("Date"), Some("view.sort-date"));
        section.append(Some("Size"), Some("view.sort-size"));
        section.append(Some("Type"), Some("view.sort-type"));

        // Add the sorting options section to the main menu
        menu.append_section(Some("Sort By"), &section);
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

    /// Sets the current view mode and updates the button appearance
    ///
    /// Changes the view mode and automatically updates the button's icon
    /// and tooltip to reflect the new mode.
    ///
    /// # Parameters
    ///
    /// * `mode` - The new `ViewMode` to set
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
        self.update_main_button();
    }

    /// Returns the current view mode
    ///
    /// # Returns
    ///
    /// The current `ViewMode` of the button
    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
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
