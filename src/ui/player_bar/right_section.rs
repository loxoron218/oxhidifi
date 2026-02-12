//! Right section of the player bar with volume control and Hi-Fi indicator.

use libadwaita::{
    gtk::{
        AccessibleRole::Button,
        Align::{Center, End},
        Box, Button as GtkButton, Label,
        Orientation::Horizontal,
        Popover, Scale, Switch, ToggleButton, Widget,
    },
    prelude::{BoxExt, Cast},
};

use crate::ui::player_bar::{
    hifi_popover::create_hifi_popover, volume_popover::create_volume_popover,
};

/// Widgets created by the right section of the player bar.
pub struct RightSectionWidgets {
    /// The container box for the right section.
    pub container: Box,
    /// Volume button (clickable icon).
    pub volume_button: GtkButton,
    /// Volume scale widget.
    pub volume_scale: Scale,
    /// Mute toggle button.
    pub mute_button: ToggleButton,
    /// Volume mode switch (app vs system).
    pub volume_mode_switch: Switch,
    /// Volume control popover.
    pub volume_popover: Popover,
    /// Hi-Fi indicator button.
    pub hifi_button: GtkButton,
    /// Hi-Fi details popover.
    pub hifi_popover: Popover,
    /// Source format label in popover.
    pub popover_source_format: Label,
    /// Source sample rate label in popover.
    pub popover_source_rate: Label,
    /// Source bit depth label in popover.
    pub popover_source_bits: Label,
    /// Processing status label in popover.
    pub popover_processing: Label,
    /// Output device label in popover.
    pub popover_output_device: Label,
    /// Output format label in popover.
    pub popover_output_format: Label,
    /// Bit-perfect badge widget.
    pub bitperfect_badge: Label,
    /// Gapless badge widget.
    pub gapless_badge: Label,
    /// Hi-Res badge widget.
    pub hires_badge: Label,
}

/// Creates the right section containing volume control and Hi-Fi indicator.
///
/// # Returns
///
/// A `RightSectionWidgets` struct containing all widgets.
#[must_use]
pub fn create_right_section() -> RightSectionWidgets {
    let right_section = Box::builder()
        .orientation(Horizontal)
        .spacing(18)
        .hexpand(false)
        .vexpand(false)
        .halign(End)
        .valign(Center)
        .width_request(280)
        .css_classes(["player-bar-right-section"])
        .build();

    let volume_button = GtkButton::builder()
        .icon_name("audio-volume-high-symbolic")
        .tooltip_text("Volume control")
        .use_underline(true)
        .has_frame(false)
        .build();
    right_section.append(volume_button.upcast_ref::<Widget>());

    let (volume_popover, volume_scale, mute_button, volume_mode_switch) =
        create_volume_popover(&volume_button);

    let hifi_button = GtkButton::builder()
        .icon_name("audio-card-symbolic")
        .tooltip_text("Audio quality and routing details")
        .css_classes(["hifi-button", "hifi-inactive", "circular"])
        .accessible_role(Button)
        .build();

    right_section.append(&hifi_button);

    let spacer = Box::builder().orientation(Horizontal).hexpand(true).build();
    right_section.prepend(&spacer);

    let (
        hifi_popover,
        popover_source_format,
        popover_source_rate,
        popover_source_bits,
        popover_processing,
        popover_output_device,
        popover_output_format,
        bitperfect_badge,
        gapless_badge,
        hires_badge,
    ) = create_hifi_popover(&hifi_button);

    RightSectionWidgets {
        container: right_section,
        volume_button,
        volume_scale,
        mute_button,
        volume_mode_switch,
        volume_popover,
        hifi_button,
        hifi_popover,
        popover_source_format,
        popover_source_rate,
        popover_source_bits,
        popover_processing,
        popover_output_device,
        popover_output_format,
        bitperfect_badge,
        gapless_badge,
        hires_badge,
    }
}
