//! Application-level utilities including XDG base directory resolution and
//! Libadwaita `AdwApplication` setup.

use std::{
    env::{var, var_os},
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

use {
    anyhow::{Context, Result},
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        Application,
        glib::spawn_future_local,
        prelude::{ApplicationExt, ApplicationExtManual, GtkWindowExt},
    },
    tokio::{
        fs::create_dir_all,
        spawn,
        sync::{
            mpsc::UnboundedReceiver,
            watch::{Sender as TokioSender, channel},
        },
        task::spawn_blocking,
    },
    tracing::info,
};

use crate::{
    library::{
        artwork::check_cache_version,
        scanner::{FsScanner, ScanEvent},
        watcher::{LibraryWatcher, WatcherEvent},
    },
    playback::{engine::PlaybackEngine, output::startup_device_check},
    storage::{
        database::SqliteStorage,
        settings::{ActiveTab, ViewMode},
    },
    threading::ThreadManager,
    ui::{CoverArtCache, window::build_window},
};

/// Application identifier for D-Bus and resource paths.
const APP_ID: &str = "com.github.oxhidifi";

/// Holds the channel pairs that are common across all `AppState` constructions.
pub struct AppChannels {
    /// Sender for forwarding scan events to the UI (status bar).
    pub scan_event_tx: Sender<ScanEvent>,
    /// Receiver for consuming scan events (cloned for each subscriber).
    pub scan_event_rx: Receiver<ScanEvent>,
    /// Sender for toast notifications displayed to the user.
    pub toast_tx: Sender<String>,
    /// Receiver for toast notifications.
    pub toast_rx: Receiver<String>,
    /// Sender for navigation events (detail page navigation).
    pub navigation_tx: Sender<NavigationEvent>,
    /// Receiver for navigation events.
    pub navigation_rx: Receiver<NavigationEvent>,
}

/// Shared application state passed to the window.
pub struct AppState {
    /// The playback engine controlling audio output.
    pub playback: Arc<PlaybackEngine>,
    /// The storage backend for library data.
    pub storage: Arc<SqliteStorage>,
    /// The library scanner for discovering audio files.
    pub scanner: Arc<FsScanner<SqliteStorage>>,
    /// Notifies the UI when the library changes (scan complete, etc.).
    pub refresh_tx: TokioSender<()>,
    /// Broadcasts view mode changes (grid/column) to library views.
    pub view_mode_tx: TokioSender<ViewMode>,
    /// Broadcasts active tab changes (albums/artists) to the UI.
    pub active_tab_tx: TokioSender<ActiveTab>,
    /// Channel sender for forwarding scan events to the UI (status bar).
    pub scan_event_tx: Sender<ScanEvent>,
    /// Channel receiver for consuming scan events (cloned for each subscriber).
    pub scan_event_rx: Receiver<ScanEvent>,
    /// Channel sender for toast notifications displayed to the user.
    pub toast_tx: Sender<String>,
    /// Channel receiver for toast notifications.
    pub toast_rx: Receiver<String>,
    /// Flag set while the user is dragging the seek bar. Prevents the polling
    /// timer from fighting the user's drag position and avoids redundant seeks.
    pub is_seeking: Arc<AtomicBool>,
    /// Sender for navigation events (detail page navigation).
    pub navigation_tx: Sender<NavigationEvent>,
    /// Receiver for navigation events.
    pub navigation_rx: Receiver<NavigationEvent>,
    /// Shared cache for decoded cover art textures.
    pub cover_art_cache: Arc<CoverArtCache>,
    /// Thread lifecycle manager for named OS threads.
    pub thread_manager: Arc<ThreadManager>,
}

impl AppState {
    /// Send a navigation event and log on failure.
    pub async fn send_navigation_event(&self, event: NavigationEvent) {
        if let Err(e) = self.navigation_tx.send(event).await {
            info!(error = %e, "Failed to send navigation event");
        }
    }

