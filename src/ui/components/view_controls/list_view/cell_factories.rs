use std::{path::Path, rc::Rc};

use gtk4::{
    Align::{Center, Start},
    ColumnViewColumn, Label, ListItem, Picture, SignalListItemFactory,
    pango::EllipsizeMode::End,
};
use libadwaita::prelude::{Cast, ListItemExt, WidgetExt};

use super::data_model::AlbumListItemObject;

use crate::utils::image::AsyncImageLoader;

/// Creates a cell factory for displaying album cover images in a ColumnView column.
///
/// This function sets up a `SignalListItemFactory` that creates and manages `Picture` widgets
/// for each cell in the column. The factory handles two key phases:
/// 1. Setup phase: Creates the UI widgets for each cell
/// 2. Bind phase: Updates the widgets with data from the model
///
/// # Arguments
///
/// * `column` - The `ColumnViewColumn` to configure with the image cell factory
///
/// # Implementation Details
///
/// The function connects two signals to the factory:
/// - `setup`: Called when a new cell needs to be created, sets up the Picture widget
/// - `bind`: Called when a cell needs to be updated with data, loads the album cover
pub fn create_cover_image_column(column: &ColumnViewColumn) {
    // Create a new SignalListItemFactory which will manage the creation and updating of cells
    let factory = SignalListItemFactory::new();

    // Associate the factory with the column
    column.set_factory(Some(&factory));

    // Setup callback - creates the widgets for each cell when they are first needed
    // This is called once per cell during the initial rendering or when new cells are needed
    factory.connect_setup(move |_, list_item| {
        // Downcast the generic ListItem to the specific type we're working with
        let list_item = list_item.downcast_ref::<ListItem>().unwrap();

        // Create a Picture widget centered both horizontally and vertically
        // This will display the album cover art or a placeholder
        let picture = Picture::builder().halign(Center).valign(Center).build();
        picture.set_size_request(48, 48);

        // Set the created picture as the child widget of this list item
        list_item.set_child(Some(&picture));
    });

    // Bind callback - updates the widgets with data from the model
    // This is called whenever a cell needs to be updated with new data (e.g., scrolling)
    factory.connect_bind(move |_, list_item| {
        // Downcast the generic ListItem to the specific type we're working with
        let list_item = list_item.downcast_ref::<ListItem>().unwrap();

        // Get the Picture widget that was created in the setup phase
        let picture = list_item.child().unwrap().downcast::<Picture>().unwrap();

        // Check if there's an item associated with this list item
        if let Some(item) = list_item.item() {
            // Downcast the generic item to our specific AlbumListItemObject
            let album_item = item.downcast_ref::<AlbumListItemObject>().unwrap();

            // Get the cover art path from the album item
            let cover_art_path = album_item.cover_art();

            // Create an AsyncImageLoader to load the image asynchronously
            if let Ok(loader) = AsyncImageLoader::new() {
                // Convert the cover art path to a Path if it exists
                let path = cover_art_path.as_ref().map(|p| Path::new(p));

                // Load the image asynchronously
                loader.load_image_async(picture, path, 48);
            }
        }
    });
}

