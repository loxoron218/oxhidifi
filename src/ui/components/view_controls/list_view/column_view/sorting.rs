use gtk4::{ColumnView, ColumnViewColumn, CustomSorter};
use libadwaita::prelude::{Cast, SorterExt};

use super::super::data_model::AlbumListItemObject;

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
/// * `sample_rate_column` - The sample rate column
/// * `year_column` - The year column
/// * `dr_column` - The DR column
pub fn setup_column_sorting(
    column_view: &ColumnView,
    name_column: &ColumnViewColumn,
    artist_column: &ColumnViewColumn,
    format_column: &ColumnViewColumn,
    bit_depth_column: &ColumnViewColumn,
    sample_rate_column: &ColumnViewColumn,
    year_column: &ColumnViewColumn,
    dr_column: &Option<ColumnViewColumn>,
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

    // Sample rate sorting - compares sample rate values numerically
    sample_rate_column.set_sorter(Some(&CustomSorter::new(|item1, item2| {
        // Downcast the generic items to AlbumListItemObject for access to album data
        let album1 = item1.downcast_ref::<AlbumListItemObject>().unwrap();
        let album2 = item2.downcast_ref::<AlbumListItemObject>().unwrap();

        // Extract sample rate values and compare them
        let sample_rate1 = album1.sample_rate();
        let sample_rate2 = album2.sample_rate();

        // Convert the comparison result to the expected GTK ordering type
        sample_rate1.cmp(&sample_rate2).into()
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
    if let Some(dr_column) = dr_column {
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
    }

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