    /// Construct a new `AppState` with all fields explicitly provided.
    pub fn new(
        playback: Arc<PlaybackEngine>,
        storage: Arc<SqliteStorage>,
        scanner: Arc<FsScanner<SqliteStorage>>,
        channels: AppChannels,
        broadcast: BroadcastChannels,
        thread_manager: Arc<ThreadManager>,
    ) -> Self {
        Self {
            playback,
            storage,
            scanner,
            refresh_tx: broadcast.refresh,
            view_mode_tx: broadcast.view_mode,
            active_tab_tx: broadcast.active_tab,
            scan_event_tx: channels.scan_event_tx,
            scan_event_rx: channels.scan_event_rx,
            toast_tx: channels.toast_tx,
            toast_rx: channels.toast_rx,
            is_seeking: Arc::new(AtomicBool::new(false)),
            navigation_tx: channels.navigation_tx,
            navigation_rx: channels.navigation_rx,
            cover_art_cache: CoverArtCache::new_shared(&thread_manager),
            thread_manager,
        }
    }
}

/// Holds the tokio broadcast channel senders used for UI state signals.
pub struct BroadcastChannels {
    /// Signal sender to notify the UI when the library changes.
    pub refresh: TokioSender<()>,
    /// Broadcasts view mode changes (grid/column) to the UI.
    pub view_mode: TokioSender<ViewMode>,
    /// Broadcasts active tab changes (albums/artists) to the UI.
    pub active_tab: TokioSender<ActiveTab>,
}

/// Events for navigating between library views and detail pages.
#[derive(Debug, Clone, Copy)]
pub enum NavigationEvent {
    /// Navigate to the album detail page.
    AlbumDetail(i64),
    /// Navigate to the artist detail page.
    ArtistDetail(i64),
    /// Go back to the library grid view.
    Back,
}

/// Resolve an XDG directory from an environment variable with a fallback path.
///
/// # Errors
///
/// Returns an error if `HOME` environment variable is not set.
fn resolve_xdg_dir(env_var: &str, fallback: &str) -> Result<PathBuf> {
    if let Some(dir) = var_os(env_var)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
    {
        return Ok(dir);
    }
    let home = var("HOME").context("HOME environment variable is not set")?;
    Ok(PathBuf::from(home).join(fallback))
}

/// Resolve the XDG data home directory.
///
/// Falls back to `$HOME/.local/share` when `XDG_DATA_HOME` is not set.
///
/// # Errors
///
/// Returns an error if `HOME` is not set and `XDG_DATA_HOME` is also unset.
pub fn dirs_data_home() -> Result<PathBuf> {
    resolve_xdg_dir("XDG_DATA_HOME", ".local/share")
}

/// Resolve the XDG config home directory.
///
/// Falls back to `$HOME/.config` when `XDG_CONFIG_HOME` is not set.
///
/// # Errors
///
/// Returns an error if `HOME` is not set and `XDG_CONFIG_HOME` is also unset.
pub fn dirs_config_home() -> Result<PathBuf> {
    resolve_xdg_dir("XDG_CONFIG_HOME", ".config")
}

/// Resolve the XDG cache home directory.
///
/// Falls back to `$HOME/.cache` when `XDG_CACHE_HOME` is not set.
///
/// # Errors
///
/// Returns an error if `HOME` is not set and `XDG_CACHE_HOME` is also unset.
pub fn dirs_cache_home() -> Result<PathBuf> {
    resolve_xdg_dir("XDG_CACHE_HOME", ".cache")
}

/// Build the data directory for the application database.
fn data_dir() -> PathBuf {
    dirs_data_home()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("oxhidifi")
}

/// Run the filesystem watcher loop in the background.
fn spawn_watcher_loop(
    watcher: LibraryWatcher<SqliteStorage>,
    mut watcher_rx: UnboundedReceiver<WatcherEvent>,
) {
    spawn(async move {
        while let Some(event) = watcher_rx.recv().await {
            watcher.process_event(event).await;
        }
    });
}

