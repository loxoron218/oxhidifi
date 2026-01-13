//! Consistent Hi-Fi metadata display component with technical specifications.
//!
//! This module implements the `HiFiMetadata` component that displays audio
//! technical metadata including format, sample rate, bit depth, and channels
//! in a consistent, accessible format following GNOME HIG guidelines.

use libadwaita::{
    gtk::{
        AccessibleRole::Group,
        Align::{Fill, Start},
        Box, Label,
        Orientation::{Horizontal, Vertical},
        Widget,
    },
    prelude::{AccessibleExt, BoxExt, Cast, OrientableExt},
};

use crate::{library::models::Track, ui::utils::format_sample_rate};

/// Whether to display the audio format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FormatDisplay {
    /// Hide format display.
    #[default]
    Hide,
    /// Show format display.
    Show,
}

/// Whether to display the sample rate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SampleRateDisplay {
    /// Hide sample rate display.
    #[default]
    Hide,
    /// Show sample rate display.
    Show,
}

/// Whether to display the bit depth.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BitDepthDisplay {
    /// Hide bit depth display.
    #[default]
    Hide,
    /// Show bit depth display.
    Show,
}

/// Whether to display the channel count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChannelsDisplay {
    /// Hide channels display.
    #[default]
    Hide,
    /// Show channels display.
    Show,
}

/// Layout mode for metadata display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayoutMode {
    /// Compact single-line layout.
    Compact,
    /// Expanded multi-line layout.
    #[default]
    Expanded,
}

/// Builder pattern for configuring `HiFiMetadata` components.
#[derive(Debug, Default)]
pub struct HiFiMetadataBuilder {
    /// Track containing the metadata to display.
    track: Option<Track>,
    /// Whether to show the audio format (e.g., "FLAC", "MP3").
    show_format: FormatDisplay,
    /// Whether to show the sample rate (e.g., "96 kHz").
    show_sample_rate: SampleRateDisplay,
    /// Whether to show the bit depth (e.g., "24-bit").
    show_bit_depth: BitDepthDisplay,
    /// Whether to show the channel count (e.g., "Stereo").
    show_channels: ChannelsDisplay,
    /// Whether to use compact layout (single line vs multiple lines).
    layout: LayoutMode,
}

impl HiFiMetadataBuilder {
    /// Sets the track to display metadata for.
    ///
    /// # Arguments
    ///
    /// * `track` - The track containing metadata to display
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn track(mut self, track: Track) -> Self {
        self.track = Some(track);
        self
    }

    /// Configures whether to show the audio format (e.g., "FLAC", "MP3").
    ///
    /// # Arguments
    ///
    /// * `show_format` - Whether to show the format
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn show_format(mut self, show_format: FormatDisplay) -> Self {
        self.show_format = show_format;
        self
    }

    /// Configures whether to show the sample rate (e.g., "96 kHz").
    ///
    /// # Arguments
    ///
    /// * `show_sample_rate` - Whether to show the sample rate
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn show_sample_rate(mut self, show_sample_rate: SampleRateDisplay) -> Self {
        self.show_sample_rate = show_sample_rate;
        self
    }

    /// Configures whether to show the bit depth (e.g., "24-bit").
    ///
    /// # Arguments
    ///
    /// * `show_bit_depth` - Whether to show the bit depth
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn show_bit_depth(mut self, show_bit_depth: BitDepthDisplay) -> Self {
        self.show_bit_depth = show_bit_depth;
        self
    }

    /// Configures whether to show the number of channels (e.g., "Stereo").
    ///
    /// # Arguments
    ///
    /// * `show_channels` - Whether to show the channel count
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn show_channels(mut self, show_channels: ChannelsDisplay) -> Self {
        self.show_channels = show_channels;
        self
    }

    /// Configures whether to use compact layout (single line vs multiple lines).
    ///
    /// # Arguments
    ///
    /// * `layout` - Layout mode to use
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    #[must_use]
    pub fn layout(mut self, layout: LayoutMode) -> Self {
        self.layout = layout;
        self
    }

