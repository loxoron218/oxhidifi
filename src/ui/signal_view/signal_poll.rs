//! Atomic live refresh for the Signal tab: newest-wins mailbox drain.
//!
//! The Signal tab polls via `timeout_add_local` on a 250 ms cadence and
//! drains its per-subscriber `async-channel` mailbox with `try_recv`. The
//! newest snapshot wins; the whole chain swaps only when `generation` or
//! track id changes, so gapless transitions never show mixed rows. Workers
//! own all I/O (catalog lookup, lock clones, snapshot builds); the main
//! thread only rebuilds rows from the ready snapshot. [`wire_signal_tab`]
//! connects the mailbox publisher and poll loop in one call so no feed is
//! left unwired. Signal actions live in the main window header bar.

use std::{sync::Arc, time::Duration};

use {
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::glib::{ControlFlow::Continue, timeout_add_local},
};

use crate::{
    app::runtime::AppState,
    playback::signal_path::SignalPathSnapshot,
    ui::signal_view::{signal_publish::spawn_snapshot_publisher, signal_tab::SignalTab},
};

/// Poll cadence for the Signal tab mailbox drain (100–500 ms budget).
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Decide whether an incoming snapshot replaces the displayed one.
///
/// Whole-snapshot swap on `generation` or track-id change; anything else is
/// a redundant poll tick.
///
/// # Arguments
///
/// * `current_generation` - Generation currently on screen.
/// * `current_track` - Track currently on screen (`None` for empty state).
/// * `incoming` - Newest drained snapshot.
///
/// # Returns
///
/// * `bool` - True when the whole chain must swap.
#[must_use]
pub fn should_apply_update(
    current_generation: u64,
    current_track: Option<i64>,
    incoming: &SignalPathSnapshot,
) -> bool {
    incoming.generation != current_generation || incoming.track_id != current_track
}

/// Select the newest snapshot by `generation`, never mixing rows.
///
/// Ties resolve to the later item so repeated polls converge.
///
/// # Arguments
///
/// * `snapshots` - Candidate snapshots in arrival order.
///
/// # Returns
///
/// * `Option<&SignalPathSnapshot>` - Newest snapshot, if any.
#[must_use]
pub fn select_newest(snapshots: &[SignalPathSnapshot]) -> Option<&SignalPathSnapshot> {
    snapshots.iter().max_by_key(|snapshot| snapshot.generation)
}

/// Drain a mailbox, keeping the newest snapshot by `generation`.
///
/// # Arguments
///
/// * `receiver` - Per-subscriber snapshot mailbox.
///
/// # Returns
///
/// * `Option<SignalPathSnapshot>` - Newest drained snapshot, if any.
#[must_use]
pub fn drain_newest(receiver: &Receiver<SignalPathSnapshot>) -> Option<SignalPathSnapshot> {
    let mut best: Option<SignalPathSnapshot> = None;
    while let Ok(item) = receiver.try_recv() {
        let newer = best
            .as_ref()
            .is_none_or(|current| item.generation >= current.generation);
        if newer {
            best = Some(item);
        }
    }
    best
}

/// Create a per-subscriber snapshot mailbox.
///
/// Workers send ready snapshots; the tab owns the receiver and drains it on
/// every poll tick.
///
/// # Returns
///
/// * `(Sender<SignalPathSnapshot>, Receiver<SignalPathSnapshot>)` - Mailbox ends.
#[must_use]
pub fn create_mailbox() -> (Sender<SignalPathSnapshot>, Receiver<SignalPathSnapshot>) {
    unbounded()
}

/// Start the Signal tab poll loop.
///
/// Drains the mailbox on every tick, swaps the whole snapshot on
/// `generation`/track-id change, shows the empty state iff `track_id` is
/// `None`, and keeps the paused/stopped ribbon via [`SignalTab`].
/// Never `spawn_local`+`recv().await`: workers own I/O.
///
/// # Arguments
///
/// * `state` - Application state owning the retained signal handle.
/// * `tab` - Signal tab handles to refresh.
/// * `receiver` - Per-subscriber snapshot mailbox.
pub fn start_signal_poll(
    state: &Arc<AppState>,
    tab: &SignalTab,
    receiver: Receiver<SignalPathSnapshot>,
) {
    let polled = tab.clone();
    let source = timeout_add_local(POLL_INTERVAL, move || {
        if let Some(newest) = drain_newest(&receiver) {
            apply_polled_snapshot(&polled, &newest);
        }
        Continue
    });
    state.handles.lock().retain_source(source);
}

