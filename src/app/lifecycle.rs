//! Libadwaita application build/run and signal handlers.
//!
//! Startup checks and session emit/persist orchestration live in the sibling
//! [`bootstrap`] module.

use std::sync::Arc;

use {
    anyhow::{Context, Result},
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        Application,
        glib::{ControlFlow::Break, idle_add_local, spawn_future_local},
        prelude::{ApplicationExt, ApplicationExtManual, GtkWindowExt},
    },
    tokio::{
        fs::create_dir_all,
        select,
        signal::unix::{SignalKind, signal},
        spawn,
        task::spawn_blocking,
    },
    tracing::{info, warn},
};

use crate::{
    app::{
        bootstrap::{
            emit_session_events, persist_session_on_shutdown, run_startup_checks,
            spawn_watcher_loop,
        },
        runtime::{AppChannels, AppState, build_broadcast_channels},
        xdg_paths::data_dir,
    },
    library::{scanner::FsScanner, watcher::LibraryWatcher},
    playback::{engine::PlaybackEngine, transport::PlaybackTransport},
    storage::database::SqliteStorage,
    threading::ThreadManager,
    ui::window::build_window,
};

/// Application identifier for D-Bus and resource paths.
const APP_ID: &str = "com.github.oxhidifi";

/// Register SIGINT/SIGTERM handlers that trigger a graceful application quit.
///
/// Pressing `Ctrl+C` in the terminal delivers SIGINT, which by default
/// terminates the process before the session is persisted. This function
/// awaits the signals on a tokio task and notifies the `GLib` main thread via
/// `quit_tx`, which then calls [`Application::quit`] so `app.run()` returns
/// and the shutdown persistence in [`run_application`] runs.
fn spawn_signal_handlers(quit_tx: Sender<()>) {
    spawn(async move {
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "Failed to register SIGINT handler");
                return;
            }
        };
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "Failed to register SIGTERM handler");
                return;
            }
        };

        select! {
            _ = sigint.recv() => info!("SIGINT received, quitting gracefully"),
            _ = sigterm.recv() => info!("SIGTERM received, quitting gracefully"),
        }

        if let Err(e) = quit_tx.send(()).await {
            warn!(error = %e, "Failed to notify main thread of shutdown signal");
        }
    });
}

/// Dispatch an application-quit request on the `GLib` main thread.
///
/// `Application` is not `Send`, so the signal task cannot call [`Application::quit`]
/// directly; instead it sends on the channel this future awaits.  Spawns a
/// local future on the `GLib` main context so the app object is only ever
/// touched on the main thread.
fn dispatch_quit_on_main(app: Application, quit_rx: Receiver<()>) {
    spawn_future_local(async move {
        if quit_rx.recv().await.is_ok() {
            app.quit();
        }
    });
}

/// Build, configure, and run the Libadwaita application to completion.
///
/// Kept separate from [`run_application`] so the non-`Send` [`Application`]
/// handle never enters the async future's captured state.
fn run_gtk_application(state: &Arc<AppState>) {
    let app = Application::builder().application_id(APP_ID).build();

    let (quit_tx, quit_rx) = unbounded();

    let startup_state = Arc::clone(state);
    app.connect_activate(move |app| {
        build_window(app, &startup_state).present();
        spawn_future_local(run_startup_checks());

        let quit_app = app.clone();
        let quit_rx = quit_rx.clone();
        dispatch_quit_on_main(quit_app, quit_rx);

        let ss = Arc::clone(&startup_state);
        idle_add_local(move || {
            emit_session_events(&ss);
            Break
        });
    });

    info!("Starting application");
    spawn_signal_handlers(quit_tx);
    app.run();
}

