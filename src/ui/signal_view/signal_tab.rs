//! Signal tab page: live vertical stage chain with empty state.
//!
//! Third `ViewStack` page named `signal` next to Albums/Artists. The chain
//! scrolls inside the tab with the header and footer reachable; a continuous
//! theme separator rails the badge icons on the left. Rows render in snapshot
//! `position` order, source at the top and output at the bottom, with a bold
//! title plus accent detail per row. Programmatic widgets only, 6 px spacing,
//! theme-aware styling, no hardcoded radii or colors. The tab itself owns no
//! header bar or popover; signal actions live in the main window header bar.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering::Relaxed},
};

use {
    libadwaita::{
        Banner,
        gtk::{ListBox, Stack, Widget},
        prelude::WidgetExt,
    },
    parking_lot::Mutex,
};

use crate::{
    playback::{
        signal_path::{SignalPathSnapshot, path_snapshot::summarize_text},
        state::PlaybackStatus::{Paused, Playing, Stopped},
    },
    ui::signal_view::{
        signal_chain::stage_row, signal_footer::SignalFooter, signal_header::SignalHeader,
    },
};

/// Live handles for the Signal tab page.
#[derive(Debug, Clone)]
pub struct SignalTab {
    /// Tab root placed into the library `ViewStack`.
    root: Widget,
    /// Chain rows in snapshot `position` order.
    list: ListBox,
    /// Ribbon shown for paused/stopped playback.
    banner: Banner,
    /// Content switcher between the chain and the empty state.
    content: Stack,
    /// Header verdict, zone name, and explainer hint.
    header: SignalHeader,
    /// Footer device card for the active rendering device.
    footer: SignalFooter,
    /// Last rendered path summary for the copy action.
    summary: Arc<Mutex<String>>,
    /// Generation currently on screen.
    generation: Arc<AtomicU64>,
    /// Track currently on screen (`None` for the empty state).
    track: Arc<Mutex<Option<i64>>>,
}

impl SignalTab {
    /// Create tab handles from built widgets.
    ///
    /// # Arguments
    ///
    /// * `root` - Tab root placed into the library `ViewStack`.
    /// * `list` - Chain rows in snapshot `position` order.
    /// * `banner` - Ribbon shown for paused/stopped playback.
    /// * `content` - Switcher between the chain and the empty state.
    /// * `header` - Header verdict, zone name, and explainer hint.
    /// * `footer` - Footer device card for the active rendering device.
    ///
    /// # Returns
    ///
    /// * `SignalTab` - Handles owning the tab root widget.
    #[must_use]
    pub fn new(
        root: Widget,
        list: ListBox,
        banner: Banner,
        content: Stack,
        header: SignalHeader,
        footer: SignalFooter,
    ) -> Self {
        Self {
            root,
            list,
            banner,
            content,
            header,
            footer,
            summary: Arc::new(Mutex::new(String::new())),
            generation: Arc::new(AtomicU64::new(0)),
            track: Arc::new(Mutex::new(None)),
        }
    }

