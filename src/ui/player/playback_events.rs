//! Event-driven updates for the player panel widgets.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Acquire},
};

use {
    async_channel::{Sender, unbounded},
    libadwaita::{
        gdk::MemoryTexture,
        glib::MainContext,
        gtk::{
            accessible::Property::Label as PropertyLabel,
            prelude::{AccessibleExtManual, ButtonExt, RangeExt, WidgetExt},
        },
    },
};

use crate::{
    app::runtime::AppState,
    playback::{
        engine::PlaybackEngine,
        state::PlaybackEvent::{
            self, OutputModeChanged, Paused, PositionTick, Resumed, Seeked, Stopped, TrackStarted,
        },
        transport::PlaybackTransport,
    },
    storage::database::SqliteStorage,
    ui::{
        image_decode::DecodedCover,
        player::{
            acoustic_fader::{mode_button_tooltip, update_volume_scale_visual},
            now_playing::{
                handle_status_change, process_cover_art, process_metadata, update_cover_from_cache,
            },
            sidebar::{MetaResult, PlaybackWidgets, format_time},
        },
        texture_pool::CoverArtCache,
    },
};

/// Set up async listeners for playback events, metadata, and cover art.
pub fn spawn_async_listeners(state: &Arc<AppState>, widgets: PlaybackWidgets) {
    let (meta_tx, meta_rx) = unbounded::<(i64, MetaResult)>();
    let (cover_tx, cover_rx) = unbounded::<(i64, i32, DecodedCover)>();

    let playback = Arc::clone(&state.playback);
    let is_seeking = Arc::clone(&state.is_seeking);
    let cover_cache = Arc::clone(&state.cover_art_cache);
    let storage = Arc::clone(&state.storage);

    let ev_rx = state.playback.subscribe();
    let w1 = widgets.clone();
    let p1 = Arc::clone(&playback);
    let s1 = Arc::clone(&is_seeking);
    let c1 = Arc::clone(&cover_cache);
    let st1 = Arc::clone(&storage);
    let mt1 = meta_tx;
    state
        .handles
        .lock()
        .retain_task(MainContext::default().spawn_local(async move {
            while let Ok(event) = ev_rx.recv().await {
                on_playback_event(&event, &w1, &p1, &s1, &c1, &st1, &mt1);
            }
        }));

    let mw = widgets.clone();
    let mp = Arc::clone(&playback);
    let mc = Arc::clone(&cover_cache);
    state
        .handles
        .lock()
        .retain_task(MainContext::default().spawn_local(async move {
            while let Ok((tid, meta)) = meta_rx.recv().await {
                process_metadata(tid, meta, &mw, &mp, &mc, &cover_tx, 280);
            }
        }));

    state
        .handles
        .lock()
        .retain_task(MainContext::default().spawn_local(async move {
            while let Ok((tid, size, cover)) = cover_rx.recv().await {
                process_cover_art(tid, size, &cover, &widgets, &playback, &cover_cache);
            }
        }));
}

/// Update the seek scale and time labels for the current position.
///
/// # Arguments
///
/// * `widgets` - Player widgets holding the seek scale and time labels.
/// * `elapsed` - Elapsed playback position in seconds.
/// * `duration` - Total track duration in seconds.
/// * `is_seeking` - Seek-in-progress flag; scale and elapsed label are left untouched while set.
///
/// # Returns
///
/// Nothing.
fn update_progress_labels(
    widgets: &PlaybackWidgets,
    elapsed: f64,
    duration: f64,
    is_seeking: &AtomicBool,
) {
    if !is_seeking.load(Acquire) {
        let fraction = match duration {
            d if d > 0.0 => elapsed / d,
            _ => 0.0,
        };
        widgets.seek_scale.set_value(fraction * 100.0);
        widgets.current_time.set_label(&format_time(elapsed));
    }
    widgets.total_time.set_label(&format_time(duration));
}

/// Reset the player widgets to the stopped state.
///
/// # Arguments
///
/// * `widgets` - Player widgets to reset.
/// * `is_seeking` - Seek-in-progress flag; scale and elapsed label are left untouched while set.
///
/// # Returns
///
/// Nothing.
fn apply_stopped_state(widgets: &PlaybackWidgets, is_seeking: &AtomicBool) {
    widgets.labels.title.set_label("No track playing");
    widgets
        .labels
        .title
        .update_property(&[PropertyLabel("No track playing")]);
    widgets.labels.artist.set_label("");
    widgets.labels.artist.update_property(&[PropertyLabel("")]);
    widgets.labels.album.set_label("");
    widgets.labels.album.update_property(&[PropertyLabel("")]);
    widgets.labels.format.set_label("");
    widgets.labels.format.update_property(&[PropertyLabel("")]);
    widgets
        .play_button
        .set_icon_name("media-playback-start-symbolic");
    widgets.total_time.set_label("00:00");
    if !is_seeking.load(Acquire) {
        widgets.seek_scale.set_value(0.0);
        widgets.current_time.set_label("00:00");
    }
}

/// Handle a single playback event, updating UI widgets.
fn on_playback_event(
    event: &PlaybackEvent,
    widgets: &PlaybackWidgets,
    playback: &PlaybackEngine,
    is_seeking: &AtomicBool,
    cover_cache: &CoverArtCache,
    storage: &Arc<SqliteStorage>,
    meta_tx: &Sender<(i64, MetaResult)>,
) {
    match event {
        TrackStarted { track_id } => {
            widgets.artwork_image.set_paintable(None::<&MemoryTexture>);
            update_cover_from_cache(
                Some(*track_id),
                cover_cache,
                &widgets.artwork_image,
                playback,
            );
            handle_status_change(false, Some(*track_id), &widgets.labels, storage, meta_tx);
            widgets
                .play_button
                .set_icon_name("media-playback-pause-symbolic");
        }
        Paused => {
            widgets
                .play_button
                .set_icon_name("media-playback-start-symbolic");
        }
        Resumed => {
            widgets
                .play_button
                .set_icon_name("media-playback-pause-symbolic");
        }
        Stopped => {
            apply_stopped_state(widgets, is_seeking);
        }
        Seeked { .. } => {
            let s = playback.state();
            update_progress_labels(widgets, s.elapsed_seconds, s.duration_seconds, is_seeking);
        }
        PositionTick {
            elapsed_seconds,
            duration_seconds,
        } => {
            update_progress_labels(widgets, *elapsed_seconds, *duration_seconds, is_seeking);
        }
        OutputModeChanged { mode } => {
            widgets.output_mode_btn.set_icon_name(mode.icon_name());
            widgets
                .output_mode_btn
                .set_tooltip_text(Some(mode_button_tooltip(*mode)));
            widgets
                .output_mode_btn
                .update_property(&[PropertyLabel(mode_button_tooltip(*mode))]);
            update_volume_scale_visual(&widgets.volume_scale, *mode);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::gtk::{self, test},
    };

    use crate::{
        app::runtime::AppState,
        ui::player::{playback_events::spawn_async_listeners, sidebar::PlaybackWidgets},
    };

    #[test]
    fn async_listeners_spawn_all_three_tasks() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let widgets = PlaybackWidgets::test_fixture();
        spawn_async_listeners(&state, widgets);
        ensure!(
            format!("{:?}", state.handles.lock()).contains("tasks: 3"),
            "all three listeners should be spawned"
        );
        Ok(())
    }
}
