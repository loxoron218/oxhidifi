//! Artist detail playback actions: Play and Shuffle pill buttons.
//!
//! Groups the artist header actions in a single horizontal box per the GNOME
//! HIG: the primary `Play` action keeps `suggested-action`, while the
//! secondary `Shuffle` toggle is a plain `pill` so only one action is
//! emphasized.

use libadwaita::{
    ButtonContent,
    gtk::{
        Align::Start, Box, Button, Orientation::Horizontal, ToggleButton,
        accessible::Property::Label,
    },
    prelude::{AccessibleExtManual, BoxExt},
};

/// Tooltip and accessible label for ordered artist playback.
const PLAY_TOOLTIP: &str = "Play all albums by this artist";

/// Tooltip and accessible label for shuffled artist playback.
const SHUFFLE_TOOLTIP: &str = "Shuffle all songs by this artist";

/// Build labeled icon content for an action button.
///
/// GTK buttons drop their text when `label` and `icon-name` are set directly,
/// so icon-plus-label buttons need an explicit [`ButtonContent`] child.
///
/// # Arguments
///
/// * `label` - Visible button text.
/// * `icon_name` - Symbolic icon name.
///
/// # Returns
///
/// * `ButtonContent` - Icon-plus-label content with mnemonics enabled.
fn action_content(label: &str, icon_name: &str) -> ButtonContent {
    ButtonContent::builder()
        .label(label)
        .icon_name(icon_name)
        .use_underline(true)
        .build()
}

/// Build the primary ordered-playback pill button.
///
/// # Returns
///
/// * `Button` - `suggested-action` pill with `Play` label and start icon.
#[must_use]
pub fn build_artist_play_button() -> Button {
    let button = Button::builder()
        .css_classes(["suggested-action", "pill"])
        .tooltip_text(PLAY_TOOLTIP)
        .can_focus(true)
        .child(&action_content("Play", "media-playback-start-symbolic"))
        .build();
    button.update_property(&[Label(PLAY_TOOLTIP)]);
    button
}

/// Build the secondary shuffle toggle pill button.
///
/// The toggle reflects the persistent shuffle mode: active while shuffled
/// playback is on. It is intentionally not `suggested-action` so only the
/// primary `Play` action is emphasized.
///
/// # Arguments
///
/// * `active` - Whether shuffle mode is currently enabled.
///
/// # Returns
///
/// * `ToggleButton` - Plain `pill` toggle with `Shuffle` label and icon.
#[must_use]
pub fn build_artist_shuffle_button(active: bool) -> ToggleButton {
    let button = ToggleButton::builder()
        .css_classes(["pill"])
        .tooltip_text(SHUFFLE_TOOLTIP)
        .can_focus(true)
        .active(active)
        .child(&action_content(
            "Shuffle",
            "media-playlist-shuffle-symbolic",
        ))
        .build();
    button.update_property(&[Label(SHUFFLE_TOOLTIP)]);
    button
}

/// Build the artist header action box with Play and Shuffle buttons.
///
/// # Arguments
///
/// * `shuffle_active` - Initial active state for the shuffle toggle.
///
/// # Returns
///
/// * `(Box, Button, ToggleButton)` - Horizontal action box with the play button and the shuffle
///   toggle in order.
#[must_use]
pub fn build_artist_actions(shuffle_active: bool) -> (Box, Button, ToggleButton) {
    let actions = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .halign(Start)
        .build();
    actions.update_property(&[Label("Artist playback actions")]);

    let play_button = build_artist_play_button();
    actions.append(&play_button);

    let shuffle_button = build_artist_shuffle_button(shuffle_active);
    actions.append(&shuffle_button);

    (actions, play_button, shuffle_button)
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, anyhow, ensure},
        libadwaita::{
            ButtonContent,
            gtk::{self, test},
            prelude::{ButtonExt, Cast, ToggleButtonExt, WidgetExt},
        },
    };

    use crate::ui::detail::artist_actions::{
        PLAY_TOOLTIP, SHUFFLE_TOOLTIP, build_artist_actions, build_artist_play_button,
        build_artist_shuffle_button,
    };

    fn button_content(button: &impl ButtonExt) -> Result<ButtonContent> {
        let child = button
            .child()
            .ok_or_else(|| anyhow!("button must have labeled content"))?;
        child
            .downcast::<ButtonContent>()
            .map_err(|child| anyhow!("button content must be ButtonContent, got {child:?}"))
    }

    #[test]
    fn play_button_is_suggested_pill_with_label_and_icon() -> Result<()> {
        let button = build_artist_play_button();
        ensure!(
            button.css_classes().contains(&"suggested-action".into()),
            "play must be suggested-action"
        );
        ensure!(
            button.css_classes().contains(&"pill".into()),
            "play must be pill shaped"
        );
        let content = button_content(&button)?;
        ensure!(content.label() == "Play", "play needs a text label");
        ensure!(
            content.icon_name() == "media-playback-start-symbolic",
            "play needs the start icon"
        );
        ensure!(content.uses_underline(), "play needs a mnemonic");
        ensure!(
            button.tooltip_text().as_deref() == Some(PLAY_TOOLTIP),
            "play needs its tooltip"
        );
        ensure!(button.can_focus(), "play must be focusable");
        Ok(())
    }

    #[test]
    fn shuffle_button_is_plain_pill_toggle() -> Result<()> {
        let button = build_artist_shuffle_button(false);
        ensure!(
            !button.css_classes().contains(&"suggested-action".into()),
            "shuffle must not steal emphasis"
        );
        ensure!(
            button.css_classes().contains(&"pill".into()),
            "shuffle must be pill shaped"
        );
        let content = button_content(&button)?;
        ensure!(content.label() == "Shuffle", "shuffle needs a text label");
        ensure!(
            content.icon_name() == "media-playlist-shuffle-symbolic",
            "shuffle needs the shuffle icon"
        );
        ensure!(content.uses_underline(), "shuffle needs a mnemonic");
        ensure!(
            button.tooltip_text().as_deref() == Some(SHUFFLE_TOOLTIP),
            "shuffle needs its tooltip"
        );
        ensure!(!button.is_active(), "shuffle must start inactive");
        ensure!(
            build_artist_shuffle_button(true).is_active(),
            "shuffle must honor its initial state"
        );
        Ok(())
    }

    #[test]
    fn action_box_groups_play_before_shuffle() -> Result<()> {
        let (actions, _, _) = build_artist_actions(false);
        let first = actions.first_child();
        let second = first.as_ref().and_then(WidgetExt::next_sibling);
        let third = second.as_ref().and_then(WidgetExt::next_sibling);
        ensure!(
            first.is_some() && second.is_some() && third.is_none(),
            "action box must hold exactly play and shuffle"
        );
        Ok(())
    }
}
