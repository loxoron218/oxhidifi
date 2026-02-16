//! Hi-Fi quality display popover with audio routing information.

use libadwaita::{
    gtk::{Align, Box, Button, Label, Orientation::Vertical, Popover},
    prelude::{BoxExt, PopoverExt, WidgetExt},
};

use crate::{
    audio::engine::AudioEngine,
    ui::player_bar::hifi_calculations::{
        HifiPopoverWidgets, HifiQualityState, calculate_bit_perfect, calculate_gapless,
        calculate_hifi_button_class, calculate_hires, is_format_conversion_active, is_lossy_device,
        update_badge_css_class, update_hifi_popover_labels,
    },
};

/// CSS classes that can be applied to the Hi-Fi button indicator.
/// These are removed before applying the new state class.
const HIFI_BUTTON_CLASSES: [&str; 5] = [
    "hifi-perfect",
    "hifi-good",
    "hifi-compromised",
    "hifi-lossy",
    "hifi-inactive",
];

/// Creates the Hi-Fi details popover with audio routing information.
///
/// # Arguments
///
/// * `parent_button` - The button that triggers the popover
///
/// # Returns
///
/// A tuple containing:
/// - The popover widget
/// - The source format label
/// - The source sample rate label
/// - The source bit depth label
/// - The processing status label
/// - The output device label
/// - The output format label
/// - The bit-perfect badge widget
/// - The gapless badge widget
/// - The hi-res badge widget
#[must_use]
pub fn create_hifi_popover(
    parent_button: &Button,
) -> (
    Popover,
    Label,
    Label,
    Label,
    Label,
    Label,
    Label,
    Label,
    Label,
    Label,
) {
    let popover = Popover::builder()
        .css_classes(["hifi-popover"])
        .has_arrow(true)
        .autohide(true)
        .build();

    let content_box = Box::builder()
        .orientation(Vertical)
        .spacing(0)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .width_request(320)
        .build();

    let header_label = Label::builder()
        .label("Audio Routing Information")
        .halign(Align::Start)
        .css_classes(["heading"])
        .margin_bottom(12)
        .build();
    content_box.append(&header_label);

    let source_group = create_preferences_group("Source");

    let (popover_source_format_row, popover_source_format) = create_property_row("Format", "-");
    source_group.append(&popover_source_format_row);

    let (popover_source_rate_row, popover_source_rate) = create_property_row("Sample Rate", "-");
    source_group.append(&popover_source_rate_row);

    let (popover_source_bits_row, popover_source_bits) = create_property_row("Bit Depth", "-");
    source_group.append(&popover_source_bits_row);

    content_box.append(&source_group);

    let processing_group = create_preferences_group("Processing");

    let (popover_processing_row, popover_processing) = create_property_row("Status", "-");
    processing_group.append(&popover_processing_row);

    content_box.append(&processing_group);

    let output_group = create_preferences_group("Output");

    let (popover_output_device_row, popover_output_device) = create_property_row("Device", "-");
    output_group.append(&popover_output_device_row);

    let (popover_output_format_row, popover_output_format) = create_property_row("Format", "-");
    output_group.append(&popover_output_format_row);

    content_box.append(&output_group);

    let badges_row = Box::builder()
        .orientation(libadwaita::gtk::Orientation::Horizontal)
        .spacing(6)
        .halign(Align::Center)
        .margin_top(12)
        .build();

    let bitperfect_badge = Label::builder()
        .label("Bit-perfect")
        .css_classes(["hifi-status-badge", "inactive", "tag"])
        .build();
    bitperfect_badge.set_tooltip_text(Some(
        "Audio is being played without any modification or resampling",
    ));
    badges_row.append(&bitperfect_badge);

    let gapless_badge = Label::builder()
        .label("Gapless")
        .css_classes(["hifi-status-badge", "inactive", "tag"])
        .build();
    gapless_badge.set_tooltip_text(Some("Tracks will transition without gaps between them"));
    badges_row.append(&gapless_badge);

    let hires_badge = Label::builder()
        .label("Hi-Res")
        .css_classes(["hifi-status-badge", "inactive", "tag"])
        .build();
    hires_badge.set_tooltip_text(Some(
        "Audio has high-resolution quality (greater than 44.1kHz or 16-bit)",
    ));
    badges_row.append(&hires_badge);

    content_box.append(&badges_row);

    popover.set_child(Some(&content_box));
    popover.set_parent(parent_button);

    (
        popover,
        popover_source_format,
        popover_source_rate,
        popover_source_bits,
        popover_processing,
        popover_output_device,
        popover_output_format,
        bitperfect_badge,
        gapless_badge,
        hires_badge,
    )
}

