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
    prelude::{AccessibleExtManual, BoxExt},
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
        let announced = format!("Signal path is {}", next.label());
        self.verdict.update_property(&[A11yLabel(&announced)]);
    }

    /// Reset the header to the empty-state entry.
    pub fn show_idle(&self) {
        self.last.set(None);
        self.mark.set_icon_name(Some("audio-x-generic-symbolic"));
        self.verdict.set_label("No Active Path");
        self.zone.set_label("");
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
    let summary = Box::builder().orientation(Vertical).spacing(0).build();
    summary.append(&row);
    summary.append(&hint);
    SignalHeader {
        summary,
        mark,
        verdict,
        zone,
        last: Cell::new(None),
    }
}
