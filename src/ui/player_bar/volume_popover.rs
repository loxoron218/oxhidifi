//! Volume control popover for the player bar.

use {
    libadwaita::{
        glib::Propagation::Proceed,
        gtk::{
            Align::{Center, End, Start},
            Box, Button, Label,
            Orientation::{Horizontal, Vertical},
            Popover, Scale, Switch, ToggleButton, Widget,
        },
        prelude::{BoxExt, ButtonExt, Cast, PopoverExt, RangeExt, ToggleButtonExt, WidgetExt},
    },
    tracing::debug,
};

/// Creates the volume control popover with mute, scale, and mode switch.
///
/// # Arguments
///
/// * `parent_button` - The button that triggers the popover
///
/// # Returns
///
/// A tuple containing:
/// - The popover widget
/// - The volume scale widget
/// - The mute toggle button
/// - The volume mode switch
#[must_use]
pub fn create_volume_popover(parent_button: &Button) -> (Popover, Scale, ToggleButton, Switch) {
    let popover = Popover::builder()
        .css_classes(["volume-popover"])
        .has_arrow(true)
        .autohide(true)
        .build();

    let content_box = Box::builder()
        .orientation(Vertical)
        .spacing(12)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .width_request(280)
        .build();

    let volume_box = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .hexpand(true)
        .valign(Center)
        .build();

    let volume_scale = Scale::builder()
        .orientation(Horizontal)
        .draw_value(false)
        .hexpand(true)
        .build();
    volume_scale.set_range(0.0, 100.0);
    volume_scale.set_value(100.0);
    volume_box.append(volume_scale.upcast_ref::<Widget>());

    let mute_button = ToggleButton::builder()
        .icon_name("audio-volume-high-symbolic")
        .tooltip_text("Mute")
        .use_underline(true)
        .has_frame(false)
        .build();
    volume_box.append(mute_button.upcast_ref::<Widget>());

    content_box.append(&volume_box);

    let mode_switch_container = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .margin_top(6)
        .build();

    let mode_label = Label::builder()
        .label("Application Volume")
        .halign(Start)
        .css_classes(["volume-mode-label"])
        .build();
    mode_switch_container.append(&mode_label);

    let mode_subtitle_label = Label::builder()
        .label("Use System volume for bit-perfect audio")
        .halign(Start)
        .css_classes(["volume-mode-subtitle", "dim-label"])
        .build();
    mode_subtitle_label.set_margin_bottom(12);
    mode_switch_container.append(&mode_subtitle_label);

    let mode_row = Box::builder()
        .orientation(Horizontal)
        .spacing(6)
        .halign(End)
        .margin_top(2)
        .build();
    mode_row.append(&mode_switch_container);

    let volume_mode_switch = Switch::builder()
        .valign(Center)
        .tooltip_text("Toggle between application and system volume control")
        .build();
    mode_row.append(&volume_mode_switch);

    content_box.append(&mode_row);

    popover.set_child(Some(&content_box));
    popover.set_parent(parent_button);

    (popover, volume_scale, mute_button, volume_mode_switch)
}

/// Connects event handlers for volume controls.
///
/// # Arguments
///
/// * `volume_button` - Volume button widget
/// * `volume_scale` - Volume scale widget
/// * `mute_button` - Mute toggle button
/// * `volume_mode_switch` - Volume mode switch widget
pub fn connect_volume_handlers(
    volume_button: &Button,
    volume_scale: &Scale,
    mute_button: &ToggleButton,
    volume_mode_switch: &Switch,
) {
    let volume_button_for_scale = volume_button.clone();
    let mute_button_for_scale = mute_button.clone();

    volume_scale.connect_value_changed(move |scale: &Scale| {
        let volume = scale.value() / 100.0;

        debug!(volume = %volume, "Volume changed");

        let icon_name = if mute_button_for_scale.is_active() {
            "audio-volume-muted-symbolic"
        } else if volume < 0.3 {
            "audio-volume-low-symbolic"
        } else if volume < 0.7 {
            "audio-volume-medium-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        volume_button_for_scale.set_icon_name(icon_name);
    });

    let volume_button_for_mute = volume_button.clone();
    let volume_scale_for_mute = volume_scale.clone();

    mute_button.connect_toggled(move |button: &ToggleButton| {
        let muted = button.is_active();

        debug!(muted = %muted, "Mute state changed");

        if muted {
            volume_button_for_mute.set_icon_name("audio-volume-muted-symbolic");
        } else {
            let volume = volume_scale_for_mute.value() / 100.0;
            let icon_name = if volume < 0.3 {
                "audio-volume-low-symbolic"
            } else if volume < 0.7 {
                "audio-volume-medium-symbolic"
            } else {
                "audio-volume-high-symbolic"
            };
            volume_button_for_mute.set_icon_name(icon_name);
        }
    });

    let volume_mode_switch_clone = volume_mode_switch.clone();
    let volume_button_for_mode = volume_button.clone();
    volume_mode_switch_clone.connect_state_set(move |_switch, state| {
        let mode = if state { "system" } else { "app" };
        debug!(mode = %mode, "Volume mode changed");

        if state {
            volume_button_for_mode.remove_css_class("inactive");
        } else {
            volume_button_for_mode.add_css_class("inactive");
        }

        Proceed
    });
}
