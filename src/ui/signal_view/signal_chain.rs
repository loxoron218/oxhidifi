//! Signal tab chain rows: badge icon, title/detail, and verdict mark.
//!
//! One selectable [`ListBoxRow`] per [`PathStage`] in snapshot `position`
//! order. The trailing verdict mark carries the per-stage quality indicator
//! as an icon plus a text label, never color alone. Rows are owned by the tab
//! page in [`signal_tab`](crate::ui::signal_view::signal_tab).

use libadwaita::{
    gtk::{
        Align::{End, Start},
        Box, Image, Label, ListBoxRow,
        Orientation::{Horizontal, Vertical},
        accessible::Property::Label as A11yLabel,
    },
    prelude::{AccessibleExtManual, BoxExt, ListBoxRowExt, WidgetExt},
};

use crate::{playback::signal_path::PathStage, ui::player::signal_badge::verdict_icon};

/// Build one chain row: badge icon plus title/detail lines.
///
/// The trailing verdict mark carries the per-stage quality indicator as an
/// icon plus a text label, never color alone.
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
    let layout = Box::builder()
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
    layout.append(&badge);
    layout.append(&text);
    layout.append(&mark);
    row.set_child(Some(&layout));
    row
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
        ui::signal_view::signal_chain::stage_row,
    };

    #[test]
    fn stage_row_carries_badge_text_and_verdict_mark() -> Result<()> {
        let row = stage_row(&PathStage {
            position: 1,
            kind: SampleRateConverter,
            title: String::from("Sample Rate Conversion"),
            detail: String::from("44.1kHz to 48kHz"),
            explanation: String::from("Converts the rate."),
            verdict: Processed,
            badge_icon: "audio-card-symbolic",
        });
        ensure!(row.is_activatable(), "row must be activatable");
        ensure!(row.is_selectable(), "row must be selectable");
        let Some(child) = row.child() else {
            bail!("row must install a layout")
        };
        let Ok(layout) = child.downcast::<Box>() else {
            bail!("row layout must be a box")
        };
        let mut parts = 0_u32;
        let mut next = layout.first_child();
        while let Some(child) = next {
            parts = parts.saturating_add(1);
            next = child.next_sibling();
        }
        ensure!(parts == 3, "row must carry badge, text, and verdict mark");
        Ok(())
    }
}
