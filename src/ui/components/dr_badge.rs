//! Colored DR value badges with proper styling and accessibility support.
//!
//! This module implements the `DRBadge` component that displays Dynamic Range
//! values with color coding based on quality levels, following GNOME HIG guidelines.

use libadwaita::{
    gtk::{Align::End, Label, Widget},
    prelude::{Cast, WidgetExt},
};

/// DR (Dynamic Range) quality levels with corresponding colors.
#[derive(Debug, Clone, PartialEq)]
pub enum DRQuality {
    /// Excellent quality (DR14+)
    Excellent,
    /// Good quality (DR12-DR13)
    Good,
    /// Fair quality (DR10-DR11)
    Fair,
    /// Poor quality (DR8-DR9)
    Poor,
    /// Very poor quality (DR7 or below)
    VeryPoor,
    /// Unknown or invalid DR value
    Unknown,
}

impl DRQuality {
    /// Determines the quality level from a DR value string.
    ///
    /// # Arguments
    ///
    /// * `dr_value` - The DR value string (e.g., "DR12", "DR8")
    ///
    /// # Returns
    ///
    /// The corresponding `DRQuality` enum variant.
    pub fn from_dr_value(dr_value: &str) -> Self {
        // Extract numeric part from DR value (e.g., "DR12" -> 12)
        let numeric_part = dr_value
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .collect::<String>();

        if let Ok(value) = numeric_part.parse::<i32>() {
            match value {
                14.. => DRQuality::Excellent,
                12..=13 => DRQuality::Good,
                10..=11 => DRQuality::Fair,
                8..=9 => DRQuality::Poor,
                0..=7 => DRQuality::VeryPoor,
                _ => DRQuality::Unknown,
            }
        } else {
            DRQuality::Unknown
        }
    }

    /// Gets the CSS class name for this quality level.
    ///
    /// # Returns
    ///
    /// The CSS class name as a string slice.
    pub fn css_class(&self) -> &'static str {
        match self {
            DRQuality::Excellent => "dr-14", // Using highest DR value as representative
            DRQuality::Good => "dr-12",
            DRQuality::Fair => "dr-10",
            DRQuality::Poor => "dr-08",
            DRQuality::VeryPoor => "dr-00",
            DRQuality::Unknown => "dr-na",
        }
    }

    /// Gets the ARIA label for accessibility.
    ///
    /// # Returns
    ///
    /// The ARIA label as a string slice.
    pub fn aria_label(&self) -> &'static str {
        match self {
            DRQuality::Excellent => "Excellent dynamic range quality",
            DRQuality::Good => "Good dynamic range quality",
            DRQuality::Fair => "Fair dynamic range quality",
            DRQuality::Poor => "Poor dynamic range quality",
            DRQuality::VeryPoor => "Very poor dynamic range quality",
            DRQuality::Unknown => "Unknown dynamic range quality",
        }
    }
}

/// Builder pattern for configuring DRBadge components.
#[derive(Debug, Default)]
pub struct DRBadgeBuilder {
    dr_value: Option<String>,
    show_label: bool,
}

impl DRBadgeBuilder {
    /// Sets the DR value to display.
    ///
    /// # Arguments
    ///
    /// * `dr_value` - The DR value string (e.g., "DR12")
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn dr_value(mut self, dr_value: impl Into<String>) -> Self {
        self.dr_value = Some(dr_value.into());
        self
    }

    /// Configures whether to show the "DR" label prefix.
    ///
    /// # Arguments
    ///
    /// * `show_label` - Whether to show the "DR" prefix
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn show_label(mut self, show_label: bool) -> Self {
        self.show_label = show_label;
        self
    }

    /// Builds the DRBadge component.
    ///
    /// # Returns
    ///
    /// A new `DRBadge` instance.
    pub fn build(self) -> DRBadge {
        DRBadge::new(self.dr_value, self.show_label)
    }
}

/// Colored badge displaying DR (Dynamic Range) values with quality indicators.
///
/// The `DRBadge` component displays Dynamic Range values with color coding
/// based on quality levels, providing visual feedback about audio quality.
#[derive(Clone)]
pub struct DRBadge {
    /// The underlying GTK widget.
    pub widget: Widget,
    /// The label displaying the DR value.
    pub label: Label,
    /// The current DR quality level.
    pub quality: DRQuality,
}