/// Build and run the Libadwaita application.
///
/// Initializes the storage backend, playback engine, and presents the main
/// window. This is the top-level entry point for the GUI.
///
/// # Errors
///
/// Returns an error if the application cannot be built or if the storage
/// backend fails to initialize.
pub async fn run_application() -> Result<()> {
    let db_dir = data_dir();
    create_dir_all(&db_dir)
        .await
        .with_context(|| format!("Failed to create data directory: {}", db_dir.display()))?;

    let db_path = db_dir.join("library.db");
    let storage = Arc::new(
        SqliteStorage::connect(&db_path)
            .await
            .context("Failed to initialize storage")?,
    );

    let playback = Arc::new(PlaybackEngine::new());

    if let Err(e) = playback.set_volume(storage.get_settings_volume()) {
        warn!(error = %e, "Failed to apply persisted volume");
    }
    if let Err(e) = playback.set_output_mode(storage.get_output_mode()) {
        warn!(error = %e, "Failed to apply persisted output mode");
    }

    let (last_queue, last_index, last_track_id, last_position, last_duration) =
        storage.get_last_session();
    if !last_queue.is_empty() {
        let paths = storage
            .get_track_paths(&last_queue)
            .await
            .unwrap_or_default();
        playback.set_track_paths(paths);

        let queue = playback.queue();
        queue.set_queue(last_queue);
        if let Some(idx) = last_index
            && idx < queue.len()
        {
            queue.set_current_index(idx);
        }

        let mut ps = playback.shared.state.lock();
        ps.current_track_id = last_track_id;
        ps.elapsed_seconds = last_position;
        ps.duration_seconds = last_duration;
    }

    let (scan_event_tx, scan_event_rx) = unbounded();
    let (toast_tx, toast_rx) = unbounded();

    let scanner = Arc::new(FsScanner::new(
        Arc::clone(&storage),
        scan_event_tx.clone(),
        4,
    ));

    match LibraryWatcher::new(Arc::clone(&scanner)) {
        Ok((watcher, watcher_rx)) => spawn_watcher_loop(watcher, watcher_rx),
        Err(e) => warn!(error = %e, "Failed to create filesystem watcher"),
    }

    let initial_view_mode = storage.get_view_mode();
    let initial_active_tab = storage.get_active_tab();

    let (navigation_tx, navigation_rx) = unbounded();

    let channels = AppChannels {
        scan_event_tx,
        scan_event_rx,
        toast_tx,
        toast_rx,
        navigation_tx,
        navigation_rx,
    };

    let thread_manager = Arc::new(ThreadManager::new());

    let broadcast = build_broadcast_channels(initial_view_mode, initial_active_tab);

    let state = Arc::new(AppState::new(
        playback,
        storage,
        scanner,
        channels,
        broadcast,
        Arc::clone(&thread_manager),
    ));

    {
        run_gtk_application(&state);
    }

    let shutdown_state = Arc::clone(&state);
    if let Err(e) = spawn_blocking(move || persist_session_on_shutdown(&shutdown_state)).await {
        warn!(error = %e, "Session persistence task panicked");
    }

    thread_manager.shutdown();

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

    use {
        anyhow::{Context, Result, ensure},
        async_channel::unbounded,
        libadwaita::{
            Application,
            glib::ExitCode,
            gtk::{self, test},
            prelude::{ApplicationExt, ApplicationExtManual},
        },
    };

    use crate::app::{
        lifecycle::{APP_ID, dispatch_quit_on_main},
        mocks::pump_in_test_runtime,
    };

    static QUIT_SHUTDOWN_EMITTED: AtomicBool = AtomicBool::new(false);

    fn run_quit_test() -> Result<()> {
        let app = Application::builder().application_id(APP_ID).build();
        app.connect_activate(|_| ());

        let app_hold = app.hold();

        let (quit_tx, quit_rx) = unbounded();
        quit_tx
            .try_send(())
            .context("Failed to send quit request")?;

        QUIT_SHUTDOWN_EMITTED.store(false, SeqCst);
        app.connect_shutdown(|_| QUIT_SHUTDOWN_EMITTED.store(true, SeqCst));

        dispatch_quit_on_main(app.clone(), quit_rx);

        ensure!(
            app.run_with_args::<&str>(&[]) == ExitCode::new(0),
            "run() should return once the quit request was consumed"
        );
        ensure!(
            QUIT_SHUTDOWN_EMITTED.load(SeqCst),
            "application should shut down after the quit request"
        );
        drop(app_hold);
        Ok(())
    }

    #[test]
    fn dispatch_quit_on_main_requests_application_quit() -> Result<()> {
        pump_in_test_runtime(run_quit_test)?
    }
}
