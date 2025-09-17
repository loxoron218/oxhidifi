use gtk4::glib::markup_escape_text;

/// Helper function to highlight matching text.
///
/// This function takes a string and a query, then wraps occurrences
/// of the query within the string with `<span background='yellow'>`
/// markup for highlighting. It is case-insensitive.
///
/// # Arguments
/// * `s` - The original string to search within.
/// * `query` - The substring to highlight.
///
/// # Returns
/// A new string with the `query` parts highlighted using Pango markup.
pub fn highlight(s: &str, query: &str) -> String {
    if query.is_empty() {
        return markup_escape_text(s).to_string();
    }
    let mut result = String::new();
    let mut last = 0;
    let s_lower = s.to_lowercase();
    let q = query.to_lowercase();
    let q_len = q.len();
    let mut i = 0;
    while let Some(pos) = s_lower[i..].find(&q) {
        let start = i + pos;
        let end = start + q_len;
        result.push_str(&markup_escape_text(&s[last..start]));
        result.push_str(&format!(
            "<span background='yellow'>{}</span>",
            markup_escape_text(&s[start..end])
        ));
        last = end;
        i = end;
    }
    result.push_str(&markup_escape_text(&s[last..]));
    result
}