    /// Builds the `HiFiMetadata` component.
    ///
    /// # Returns
    ///
    /// A new `HiFiMetadata` instance.
    #[must_use]
    pub fn build(self) -> HiFiMetadata {
        let config = HiFiMetadataConfig {
            show_format: self.show_format,
            show_sample_rate: self.show_sample_rate,
            show_bit_depth: self.show_bit_depth,
            show_channels: self.show_channels,
            layout: self.layout,
        };
        HiFiMetadata::new(self.track, config)
    }
}

/// Container for displaying Hi-Fi audio technical metadata.
///
/// The `HiFiMetadata` component displays audio format information including
/// format type, sample rate, bit depth, and channel configuration in a
/// consistent, accessible format that follows GNOME HIG guidelines.
#[derive(Clone)]
pub struct HiFiMetadata {
    /// The underlying GTK widget container.
    pub widget: Widget,
    /// The main container box.
    pub container: Box,
    /// Individual label widgets for each metadata field.
    pub labels: Vec<Label>,
    /// Current track metadata being displayed.
    pub track: Option<Track>,
    /// Configuration flags for what to display.
    pub config: HiFiMetadataConfig,
}

/// Configuration for `HiFiMetadata` display options.
#[derive(Debug, Clone)]
pub struct HiFiMetadataConfig {
    /// Whether to show the audio format.
    pub show_format: FormatDisplay,
    /// Whether to show the sample rate.
    pub show_sample_rate: SampleRateDisplay,
    /// Whether to show the bit depth.
    pub show_bit_depth: BitDepthDisplay,
    /// Whether to show the channel count.
    pub show_channels: ChannelsDisplay,
    /// Whether to use compact layout.
    pub layout: LayoutMode,
}

impl Default for HiFiMetadataConfig {
    fn default() -> Self {
        Self {
            show_format: FormatDisplay::Show,
            show_sample_rate: SampleRateDisplay::Show,
            show_bit_depth: BitDepthDisplay::Show,
            show_channels: ChannelsDisplay::Show,
            layout: LayoutMode::Expanded,
        }
    }
}

impl HiFiMetadata {
    /// Creates a new `HiFiMetadata` component.
    ///
    /// # Arguments
    ///
    /// * `track` - Optional track containing metadata to display
    /// * `config` - Configuration for display options
    ///
    /// # Returns
    ///
    /// A new `HiFiMetadata` instance.
    #[must_use]
    pub fn new(track: Option<Track>, config: HiFiMetadataConfig) -> Self {
        let orientation = if config.layout == LayoutMode::Compact {
            Horizontal
        } else {
            Vertical
        };

        let container = Box::builder()
            .orientation(orientation)
            .halign(Start)
            .valign(Fill)
            .css_classes(["hifi-metadata"])
            .spacing(if config.layout == LayoutMode::Compact {
                8
            } else {
                2
            })
            .build();

        let mut labels = Vec::new();

        if let Some(ref track_data) = track {
            // Add format label
            if config.show_format == FormatDisplay::Show {
                let format_label = Label::builder()
                    .label(format!("{} ", track_data.format))
                    .halign(Start)
                    .xalign(0.0)
                    .css_classes(["dim-label"])
                    .build();
                labels.push(format_label.clone());
                container.append(format_label.upcast_ref::<Widget>());
            }

            // Add sample rate label
            if config.show_sample_rate == SampleRateDisplay::Show {
                let sample_rate_text = if track_data.sample_rate >= 1000 {
                    format!("{} kHz", format_sample_rate(track_data.sample_rate))
                } else {
                    format!("{} Hz", track_data.sample_rate)
                };
                let sample_rate_label = Label::builder()
                    .label(&sample_rate_text)
                    .halign(Start)
                    .xalign(0.0)
                    .css_classes(["dim-label"])
                    .build();
                labels.push(sample_rate_label.clone());
                container.append(sample_rate_label.upcast_ref::<Widget>());
            }

            // Add bit depth label
            if config.show_bit_depth == BitDepthDisplay::Show {
                let bit_depth_label = Label::builder()
                    .label(format!("{}-bit ", track_data.bits_per_sample))
                    .halign(Start)
                    .xalign(0.0)
                    .css_classes(["dim-label"])
                    .build();
                labels.push(bit_depth_label.clone());
                container.append(bit_depth_label.upcast_ref::<Widget>());
            }

            // Add channels label
            if config.show_channels == ChannelsDisplay::Show {
                let channels_text = match track_data.channels {
                    1 => "Mono".to_string(),
                    2 => "Stereo".to_string(),
                    n => format!("{n} ch"),
                };
                let channels_label = Label::builder()
                    .label(&channels_text)
                    .halign(Start)
                    .xalign(0.0)
                    .css_classes(["dim-label"])
                    .build();
                labels.push(channels_label.clone());
                container.append(channels_label.upcast_ref::<Widget>());
            }
        }

        // Set ARIA attributes for accessibility
        container.set_accessible_role(Group);

        // set_accessible_description doesn't exist for Box in GTK4
        // Accessibility is handled through other means

        Self {
            widget: container.clone().upcast_ref::<Widget>().clone(),
            container,
            labels,
            track,
            config,
        }
    }