/// Connect every live service for one Signal tab.
///
/// Creates the per-subscriber snapshot mailbox, spawns the off-thread
/// publisher feeding it, and starts the poll loop draining it. A single call
/// keeps the mailbox ends paired with their publisher and poll owners.
///
/// # Arguments
///
/// * `state` - Application state owning channels, handles, and settings.
/// * `tab` - Signal tab handles to refresh.
pub fn wire_signal_tab(state: &Arc<AppState>, tab: &SignalTab) {
    let (snapshot_tx, snapshot_rx) = create_mailbox();
    spawn_snapshot_publisher(&state.playback, &state.storage, snapshot_tx);
    start_signal_poll(state, tab, snapshot_rx);
}

/// Apply one drained snapshot to the tab.
///
/// Shows the empty state iff `track_id` is `None`, otherwise swaps the whole
/// chain on `generation`/track-id change.
///
/// # Arguments
///
/// * `tab` - Signal tab handles to refresh.
/// * `snapshot` - Newest drained snapshot.
fn apply_polled_snapshot(tab: &SignalTab, snapshot: &SignalPathSnapshot) {
    if snapshot.track_id.is_none() {
        tab.show_empty();
        return;
    }
    if should_apply_update(tab.generation(), tab.track(), snapshot) {
        tab.render_snapshot(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Error, Result, bail, ensure};

    use crate::{
        playback::{
            signal_path::{QualityVerdict::BitPerfect, SignalPathSnapshot},
            state::PlaybackStatus::Playing,
        },
        ui::signal_view::signal_poll::{
            create_mailbox, drain_newest, select_newest, should_apply_update,
        },
    };

    fn polled_snapshot(generation: u64, track_id: Option<i64>) -> SignalPathSnapshot {
        SignalPathSnapshot {
            generation,
            track_id,
            zone_name: String::from("Test DAC"),
            verdict: BitPerfect,
            stages: Vec::new(),
            devices: Vec::new(),
            processing_speed: None,
            playback_status: Playing,
        }
    }

    #[test]
    fn generation_guard_applies_newest_wins() -> Result<()> {
        let current = polled_snapshot(2, Some(11));
        let same = polled_snapshot(2, Some(11));
        ensure!(
            !should_apply_update(current.generation, current.track_id, &same),
            "identical generation and track must not swap"
        );
        let next_gen = polled_snapshot(3, Some(11));
        ensure!(
            should_apply_update(current.generation, current.track_id, &next_gen),
            "bumped generation must swap"
        );
        let next_track = polled_snapshot(2, Some(12));
        ensure!(
            should_apply_update(current.generation, current.track_id, &next_track),
            "changed track must swap even at the same generation"
        );
        let out_of_order = vec![
            polled_snapshot(2, Some(11)),
            polled_snapshot(5, Some(14)),
            polled_snapshot(3, Some(12)),
        ];
        let Some(newest) = select_newest(&out_of_order) else {
            bail!("newest snapshot must exist")
        };
        ensure!(
            newest.generation == 5,
            "stale mailbox contents must resolve newest-wins"
        );
        ensure!(
            select_newest(&[]).is_none(),
            "empty mailbox must select nothing"
        );
        let (sender, receiver) = create_mailbox();
        for snapshot in out_of_order {
            sender.try_send(snapshot).map_err(Error::msg)?;
        }
        let Some(drained) = drain_newest(&receiver) else {
            bail!("drained snapshot must exist")
        };
        ensure!(
            drained.generation == 5,
            "drain must keep the newest generation, got {}",
            drained.generation
        );
        ensure!(
            drain_newest(&receiver).is_none(),
            "drain must consume every queued snapshot"
        );
        Ok(())
    }
}