/// Creates a cell factory for displaying text fields in a ColumnView column.
///
/// This generic function creates a factory that can display any text field from an `AlbumListItemObject`.
/// It uses a closure (`field_getter`) to extract the specific field value, making it reusable for
/// different text fields like album title, artist name, etc.
///
/// # Arguments
///
/// * `column` - The `ColumnViewColumn` to configure with the text cell factory
/// * `field_getter` - A closure that extracts a String value from an `AlbumListItemObject`
///
/// # Type Parameters
///
/// * `F` - The type of the field_getter closure, which must implement `Fn(&AlbumListItemObject) -> String`
///        and have a static lifetime so it can be moved into the callbacks
///
/// # Implementation Details
///
/// The function uses `Rc` (Reference Counted) smart pointers to share the `field_getter` closure
/// between the setup and bind callbacks. This is necessary because the callbacks are moved into
/// the signal handlers and need to maintain access to the closure.
pub fn create_text_column<F>(column: &ColumnViewColumn, field_getter: F)
where
    F: Fn(&AlbumListItemObject) -> String + 'static,
{
    // Create a new SignalListItemFactory which will manage the creation and updating of cells
    let factory = SignalListItemFactory::new();

    // Associate the factory with the column
    column.set_factory(Some(&factory));

    // Wrap the field_getter in an Rc so it can be shared between callbacks
    // This is necessary because the callbacks take ownership of their captured variables
    let field_getter = Rc::new(field_getter);

    // Setup callback - creates the widgets for each cell when they are first needed
    // This is called once per cell during the initial rendering or when new cells are needed
    factory.connect_setup(move |_, list_item| {
        // Downcast the generic ListItem to the specific type we're working with
        let list_item = list_item.downcast_ref::<ListItem>().unwrap();

        // Create a Label widget with specific alignment and text properties
        let label = Label::builder()
            .halign(Start)
            .valign(Center)
            .ellipsize(End)
            .build();

        // Set the created label as the child widget of this list item
        list_item.set_child(Some(&label));
    });

    // Bind callback - updates the widgets with data from the model
    // This is called whenever a cell needs to be updated with new data (e.g., scrolling)
    factory.connect_bind(move |_, list_item| {
        // Downcast the generic ListItem to the specific type we're working with
        let list_item = list_item.downcast_ref::<ListItem>().unwrap();

        // Get the Label widget that was created in the setup phase
        let label = list_item.child().unwrap().downcast::<Label>().unwrap();

        // Clone the Rc to get access to the field_getter closure
        // This is necessary because we need to move it into the closure
        let field_getter = field_getter.clone();

        // Check if there's an item associated with this list item
        if let Some(item) = list_item.item() {
            // Downcast the generic item to our specific AlbumListItemObject
            let album_item = item.downcast_ref::<AlbumListItemObject>().unwrap();

            // Use the field_getter closure to extract the text value and set it on the label
            label.set_text(&field_getter(album_item));
        }
    });
}

/// Creates a cell factory for displaying numeric fields with optional formatting in a ColumnView column.
///
/// This generic function creates a factory for numeric fields that may or may not be present
/// (represented as `Option<i32>`). It uses two closures:
/// 1. `field_getter`: Extracts the numeric value from an `AlbumListItemObject`
/// 2. `formatter`: Converts the numeric value to a formatted String
///
/// This design allows for flexible formatting of numeric values (e.g., "16-bit", "44100 Hz", "DR14").
///
/// # Arguments
///
/// * `column` - The `ColumnViewColumn` to configure with the numeric cell factory
/// * `field_getter` - A closure that extracts an `Option<i32>` value from an `AlbumListItemObject`
/// * `formatter` - A closure that formats an `Option<i32>` into a displayable String
///
/// # Type Parameters
///
/// * `F` - The type of the field_getter closure, which must implement `Fn(&AlbumListItemObject) -> Option<i32>`
/// * `G` - The type of the formatter closure, which must implement `Fn(Option<i32>) -> String`
///
/// # Implementation Details
///
/// Both closures are wrapped in `Rc` smart pointers to allow them to be shared between the
/// setup and bind callbacks. This is necessary because the callbacks take ownership of their
/// captured variables.
pub fn create_numeric_column<F, G>(column: &ColumnViewColumn, field_getter: F, formatter: G)
where
    F: Fn(&AlbumListItemObject) -> Option<i32> + 'static,
    G: Fn(Option<i32>) -> String + 'static,
{
    // Create a new SignalListItemFactory which will manage the creation and updating of cells
    let factory = SignalListItemFactory::new();

    // Associate the factory with the column
    column.set_factory(Some(&factory));

    // Wrap the closures in Rc so they can be shared between callbacks
    // This is necessary because the callbacks take ownership of their captured variables
    let field_getter = Rc::new(field_getter);
    let formatter = Rc::new(formatter);

    // Setup callback - creates the widgets for each cell when they are first needed
    // This is called once per cell during the initial rendering or when new cells are needed
    factory.connect_setup(move |_, list_item| {
        // Downcast the generic ListItem to the specific type we're working with
        let list_item = list_item.downcast_ref::<ListItem>().unwrap();

        // Create a Label widget with specific alignment properties
        // Note: No ellipsizing is applied here as numeric fields are typically short
        let label = Label::builder().halign(Start).valign(Center).build();

        // Set the created label as the child widget of this list item
        list_item.set_child(Some(&label));
    });

    // Bind callback - updates the widgets with data from the model
    // This is called whenever a cell needs to be updated with new data (e.g., scrolling)
    factory.connect_bind(move |_, list_item| {
        // Downcast the generic ListItem to the specific type we're working with
        let list_item = list_item.downcast_ref::<ListItem>().unwrap();

        // Get the Label widget that was created in the setup phase
        let label = list_item.child().unwrap().downcast::<Label>().unwrap();

        // Clone the Rcs to get access to the closures
        // This is necessary because we need to move them into the closure
        let field_getter = field_getter.clone();
        let formatter = formatter.clone();

        // Check if there's an item associated with this list item
        if let Some(item) = list_item.item() {
            // Downcast the generic item to our specific AlbumListItemObject
            let album_item = item.downcast_ref::<AlbumListItemObject>().unwrap();

            // Extract the numeric value using the field_getter closure
            let value = field_getter(album_item);

            // Format the value using the formatter closure and set it on the label
            label.set_text(&formatter(value));
        }
    });
}

