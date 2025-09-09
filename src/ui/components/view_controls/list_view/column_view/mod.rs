pub mod columns;
pub mod sorting;

use std::{cell::Cell, rc::Rc};

use gtk4::{ColumnView, PolicyType::Automatic, ScrolledWindow, SingleSelection, gio::ListStore};

use crate::ui::components::view_controls::list_view::column_view::columns::create_columns;

use super::data_model::{AlbumListItem, AlbumListItemObject};

/// Creates a ColumnView widget for displaying albums in a list format with an optional activation callback.
///
/// This is the main function for creating a ColumnView with album data. It sets up the
/// data model, selection model, ColumnView widget, columns, and scrolling container.
/// The activation callback allows the application to respond when a user selects an item.
///
/// # Generic Parameters
///
/// * `F` - The type of the activation callback function, which must have the signature
///         `Fn(&ColumnView, u32) + 'static` where the u32 parameter is the selected item's position
///
/// # Arguments
///
/// * `albums` - A vector of [`AlbumListItem`] to display in the ColumnView
/// * `on_activate` - An optional callback function to handle item activation events.
///                   If `None`, a default handler will print the position to stdout.
/// * `use_original_year` - Whether to display the original release year instead of the release year
/// * `show_dr_badges` - A `Rc<Cell<bool>>` indicating whether to show DR badges
///
/// # Returns
///
/// A tuple containing:
/// * A [`ScrolledWindow`] that contains the ColumnView for scrollable display
/// * A [`ListStore`] model that holds the album data for the ColumnView
///
/// # Example
///
/// ```rust
/// let albums = vec![/* album data */];
/// let (scrolled_window, model) = create_column_view_with_activate(albums, Some(|column_view, position| {
///     println!("Album at position {} was activated", position);
///     // Handle album selection, e.g., navigate to album details page
/// }));
/// // Add scrolled_window to your UI
/// ```
pub fn create_column_view_with_activate_and_year_setting<F>(
    albums: Vec<AlbumListItem>,
    on_activate: Option<F>,
    use_original_year: bool,
    show_dr_badges: Rc<Cell<bool>>,
) -> (ScrolledWindow, ListStore, ColumnView)
where
    F: Fn(&ColumnView, u32) + 'static,
{
    // Create the model store to hold AlbumListItemObject instances
    // This is the data source for the ColumnView
    let model = ListStore::new::<AlbumListItemObject>();

    // Populate the model with album data by converting each AlbumListItem
    // into an AlbumListItemObject (which is a GObject wrapper)
    for album in albums {
        let album_object = AlbumListItemObject::new(album);
        model.append(&album_object);
    }

    // Wrap the model with SingleSelection to make it compatible with ColumnView
    // SingleSelection allows only one item to be selected at a time
    let selection_model = SingleSelection::new(Some(model.clone()));

    // Create the ColumnView widget with the selection model
    // single_click_activate(true) enables item activation with a single click rather than double-click
    let column_view = ColumnView::builder()
        .model(&selection_model)
        .single_click_activate(true)
        .build();

    // Connect the activate signal if provided, otherwise use a default handler
    // The activate signal is emitted when a user selects an item (single click with single_click_activate(true))
    if let Some(on_activate) = on_activate {
        // Connect the provided activation callback
        column_view.connect_activate(move |column_view, position| {
            // Call the user-provided callback with the ColumnView and selected position
            on_activate(column_view, position);
        });
    } else {
        // Connect a default activate signal that simply prints the position
        // This is useful for debugging or when no specific action is needed
        column_view.connect_activate(|_column_view, position| {
            println!("Item activated at position: {}", position);
        });
    }

    // Create and configure all columns for the ColumnView
    // This includes setting up column titles, widths, and cell factories
    create_columns(&column_view, use_original_year, show_dr_badges);

    // Create a scrolled window to contain the ColumnView
    // This allows the view to be scrollable when there are more items than can fit on screen
    let scrolled_window = ScrolledWindow::builder()
        .hscrollbar_policy(Automatic)
        .vscrollbar_policy(Automatic)
        .child(&column_view)
        .min_content_height(500)
        .min_content_width(410)
        .vexpand(true)
        .margin_start(24)
        .margin_end(24)
        .margin_top(24)
        .margin_bottom(24)
        .hexpand(true)
        .build();

    // Return the scrolled window, model, and ColumnView widget for further use by the caller
    (scrolled_window, model, column_view)
}