/// Check artwork cache version and test audio device at startup.
async fn run_startup_checks() {
    if spawn_blocking(check_cache_version).await.is_err() {
        info!(target: "app::startup", "Failed to check artwork cache version");
    }
    match spawn_blocking(startup_device_check).await {
        Ok(Some(msg)) => {
            info!(target: "app::startup", "No audio device at startup: {msg}");
        }
        Ok(None) => {}
        Err(e) => {
            info!(
                target: "app::startup",
                error = %e,
                "Startup device check failed",
            );
        }
    }
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

    let (scan_event_tx, scan_event_rx) = unbounded();
    let (toast_tx, toast_rx) = unbounded();

    let scanner = Arc::new(FsScanner::new(
        Arc::clone(&storage),
        scan_event_tx.clone(),
        4,
    ));

    if let Ok((watcher, watcher_rx)) = LibraryWatcher::new(Arc::clone(&scanner)) {
        spawn_watcher_loop(watcher, watcher_rx);
    } else {
        info!(target: "app::startup", "Failed to create filesystem watcher");
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

    let broadcast = BroadcastChannels {
        refresh: channel(()).0,
        view_mode: channel(initial_view_mode).0,
        active_tab: channel(initial_active_tab).0,
    };

    let state = Arc::new(AppState::new(
        playback,
        storage,
        scanner,
        channels,
        broadcast,
        Arc::clone(&thread_manager),
    ));

    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(move |app| {
        build_window(app, &state).present();
        spawn_future_local(run_startup_checks());
    });

    info!("Starting application");
    app.run();
    thread_manager.shutdown();

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        sync::{Arc, LazyLock},
    };

    use {
        anyhow::{Context, Result, anyhow},
        async_channel::unbounded,
        tokio::{runtime::Runtime, sync::watch::channel},
    };

    use crate::{
        app::{AppChannels, AppState, BroadcastChannels},
        library::scanner::FsScanner,
        playback::engine::PlaybackEngine,
        storage::{
            database::SqliteStorage,
            settings::{ActiveTab::Albums, ViewMode::Grid},
        },
        threading::ThreadManager,
    };

    impl AppState {
        /// Create a mock `AppState` for testing.
        ///
        /// # Errors
        ///
        /// Returns an error if the underlying mock storage cannot be initialized.
        pub fn mock() -> Result<Self> {
            static MOCK_STORAGE: LazyLock<Result<Arc<SqliteStorage>>> =
                LazyLock::new(init_mock_storage);

            let storage = MOCK_STORAGE
                .as_ref()
                .map(Arc::clone)
                .map_err(|e| anyhow!("{e:#}"))?;

            let scanner_storage = Arc::clone(&storage);

            let (scan_event_tx, scan_event_rx) = unbounded();
            let (toast_tx, toast_rx) = unbounded();

            let (navigation_tx, navigation_rx) = unbounded();

            let channels = AppChannels {
                scan_event_tx,
                scan_event_rx,
                toast_tx,
                toast_rx,
                navigation_tx,
                navigation_rx,
            };

            let broadcast = BroadcastChannels {
                refresh: channel(()).0,
                view_mode: channel(Grid).0,
                active_tab: channel(Albums).0,
            };

            Ok(Self::new(
                Arc::new(PlaybackEngine::new()),
                storage,
                Arc::new(FsScanner::new(
                    scanner_storage,
                    channels.scan_event_tx.clone(),
                    4,
                )),
                channels,
                broadcast,
                Arc::new(ThreadManager::new()),
            ))
        }
    }

    fn init_mock_storage() -> Result<Arc<SqliteStorage>> {
        let rt = Runtime::new().context("Failed to create tokio runtime")?;
        let storage = rt.block_on(create_mock_storage())?;
        Ok(Arc::new(storage))
    }

    async fn create_mock_storage() -> Result<SqliteStorage> {
        SqliteStorage::connect(Path::new(":memory:"))
            .await
            .context("Failed to create mock storage")
    }
}
