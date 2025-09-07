use gtk4::{
    ColumnView, ColumnViewColumn, CustomSorter, PolicyType::Automatic, ScrolledWindow,
    SingleSelection, gio::ListStore,
};
use libadwaita::prelude::{Cast, SorterExt};

use crate::utils::formatting::{format_freq_khz, format_year_info};

use super::{
    cell_factories::{create_cover_image_column, create_numeric_column, create_text_column},
    data_model::{AlbumListItem, AlbumListItemObject},
};

/// Creates a ColumnView widget for displaying albums in a list format with configurable year display.
///
/// This function creates a ColumnView with the option to display original release years
/// based on user settings, similar to how the album grid works.
///
/// # Arguments
///
/// * `albums` - A vector of [`AlbumListItem`] to display in the ColumnView
/// * `use_original_year` - Whether to display the original release year instead of the release year
///
/// # Returns
///
/// A tuple containing:
/// * A [`ScrolledWindow`] that contains the ColumnView for scrollable display
/// * A [`ListStore`] model that holds the album data for the ColumnView
pub fn create_column_view_with_year_setting(
    albums: Vec<AlbumListItem>,
    use_original_year: bool,
) -> (ScrolledWindow, ListStore) {
    // Use the generic version with no activation callback
    create_column_view_with_activate_and_year_setting::<fn(&ColumnView, u32)>(
        albums,
        None,
        use_original_year,
    )
}

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
) -> (ScrolledWindow, ListStore)
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
    create_columns(&column_view, use_original_year);

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

    // Return the scrolled window and model for further use by the caller
    (scrolled_window, model)
}

