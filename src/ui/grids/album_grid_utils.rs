use gdk_pixbuf::{InterpType::Bilinear, PixbufLoader};
use gtk4::{
    Align::{End, Start},
    Label, Picture,
    pango::{EllipsizeMode, WrapMode},
};
use libadwaita::prelude::{PixbufLoaderExt, WidgetExt};

/// Helper function to create a styled GTK Label.
///
/// This function centralizes the creation of `gtk4::Label` widgets used for displaying
/// album metadata, ensuring consistent styling and property setting across the grid.
///
/// # Arguments
/// * `text` - The string content for the label.
/// * `css_classes` - A slice of string slices representing CSS classes to apply.
/// * `max_width` - An optional maximum character width for the label.
/// * `ellipsize` - An optional `EllipsizeMode` for text truncation.
/// * `wrap` - A boolean indicating if text should wrap.
/// * `wrap_mode` - An optional `WrapMode` for text wrapping.
/// * `lines` - An optional maximum number of lines for the label.
///
/// # Returns
/// A configured `gtk4::Label` widget.
pub fn create_styled_label(
    text: &str,
    css_classes: &[&str],
    max_width: Option<i32>,
    ellipsize: Option<EllipsizeMode>,
    wrap: bool,
    wrap_mode: Option<WrapMode>,
    lines: Option<i32>,
) -> Label {
    let label = Label::builder()
        .label(text)
        .halign(Start)
        .xalign(0.0) // Align text to the start (left) within the label's allocated space
        .build();

    if let Some(width) = max_width {
        label.set_max_width_chars(width);
    }
    if let Some(mode) = ellipsize {
        label.set_ellipsize(mode);
    }
    label.set_wrap(wrap);
    if let Some(mode) = wrap_mode {
        label.set_wrap_mode(mode);
    }
    if let Some(l) = lines {
        label.set_lines(l);
    }
    for class in css_classes {
        label.add_css_class(class);
    }
    label
}

/// Creates a `gtk4::Picture` widget for an album cover, handling scaling and fallbacks.
///
/// This function takes optional raw image data, attempts to load it into a `Pixbuf`,
/// crops it to a square, scales it to the desired `cover_size`, and applies a CSS class.
/// If no cover art is provided or loading fails, it returns an empty `Picture` with the
/// correct size and styling.
///
/// # Arguments
/// * `cover_art` - An `Option<&Vec<u8>>` containing the raw image bytes for the cover.
/// * `cover_size` - The desired size (width and height) for the square cover in pixels.
///
/// # Returns
/// A `gtk4::Picture` widget displaying the album cover or a placeholder.
pub fn create_album_cover_picture(cover_art: Option<&Vec<u8>>, cover_size: i32) -> Picture {
    let pic = Picture::new();
    pic.set_size_request(cover_size, cover_size);
    pic.set_halign(Start);
    pic.set_valign(Start);
    pic.add_css_class("album-cover-border");

    if let Some(art) = cover_art {
        let loader = PixbufLoader::new();
        if loader.write(art).is_ok() {
            loader.close().ok(); // Close the loader even if there's an error to prevent resource leaks
            if let Some(pixbuf) = loader.pixbuf() {
                let (w, h) = (pixbuf.width(), pixbuf.height());
                let side = w.min(h);
                let cropped = pixbuf.new_subpixbuf((w - side) / 2, (h - side) / 2, side, side);
                if let Some(scaled) = cropped.scale_simple(cover_size, cover_size, Bilinear) {
                    pic.set_pixbuf(Some(&scaled));
                }
            }
        }
    }
    pic
}

/// Creates a `gtk4::Label` for the Dynamic Range (DR) badge overlay.
///
/// This label displays the DR value (or "N/A"), applies appropriate CSS classes
/// based on the value, sets a tooltip, and sizes it for the grid view.
///
/// # Arguments
/// * `dr_value` - An `Option<u8>` representing the album's DR value.
/// * `dr_completed` - A boolean indicating if the DR value has been marked as completed.
///
/// # Returns
/// A `gtk4::Label` widget configured as a DR badge.
pub fn create_dr_badge_label(dr_value: Option<u8>, dr_completed: bool) -> Label {
    let (dr_str, tooltip_text, mut css_classes) = match dr_value {
        Some(value) => (
            format!("{:02}", value),
            Some("Official Dynamic Range Value"),
            vec![format!("dr-{:02}", value)],
        ),
        None => (
            "N/A".to_string(),
            Some("Dynamic Range Value not available"),
            vec!["dr-na".to_string()],
        ),
    };

    let dr_label = Label::builder()
        .label(&dr_str)
        .css_classes(&["dr-badge-label", "dr-badge-label-grid"] as &[&str])
        .tooltip_text(
            tooltip_text
                .map(|s| s.to_string())
                .unwrap_or_else(String::new),
        )
        .halign(End)
        .valign(End)
        .build();
    dr_label.set_size_request(28, 28); // Fixed size for the badge in the grid

    if dr_completed {
        css_classes.push("dr-completed".to_string());
    }
    for class in css_classes {
        dr_label.add_css_class(&class);
    }
    dr_label
}