impl DRBadge {
    /// Creates a new DRBadge with the specified DR value.
    ///
    /// # Arguments
    ///
    /// * `dr_value` - Optional DR value string (e.g., "DR12")
    /// * `show_label` - Whether to show the "DR" prefix
    ///
    /// # Returns
    ///
    /// A new `DRBadge` instance.
    pub fn new(dr_value: Option<String>, show_label: bool) -> Self {
        let quality = dr_value
            .as_deref()
            .map(DRQuality::from_dr_value)
            .unwrap_or(DRQuality::Unknown);

        // Extract numeric part for CSS class and display
        let (display_text, css_class) = if let Some(value) = dr_value {
            if let Ok(numeric_part) = value
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .collect::<String>()
                .parse::<u8>()
            {
                let formatted_display = if show_label {
                    format!("DR{:02}", numeric_part)
                } else {
                    format!("{:02}", numeric_part)
                };
                let css_class = format!("dr-{:02}", numeric_part);
                (formatted_display, css_class)
            } else {
                // Invalid DR value format
                let display_text = if show_label {
                    "DR N/A".to_string()
                } else {
                    "N/A".to_string()
                };
                (display_text, "dr-na".to_string())
            }
        } else {
            // No DR value provided
            let display_text = if show_label {
                "DR N/A".to_string()
            } else {
                "N/A".to_string()
            };
            (display_text, "dr-na".to_string())
        };

        let label_builder = Label::builder()
            .label(&display_text)
            .halign(End)
            .valign(End)
            .width_request(28)
            .height_request(28)
            .css_classes(&["dr-badge-label", "dr-badge-label-grid"] as &[&str])
            .tooltip_text(quality.aria_label());
        let label = label_builder.build();
        label.add_css_class(&css_class);

        // Set ARIA attributes for accessibility
        // set_accessible_description doesn't exist in GTK4, remove this line

        let binding = label.clone();
        let widget = binding.upcast_ref::<Widget>();

        Self {
            widget: widget.clone(),
            label,
            quality,
        }
    }

    /// Creates a DRBadge builder for configuration.
    ///
    /// # Returns
    ///
    /// A new `DRBadgeBuilder` instance.
    pub fn builder() -> DRBadgeBuilder {
        DRBadgeBuilder::default()
    }

    /// Updates the DR value displayed by this badge.
    ///
    /// # Arguments
    ///
    /// * `dr_value` - New DR value string (e.g., "DR12")
    pub fn update_dr_value(&mut self, dr_value: Option<String>) {
        let quality = dr_value
            .as_deref()
            .map(DRQuality::from_dr_value)
            .unwrap_or(DRQuality::Unknown);

        // Extract numeric part for CSS class and display
        let (display_text, css_class) = if let Some(value) = dr_value {
            if let Ok(numeric_part) = value
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .collect::<String>()
                .parse::<u8>()
            {
                let formatted_display = format!("DR{:02}", numeric_part);
                let css_class = format!("dr-{:02}", numeric_part);
                (formatted_display, css_class)
            } else {
                // Invalid DR value format
                ("DR N/A".to_string(), "dr-na".to_string())
            }
        } else {
            // No DR value provided
            ("DR N/A".to_string(), "dr-na".to_string())
        };

        self.label.set_label(&display_text);
        self.label.set_tooltip_text(Some(quality.aria_label()));

        // Update CSS classes - first set base classes, then add dynamic class
        self.label
            .set_css_classes(&["dr-badge-label", "dr-badge-label-grid"]);
        self.label.add_css_class(&css_class);

        self.quality = quality;
    }

    /// Gets the current DR quality level.
    ///
    /// # Returns
    ///
    /// The current `DRQuality`.
    pub fn quality(&self) -> &DRQuality {
        &self.quality
    }
}

impl Default for DRBadge {
    fn default() -> Self {
        Self::new(None, true)
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::components::dr_badge::{DRBadge, DRQuality};

    #[test]
    fn test_dr_quality_from_value() {
        assert_eq!(DRQuality::from_dr_value("DR15"), DRQuality::Excellent);
        assert_eq!(DRQuality::from_dr_value("DR13"), DRQuality::Good);
        assert_eq!(DRQuality::from_dr_value("DR11"), DRQuality::Fair);
        assert_eq!(DRQuality::from_dr_value("DR9"), DRQuality::Poor);
        assert_eq!(DRQuality::from_dr_value("DR6"), DRQuality::VeryPoor);
        assert_eq!(DRQuality::from_dr_value("invalid"), DRQuality::Unknown);
        assert_eq!(DRQuality::from_dr_value(""), DRQuality::Unknown);
    }

    #[test]
    fn test_dr_quality_css_classes() {
        assert_eq!(DRQuality::Excellent.css_class(), "dr-14");
        assert_eq!(DRQuality::Good.css_class(), "dr-12");
        assert_eq!(DRQuality::Fair.css_class(), "dr-10");
        assert_eq!(DRQuality::Poor.css_class(), "dr-08");
        assert_eq!(DRQuality::VeryPoor.css_class(), "dr-00");
        assert_eq!(DRQuality::Unknown.css_class(), "dr-na");
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_dr_badge_builder() {
        let badge = DRBadge::builder()
            .dr_value("DR12")
            .show_label(false)
            .build();

        assert_eq!(badge.label.text().as_str(), "12");
        assert_eq!(badge.quality, DRQuality::Good);
    }

    #[test]
    #[ignore = "Requires GTK display for UI testing"]
    fn test_dr_badge_update() {
        let mut badge = DRBadge::new(Some("DR8".to_string()), true);
        assert_eq!(badge.quality, DRQuality::Poor);

        badge.update_dr_value(Some("DR14".to_string()));
        assert_eq!(badge.quality, DRQuality::Excellent);
        assert_eq!(badge.label.text().as_str(), "DR14");
    }
}