/// Creates a cell factory for displaying DR (Dynamic Range) badges in a ColumnView column.
///
/// This function creates a factory that displays DR values as color-coded badges similar to
/// the ones used in the album grid view. Each badge shows the DR value (or "N/A") with
/// appropriate styling based on the value and completion status.
///
/// # Arguments
///
/// * `column` - The `ColumnViewColumn` to configure with the DR badge cell factory
/// * `show_dr_badges` - A `Rc<Cell<bool>>` indicating whether to show DR badges
///
/// # Implementation Details
///
/// The function creates a SignalListItemFactory that manages Label widgets for each cell.
/// In the setup phase, it creates a Label with the base CSS classes for DR badges.
/// In the bind phase, it updates the Label with the correct DR value, color coding,
/// completion status, and tooltip based on the album data and the show_dr_badges setting.
pub fn create_dr_badge_column(column: &ColumnViewColumn) {
    // Create a new SignalListItemFactory which will manage the creation and updating of cells
    let factory = SignalListItemFactory::new();

    // Associate the factory with the column
    column.set_factory(Some(&factory));

    // Setup callback - creates the widgets for each cell when they are first needed
    // This is called once per cell during the initial rendering or when new cells are needed
    factory.connect_setup(move |_, list_item| {
        // Downcast the generic ListItem to the specific type we're working with
        let list_item = list_item.downcast_ref::<ListItem>().unwrap();

        // Create a Label widget for the DR badge with base CSS classes
        let label = Label::builder().halign(Center).valign(Center).build();

        // Add the base CSS classes for DR badges
        label.add_css_class("dr-badge-label");
        label.add_css_class("dr-badge-label-list");

        // Set the created label as the child widget of this list item
        list_item.set_child(Some(&label));
    });

    // Bind callback - updates the widgets with data from the model
    // This is called whenever a cell needs to be updated with new data (e.g., scrolling)
    factory.connect_bind(move |_, list_item| {
        // Downcast the generic ListItem to the specific type we're working with
        let list_item = list_item.downcast_ref::<ListItem>().unwrap();

        // Get the Label widget that was created in the setup phase
        let label = list_item.child().unwrap().downcast::<Label>().unwrap();

        // Check if there's an item associated with this list item
        if let Some(item) = list_item.item() {
            // Downcast the generic item to our specific AlbumListItemObject
            let album_item = item.downcast_ref::<AlbumListItemObject>().unwrap();

            // Get the DR value and best status from the album item
            let dr_value = album_item.dr_value();
            let dr_is_best = album_item.dr_is_best();

            // Determine the display values based on whether DR is available
            let (dr_str, tooltip_text, css_class) = match dr_value {
                Some(value) => (
                    // Format DR value as two-digit number (e.g., "08", "12")
                    format!("{:02}", value),
                    "Official Dynamic Range Value",
                    // CSS class for color coding based on DR value
                    format!("dr-{:02}", value),
                ),
                None => (
                    // Display "N/A" when DR value is not available
                    "N/A".to_string(),
                    "Dynamic Range Value not available",
                    // CSS class for "not available" state
                    "dr-na".to_string(),
                ),
            };

            // Update the label with the DR value
            label.set_text(&dr_str);

            // Set the tooltip text
            label.set_tooltip_text(Some(tooltip_text));

            // Set the CSS classes for the label
            let mut new_classes = vec!["dr-badge-label", "dr-badge-label-list"];

            // Add the value-specific CSS class
            new_classes.push(&css_class);

            // Add dr-best class if the DR value is marked as the best
            if dr_is_best {
                new_classes.push("dr-best");
            }

            // Apply the updated CSS classes
            label.set_css_classes(&new_classes);
        }
    });
}