/// Updates the Hi-Fi indicator state based on current audio configuration.
///
/// # Arguments
///
/// * `audio_engine` - Audio engine reference
/// * `hifi_button` - Hi-Fi button widget
/// * `popover_widgets` - Context struct with popover label references
/// * `bitperfect_badge` - Bit-perfect badge widget
/// * `gapless_badge` - Gapless badge widget
/// * `hires_badge` - Hi-Res badge widget
pub fn update_hifi_indicator(
    audio_engine: &AudioEngine,
    hifi_button: &Button,
    popover_widgets: &HifiPopoverWidgets<'_>,
    bitperfect_badge: &Label,
    gapless_badge: &Label,
    hires_badge: &Label,
) {
    let track_info = audio_engine.current_track_info();
    let output_config = audio_engine.output_config();
    let output_config_opt = Some(&output_config);

    let is_bit_perfect = calculate_bit_perfect(track_info.as_ref(), output_config_opt);
    let format_conversion = is_format_conversion_active(track_info.as_ref(), output_config_opt);
    let is_gapless = calculate_gapless(audio_engine);
    let is_hires = calculate_hires(track_info.as_ref());
    let is_lossy = is_lossy_device(&output_config);

    for class in HIFI_BUTTON_CLASSES {
        hifi_button.remove_css_class(class);
    }

    let new_class =
        calculate_hifi_button_class(track_info.as_ref(), is_bit_perfect, is_gapless, is_lossy);
    hifi_button.add_css_class(new_class);

    update_badge_css_class(bitperfect_badge, is_bit_perfect);
    update_badge_css_class(gapless_badge, is_gapless);
    update_badge_css_class(hires_badge, is_hires);

    update_hifi_popover_labels(
        track_info.as_ref(),
        output_config_opt,
        HifiQualityState {
            bit_perfect: is_bit_perfect,
            format_conversion,
        },
        popover_widgets,
    );
}

/// Creates a preferences group container with a title.
///
/// # Arguments
///
/// * `title` - The group title
///
/// # Returns
///
/// A configured group box widget.
fn create_preferences_group(title: &str) -> Box {
    let group = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .margin_bottom(12)
        .css_classes(["preferences-group"])
        .build();

    let title_label = Label::builder()
        .label(title)
        .halign(Align::Start)
        .css_classes(["group-title"])
        .build();
    title_label.set_margin_bottom(6);
    group.append(&title_label);

    group
}

/// Creates a property row with label and value.
///
/// # Arguments
///
/// * `label_text` - The property label
/// * `value_text` - The initial value
///
/// # Returns
///
/// A tuple containing the row box and the value label widget.
fn create_property_row(label_text: &str, value_text: &str) -> (Box, Label) {
    let row = Box::builder()
        .orientation(libadwaita::gtk::Orientation::Horizontal)
        .spacing(12)
        .css_classes(["hifi-property-row"])
        .build();

    let label = Label::builder()
        .label(label_text)
        .halign(Align::Start)
        .hexpand(true)
        .css_classes(["hifi-property-label", "dim-label"])
        .build();

    let tooltip = match label_text {
        "Format" => "Audio format of the source audio file",
        "Sample Rate" => "Sample rate of the source audio file in Hz",
        "Bit Depth" => "Bit depth of the source audio file in bits",
        "Status" => "Current audio processing status",
        "Device" => "Name of the output audio device",
        _ => "",
    };
    if !tooltip.is_empty() {
        label.set_tooltip_text(Some(tooltip));
    }
    row.append(&label);

    let value = Label::builder()
        .label(value_text)
        .halign(Align::End)
        .css_classes(["hifi-property-value"])
        .build();

    let value_tooltip = match label_text {
        "Format" => "The codec/container format of the audio file (e.g., FLAC, MP3, WAV)",
        "Sample Rate" => "Number of samples per second, affecting audio frequency range",
        "Bit Depth" => "Number of bits per sample, affecting dynamic range",
        "Status" => "Indicates if any audio processing is being applied (e.g., resampling)",
        "Device" => "The audio output device currently being used",
        _ => "",
    };
    if !value_tooltip.is_empty() {
        value.set_tooltip_text(Some(value_tooltip));
    }
    row.append(&value);

    (row, value)
}
