//! Album metadata text for artist detail headers.
//!
//! Pure text helpers for the two single-line labels in each album section:
//! primary `"{year} • N tracks"` and secondary `{format details}`.

use crate::storage::{catalog::Album, formats::FormatInfo};

/// Year text for the album metadata line.
///
/// # Arguments
///
/// * `album` - Album to display.
///
/// # Returns
///
/// * `String` - Release year, or `"Unknown year"` when missing.
#[must_use]
pub fn album_year_text(album: &Album) -> String {
    album
        .year
        .map_or_else(|| String::from("Unknown year"), |year| year.to_string())
}

/// Primary metadata line for the album header.
///
/// # Arguments
///
/// * `album` - Album to display.
///
/// # Returns
///
/// * `String` - e.g. `"1994 • 25 tracks"` or `"Unknown year • 25 tracks"`.
#[must_use]
pub fn album_meta_primary(album: &Album) -> String {
    format!(
        "{} \u{2022} {} tracks",
        album_year_text(album),
        album.track_count
    )
}

/// Secondary metadata line for the album header.
///
/// # Arguments
///
/// * `format_info` - Precomputed format summary for the album.
///
/// # Returns
///
/// * `String` - Detailed format summary, or empty when unknown.
#[must_use]
pub fn album_meta_secondary(format_info: &FormatInfo) -> String {
    format_info.summary_detailed()
}

/// Accessible label for the album metadata line.
///
/// # Arguments
///
/// * `album` - Album to display.
///
/// # Returns
///
/// * `String` - e.g. `"25 tracks in Selected Ambient Works, released 1994"`.
#[must_use]
pub fn album_meta_label(album: &Album) -> String {
    format!(
        "{} tracks in {}, released {}",
        album.track_count,
        album.title,
        album_year_text(album)
    )
}

/// Accessible label for the album format line.
///
/// # Arguments
///
/// * `album` - Album owning the format line.
/// * `secondary` - Visible format text; empty when unknown.
///
/// # Returns
///
/// * `String` - e.g. `"Format FLAC • 24-bit / 96.0 kHz • Stereo for Selected Ambient Works"`.
#[must_use]
pub fn album_format_label(album: &Album, secondary: &str) -> String {
    if secondary.is_empty() {
        format!("Format unknown for {}", album.title)
    } else {
        format!("Format {secondary} for {}", album.title)
    }
}

/// Unit tests for album metadata text and shared fixtures.
#[cfg(test)]
pub mod tests {
    use anyhow::{Result, ensure};

    use crate::{
        storage::{catalog::Album, formats::FormatInfo},
        ui::detail::album_meta::{
            album_format_label, album_meta_label, album_meta_primary, album_meta_secondary,
            album_year_text,
        },
    };

    /// Album fixture with a fixed title and track count, varying only by year.
    ///
    /// # Arguments
    ///
    /// * `year` - Release year to embed in the fixture.
    ///
    /// # Returns
    ///
    /// * `Album` - Fixture album for metadata tests.
    #[must_use]
    pub fn fixture_album(year: Option<i32>) -> Album {
        Album {
            id: 1,
            title: "Selected Ambient Works Volume II".into(),
            artist_id: 1,
            year,
            genre: None,
            artwork_path: None,
            track_count: 25,
            total_duration: 3000.0,
            format_summary: String::new(),
            lossless: true,
            format: "FLAC".into(),
            bit_depth: Some(24),
            sample_rate: Some(96_000),
        }
    }

    /// Uniform lossless FLAC format info matching the fixture album.
    ///
    /// # Returns
    ///
    /// * `FormatInfo` - Single-format 24-bit / 96 kHz stereo info.
    #[must_use]
    pub fn flac_info() -> FormatInfo {
        FormatInfo {
            formats: vec!["FLAC".into()],
            sample_rates: vec![96_000],
            bit_depths: vec![24],
            channels: vec![2],
        }
    }

    #[test]
    fn meta_splits_year_tracks_and_format() -> Result<()> {
        ensure!(
            album_meta_primary(&fixture_album(Some(1994))) == "1994 \u{2022} 25 tracks",
            "primary must be year plus track count"
        );
        ensure!(
            album_meta_primary(&fixture_album(None)) == "Unknown year \u{2022} 25 tracks",
            "missing year must render Unknown year"
        );
        ensure!(
            album_year_text(&fixture_album(None)) == "Unknown year",
            "year helper must return Unknown year"
        );
        let secondary = album_meta_secondary(&flac_info());
        ensure!(
            secondary.contains("FLAC") && secondary.contains("Stereo"),
            "unexpected format text: {secondary}"
        );
        ensure!(
            album_meta_secondary(&FormatInfo::default()).is_empty(),
            "empty format must yield empty secondary"
        );
        Ok(())
    }

    #[test]
    fn meta_labels_include_year_and_format() -> Result<()> {
        ensure!(
            album_meta_label(&fixture_album(Some(1994))).contains("1994"),
            "primary accessible label must include the year"
        );
        ensure!(
            album_meta_label(&fixture_album(None)).contains("Unknown year"),
            "primary accessible label must include Unknown year"
        );
        let album = fixture_album(Some(1994));
        let secondary = album_meta_secondary(&flac_info());
        ensure!(
            album_format_label(&album, &secondary).contains("FLAC"),
            "format accessible label must include the format"
        );
        ensure!(
            album_format_label(&album, "").contains("unknown"),
            "empty format accessible label must mention unknown"
        );
        Ok(())
    }
}
