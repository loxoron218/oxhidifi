use std::path::Path;

use gdk_pixbuf::Pixbuf;
use gtk4::{Align::Start, Label, Picture, pango};
use libadwaita::prelude::WidgetExt;

/// Helper to create the album cover as a Picture widget.
///
/// This function takes an optional path to a cached image file and a desired size,
/// then creates a `Picture` widget displaying the scaled album cover.
/// If no path is provided or the file doesn't exist, an empty `Picture` is returned.
pub fn create_album_cover(cover_art_path: Option<&Path>, cover_size: i32) -> Picture {
    // Initialize a new Picture widget
    let pic = Picture::new();

    // Set fixed size to ensure consistent layout
    pic.set_size_request(cover_size, cover_size);

    // Align to the start (top-left) for consistent positioning
    pic.set_halign(Start);
    pic.set_valign(Start);

    // Apply CSS class for consistent border styling across the application
    pic.add_css_class("album-cover-border");

    // Attempt to load and scale the cover art if a path is provided
    if let Some(path) = cover_art_path {
        // Load the image file and scale it to fit within the specified dimensions
        // while preserving aspect ratio (the `true` parameter)
        if let Ok(pixbuf) = Pixbuf::from_file_at_scale(path, cover_size, cover_size, true) {
            // Set the scaled image as the picture's content
            pic.set_pixbuf(Some(&pixbuf));
        }
        // If loading fails, the picture remains empty but still maintains
        // the correct size and styling
    }
    pic
}

/// Helper to create the DR badge overlay if present.
///
/// This function generates a `Label` widget to display the Dynamic Range (DR) value
/// of an album. It applies specific CSS classes based on the DR value and completion status,
/// and provides a tooltip for additional information.
pub fn create_dr_overlay(dr_value: Option<u8>, dr_completed: bool) -> Option<Label> {
    // Deconstruct the DR value to determine display text, tooltip, and CSS classes
    let (dr_str, tooltip_text, mut css_classes) = match dr_value {
        // For valid DR values (0-15), format as two-digit number
        Some(value) => (
            format!("{:02}", value),
            Some("Official Dynamic Range Value"),
            vec![format!("dr-{:02}", value)],
        ),
        // For missing values, display "N/A" with appropriate styling
        None => (
            "N/A".to_string(),
            Some("Dynamic Range Value not available"),
            vec!["dr-na".to_string()],
        ),
    };

    // Create the label widget with initial properties
    let dr_label = Label::builder().label(&dr_str).build();

    // Apply base CSS classes for DR badge styling
    dr_label.add_css_class("dr-badge-label");
    dr_label.add_css_class("dr-badge-label-grid");

    // Set fixed size for consistent appearance
    dr_label.set_size_request(28, 28);

    // Add completion status styling if applicable
    if dr_completed {
        css_classes.push("dr-completed".to_string());
    }

    // Apply all computed CSS classes
    for class in css_classes {
        dr_label.add_css_class(&class);
    }

    // Set tooltip to provide context for the DR value
    dr_label.set_tooltip_text(tooltip_text);

    // Position in the bottom-right corner for overlay placement
    dr_label.set_halign(gtk4::Align::End);
    dr_label.set_valign(gtk4::Align::End);
    Some(dr_label)
}

/// Helper to create a styled label for album metadata.
///
/// This function creates a `Label` widget with common styling properties
/// for displaying album-related text, such as title, artist, format, and year.
/// It supports markup, text wrapping, ellipsizing, and custom CSS classes.
pub fn create_album_label(
    text: &str,
    css_classes: &[&str],
    max_width: Option<i32>,
    ellipsize: Option<pango::EllipsizeMode>,
    wrap: bool,
    wrap_mode: Option<pango::WrapMode>,
    lines: Option<i32>,
    use_markup: bool,
) -> Label {
    // Create label with initial properties using the builder pattern
    let builder = Label::builder()
        .label(text)
        .halign(Start)
        .use_markup(use_markup);
    let label = builder.build();

    // Set left alignment for text within the label
    label.set_xalign(0.0);

    // Apply maximum width constraint if specified
    if let Some(width) = max_width {
        label.set_max_width_chars(width);
    }

    // Configure ellipsizing behavior for text that exceeds the width
    if let Some(mode) = ellipsize {
        label.set_ellipsize(mode);
    }

    // Enable text wrapping if requested
    if wrap {
        label.set_wrap(true);
    }

    // Set wrapping mode if specified and wrapping is enabled
    if let Some(mode) = wrap_mode {
        label.set_wrap_mode(mode);
    }

    // Limit the number of displayed lines if specified
    if let Some(l) = lines {
        label.set_lines(l);
    }

    // Apply all specified CSS classes for styling
    for class in css_classes {
        label.add_css_class(class);
    }

    label
}
