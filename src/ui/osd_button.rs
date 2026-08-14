//! Circular OSD play button factory for album overlays.

use libadwaita::{
    gtk::{Align::Center, Button, accessible::Property::Label},
    prelude::AccessibleExtManual,
};

/// Build a circular OSD play button for album overlays.
#[must_use]
pub fn build_album_play_button() -> Button {
    let btn = Button::builder()
        .icon_name("media-playback-start-symbolic")
        .css_classes(["circular", "osd"])
        .halign(Center)
        .valign(Center)
        .tooltip_text("Play or pause album")
        .can_focus(true)
        .build();
    btn.update_property(&[Label("Play or pause album")]);
    btn
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, Align::Center, test},
            prelude::{ButtonExt, WidgetExt},
        },
    };

    use crate::ui::osd_button::build_album_play_button;

    #[test]
    fn build_album_play_button_sets_icon_and_tooltip() -> Result<()> {
        let button = build_album_play_button();
        ensure!(button.icon_name().as_deref() == Some("media-playback-start-symbolic"));
        ensure!(button.tooltip_text().as_deref() == Some("Play or pause album"));
        ensure!(button.css_classes().contains(&"circular".into()));
        ensure!(button.css_classes().contains(&"osd".into()));
        ensure!(button.halign() == Center);
        ensure!(button.valign() == Center);
        ensure!(button.can_focus());
        Ok(())
    }
}
