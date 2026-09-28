//! Signal tab chain rows: badge icon, title/detail, and inline explainer.
//!
//! One selectable [`ListBoxRow`] per [`PathStage`] in snapshot `position`
//! order. The trailing verdict mark carries the per-stage quality indicator
//! as an icon plus a text label, never color alone. Each row also owns a
//! hidden plain-language explanation revealed inline directly under its
//! title/detail lines on selection, without popovers, dialogs, or hiding
//! surrounding rows. Rows are owned by the tab page in
//! [`signal_tab`](crate::ui::signal_view::signal_tab).

use libadwaita::{
    glib::SignalHandlerId,
    gtk::{
        Align::{End, Start},
        Box, Image, Label, ListBox, ListBoxRow,
        Orientation::{Horizontal, Vertical},
        accessible::Property::Label as A11yLabel,
    },
    prelude::{AccessibleExtManual, BoxExt, Cast, ListBoxRowExt, WidgetExt},
};

use crate::{playback::signal_path::PathStage, ui::player::signal_badge::verdict_icon};

/// Build one chain row: badge icon plus title/detail lines.
///
/// The trailing verdict mark carries the per-stage quality indicator as an
/// icon plus a text label, never color alone. The plain-language explanation
/// starts hidden and is revealed inline under the title/detail lines on
/// selection, keeping every surrounding row visible.
///
/// # Arguments
///
/// * `stage` - Snapshot stage to render.
///
/// # Returns
///
/// * `ListBoxRow` - Selectable row with an accessible `title — detail` name.
#[must_use]
pub fn stage_row(stage: &PathStage) -> ListBoxRow {
    let row = ListBoxRow::builder()
        .activatable(true)
        .selectable(true)
        .can_focus(true)
        .build();
    row.update_property(&[A11yLabel(&format!("{} — {}", stage.title, stage.detail))]);
    let top = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .margin_start(6)
        .margin_end(6)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    let badge = Image::builder()
        .icon_name(stage.badge_icon)
        .pixel_size(32)
        .tooltip_text(stage.title.as_str())
        .build();
    badge.update_property(&[A11yLabel(stage.title.as_str())]);
    let text = Box::builder().orientation(Vertical).spacing(0).build();
    let title = Label::builder()
        .label(stage.title.as_str())
        .css_classes(["heading"])
        .halign(Start)
        .build();
    let detail = Label::builder()
        .label(stage.detail.as_str())
        .css_classes(["accent"])
        .halign(Start)
        .wrap(true)
        .build();
    text.append(&title);
    text.append(&detail);
    text.set_hexpand(true);
    let mark = Box::builder()
        .orientation(Vertical)
        .spacing(0)
        .halign(End)
        .build();
    mark.append(
        &Image::builder()
            .icon_name(verdict_icon(stage.verdict))
            .pixel_size(16)
            .tooltip_text(stage.verdict.label())
            .build(),
    );
    mark.append(
        &Label::builder()
            .label(stage.verdict.label())
            .css_classes(["caption", "dim-label"])
            .halign(End)
            .build(),
    );
    top.append(&badge);
    top.append(&text);
    top.append(&mark);
    let explanation = Label::builder()
        .label(stage.explanation.as_str())
        .css_classes(["dim-label"])
        .halign(Start)
        .wrap(true)
        .margin_start(6)
        .margin_end(6)
        .margin_bottom(6)
        .visible(false)
        .build();
    let layout = Box::builder().orientation(Vertical).spacing(0).build();
    layout.append(&top);
    layout.append(&explanation);
    row.set_child(Some(&layout));
    row
}

/// Find the inline explanation label owned by one chain row.
///
/// # Arguments
///
/// * `row` - Chain row built by [`stage_row`].
///
/// # Returns
///
/// * `Option<Label>` - Explanation label when the row keeps its layout.
#[must_use]
pub fn explanation_label(row: &ListBoxRow) -> Option<Label> {
    let child = row.child()?;
    let Ok(layout) = child.downcast::<Box>() else {
        return None;
    };
    let top = layout.first_child()?;
    let Ok(explanation) = top.next_sibling()?.downcast::<Label>() else {
        return None;
    };
    Some(explanation)
}

