//! Artist detail playback and layout actions: Play, Shuffle, Collapse-all pills.
//!
//! Groups the artist header actions in a wrapping box per the GNOME HIG: the
//! primary `Play` action keeps `suggested-action`, while the secondary
//! `Shuffle` and `Collapse all` toggles are plain `pill`s so only one action
//! is emphasized. `AdwWrapBox` flows the third pill onto a second line on the
//! 360 px minimum window instead of clipping it.

use libadwaita::{
    ButtonContent, WrapBox,
    gtk::{Align::Start, Button, ToggleButton, accessible::Property::Label},
    prelude::{AccessibleExtManual, ButtonExt, ToggleButtonExt, WidgetExt},
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

/// Tooltip for the collapse-all toggle.
///
/// # Arguments
///
/// * `expanded` - Whether album sections are currently revealed.
///
/// # Returns
///
/// * `String` - Collapse tooltip when revealed, expand tooltip when hidden.
fn collapse_all_tooltip(expanded: bool) -> String {
    if expanded {
        String::from("Collapse all albums")
    } else {
        String::from("Expand all albums")
    }
}

/// Accessible label for the collapse-all toggle.
///
/// # Arguments
///
/// * `expanded` - Whether album sections are currently revealed.
///
/// # Returns
///
/// * `String` - Collapse label when revealed, expand label when hidden.
fn collapse_all_a11y(expanded: bool) -> String {
    if expanded {
        String::from("Collapse all albums")
    } else {
        String::from("Expand all albums")
    }
}

/// Labeled icon content for the collapse-all toggle.
///
/// # Arguments
///
/// * `expanded` - Whether album sections are currently revealed.
///
/// # Returns
///
/// * `ButtonContent` - `Collapse all` content when revealed, `Expand all` content when hidden.
fn collapse_all_content(expanded: bool) -> ButtonContent {
    if expanded {
        action_content("Collapse all", "pan-down-symbolic")
    } else {
        action_content("Expand all", "pan-end-symbolic")
    }
}

/// Build the collapse-all toggle pill button.
///
/// Active means album sections are revealed, matching the per-section
/// disclosure toggles (`active` = expanded everywhere) so the master and the
/// sections share one polarity.
///
/// # Arguments
///
/// * `expanded` - Whether album sections start revealed.
///
/// # Returns
///
/// * `ToggleButton` - Plain `pill` toggle driving every album section.
#[must_use]
pub fn build_collapse_all_button(expanded: bool) -> ToggleButton {
    let button = ToggleButton::builder()
        .css_classes(["pill"])
        .tooltip_text(collapse_all_tooltip(expanded))
        .can_focus(true)
        .active(expanded)
        .child(&collapse_all_content(expanded))
        .build();
    button.update_property(&[Label(&collapse_all_a11y(expanded))]);
    button
}

/// Refresh the collapse-all label, icon, tooltip, and accessible name.
///
/// Updates visuals only; does not touch `active` so signal handlers can call
/// it without recursing into `toggled`.
///
/// # Arguments
///
/// * `button` - Collapse-all toggle to refresh.
/// * `expanded` - Whether album sections are currently revealed.
pub fn refresh_collapse_all_visual(button: &ToggleButton, expanded: bool) {
    button.set_child(Some(&collapse_all_content(expanded)));
    button.set_tooltip_text(Some(&collapse_all_tooltip(expanded)));
    button.update_property(&[Label(&collapse_all_a11y(expanded))]);
}

/// Set the collapse-all toggle state and refresh its visuals.
///
/// # Arguments
///
/// * `button` - Collapse-all toggle to update.
/// * `expanded` - Whether album sections should be revealed.
pub fn set_collapse_all_state(button: &ToggleButton, expanded: bool) {
    if button.is_active() != expanded {
        button.set_active(expanded);
    }
    refresh_collapse_all_visual(button, expanded);
}

/// Build the artist header action wrap box with Play, Shuffle, and Collapse-all.
///
/// # Arguments
///
/// * `shuffle_active` - Initial active state for the shuffle toggle.
/// * `albums_expanded` - Whether album sections start revealed.
///
/// # Returns
///
/// * `(WrapBox, Button, ToggleButton, ToggleButton)` - Wrapping action box with the play button,
///   the shuffle toggle, and the collapse-all toggle.
#[must_use]
pub fn build_artist_actions(
    shuffle_active: bool,
    albums_expanded: bool,
) -> (WrapBox, Button, ToggleButton, ToggleButton) {
    let actions = WrapBox::builder()
        .child_spacing(12)
        .line_spacing(12)
        .halign(Start)
        .build();
    actions.update_property(&[Label("Artist playback actions")]);

    let play_button = build_artist_play_button();
    actions.append(&play_button);

    let shuffle_button = build_artist_shuffle_button(shuffle_active);
    actions.append(&shuffle_button);

    let collapse_button = build_collapse_all_button(albums_expanded);
    actions.append(&collapse_button);

    (actions, play_button, shuffle_button, collapse_button)
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

    use crate::ui::detail::{
        artist_actions::{
            PLAY_TOOLTIP, SHUFFLE_TOOLTIP, build_artist_actions, build_artist_play_button,
            build_artist_shuffle_button, build_collapse_all_button, refresh_collapse_all_visual,
            set_collapse_all_state,
        },
        min_width::assert_fits_minimum_window,
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
    fn collapse_all_button_marks_expanded_state() -> Result<()> {
        let expanded = build_collapse_all_button(true);
        ensure!(expanded.is_active(), "expanded must start active");
        ensure!(
            button_content(&expanded)?.label() == "Collapse all",
            "expanded must offer collapse"
        );
        ensure!(
            expanded
                .tooltip_text()
                .is_some_and(|tip| tip.contains("Collapse")),
            "expanded needs a collapse tooltip"
        );
        let collapsed = build_collapse_all_button(false);
        ensure!(!collapsed.is_active(), "collapsed must start inactive");
        ensure!(
            button_content(&collapsed)?.label() == "Expand all",
            "collapsed must offer expand"
        );
        Ok(())
    }

    #[test]
    fn collapse_all_visual_refresh_flips_label() -> Result<()> {
        let button = build_collapse_all_button(true);
        refresh_collapse_all_visual(&button, false);
        ensure!(
            button_content(&button)?.label() == "Expand all",
            "refresh must swap to the expand label"
        );
        ensure!(button.is_active(), "refresh must not touch active state");
        set_collapse_all_state(&button, false);
        ensure!(!button.is_active(), "state setter must flip active");
        ensure!(
            button_content(&button)?.label() == "Expand all",
            "state setter must keep the expand label"
        );
        Ok(())
    }

    #[test]
    fn action_box_groups_play_shuffle_and_collapse() -> Result<()> {
        let (actions, _, _, _) = build_artist_actions(false, true);
        let mut count: usize = 0;
        let mut next = actions.first_child();
        while let Some(child) = next {
            count = count.saturating_add(1);
            next = child.next_sibling();
        }
        ensure!(
            count == 3,
            "action box must hold play, shuffle, and collapse-all"
        );
        ensure!(
            actions.child_spacing() == 12 && actions.line_spacing() == 12,
            "action wrap box must keep 12 px spacing"
        );
        Ok(())
    }

    #[test]
    fn action_wrap_box_fits_minimum_window() -> Result<()> {
        let (actions, _, _, _) = build_artist_actions(false, true);
        assert_fits_minimum_window(&actions, "wrapped actions")
    }
}