    /// Generation currently on screen.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Relaxed)
    }

    /// Track currently on screen (`None` for the empty state).
    #[must_use]
    pub fn track(&self) -> Option<i64> {
        *self.track.lock()
    }

    /// Tab root widget for the library `ViewStack`.
    #[must_use]
    pub const fn widget(&self) -> &Widget {
        &self.root
    }

    /// Render one snapshot, swapping the whole chain atomically.
    ///
    /// Rebuilds every row from `snapshot` in `position` order so gapless
    /// transitions never show mixed rows. Shows the paused/stopped ribbon
    /// when the track is known but not playing.
    pub fn render_snapshot(&self, snapshot: &SignalPathSnapshot) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        for stage in &snapshot.stages {
            self.list.append(&stage_row(stage));
        }
        self.header.apply_snapshot(snapshot);
        self.footer.apply_devices(&snapshot.devices);
        *self.summary.lock() = summarize_text(snapshot);
        match snapshot.playback_status {
            Playing => self.banner.set_revealed(false),
            Paused => {
                self.banner
                    .set_title("Paused — showing the last-known path");
                self.banner.set_revealed(true);
            }
            Stopped => {
                self.banner
                    .set_title("Stopped — showing the last-known path");
                self.banner.set_revealed(true);
            }
        }
        self.content.set_visible_child_name("chain");
        self.generation.store(snapshot.generation, Relaxed);
        *self.track.lock() = snapshot.track_id;
    }

    /// Show the empty state explaining how to start playback.
    pub fn show_empty(&self) {
        self.banner.set_revealed(false);
        self.header.show_idle();
        self.footer.hide();
        *self.summary.lock() = String::new();
        self.content.set_visible_child_name("empty");
        *self.track.lock() = None;
    }

    /// Count the rendered chain rows (test hook for atomic-swap coverage).
    #[must_use]
    pub fn row_count(&self) -> u32 {
        let mut count = 0_u32;
        let mut next = self.list.first_child();
        while let Some(child) = next {
            count = count.saturating_add(1);
            next = child.next_sibling();
        }
        count
    }

    /// Header verdict label text (badge/header parity hook).
    #[must_use]
    pub fn verdict_text(&self) -> String {
        self.header.verdict_text()
    }

    /// Header zone name text (badge/header parity hook).
    #[must_use]
    pub fn zone_text(&self) -> String {
        self.header.zone_text()
    }

    /// Header verdict icon name (badge/header parity hook).
    #[must_use]
    pub fn header_icon_name(&self) -> String {
        self.header.header_icon_name()
    }

    /// Footer device name text (empty while the card is hidden).
    #[must_use]
    pub fn footer_device_text(&self) -> String {
        self.footer.device_text()
    }

    /// Last rendered path summary for the copy action.
    #[must_use]
    pub fn pending_summary(&self) -> String {
        self.summary.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, bail, ensure},
        libadwaita::{
            glib::object::ObjectExt,
            gtk::{self, test},
        },
    };

    use crate::{
        playback::{
            signal_path::{
                PathStage,
                QualityVerdict::BitPerfect,
                SignalPathSnapshot,
                StageKind::{Output, Source, Transport},
                stage_output::lab_test_device,
            },
            state::PlaybackStatus::{Paused, Playing},
        },
        ui::signal_view::{signal_chain::is_row_expanded, signal_tab_build::build_signal_page},
    };

    fn three_stage_snapshot() -> SignalPathSnapshot {
        let stages = vec![
            PathStage {
                position: 0,
                kind: Source,
                title: String::from("Source"),
                detail: String::from("FLAC 44.1kHz 16-bit Stereo"),
                explanation: String::from("Original audio."),
                verdict: BitPerfect,
                badge_icon: "audio-x-generic-symbolic",
            },
            PathStage {
                position: 1,
                kind: Transport,
                title: String::from("ALSA Direct Output"),
                detail: String::from("Test DAC (ALSA direct exclusive)"),
                explanation: String::from("Exclusive delivery."),
                verdict: BitPerfect,
                badge_icon: "audio-card-symbolic",
            },
            PathStage {
                position: 2,
                kind: Output,
                title: String::from("Output"),
                detail: String::from("Test DAC"),
                explanation: String::from("Final render."),
                verdict: BitPerfect,
                badge_icon: "audio-speakers-symbolic",
            },
        ];
        SignalPathSnapshot {
            generation: 4,
            track_id: Some(7),
            zone_name: String::from("Test DAC"),
            verdict: BitPerfect,
            stages,
            devices: vec![lab_test_device()],
            processing_speed: None,
            playback_status: Playing,
        }
    }

    #[test]
    fn signal_tab_renders_chain_rows_in_order() -> Result<()> {
        let tab = build_signal_page();
        tab.show_empty();
        ensure!(tab.row_count() == 0, "empty tab must show no rows");
        ensure!(tab.track().is_none(), "empty tab must track no id");
        let snapshot = three_stage_snapshot();
        tab.render_snapshot(&snapshot);
        ensure!(tab.row_count() == 3, "chain must render every stage");
        ensure!(tab.track() == Some(7), "tab must track the snapshot id");
        ensure!(tab.generation() == 4, "tab must track the generation");
        ensure!(
            tab.verdict_text() == "Bit-Perfect",
            "header must show the snapshot verdict"
        );
        ensure!(
            tab.zone_text() == "Test DAC",
            "header must show the zone name"
        );
        ensure!(
            tab.footer_device_text() == "Test DAC",
            "footer must name the rendering device"
        );
        ensure!(
            tab.pending_summary().contains("Bit-Perfect"),
            "copy summary must carry the header verdict"
        );
        let paused = SignalPathSnapshot {
            playback_status: Paused,
            ..snapshot
        };
        tab.render_snapshot(&paused);
        ensure!(tab.row_count() == 3, "ribbon swap must keep every row");
        tab.show_empty();
        ensure!(
            tab.verdict_text() == "No Active Path",
            "empty tab must reset the header"
        );
        ensure!(tab.zone_text().is_empty(), "empty tab must clear the zone");
        ensure!(
            tab.pending_summary().is_empty(),
            "empty tab must clear the copy summary"
        );
        Ok(())
    }

    #[test]
    fn explainer_starts_expanded_and_toggles_inline() -> Result<()> {
        let tab = build_signal_page();
        tab.render_snapshot(&three_stage_snapshot());
        let Some(source) = tab.list.row_at_index(0) else {
            bail!("chain must render a Source row")
        };
        let Some(converter) = tab.list.row_at_index(1) else {
            bail!("chain must render a converter row")
        };
        ensure!(is_row_expanded(&source), "explanations start expanded");
        ensure!(is_row_expanded(&converter), "siblings start expanded");
        tab.list.select_row(Some(&source));
        ensure!(is_row_expanded(&source), "selection keeps inline");
        ensure!(is_row_expanded(&converter), "siblings stay expanded");
        tab.list.select_row(Some(&converter));
        ensure!(is_row_expanded(&converter), "converter explains inline");
        ensure!(is_row_expanded(&source), "first row stays expanded");
        ensure!(tab.row_count() == 3, "expansion keeps every row");
        tab.list.emit_by_name::<()>("row-activated", &[&converter]);
        ensure!(!is_row_expanded(&converter), "activation toggles");
        ensure!(is_row_expanded(&source), "toggle spares siblings");
        Ok(())
    }
}