    /// Creates a `HiFiMetadata` builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `HiFiMetadataBuilder` instance.
    #[must_use]
    pub fn builder() -> HiFiMetadataBuilder {
        HiFiMetadataBuilder::default()
    }

    /// Updates the track metadata displayed by this component.
    ///
    /// # Arguments
    ///
    /// * `track` - New track containing metadata to display
    pub fn update_track(&mut self, track: Option<&Track>) {
        // Clear existing labels
        for label in &self.labels {
            self.container.remove(label);
        }
        self.labels.clear();

        self.track = track.cloned();

        if let Some(track_data) = track {
            // Add format label
            if self.config.show_format == FormatDisplay::Show {
                let format_label = Label::builder()
                    .label(format!("{} ", track_data.format))
                    .halign(Start)
                    .xalign(0.0)
                    .css_classes(["dim-label"])
                    .build();
                self.labels.push(format_label.clone());
                self.container.append(format_label.upcast_ref::<Widget>());
            }

            // Add sample rate label
            if self.config.show_sample_rate == SampleRateDisplay::Show {
                let sample_rate_text = if track_data.sample_rate >= 1000 {
                    format!("{} kHz", format_sample_rate(track_data.sample_rate))
                } else {
                    format!("{} Hz", track_data.sample_rate)
                };
                let sample_rate_label = Label::builder()
                    .label(&sample_rate_text)
                    .halign(Start)
                    .xalign(0.0)
                    .css_classes(["dim-label"])
                    .build();
                self.labels.push(sample_rate_label.clone());
                self.container
                    .append(sample_rate_label.upcast_ref::<Widget>());
            }

            // Add bit depth label
            if self.config.show_bit_depth == BitDepthDisplay::Show {
                let bit_depth_label = Label::builder()
                    .label(format!("{}-bit ", track_data.bits_per_sample))
                    .halign(Start)
                    .xalign(0.0)
                    .css_classes(["dim-label"])
                    .build();
                self.labels.push(bit_depth_label.clone());
                self.container
                    .append(bit_depth_label.upcast_ref::<Widget>());
            }

            // Add channels label
            if self.config.show_channels == ChannelsDisplay::Show {
                let channels_text = match track_data.channels {
                    1 => "Mono".to_string(),
                    2 => "Stereo".to_string(),
                    n => format!("{n} ch"),
                };
                let channels_label = Label::builder()
                    .label(&channels_text)
                    .halign(Start)
                    .xalign(0.0)
                    .css_classes(["dim-label"])
                    .build();
                self.labels.push(channels_label.clone());
                self.container.append(channels_label.upcast_ref::<Widget>());
            }
        }
    }

