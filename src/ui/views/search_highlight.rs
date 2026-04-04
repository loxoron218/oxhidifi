//! Search query text highlighting utility.
//!
//! Provides functions to highlight matching substrings of a search query
//! within result text using Pango markup with accent color.

use std::{cell::RefCell, fmt::Write, rc::Rc};

use {
    libadwaita::{gtk::Label, prelude::WidgetExt},
    num_traits::cast::NumCast,
    tracing::warn,
};

/// Escapes text for safe use in Pango markup.
///
/// # Arguments
///
/// * `text` - The text to escape
///
/// # Returns
///
/// The escaped text safe for Pango markup
fn escape_pango(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&apos;")
        .replace('"', "&quot;")
}

/// Highlights all case-insensitive contiguous substring matches of `query` in `text`.
///
/// Wraps matching substrings in `<span background="accent_hex" foreground="#ffffff">...</span>`.
/// Returns escaped plain text if query is empty or no match is found.
///
/// # Arguments
///
/// * `text` - The text to highlight
/// * `query` - The search query to match against
/// * `accent_hex` - Hex color string (e.g., "#3584e4") for highlighted background
///
/// # Returns
///
/// A Pango markup string with highlighted matches
#[must_use]
pub fn highlight_query(text: &str, query: &str, accent_hex: &str) -> String {
    if query.is_empty() {
        return escape_pango(text);
    }

    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();

    let mut result = String::with_capacity(text.len() + 64);
    let mut last_end = 0;
    let mut has_match = false;

    for start in text_lower.match_indices(&query_lower).map(|(i, _)| i) {
        has_match = true;

        let before = &text[last_end..start];
        result.push_str(&escape_pango(before));

        let matched = &text[start..start + query_lower.len()];
        if let Err(e) = write!(
            result,
            "<span background=\"{accent_hex}\" foreground=\"#ffffff\">{}</span>",
            escape_pango(matched)
        ) {
            warn!(error = %e, "Failed to write highlight markup");
        }

        last_end = start + query_lower.len();
    }

    if !has_match {
        return escape_pango(text);
    }

    result.push_str(&escape_pango(&text[last_end..]));
    result
}

/// Resolves the accent color hex string from the current theme.
///
/// Caches the result in the shared cell to avoid repeated lookups.
///
/// # Arguments
///
/// * `label` - A realized label widget to temporarily style
/// * `accent_color_hex` - Optional shared cache cell
///
/// # Returns
///
/// The accent color as a hex string (e.g., "#3584e4")
#[must_use]
pub fn resolve_accent_color(
    label: &Label,
    accent_color_hex: Option<&Rc<RefCell<Option<String>>>>,
) -> String {
    if let Some(cache) = accent_color_hex
        && let Some(cached) = cache.borrow().clone()
    {
        return cached;
    }

    label.add_css_class("accent");
    let rgba = label.color();
    label.remove_css_class("accent");

    let hex = format!(
        "#{:02x}{:02x}{:02x}",
        rgba_to_u8(rgba.red()),
        rgba_to_u8(rgba.green()),
        rgba_to_u8(rgba.blue()),
    );

    if let Some(cache) = accent_color_hex {
        *cache.borrow_mut() = Some(hex.clone());
    }

    hex
}

/// Converts an f32 color component in the range 0.0–1.0 to a u8 in the range 0–255.
///
/// Scales the value by 255, clamps to the valid range, and uses `NumCast` for a
/// safe conversion without raw `as` casts.
///
/// # Arguments
///
/// * `value` - The color component value (expected 0.0–1.0)
///
/// # Returns
///
/// The scaled and clamped value as a u8 (0–255)
fn rgba_to_u8(value: f32) -> u8 {
    // Scale from [0.0, 1.0] to [0.0, 255.0]
    let scaled = value * 255.0;

    // Clamp to prevent overflow on edge cases (e.g., 1.0001 * 255 > 255)
    let clamped = scaled.clamp(0.0, 255.0);

    // Use NumCast instead of `as u8` to satisfy clippy pedantic
    NumCast::from(clamped).unwrap_or(0)
}
