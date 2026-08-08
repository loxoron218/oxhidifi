//! Album format summaries computed from track metadata.

use std::borrow::Cow;

use crate::playback::layout::{AudioLayout, format_channel_label};

/// Distinct format values for an album, computed from its tracks.
#[derive(Debug, Clone, Default)]
pub struct FormatInfo {
    /// Distinct format/codec names (uppercased).
    pub formats: Vec<String>,
    /// Distinct sample rates in Hz.
    pub sample_rates: Vec<i32>,
    /// Distinct bit depths.
    pub bit_depths: Vec<i32>,
    /// Distinct channel counts.
    pub channels: Vec<i32>,
}

impl FormatInfo {
    /// Whether all tracks share the same format properties.
    #[must_use]
    pub const fn is_uniform(&self) -> bool {
        self.formats.len() <= 1 && self.sample_rates.len() <= 1 && self.bit_depths.len() <= 1
    }

    /// Compact summary for album **grid cards** (no units, no bullets).
    ///
    /// Order is always: format(s) → bit-depth(s) → sample-rate(s).
    /// Bit depth and sample rate are joined with `/` when both present.
    ///
    /// Uniform lossless: `"FLAC 24/96"`
    /// Uniform lossy:    `"MP3 44.1"`
    /// Mixed:           `"FLAC, MP3 16, 24/44.1, 96"`
    #[must_use]
    pub fn summary(&self) -> String {
        let fmt = self.formats_display();
        let bd = self
            .bit_depths
            .iter()
            .map(|&b| b.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let sr = self
            .sample_rates
            .iter()
            .map(|&hz| format_sample_rate_str(hz))
            .collect::<Vec<_>>()
            .join(", ");

        let mut parts: Vec<String> = Vec::new();
        if !fmt.is_empty() {
            parts.push(fmt);
        }
        match (!bd.is_empty(), !sr.is_empty()) {
            (true, true) => parts.push(format!("{bd}/{sr}")),
            (false, true) => parts.push(sr),
            (true, false) => parts.push(bd),
            (false, false) => {}
        }
        parts.join(" ")
    }

    /// Full summary for **detail pages** (with units and channels, matches side panel).
    ///
    /// Format first, then bit depth + sample rate grouped with ` / ` when both
    /// present, then channel label. Sample rates always show one decimal place
    /// to match the side panel (e.g. `96.0 kHz`).
    ///
    /// Uniform lossless: `"FLAC \u{2022} 24-bit / 96.0 kHz \u{2022} Stereo"`
    /// Uniform lossy:    `"MP3 \u{2022} 44.1 kHz \u{2022} Stereo"`
    /// Mixed:           `"FLAC, MP3 \u{2022} 16, 24-bit / 44.1, 96.0 kHz \u{2022} Stereo"`
    pub fn summary_detailed(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let fmt = self.formats_display();
        if !fmt.is_empty() {
            parts.push(fmt);
        }
        let bd = self.bit_depth_display();
        let sr = self.sample_rate_display();
        match (!bd.is_empty(), !sr.is_empty()) {
            (true, true) => parts.push(format!("{bd} / {sr}")),
            (true, false) => parts.push(bd),
            (false, true) => parts.push(sr),
            (false, false) => {}
        }
        if !self.channels.is_empty() {
            let ch: Vec<Cow<'static, str>> =
                self.channels.iter().copied().map(fmt_channel).collect();
            parts.push(ch.join(", "));
        }
        parts.join(" \u{2022} ")
    }

    /// Display string for the format column in column view.
    #[must_use]
    pub fn formats_display(&self) -> String {
        if self.formats.is_empty() {
            String::new()
        } else {
            self.formats.join(", ")
        }
    }

    /// Display string for sample rates. Always shows one decimal place
    /// (e.g. `96.0 kHz`) to match the side panel formatting.
    #[must_use]
    pub fn sample_rate_display(&self) -> String {
        if self.sample_rates.is_empty() {
            String::new()
        } else {
            let srs: Vec<String> = self
                .sample_rates
                .iter()
                .map(|&hz| format!("{:.1}", f64::from(hz) / 1000.0))
                .collect();
            format!("{} kHz", srs.join(", "))
        }
    }

    /// Display string for the bit depth column in column view.
    #[must_use]
    pub fn bit_depth_display(&self) -> String {
        if self.bit_depths.is_empty() {
            String::new()
        } else {
            let bds: Vec<String> = self.bit_depths.iter().map(|&b| b.to_string()).collect();
            format!("{}-bit", bds.join(", "))
        }
    }
}

/// Format a channel count to a human-readable label.
fn fmt_channel(c: i32) -> Cow<'static, str> {
    format_channel_label(AudioLayout::from_count(u32::try_from(c).unwrap_or(0)))
}

/// Format a sample rate in Hz to a short kHz string.
#[must_use]
pub fn format_sample_rate_str(hz: i32) -> String {
    if hz % 1000 == 0 {
        (hz / 1000).to_string()
    } else {
        format!("{:.1}", f64::from(hz) / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::formats::{FormatInfo, format_sample_rate_str};

    fn flac_24_96() -> FormatInfo {
        FormatInfo {
            formats: vec!["FLAC".to_string()],
            sample_rates: vec![96_000],
            bit_depths: vec![24],
            channels: vec![2],
        }
    }

    fn mixed_audio() -> FormatInfo {
        FormatInfo {
            formats: vec!["FLAC".to_string(), "MP3".to_string()],
            sample_rates: vec![44_100, 96_000],
            bit_depths: vec![16, 24],
            channels: vec![2],
        }
    }

    #[test]
    fn format_summary_uniform_lossless() {
        assert_eq!(flac_24_96().summary(), "FLAC 24/96");
    }

    #[test]
    fn format_summary_mixed() {
        assert_eq!(mixed_audio().summary(), "FLAC, MP3 16, 24/44.1, 96");
    }

    #[test]
    fn format_summary_detailed_units_and_channels() {
        assert_eq!(
            flac_24_96().summary_detailed(),
            "FLAC \u{2022} 24-bit / 96.0 kHz \u{2022} Stereo"
        );
    }

    #[test]
    fn format_is_uniform() {
        assert!(
            flac_24_96().is_uniform(),
            "single format properties are uniform"
        );
        assert!(
            !mixed_audio().is_uniform(),
            "mixed format properties are not uniform"
        );
        assert!(FormatInfo::default().is_uniform(), "empty info is uniform");
    }

    #[test]
    fn format_sample_rate_strings() {
        assert_eq!(format_sample_rate_str(44_100), "44.1");
        assert_eq!(format_sample_rate_str(96_000), "96");
        assert_eq!(format_sample_rate_str(48_000), "48");
        assert_eq!(format_sample_rate_str(192_000), "192");
    }
}