/// Creates and configures all columns for the ColumnView.
///
/// This function sets up each column in the ColumnView with appropriate titles,
/// widths, and cell factories for displaying different album properties.
/// It also configures sorting for all columns.
///
/// # Arguments
///
/// * `column_view` - A reference to the [`ColumnView`] to add columns to
/// * `use_original_year` - Whether to display the original release year instead of the release year
fn create_columns(column_view: &ColumnView, use_original_year: bool) {
    // Cover column - displays album cover art
    // Fixed width column with no title (cover art is self-explanatory)
    let cover_column = ColumnViewColumn::builder()
        .title("")
        .expand(false)
        .fixed_width(60)
        .build();

    // Configure the cell factory for displaying cover images
    create_cover_image_column(&cover_column);

    // Add the column to the ColumnView
    column_view.append_column(&cover_column);

    // Album name column - displays the album title
    // Expanding column with "Album" title
    let name_column = ColumnViewColumn::builder()
        .title("Album")
        .expand(true)
        .build();

    // Configure the cell factory for displaying text (album title)
    // The closure extracts the title from the AlbumListItemObject
    create_text_column(&name_column, |album| album.title());

    // Add the column to the ColumnView
    column_view.append_column(&name_column);

    // Artist column - displays the album artist
    // Expanding column with "Artist" title
    let artist_column = ColumnViewColumn::builder()
        .title("Artist")
        .expand(true)
        .build();

    // Configure the cell factory for displaying text (artist name)
    // The closure extracts the artist from the AlbumListItemObject
    create_text_column(&artist_column, |album| album.artist());

    // Add the column to the ColumnView
    column_view.append_column(&artist_column);

    // Format column - displays the audio format (e.g., FLAC, MP3)
    // Non-expanding column with "Format" title
    let format_column = ColumnViewColumn::builder()
        .title("Format")
        .expand(false)
        .build();

    // Configure the cell factory for displaying text (format)
    // The closure extracts the format, providing "Unknown" as fallback
    create_text_column(&format_column, |album| {
        album
            .format()
            .map(|f| f.to_uppercase())
            .unwrap_or_else(|| "Unknown".to_string())
    });

    // Add the column to the ColumnView
    column_view.append_column(&format_column);

    // Bit depth column - displays the audio bit depth (e.g., 16-bit)
    // Non-expanding column with "Bit" title
    let bit_depth_column = ColumnViewColumn::builder()
        .title("Bit")
        .expand(false)
        .build();

    // Configure the cell factory for displaying numeric values with formatting
    // The first closure extracts the bit depth value
    // The second closure formats the value (e.g., "16-bit" or "Unknown")
    create_numeric_column(
        &bit_depth_column,
        // Extract bit depth value
        |album| album.bit_depth(),
        // Format the value for display
        |value| match value {
            Some(v) => format!("{}-bit", v),
            None => "Unknown".to_string(),
        },
    );

    // Add the column to the ColumnView
    column_view.append_column(&bit_depth_column);

    // Frequency column - displays the sample frequency (e.g., 44100 Hz)
    // Non-expanding column with "Freq" title
    let frequency_column = ColumnViewColumn::builder()
        .title("Freq")
        .expand(false)
        .build();

    // Configure the cell factory for displaying numeric values with formatting
    // The first closure extracts the frequency value
    // The second closure formats the value (e.g., "44100 Hz" or "Unknown")
    create_numeric_column(
        &frequency_column,
        // Extract frequency value
        |album| album.frequency(),
        // Format the value for display
        |value| match value {
            Some(v) => {
                if v >= 0 {
                    format_freq_khz(v as u32)
                } else {
                    "Unknown".to_string()
                }
            }
            None => "Unknown".to_string(),
        },
    );
    // Add the column to the ColumnView
    column_view.append_column(&frequency_column);

    // Year column - displays the release year
    // Non-expanding column with "Year" title
    let year_column = ColumnViewColumn::builder()
        .title("Year")
        .expand(false)
        .build();

    // Configure the cell factory for displaying text values with formatting
    // This allows us to access the full album information for year formatting
    create_text_column(&year_column, move |album| {
        // Format the year based on settings
        format_year_info(
            album.year(),
            album
                .item()
                .as_ref()
                .and_then(|item| item.original_release_date.as_deref()),
            use_original_year,
        )
    });

    // Add the column to the ColumnView
    column_view.append_column(&year_column);

    // DR column - displays the Dynamic Range value
    // Non-expanding column with "DR" title
    let dr_column = ColumnViewColumn::builder()
        .title("DR")
        .expand(false)
        .build();

    // Configure the cell factory for displaying numeric values with formatting
    // The first closure extracts the DR value
    // The second closure formats the value (e.g., "DR14" or "Unknown")
    create_numeric_column(
        &dr_column,
        // Extract DR value
        |album| album.dr_value(),
        // Format the value for display
        |value| match value {
            Some(v) => format!("DR{}", v),
            None => "Unknown".to_string(),
        },
    );

    // Add the column to the ColumnView
    column_view.append_column(&dr_column);

    // Set up sorting for all columns
    // This connects custom sorters to each column that define how to compare items
    setup_column_sorting(
        column_view,
        &name_column,
        &artist_column,
        &format_column,
        &bit_depth_column,
        &frequency_column,
        &year_column,
        &dr_column,
    );
}

