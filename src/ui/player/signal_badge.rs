//! Player-area signal path badge: verdict entry button for the `Signal` tab.
//!
//! A keyboard-activatable button showing the current whole-path verdict text
//! plus a dedicated indicator icon per state (never color alone). Activating
//! it switches the library to the `Signal` tab; when idle it navigates to the
//! tab empty state. Live verdict flips arrive through a per-subscriber
//! snapshot mailbox drained on a `timeout_add_local` poll; workers own I/O.

use std::{sync::Arc, time::Duration};

use {
    async_channel::Receiver,
    libadwaita::{
        glib::{ControlFlow::Continue, timeout_add_local},
        gtk::{
            Box, Button, Image, Label, Orientation::Horizontal,
            accessible::Property::Label as A11yLabel,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, WidgetExt},
    },
    parking_lot::Mutex,
    tracing::info,
};

use crate::{
    app::runtime::AppState,
    playback::signal_path::{
        QualityVerdict::{self, BitPerfect, Limited, Processed},
        SignalPathSnapshot,
    },
    ui::{navigation::persist_active_tab, signal_view::signal_poll::drain_newest},
};

/// Idle icon shown when no track is playing.
const IDLE_ICON: &str = "audio-x-generic-symbolic";

/// Poll cadence for the badge mailbox drain (100–500 ms budget).
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Player-area badge button mirroring the live signal path verdict.
#[derive(Debug, Clone)]
pub struct SignalBadge {
    /// Entry button navigating to the `Signal` tab.
    button: Button,
    /// Verdict indicator icon (dedicated shape per state).
    icon: Image,
    /// Verdict text label (`Signal` while idle).
    label: Label,
    /// Last verdict shown (`None` while idle with no track).
    verdict: Arc<Mutex<Option<QualityVerdict>>>,
}

impl SignalBadge {
    /// Button widget for embedding in the player area.
    #[must_use]
    pub const fn widget(&self) -> &Button {
        &self.button
    }

    /// Current badge label text (test hook for verdict coverage).
    ///
    /// # Returns
    ///
    /// * `String` - Visible label (`Signal` while idle, verdict text live).
    #[must_use]
    pub fn label_text(&self) -> String {
        self.label.label().to_string()
    }

    /// Current badge icon name (test hook for verdict coverage).
    ///
    /// # Returns
    ///
    /// * `String` - Symbolic icon name currently on the button.
    pub fn icon_name_str(&self) -> String {
        self.icon
            .icon_name()
            .map_or_else(String::new, |icon| icon.to_string())
    }

    /// Apply one drained snapshot to the badge.
    ///
    /// Shows the idle entry while `track_id` is `None`, otherwise the verdict
    /// text plus its dedicated icon. Verdict flips emit structured `tracing`
    /// fields with the snapshot generation and track id.
    ///
    /// # Arguments
    ///
    /// * `snapshot` - Newest drained snapshot.
    pub fn apply_snapshot(&self, snapshot: &SignalPathSnapshot) {
        let Some(track_id) = snapshot.track_id else {
            self.show_idle();
            return;
        };
        let next = snapshot.verdict;
        if let Some(previous) = *self.verdict.lock()
            && previous != next
        {
            trace_verdict_flip(previous, next, snapshot.generation, Some(track_id), "badge");
        }
        *self.verdict.lock() = Some(next);
        self.label.set_label(next.label());
        self.icon.set_icon_name(Some(verdict_icon(next)));
        let announced = format!("Signal path is {} — open the Signal tab", next.label());
        self.button.set_tooltip_text(Some(&announced));
        self.button.update_property(&[A11yLabel(&announced)]);
    }

    /// Show the idle entry navigating to the tab empty state.
    fn show_idle(&self) {
        *self.verdict.lock() = None;
        self.label.set_label("Signal");
        self.icon.set_icon_name(Some(IDLE_ICON));
        self.button.set_tooltip_text(Some("Open the Signal tab"));
        self.button
            .update_property(&[A11yLabel("Open the Signal tab")]);
    }
}

/// Dedicated indicator icon per verdict state (never color alone).
///
/// # Arguments
///
/// * `verdict` - Whole-path verdict to illustrate.
///
/// # Returns
///
/// * `&'static str` - Symbolic icon name with a distinct shape per state.
#[must_use]
pub const fn verdict_icon(verdict: QualityVerdict) -> &'static str {
    match verdict {
        BitPerfect => "media-optical-cd-audio-symbolic",
        Processed => "audio-card-symbolic",
        Limited => "dialog-warning-symbolic",
    }
}

/// Emit structured tracing fields for a whole-path verdict flip.
///
/// Both live surfaces (player badge and tab header) report through here so
/// DSP-volume and output-mode changes are observable without leaving the tab.
///
/// # Arguments
///
/// * `previous` - Verdict previously shown on the surface.
/// * `next` - Verdict carried by the incoming snapshot.
/// * `generation` - Snapshot generation carrying the flip.
/// * `track_id` - Track carried by the snapshot, if any.
/// * `surface` - UI surface reporting the flip.
pub fn trace_verdict_flip(
    previous: QualityVerdict,
    next: QualityVerdict,
    generation: u64,
    track_id: Option<i64>,
    surface: &str,
) {
    info!(
        previous_verdict = previous.label(),
        new_verdict = next.label(),
        generation,
        track_id,
        surface,
        "Signal path verdict changed",
    );
}

