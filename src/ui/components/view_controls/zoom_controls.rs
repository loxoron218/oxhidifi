use gtk4::{Align::Start, Box, Button, Label, Orientation::Horizontal};
use libadwaita::prelude::{BoxExt, WidgetExt};

/// Creates a custom visual-only zoom control widget with flat-style buttons.
///
/// This function creates a horizontal box containing a label and two linked flat-style buttons
/// with zoom in/out icons, similar to the Nautilus file manager. The buttons have no background
/// by default and show a subtle highlight on hover, matching the GNOME HIG.
///
/// # Returns
///
/// A `gtk::Box` containing the custom zoom control widget.
pub fn create_zoom_control_row() -> Box {
    // Main Container: A horizontal box for the whole row.
    let main_box = Box::builder()
        .orientation(Horizontal)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .spacing(12)
        .build();

    // Label: The text label on the left.
    let label = Label::builder()
        .label("Icon Size")
        .halign(Start)
        .hexpand(true)
        .build();

    // Button Group Container: A box for the two buttons.
    let button_box = Box::builder().orientation(Horizontal).build();

    // Apply the .linked style for the "pill" effect.
    button_box.add_css_class("linked");

    // Buttons: The actual + and - buttons with symbolic icons.
    let zoom_out_button = Button::builder()
        .icon_name("zoom-out-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Zoom Out")
        .build();
    let zoom_in_button = Button::builder()
        .icon_name("zoom-in-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Zoom In")
        .build();

    // Assemble the Widget: Pack everything together.
    button_box.append(&zoom_out_button);
    button_box.append(&zoom_in_button);

    // Assemble the Widget: Pack everything together.
    main_box.append(&label);
    main_box.append(&button_box);
    main_box
}
