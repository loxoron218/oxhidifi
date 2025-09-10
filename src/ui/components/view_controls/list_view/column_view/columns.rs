use std::{cell::Cell, rc::Rc};

use gtk4::{ColumnView, ColumnViewColumn};

use crate::utils::formatting::{format_sample_rate_khz, format_year_info};

use super::super::cell_factories::{
    create_cover_image_column, create_dr_badge_column, create_numeric_column, create_text_column,
};

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
/// * `show_dr_badges` - A `Rc<Cell<bool>>` indicating whether to show DR badges
pub fn create_columns(
    column_view: &ColumnView,
    use_original_year: bool,
    show_dr_badges: Rc<Cell<bool>>,
) {
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
        .resizable(true)
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
        .resizable(true)
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
        .title("Bit depth")
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

    // Sample rate column - displays the sample rate (e.g., 44.1 kHz)
    // Non-expanding column with "Sample Rate" title
    let sample_rate_column = ColumnViewColumn::builder()
        .title("Sample Rate")
        .expand(false)
        .build();

    // Configure the cell factory for displaying numeric values with formatting
    // The first closure extracts the sample rate value
    // The second closure formats the value (e.g., "44.1 kHz" or "Unknown")
    create_numeric_column(
        &sample_rate_column,
        // Extract sample rate value
        |album| album.sample_rate(),
        // Format the value for display
        |value| match value {
            Some(v) => {
                if v >= 0 {
                    format_sample_rate_khz(v as u32)
                } else {
                    "Unknown".to_string()
                }
            }
            None => "Unknown".to_string(),
        },
    );
    // Add the column to the ColumnView
    column_view.append_column(&sample_rate_column);

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
            album.original_release_date().as_deref(),
            use_original_year,
        )
    });

    // Add the column to the ColumnView
    column_view.append_column(&year_column);

    // Conditionally add DR column based on settings
    let dr_column = if show_dr_badges.get() {
        // DR column - displays the Dynamic Range value as a badge
        // Non-expanding column with "DR" title
        let dr_column = ColumnViewColumn::builder()
            .title("DR")
            .expand(false)
            .build();

        // Configure the cell factory for displaying DR badges
        create_dr_badge_column(&dr_column);

        // Add the column to the ColumnView
        column_view.append_column(&dr_column);

        // Return Some(dr_column) to indicate that the DR column was added
        Some(dr_column)
    } else {
        // Return None to indicate that the DR column was not added
        None
    };

    // Set up sorting for all columns
    // This connects custom sorters to each column that define how to compare items
    super::sorting::setup_column_sorting(
        column_view,
        &name_column,
        &artist_column,
        &format_column,
        &bit_depth_column,
        &sample_rate_column,
        &year_column,
        &dr_column,
    );
}