/// Reveal or hide one row's inline explanation, leaving siblings untouched.
///
/// # Arguments
///
/// * `row` - Chain row to update.
/// * `expanded` - Whether the explanation is visible.
pub fn set_row_expanded(row: &ListBoxRow, expanded: bool) {
    if let Some(explanation) = explanation_label(row) {
        explanation.set_visible(expanded);
    }
}

/// Whether one row currently shows its inline explanation.
///
/// # Arguments
///
/// * `row` - Chain row to inspect.
///
/// # Returns
///
/// * `bool` - Whether the explanation label is visible.
#[must_use]
pub fn is_row_expanded(row: &ListBoxRow) -> bool {
    explanation_label(row).is_some_and(|explanation| explanation.is_visible())
}

/// Toggle one row's inline explanation, leaving siblings untouched.
///
/// # Arguments
///
/// * `row` - Chain row to toggle.
pub fn toggle_row_expanded(row: &ListBoxRow) {
    set_row_expanded(row, !is_row_expanded(row));
}

/// Wire inline explainer toggling for every row of a chain list.
///
/// Selecting a row reveals its explanation; activating a row toggles it.
/// Expanding one row never collapses or hides surrounding rows. The returned
/// handler ids are retained by the caller for later disconnection.
///
/// # Arguments
///
/// * `list` - Chain list owning rows built by [`stage_row`].
///
/// # Returns
///
/// * `[SignalHandlerId; 2]` - Retained selection/activation wiring.
#[must_use]
pub fn wire_explainer(list: &ListBox) -> [SignalHandlerId; 2] {
    [
        list.connect_row_selected(|_, row| {
            if let Some(row) = row {
                set_row_expanded(row, true);
            }
        }),
        list.connect_row_activated(|_, row| {
            toggle_row_expanded(row);
        }),
    ]
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, bail, ensure},
        libadwaita::{
            gtk::{self, Box, test},
            prelude::{Cast, ListBoxRowExt, WidgetExt},
        },
    };

    use crate::{
        playback::signal_path::{
            PathStage, QualityVerdict::Processed, StageKind::SampleRateConverter,
        },
        ui::signal_view::signal_chain::{
            explanation_label, is_row_expanded, set_row_expanded, stage_row, toggle_row_expanded,
        },
    };

    fn converter_stage() -> PathStage {
        PathStage {
            position: 1,
            kind: SampleRateConverter,
            title: String::from("Sample Rate Conversion"),
            detail: String::from("44.1kHz to 48kHz"),
            explanation: String::from("Converts the rate."),
            verdict: Processed,
            badge_icon: "audio-card-symbolic",
        }
    }

    #[test]
    fn stage_row_carries_badge_text_and_verdict_mark() -> Result<()> {
        let row = stage_row(&converter_stage());
        ensure!(row.is_activatable(), "row must be activatable");
        ensure!(row.is_selectable(), "row must be selectable");
        let Some(child) = row.child() else {
            bail!("row must install a layout")
        };
        let Ok(layout) = child.downcast::<Box>() else {
            bail!("row layout must be a box")
        };
        let Some(top) = layout.first_child() else {
            bail!("row must keep its top line")
        };
        let Ok(top) = top.downcast::<Box>() else {
            bail!("top line must be a box")
        };
        let mut parts = 0_u32;
        let mut next = top.first_child();
        while let Some(child) = next {
            parts = parts.saturating_add(1);
            next = child.next_sibling();
        }
        ensure!(parts == 3, "row must carry badge, text, and verdict mark");
        let Some(explanation) = explanation_label(&row) else {
            bail!("row must own an inline explanation")
        };
        ensure!(!explanation.is_visible(), "explanation starts hidden");
        Ok(())
    }

    #[test]
    fn explainer_expands_one_row_without_touching_siblings() -> Result<()> {
        let first = stage_row(&converter_stage());
        let second = stage_row(&converter_stage());
        ensure!(!is_row_expanded(&first), "explanation starts hidden");
        set_row_expanded(&first, true);
        ensure!(is_row_expanded(&first), "selection reveals inline");
        ensure!(!is_row_expanded(&second), "siblings stay collapsed");
        toggle_row_expanded(&first);
        ensure!(!is_row_expanded(&first), "activation toggles closed");
        ensure!(!is_row_expanded(&second), "toggle spares siblings");
        toggle_row_expanded(&second);
        ensure!(is_row_expanded(&second), "each row toggles alone");
        ensure!(!is_row_expanded(&first), "first row stays closed");
        Ok(())
    }
}
