//! Audio channel layout types and human-readable formatting.
//!
//! Provides [`AudioLayout`] to represent standard, immersive, and
//! object-based spatial audio configurations. Replaces raw channel counts
//! with a type-safe enum that the UI and playback engine can reason about.

use std::borrow::Cow;

/// Describes the spatial arrangement of audio channels.
///
/// Distinguishes between standard channel-count layouts (e.g., stereo, 5.1),
/// explicit immersive bed+height configurations (e.g., 5.1.2, 7.1.4), and
/// object-based spatial formats (Dolby Atmos, DTS:X) where the renderer
/// handles speaker mapping dynamically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioLayout {
    /// Standard channel-count layout where only the total count is known.
    Channels(u32),
    /// Object-based spatial format (Dolby Atmos, DTS:X, Sony 360 Reality Audio).
    /// The renderer maps audio objects to the available speaker array.
    Spatial,
    /// Explicit bed + height configuration.
    ///
    /// * `surround` — ear-level surround channels (excluding LFE).
    /// * `lfe` — low-frequency effect (subwoofer) channels.
    /// * `height` — overhead / height channels.
    ///
    /// Example: `Immersive { surround: 5, lfe: 1, height: 2 }` → 5.1.2.
    Immersive {
        /// Ear-level surround channel count (excluding LFE).
        surround: u8,
        /// Low-frequency effect channel count.
        lfe: u8,
        /// Height / overhead channel count.
        height: u8,
    },
}

impl AudioLayout {
    /// Construct from a raw channel count when no spatial metadata is available.
    #[must_use]
    pub fn from_count(channels: u32) -> Self {
        Self::Channels(channels)
    }

    /// Total physical channel count for this layout.
    #[must_use]
    pub fn total_channels(self) -> u32 {
        match self {
            Self::Channels(n) => n,
            Self::Spatial => 0,
            Self::Immersive {
                surround,
                lfe,
                height,
            } => u32::from(surround) + u32::from(lfe) + u32::from(height),
        }
    }
}

/// Format an [`AudioLayout`] into a human-readable UI string.
///
/// Returns [`Cow::Borrowed`] for all static standard labels (zero allocation).
/// Returns [`Cow::Owned`] only for dynamic immersive or fallback strings.
///
/// # Examples
///
/// ```
/// use crate::oxhidifi::playback::layout::{AudioLayout, format_channel_label};
///
/// assert_eq!(format_channel_label(AudioLayout::Channels(2)), "Stereo");
/// assert_eq!(format_channel_label(AudioLayout::Spatial), "Spatial Audio");
/// assert_eq!(
///     format_channel_label(AudioLayout::Immersive {
///         surround: 7,
///         lfe: 1,
///         height: 4
///     }),
///     "7.1.4 Immersive"
/// );
/// ```
#[must_use]
pub fn format_channel_label(layout: AudioLayout) -> Cow<'static, str> {
    match layout {
        AudioLayout::Spatial => Cow::Borrowed("Spatial Audio"),

        AudioLayout::Immersive {
            surround,
            lfe,
            height,
        } => Cow::Owned(format!("{surround}.{lfe}.{height} Immersive")),

        AudioLayout::Channels(count) => match count {
            1 => Cow::Borrowed("Mono"),
            2 => Cow::Borrowed("Stereo"),
            4 => Cow::Borrowed("Quadraphonic"),
            6 => Cow::Borrowed("5.1 Surround"),
            7 => Cow::Borrowed("6.1 Surround"),
            8 => Cow::Borrowed("7.1 Surround"),
            n => Cow::Owned(format!("{n}-channel")),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::playback::layout::{
        AudioLayout::{self, Channels, Immersive, Spatial},
        format_channel_label,
    };

    #[test]
    fn standard_mono() {
        assert_eq!(format_channel_label(Channels(1)), "Mono");
    }

    #[test]
    fn standard_stereo() {
        assert_eq!(format_channel_label(Channels(2)), "Stereo");
    }

    #[test]
    fn standard_quad() {
        assert_eq!(format_channel_label(Channels(4)), "Quadraphonic");
    }

    #[test]
    fn standard_5_1() {
        assert_eq!(format_channel_label(Channels(6)), "5.1 Surround");
    }

    #[test]
    fn standard_6_1() {
        assert_eq!(format_channel_label(Channels(7)), "6.1 Surround");
    }

    #[test]
    fn standard_7_1() {
        assert_eq!(format_channel_label(Channels(8)), "7.1 Surround");
    }

    #[test]
    fn fallback_unknown_count() {
        assert_eq!(format_channel_label(Channels(3)), "3-channel");
        assert_eq!(format_channel_label(Channels(12)), "12-channel");
    }

    #[test]
    fn spatial_audio() {
        assert_eq!(format_channel_label(Spatial), "Spatial Audio");
    }

    #[test]
    fn immersive_5_1_2() {
        assert_eq!(
            format_channel_label(Immersive {
                surround: 5,
                lfe: 1,
                height: 2,
            }),
            "5.1.2 Immersive"
        );
    }

    #[test]
    fn immersive_7_1_4() {
        assert_eq!(
            format_channel_label(Immersive {
                surround: 7,
                lfe: 1,
                height: 4,
            }),
            "7.1.4 Immersive"
        );
    }

    #[test]
    fn from_count_constructor() {
        let layout = AudioLayout::from_count(6);
        assert_eq!(layout, Channels(6));
    }

    #[test]
    fn total_channels_standard() {
        assert_eq!(Channels(6).total_channels(), 6);
    }

    #[test]
    fn total_channels_spatial() {
        assert_eq!(Spatial.total_channels(), 0);
    }

    #[test]
    fn total_channels_immersive() {
        let layout = Immersive {
            surround: 5,
            lfe: 1,
            height: 2,
        };
        assert_eq!(layout.total_channels(), 8);
    }
}