/// Build the player-area signal path badge button.
///
/// Always available: activating it switches the library to the `Signal` tab
/// (empty state with start-playback guidance while idle). Keyboard-activatable
/// with an accessible name; no dialog or popover.
///
/// # Arguments
///
/// * `state` - Application state owning the tab signal and handles.
///
/// # Returns
///
/// * `SignalBadge` - Handles owning the badge button.
#[must_use]
pub fn build_signal_badge(state: &Arc<AppState>) -> SignalBadge {
    let icon = Image::builder().icon_name(IDLE_ICON).pixel_size(16).build();
    let label = Label::builder().label("Signal").build();
    let content = Box::builder().orientation(Horizontal).spacing(6).build();
    content.append(&icon);
    content.append(&label);
    let button = Button::builder()
        .child(&content)
        .tooltip_text("Open the Signal tab")
        .can_focus(true)
        .build();
    button.update_property(&[A11yLabel("Open the Signal tab")]);
    let navigator = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(button.connect_clicked(move |_| {
            persist_active_tab(&navigator.storage, &navigator.active_tab, "signal");
        }));
    SignalBadge {
        button,
        icon,
        label,
        verdict: Arc::new(Mutex::new(None)),
    }
}

/// Start the badge poll loop draining one snapshot mailbox.
///
/// Mirrors the tab poll: newest snapshot wins, idle snapshots show the idle
/// entry. Never `spawn_local`+`recv().await`: workers own I/O.
///
/// # Arguments
///
/// * `state` - Application state owning the retained signal handle.
/// * `badge` - Badge handles to refresh.
/// * `receiver` - Per-subscriber snapshot mailbox.
pub fn start_badge_poll(
    state: &Arc<AppState>,
    badge: &SignalBadge,
    receiver: Receiver<SignalPathSnapshot>,
) {
    let live = badge.clone();
    let source = timeout_add_local(POLL_INTERVAL, move || {
        if let Some(newest) = drain_newest(&receiver) {
            live.apply_snapshot(&newest);
        }
        Continue
    });
    state.handles.lock().retain_source(source);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, test},
            prelude::ButtonExt,
        },
    };

    use crate::{
        app::runtime::AppState,
        playback::{
            signal_path::{
                QualityVerdict::{self, BitPerfect, Limited, Processed},
                SignalPathSnapshot,
            },
            state::PlaybackStatus::Playing,
        },
        storage::active_tab::ActiveTab::Signal,
        ui::{
            player::signal_badge::{IDLE_ICON, build_signal_badge, verdict_icon},
            signal_view::signal_tab_build::build_signal_page,
        },
    };

    fn badge_snapshot(verdict: QualityVerdict) -> SignalPathSnapshot {
        SignalPathSnapshot {
            generation: 9,
            track_id: Some(4),
            zone_name: String::from("Lab DAC"),
            verdict,
            stages: Vec::new(),
            devices: Vec::new(),
            processing_speed: None,
            playback_status: Playing,
        }
    }

    #[test]
    fn verdict_icons_are_distinct_shapes() -> Result<()> {
        let icons = [
            verdict_icon(BitPerfect),
            verdict_icon(Processed),
            verdict_icon(Limited),
        ];
        ensure!(!icons.contains(&""), "every verdict needs an icon");
        ensure!(
            icons[0] != icons[1],
            "bit-perfect must differ from processed"
        );
        ensure!(icons[1] != icons[2], "processed must differ from limited");
        ensure!(icons[0] != icons[2], "bit-perfect must differ from limited");
        ensure!(
            icons[0] != IDLE_ICON,
            "live icons must differ from the idle icon"
        );
        Ok(())
    }

    #[test]
    fn badge_shows_verdict_text_and_icon_per_state() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let badge = build_signal_badge(&state);
        ensure!(
            badge.label_text() == "Signal",
            "idle badge must read Signal"
        );
        ensure!(
            badge.icon_name_str() == IDLE_ICON,
            "idle badge needs the idle icon"
        );
        for verdict in [BitPerfect, Processed, Limited] {
            badge.apply_snapshot(&badge_snapshot(verdict));
            ensure!(
                badge.label_text() == verdict.label(),
                "badge must show verdict text"
            );
            ensure!(
                badge.icon_name_str() == verdict_icon(verdict),
                "badge needs the state icon"
            );
        }
        let idle = SignalPathSnapshot {
            track_id: None,
            ..badge_snapshot(BitPerfect)
        };
        badge.apply_snapshot(&idle);
        ensure!(
            badge.label_text() == "Signal",
            "lost track must restore the idle entry"
        );
        Ok(())
    }

    #[test]
    fn badge_activation_navigates_to_signal_tab() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let badge = build_signal_badge(&state);
        badge.widget().emit_clicked();
        ensure!(
            state.active_tab.borrow() == Signal,
            "badge must switch to the Signal tab"
        );
        Ok(())
    }

    #[test]
    fn signal_badge_matches_tab_header_per_state() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let badge = build_signal_badge(&state);
        let tab = build_signal_page();
        for verdict in [BitPerfect, Processed, Limited] {
            let snapshot = badge_snapshot(verdict);
            badge.apply_snapshot(&snapshot);
            tab.render_snapshot(&snapshot);
            ensure!(
                badge.label_text() == tab.verdict_text(),
                "badge must match the tab header"
            );
            ensure!(
                badge.icon_name_str() == tab.header_icon_name(),
                "badge icon must match the header icon"
            );
            ensure!(
                tab.zone_text() == "Lab DAC",
                "header must show the zone name"
            );
        }
        Ok(())
    }
}