/// Sets up sorting for all columns in the ColumnView.
///
/// This function configures custom sorters for each column that define how
/// to compare album items when sorting. It also connects to the sorter change
/// notification to handle sort order changes.
///
/// # Arguments
///
/// * `column_view` - The [`ColumnView`] to set up sorting for
/// * `name_column` - The album name column
/// * `artist_column` - The artist column
/// * `format_column` - The format column
/// * `bit_depth_column` - The bit depth column
/// * `frequency_column` - The frequency column
/// * `year_column` - The year column
/// * `dr_column` - The DR column
fn setup_column_sorting(
    column_view: &ColumnView,
    name_column: &ColumnViewColumn,
    artist_column: &ColumnViewColumn,
    format_column: &ColumnViewColumn,
    bit_depth_column: &ColumnViewColumn,
    frequency_column: &ColumnViewColumn,
    year_column: &ColumnViewColumn,
    dr_column: &ColumnViewColumn,
) {
    // Album name sorting - compares album titles alphabetically
    name_column.set_sorter(Some(&CustomSorter::new(|item1, item2| {
        // Downcast the generic items to AlbumListItemObject for access to album data
        let album1 = item1.downcast_ref::<AlbumListItemObject>().unwrap();
        let album2 = item2.downcast_ref::<AlbumListItemObject>().unwrap();

        // Extract titles and compare them
        let title1 = album1.title();
        let title2 = album2.title();

        // Convert the comparison result to the expected GTK ordering type
        title1.cmp(&title2).into()
    })));

    // Artist sorting - compares artist names alphabetically
    artist_column.set_sorter(Some(&CustomSorter::new(|item1, item2| {
        // Downcast the generic items to AlbumListItemObject for access to album data
        let album1 = item1.downcast_ref::<AlbumListItemObject>().unwrap();
        let album2 = item2.downcast_ref::<AlbumListItemObject>().unwrap();

        // Extract artist names and compare them
        let artist1 = album1.artist();
        let artist2 = album2.artist();

        // Convert the comparison result to the expected GTK ordering type
        artist1.cmp(&artist2).into()
    })));

    // Format sorting - compares audio formats alphabetically
    format_column.set_sorter(Some(&CustomSorter::new(|item1, item2| {
        // Downcast the generic items to AlbumListItemObject for access to album data
        let album1 = item1.downcast_ref::<AlbumListItemObject>().unwrap();
        let album2 = item2.downcast_ref::<AlbumListItemObject>().unwrap();

        // Extract formats, providing "Unknown" as fallback, and compare them
        let format1 = album1.format().unwrap_or_else(|| "Unknown".to_string());
        let format2 = album2.format().unwrap_or_else(|| "Unknown".to_string());

        // Convert the comparison result to the expected GTK ordering type
        format1.cmp(&format2).into()
    })));

    // Bit depth sorting - compares bit depth values numerically
    bit_depth_column.set_sorter(Some(&CustomSorter::new(|item1, item2| {
        // Downcast the generic items to AlbumListItemObject for access to album data
        let album1 = item1.downcast_ref::<AlbumListItemObject>().unwrap();
        let album2 = item2.downcast_ref::<AlbumListItemObject>().unwrap();

        // Extract bit depth values and compare them
        let bit1 = album1.bit_depth();
        let bit2 = album2.bit_depth();

        // Convert the comparison result to the expected GTK ordering type
        bit1.cmp(&bit2).into()
    })));

    // Frequency sorting - compares frequency values numerically
    frequency_column.set_sorter(Some(&CustomSorter::new(|item1, item2| {
        // Downcast the generic items to AlbumListItemObject for access to album data
        let album1 = item1.downcast_ref::<AlbumListItemObject>().unwrap();
        let album2 = item2.downcast_ref::<AlbumListItemObject>().unwrap();

        // Extract frequency values and compare them
        let freq1 = album1.frequency();
        let freq2 = album2.frequency();

        // Convert the comparison result to the expected GTK ordering type
        freq1.cmp(&freq2).into()
    })));

    // Year sorting - compares release years numerically
    year_column.set_sorter(Some(&CustomSorter::new(|item1, item2| {
        // Downcast the generic items to AlbumListItemObject for access to album data
        let album1 = item1.downcast_ref::<AlbumListItemObject>().unwrap();
        let album2 = item2.downcast_ref::<AlbumListItemObject>().unwrap();

        // Extract year values and compare them
        let year1 = album1.year();
        let year2 = album2.year();

        // Convert the comparison result to the expected GTK ordering type
        year1.cmp(&year2).into()
    })));

    // DR sorting - compares Dynamic Range values numerically
    dr_column.set_sorter(Some(&CustomSorter::new(|item1, item2| {
        // Downcast the generic items to AlbumListItemObject for access to album data
        let album1 = item1.downcast_ref::<AlbumListItemObject>().unwrap();
        let album2 = item2.downcast_ref::<AlbumListItemObject>().unwrap();

        // Extract DR values and compare them
        let dr1 = album1.dr_value();
        let dr2 = album2.dr_value();

        // Convert the comparison result to the expected GTK ordering type
        dr1.cmp(&dr2).into()
    })));

    // Connect to sort changes to handle when the user clicks column headers
    // This allows the application to respond to sort order changes
    column_view.connect_sorter_notify(|column_view| {
        // Check if a sorter is currently active
        if let Some(sorter) = column_view.sorter() {
            // In a real implementation, we would sort the model here
            // For now, just print the sort order for debugging
            println!("Sorter changed: {:?}", sorter.order());
        }
    });
}
