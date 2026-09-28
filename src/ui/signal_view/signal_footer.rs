//! Signal tab device footer: active rendering device card with manual link.
//!
//! MVP renders exactly one card for the active output device: the display
//! name from CPAL device info (falling back to the device id), a generic
//! symbolic brand visual plus outline illustration (fallback icons when the
//! brand is unknown, never per-brand artwork), and the `View Product Manual`
//! link iff `manual_url` is `Some` (MVP: always `None`, so the link stays
//! hidden). Cards never name a non-rendering device; streamer-plus-DAC
//! multi-card chains are a deferred forward rule. Owned by the tab page in
//! [`signal_tab`](crate::ui::signal_view::signal_tab).

use libadwaita::{
    gtk::{
        Align::Start,
        Box, Image, Label, LinkButton,
        Orientation::{Horizontal, Vertical},
        accessible::Property::Label as A11yLabel,
    },
    prelude::{AccessibleExtManual, BoxExt, WidgetExt},
};

use crate::playback::signal_path::RenderingDevice;

/// Fallback brand tile icon when the device brand is unknown.
const FALLBACK_BRAND_ICON: &str = "audio-card-symbolic";

/// Fallback outline illustration when device art is unknown.
const FALLBACK_ART_ICON: &str = "audio-speakers-symbolic";

/// Placeholder name when a device arrives without a display name.
const UNKNOWN_DEVICE: &str = "Unknown Device";

/// Live footer widgets for the Signal tab page.
#[derive(Debug, Clone)]
pub struct SignalFooter {
    /// Card container placed below the chain content.
    root: Box,
    /// Generic symbolic brand/device tile.
    art: Image,
    /// Rendering device display name.
    name: Label,
    /// Outline device illustration.
    illustration: Image,
    /// Manual link, visible iff `manual_url` is `Some`.
    manual: LinkButton,
}

impl SignalFooter {
    /// Card container widget for the tab body.
    #[must_use]
    pub const fn widget(&self) -> &Box {
        &self.root
    }

    /// Render the MVP single-device card from the snapshot devices.
    ///
    /// Shows the first rendering device and hides the card when the list is
    /// empty (empty state). The manual link appears iff `manual_url` is
    /// `Some`; MVP snapshots always carry `None`, so the link stays hidden.
    ///
    /// # Arguments
    ///
    /// * `devices` - Snapshot rendering devices in chain order.
    pub fn apply_devices(&self, devices: &[RenderingDevice]) {
        let Some(device) = devices.first() else {
            self.hide();
            return;
        };
        let name = known_name(&device.display_name);
        self.name.set_label(&name);
        self.art
            .set_icon_name(Some(known_icon(&device.brand_visual, FALLBACK_BRAND_ICON)));
        self.illustration
            .set_icon_name(Some(known_icon(&device.illustration, FALLBACK_ART_ICON)));
        match device.manual_url.as_deref() {
            Some(url) => {
                self.manual.set_uri(url);
                self.manual.set_visible(true);
            }
            None => self.manual.set_visible(false),
        }
        let announced = format!("Rendering device {name}");
        self.root.update_property(&[A11yLabel(&announced)]);
        self.root.set_visible(true);
    }

    /// Hide the footer card (empty state with no rendering device).
    pub fn hide(&self) {
        self.root.set_visible(false);
    }

    /// Footer device name text (empty while hidden).
    #[must_use]
    pub fn device_text(&self) -> String {
        self.name.label().to_string()
    }

    /// Whether the footer card is currently visible.
    #[must_use]
    pub fn is_card_visible(&self) -> bool {
        self.root.is_visible()
    }

    /// Whether the manual link is currently visible.
    #[must_use]
    pub fn is_manual_visible(&self) -> bool {
        self.manual.is_visible()
    }
}

/// Return `value` trimmed when non-empty, otherwise the icon fallback.
fn known_icon<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}

/// Return `value` trimmed when non-empty, otherwise the unknown placeholder.
fn known_name(value: &str) -> String {
    if value.trim().is_empty() {
        String::from(UNKNOWN_DEVICE)
    } else {
        String::from(value.trim())
    }
}

/// Build the Signal tab device footer card.
///
/// # Returns
///
/// * `SignalFooter` - Handles owning the footer card.
#[must_use]
pub fn build_signal_footer() -> SignalFooter {
    let art = Image::builder()
        .icon_name(FALLBACK_BRAND_ICON)
        .pixel_size(32)
        .build();
    let name = Label::builder()
        .label("")
        .css_classes(["heading"])
        .halign(Start)
        .build();
    let illustration = Image::builder()
        .icon_name(FALLBACK_ART_ICON)
        .pixel_size(32)
        .build();
    let manual = LinkButton::with_label("about:blank", "View Product Manual");
    manual.set_visible(false);
    let row = Box::builder().orientation(Horizontal).spacing(6).build();
    row.append(&art);
    row.append(&name);
    row.append(&illustration);
    let root = Box::builder().orientation(Vertical).spacing(6).build();
    root.append(&row);
    root.append(&manual);
    root.set_visible(false);
    SignalFooter {
        root,
        art,
        name,
        illustration,
        manual,
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::gtk::{self, test},
    };

    use crate::{
        playback::signal_path::{DeviceRole::Output, RenderingDevice},
        ui::signal_view::signal_footer::build_signal_footer,
    };

    fn lab_device() -> RenderingDevice {
        RenderingDevice {
            display_name: String::from("Lab DAC"),
            role: Output,
            brand_visual: String::from("audio-card-symbolic"),
            illustration: String::from("audio-speakers-symbolic"),
            manual_url: None,
        }
    }

    #[test]
    fn footer_shows_one_card_with_hidden_manual() -> Result<()> {
        let footer = build_signal_footer();
        ensure!(!footer.is_card_visible(), "footer starts hidden");
        footer.apply_devices(&[lab_device()]);
        ensure!(footer.is_card_visible(), "device shows the card");
        ensure!(footer.device_text() == "Lab DAC", "card names the renderer");
        ensure!(!footer.is_manual_visible(), "MVP hides the manual link");
        let linked = lab_device();
        let manual = RenderingDevice {
            manual_url: Some(String::from("https://example.com/manual")),
            ..linked
        };
        footer.apply_devices(&[manual]);
        ensure!(footer.is_manual_visible(), "known manual shows the link");
        footer.apply_devices(&[]);
        ensure!(!footer.is_card_visible(), "empty devices hide the card");
        Ok(())
    }
}
