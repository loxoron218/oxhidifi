//! Signal tab header: whole-path verdict, zone name, and explainer hint.
//!
//! Theme-aware widgets above the stage chain: a dedicated verdict indicator
//! icon plus text label (never color alone), the playing output/zone name,
//! and the hint inviting stage selection. Owned by the tab page in
//! [`signal_tab`](crate::ui::signal_view::signal_tab).

use std::cell::Cell;

use libadwaita::{
    gtk::{
        Align::Start,
        Box, Image, Label,
        Orientation::{Horizontal, Vertical},
        accessible::Property::Label as A11yLabel,
    },
    prelude::{AccessibleExtManual, BoxExt, WidgetExt},
};

use crate::{
    playback::signal_path::{QualityVerdict, SignalPathSnapshot},
    ui::player::signal_badge::{trace_verdict_flip, verdict_icon},
};

/// Live header widgets for the Signal tab page.
#[derive(Debug, Clone)]
pub struct SignalHeader {
    /// Container placed above the chain content.
    summary: Box,
    /// Verdict indicator icon (dedicated shape per state).
    mark: Image,
    /// Whole-path verdict label (`Bit-Perfect`/`Processed`/`Limited`).
    verdict: Label,
    /// Output/zone name.
    zone: Label,
    /// Processing-speed readout, visible iff alteration is active.
    speed: Label,
    /// Last verdict shown (`None` before the first snapshot).
    last: Cell<Option<QualityVerdict>>,
}

impl SignalHeader {
    /// Container widget for the tab body.
    #[must_use]
    pub const fn widget(&self) -> &Box {
        &self.summary
    }

    /// Apply the header verdict label, indicator, and zone name.
    ///
    /// Verdict flips emit structured `tracing` fields with the snapshot
    /// generation and track id so DSP-volume and output-mode changes are
    /// observable without leaving the tab.
    ///
    /// # Arguments
    ///
    /// * `snapshot` - Newest rendered snapshot.
    pub fn apply_snapshot(&self, snapshot: &SignalPathSnapshot) {
        let next = snapshot.verdict;
        if let Some(previous) = self.last.get()
            && previous != next
        {
            trace_verdict_flip(
                previous,
                next,
                snapshot.generation,
                snapshot.track_id,
                "tab",
            );
        }
        self.last.set(Some(next));
        self.mark.set_icon_name(Some(verdict_icon(next)));
        self.verdict.set_label(next.label());
        self.zone.set_label(&snapshot.zone_name);
        apply_speed(&self.speed, snapshot.processing_speed);
        let announced = format!("Signal path is {}", next.label());
        self.verdict.update_property(&[A11yLabel(&announced)]);
    }

    /// Reset the header to the empty-state entry.
    pub fn show_idle(&self) {
        self.last.set(None);
        self.mark.set_icon_name(Some("audio-x-generic-symbolic"));
        self.verdict.set_label("No Active Path");
        self.zone.set_label("");
        self.speed.set_visible(false);
    }

    /// Header verdict label text (badge/header parity hook).
    pub fn verdict_text(&self) -> String {
        self.verdict.label().to_string()
    }

    /// Header zone name text (badge/header parity hook).
    pub fn zone_text(&self) -> String {
        self.zone.label().to_string()
    }

    /// Header verdict icon name (badge/header parity hook).
    pub fn header_icon_name(&self) -> String {
        self.mark
            .icon_name()
            .map_or_else(String::new, |icon| icon.to_string())
    }

    /// Processing-speed readout text (empty while hidden).
    pub fn speed_text(&self) -> String {
        self.speed.label().to_string()
    }

    /// Whether the processing-speed readout is currently visible.
    #[must_use]
    pub fn is_speed_visible(&self) -> bool {
        self.speed.is_visible()
    }
}

/// Apply one snapshot speed sample to the readout label.
///
/// Shows `Processing speed: {x.x}x` (one decimal) iff alteration is active,
/// fully hidden when `None`; the readout is announced as text for assistive
/// technologies.
///
/// # Arguments
///
/// * `label` - Readout label below the verdict/zone line.
/// * `speed` - Snapshot throughput multiple, if any alteration is active.
fn apply_speed(label: &Label, speed: Option<f64>) {
    let Some(multiple) = speed else {
        label.set_visible(false);
        return;
    };
    label.set_label(&format!("Processing speed: {multiple:.1}x"));
    let announced = format!("Processing speed {multiple:.1} times real time");
    label.update_property(&[A11yLabel(&announced)]);
    label.set_visible(true);
}

/// Build the Signal tab header widgets.
///
/// # Returns
///
/// * `SignalHeader` - Handles owning the header container.
#[must_use]
pub fn build_signal_header() -> SignalHeader {
    let mark = Image::builder()
        .icon_name("audio-x-generic-symbolic")
        .pixel_size(16)
        .build();
    let verdict = Label::builder()
        .label("No Active Path")
        .css_classes(["heading"])
        .halign(Start)
        .build();
    verdict.update_property(&[A11yLabel("No active signal path")]);
    let zone = Label::builder()
        .label("")
        .css_classes(["dim-label"])
        .halign(Start)
        .build();
    let hint = Label::builder()
        .label("Click on any stage of the path to learn more")
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .wrap(true)
        .build();
    let row = Box::builder().orientation(Horizontal).spacing(6).build();
    row.append(&mark);
    row.append(&verdict);
    row.append(&zone);
    let speed = Label::builder()
        .label("")
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .visible(false)
        .build();
    let summary = Box::builder().orientation(Vertical).spacing(0).build();
    summary.append(&row);
    summary.append(&speed);
    summary.append(&hint);
    SignalHeader {
        summary,
        mark,
        verdict,
        zone,
        speed,
        last: Cell::new(None),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use libadwaita::gtk::{self, test};

    use crate::{
        playback::{
            signal_path::{
                QualityVerdict::{BitPerfect, Processed},
                SignalPathSnapshot,
            },
            state::PlaybackStatus::Playing,
        },
        ui::signal_view::signal_header::build_signal_header,
    };

    fn header_snapshot(speed: Option<f64>) -> SignalPathSnapshot {
        SignalPathSnapshot {
            generation: 5,
            track_id: Some(9),
            zone_name: String::from("Lab DAC"),
            verdict: if speed.is_some() {
                Processed
            } else {
                BitPerfect
            },
            stages: Vec::new(),
            devices: Vec::new(),
            processing_speed: speed,
            playback_status: Playing,
        }
    }

    #[test]
    fn speed_readout_shows_iff_alteration_active() -> Result<()> {
        let header = build_signal_header();
        ensure!(!header.is_speed_visible(), "readout starts hidden");
        header.apply_snapshot(&header_snapshot(Some(35.24)));
        ensure!(header.is_speed_visible(), "alteration shows the readout");
        ensure!(
            header.speed_text() == "Processing speed: 35.2x",
            "readout keeps one decimal, got {}",
            header.speed_text()
        );
        header.apply_snapshot(&header_snapshot(None));
        ensure!(!header.is_speed_visible(), "bit-perfect hides the readout");
        header.apply_snapshot(&header_snapshot(Some(8.0)));
        header.show_idle();
        ensure!(!header.is_speed_visible(), "idle hides the readout");
        Ok(())
    }
}