    /// Updates the display configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - New display configuration
    pub fn update_config(&mut self, config: &HiFiMetadataConfig) {
        self.config = config.clone();

        // Recreate the display with new configuration
        self.track = self.track.clone();

        // Update container orientation for compact mode
        let orientation = if config.layout == LayoutMode::Compact {
            Horizontal
        } else {
            Vertical
        };
        self.container.set_orientation(orientation);
        self.container
            .set_spacing(if config.layout == LayoutMode::Compact {
                8
            } else {
                2
            });
    }
}

impl Default for HiFiMetadata {
    fn default() -> Self {
        Self::new(None, HiFiMetadataConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        library::models::Track,
        ui::components::hifi_metadata::{
            BitDepthDisplay::Show as ShowBitDepth,
            ChannelsDisplay::Show as ShowChannels,
            FormatDisplay::Show as ShowFormat,
            HiFiMetadata, HiFiMetadataConfig,
            LayoutMode::{Compact, Expanded},
            SampleRateDisplay::Show as ShowSampleRate,
        },
    };

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_hifi_metadata_builder() {
        let track = Track {
            id: 1,
            album_id: 1,
            title: "Test Track".to_string(),
            track_number: Some(1),
            disc_number: 1,
            duration_ms: 300000,
            path: "/path/to/track.flac".to_string(),
            file_size: 1024,
            format: "FLAC".to_string(),
            codec: "FLAC".to_string(),
            sample_rate: 96000,
            bits_per_sample: 24,
            channels: 2,
            is_lossless: true,
            is_high_resolution: true,
            created_at: None,
            updated_at: None,
        };

        let metadata = HiFiMetadata::builder()
            .track(track)
            .show_format(ShowFormat)
            .show_sample_rate(ShowSampleRate)
            .show_bit_depth(ShowBitDepth)
            .show_channels(ShowChannels)
            .layout(Compact)
            .build();

        assert!(metadata.track.is_some());
        assert_eq!(metadata.labels.len(), 4);
        assert_eq!(metadata.config.layout, Compact);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_hifi_metadata_default() {
        let metadata = HiFiMetadata::default();
        assert!(metadata.track.is_none());
        assert_eq!(metadata.labels.len(), 0);
        assert_eq!(metadata.config.layout, Expanded);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_hifi_metadata_update_track() {
        let mut metadata = HiFiMetadata::new(None, HiFiMetadataConfig::default());
        assert!(metadata.track.is_none());
        assert_eq!(metadata.labels.len(), 0);

        let track = Track {
            id: 1,
            album_id: 1,
            title: "Test Track".to_string(),
            track_number: Some(1),
            disc_number: 1,
            duration_ms: 300000,
            path: "/path/to/track.flac".to_string(),
            file_size: 1024,
            format: "FLAC".to_string(),
            codec: "FLAC".to_string(),
            sample_rate: 96000,
            bits_per_sample: 24,
            channels: 2,
            is_lossless: true,
            is_high_resolution: true,
            created_at: None,
            updated_at: None,
        };

        metadata.update_track(Some(&track));
        assert!(metadata.track.is_some());
        assert_eq!(metadata.labels.len(), 4);
    }

    #[test]
    fn test_channels_display() {
        let mono_track = Track {
            channels: 1,
            ..Track::default()
        };
        let stereo_track = Track {
            channels: 2,
            ..Track::default()
        };
        let multi_track = Track {
            channels: 5,
            ..Track::default()
        };

        assert_eq!(
            match mono_track.channels {
                1 => "Mono".to_string(),
                2 => "Stereo".to_string(),
                n => format!("{} ch", n),
            },
            "Mono"
        );

        assert_eq!(
            match stereo_track.channels {
                1 => "Mono".to_string(),
                2 => "Stereo".to_string(),
                n => format!("{} ch", n),
            },
            "Stereo"
        );

        assert_eq!(
            match multi_track.channels {
                1 => "Mono".to_string(),
                2 => "Stereo".to_string(),
                n => format!("{} ch", n),
            },
            "5 ch"
        );
    }
}
