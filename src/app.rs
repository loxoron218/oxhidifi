//! Application-level utilities including XDG base directory resolution and
//! Libadwaita `AdwApplication` setup.

use std::{
    collections::HashMap,
    env::{var, var_os},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64},
    },
};

use {
    anyhow::{Context, Result},
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        Application,
        glib::{ControlFlow::Break, idle_add_local, spawn_future_local},
        prelude::{ApplicationExt, ApplicationExtManual, GtkWindowExt},
    },
    parking_lot::Mutex,
    tokio::{fs::create_dir_all, spawn, sync::mpsc::UnboundedReceiver, task::spawn_blocking},
    tracing::{info, warn},
};

use crate::{
    library::{
        artwork::check_cache_version,
        scanner::{FsScanner, ScanEvent},
        watcher::{LibraryWatcher, WatcherEvent},
    },
    playback::{
        control::PlaybackController,
        devices::startup_device_check,
        engine::PlaybackEngine,
        state::PlaybackEvent::{Paused, PositionTick, QueueChanged, TrackStarted},
    },
    storage::{
        database::SqliteStorage,
        formats::FormatInfo,
        records::{Album, Artist},
        settings::{ActiveTab, ViewMode},
        sort_rules::{AlbumSortItem, ArtistSortItem},
    },
    threading::ThreadManager,
    ui::{CoverArtCache, signal::ValueSignal, window::build_window},
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
    pub refresh: ValueSignal<()>,
    /// Notifies the UI of view mode changes (grid/column).
    pub view_mode: ValueSignal<ViewMode>,
    /// Notifies the UI of active tab changes (albums/artists).
    pub active_tab: ValueSignal<ActiveTab>,
    /// Broadcasts album sort configuration changes to the album grid.
    /// Uses `async_channel` (not `tokio::sync`) so the `spawn_future_local`
    /// subscriber is woken reliably by the `GLib` main context — see the
    /// scan-event loop in `status.rs` for the same precedent.
    pub albums_sort_tx: Sender<()>,
    /// Receiver clone for [`Self::albums_sort_tx`] (grid listeners subscribe here).
    pub albums_sort_rx: Receiver<()>,
    /// Broadcasts artist sort configuration changes to the artist grid.
    pub artists_sort_tx: Sender<()>,
    /// Receiver clone for [`Self::artists_sort_tx`].
    pub artists_sort_rx: Receiver<()>,
    /// Broadcasts zoom level changes to the album grid.
    /// `async_channel` receivers compete for messages (no broadcast), so each
    /// grid gets its own channel and the header fans out every zoom change —
    /// see [`Self::artists_zoom_tx`] for the artist counterpart.
    pub albums_zoom_tx: Sender<()>,
    /// Receiver clone for [`Self::albums_zoom_tx`] (album grid subscribes here).
    pub albums_zoom_rx: Receiver<()>,
    /// Broadcasts zoom level changes to the artist grid.
    pub artists_zoom_tx: Sender<()>,
    /// Receiver clone for [`Self::artists_zoom_tx`] (artist grid subscribes here).
    pub artists_zoom_rx: Receiver<()>,
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
    /// Coordination state for the album grid (cache, dirty/ready flags, build
    /// generations). Owned by `AppState` so the atomics need no `Arc` wrapper.
    pub album_grid: GridState<CachedAlbumData, Vec<AlbumSortItem>>,
    /// Coordination state for the artist grid, mirroring [`Self::album_grid`].
    pub artist_grid: GridState<CachedArtistData, Vec<ArtistSortItem>>,
    /// `(album_id, overlay index, artwork path)` for the current album grid,
    /// index-aligned with the grid's card order. Shared via `Arc` so zoom
    /// resizes clone the handle instead of deep-copying every path string.
    /// Album-only: artists have no per-size cover art to resize.
    pub album_grid_covers: Mutex<Arc<Vec<(i64, usize, String)>>>,
    /// Thread lifecycle manager for named OS threads.
    pub thread_manager: Arc<ThreadManager>,
}

impl AppState {
    /// Send a navigation event and log on failure.
    pub async fn send_navigation_event(&self, event: NavigationEvent) {
        if let Err(e) = self.navigation_tx.send(event).await {
            warn!(error = %e, "Failed to send navigation event");
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
            refresh: broadcast.refresh,
            view_mode: broadcast.view_mode,
            active_tab: broadcast.active_tab,
            albums_sort_tx: broadcast.albums_sort,
            albums_sort_rx: broadcast.albums_sort_rx,
            artists_sort_tx: broadcast.artists_sort,
            artists_sort_rx: broadcast.artists_sort_rx,
            albums_zoom_tx: broadcast.albums_zoom,
            albums_zoom_rx: broadcast.albums_zoom_rx,
            artists_zoom_tx: broadcast.artists_zoom,
            artists_zoom_rx: broadcast.artists_zoom_rx,
            scan_event_tx: channels.scan_event_tx,
            scan_event_rx: channels.scan_event_rx,
            toast_tx: channels.toast_tx,
            toast_rx: channels.toast_rx,
            is_seeking: Arc::new(AtomicBool::new(false)),
            navigation_tx: channels.navigation_tx,
            navigation_rx: channels.navigation_rx,
            cover_art_cache: CoverArtCache::new_shared(&thread_manager),
            album_grid: GridState::default(),
            artist_grid: GridState::default(),
            album_grid_covers: Mutex::new(Arc::new(Vec::new())),
            thread_manager,
        }
    }
}

/// Holds the UI state signal channels used for UI synchronization.
pub struct BroadcastChannels {
    /// Signal to notify the UI when the library changes.
    pub refresh: ValueSignal<()>,
    /// Broadcasts view mode changes (grid/column) to the UI.
    pub view_mode: ValueSignal<ViewMode>,
    /// Broadcasts active tab changes (albums/artists) to the UI.
    pub active_tab: ValueSignal<ActiveTab>,
    /// Broadcasts album sort configuration changes to the album grid.
    pub albums_sort: Sender<()>,
    /// Receiver clone for [`Self::albums_sort`] (grid listeners subscribe here).
    pub albums_sort_rx: Receiver<()>,
    /// Broadcasts artist sort configuration changes to the artist grid.
    pub artists_sort: Sender<()>,
    /// Receiver clone for [`Self::artists_sort`].
    pub artists_sort_rx: Receiver<()>,
    /// Broadcasts zoom level changes to the album grid.
    pub albums_zoom: Sender<()>,
    /// Receiver clone for [`Self::albums_zoom`] (album grid subscribes here).
    pub albums_zoom_rx: Receiver<()>,
    /// Broadcasts zoom level changes to the artist grid.
    pub artists_zoom: Sender<()>,
    /// Receiver clone for [`Self::artists_zoom`] (artist grid subscribes here).
    pub artists_zoom_rx: Receiver<()>,
}

/// Cached album library data used to avoid re-fetching from the database
/// on sort/zoom changes.
///
/// The collections are shared via [`Arc`]: rebuilds borrow the album data
/// and derive the display order from a sorted index vector, so no deep
/// clone of the `Vec<Album>` (or the lookup maps) is performed.
pub struct CachedAlbumData {
    /// All albums in the library.
    pub albums: Arc<Vec<Album>>,
    /// Map of artist ID → display name.
    pub artist_names: Arc<HashMap<i64, String>>,
    /// Map of album ID → format summary.
    pub format_info: Arc<HashMap<i64, FormatInfo>>,
}

/// Cached artist library data.
pub struct CachedArtistData {
    /// All artists with at least one album.
    pub artists: Arc<Vec<Artist>>,
}

/// Coordination state for a single library grid (albums or artists).
///
/// Owned by `AppState` (shared via `Arc`), so the atomics need no additional
/// `Arc` wrapper. The cached data's inner `Arc` handles are retained so that
/// batched idle-build closures can clone the collections cheaply.
///
/// `C` is the grid's sort-configuration type (e.g. `Vec<AlbumSortItem>`),
/// used as the memo key for cached sort indices. The default `()` keeps
/// construction generic-free for callers that never touch the memo.
pub struct GridState<T, C = ()> {
    /// In-memory library data cache (avoids DB re-fetch on sort/zoom changes).
    pub cache: Mutex<Option<T>>,
    /// Set when pending sort/zoom changes arrived while the tab was hidden,
    /// so switching back rebuilds once.
    pub dirty: AtomicBool,
    /// Set when the grid has finished populating, so zoom events can resize
    /// the live cards in place instead of rebuilding. Cleared on rebuild.
    pub ready: AtomicBool,
    /// Incremented on every grid build. Batched population closures capture
    /// the value at schedule time and bail if a newer build started, so a
    /// superseded build can't write stale state.
    pub build_seq: AtomicU64,
    /// Incremented whenever the library is refreshed. Build futures capture
    /// the value before their DB awaits and bail afterwards if it changed,
    /// so a stale in-flight build can't add a duplicate mode child or commit
    /// pre-refresh data to the in-memory cache.
    pub generation: AtomicU64,
    /// Memoized display-order indices, valid while `generation` and the sort
    /// configuration both match. Lets repeated tab/mode switches reuse the
    /// sort instead of re-running the comparison on every rebuild.
    pub memo: Mutex<Option<SortMemo<C>>>,
}

impl<T, C> Default for GridState<T, C> {
    fn default() -> Self {
        Self {
            cache: Mutex::new(None),
            dirty: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            build_seq: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            memo: Mutex::new(None),
        }
    }
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

/// Cached display-order indices for a library grid.
///
/// Keyed by the grid `generation` (incremented on library refresh) and the
/// exact sort configuration the indices were computed for. The `indices`
/// are shared via [`Arc`] so rebuilds clone the handle, not the vector.
pub struct SortMemo<C> {
    /// The grid generation the indices were computed for.
    pub generation: u64,
    /// The sort configuration the indices were computed for.
    pub config: C,
    /// Display order of the cached items (index into the cached collection).
    pub indices: Arc<[usize]>,
}

/// Build the app's broadcast channel set with the given initial UI state.
///
/// Shared by `main` and the test mock so both construct their channels
/// identically. `view_mode`/`active_tab` seed the respective channels with
/// the state the UI should present on startup.
fn build_broadcast_channels(
    initial_view_mode: ViewMode,
    initial_active_tab: ActiveTab,
) -> BroadcastChannels {
    let (albums_sort, albums_sort_rx) = unbounded();
    let (artists_sort, artists_sort_rx) = unbounded();
    let (albums_zoom, albums_zoom_rx) = unbounded();
    let (artists_zoom, artists_zoom_rx) = unbounded();
    BroadcastChannels {
        refresh: ValueSignal::new(()),
        view_mode: ValueSignal::new(initial_view_mode),
        active_tab: ValueSignal::new(initial_active_tab),
        albums_sort,
        albums_sort_rx,
        artists_sort,
        artists_sort_rx,
        albums_zoom,
        albums_zoom_rx,
        artists_zoom,
        artists_zoom_rx,
    }
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
    if let Err(e) = spawn_blocking(check_cache_version).await {
        warn!(error = %e, "Failed to check artwork cache version");
    }
    match spawn_blocking(startup_device_check).await {
        Ok(Some(msg)) => {
            info!(msg, "No audio device at startup");
        }
        Ok(None) => {}
        Err(e) => {
            warn!(
                error = %e,
                "Startup device check failed",
            );
        }
    }
}

/// Emit playback events on startup to reflect the restored session in the UI.
fn emit_session_events(state: &AppState) {
    let track_ids = state.playback.queue().tracks();
    if track_ids.is_empty() {
        return;
    }

    state
        .playback
        .shared
        .send_event(&QueueChanged { track_ids });

    let s = state.playback.state();
    let Some(track_id) = s.current_track_id else {
        return;
    };
    state.playback.shared.send_event(&TrackStarted { track_id });
    state.playback.shared.send_event(&Paused);
    state.playback.shared.send_event(&PositionTick {
        elapsed_seconds: s.elapsed_seconds,
        duration_seconds: s.duration_seconds,
    });
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

    let app = Application::builder().application_id(APP_ID).build();

    let startup_state = Arc::clone(&state);
    app.connect_activate(move |app| {
        build_window(app, &startup_state).present();
        spawn_future_local(run_startup_checks());

        let ss = Arc::clone(&startup_state);
        idle_add_local(move || {
            emit_session_events(&ss);
            Break
        });
    });

    info!("Starting application");
    app.run();
    thread_manager.shutdown();

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        path::Path,
        process::id,
        sync::{Arc, LazyLock},
    };

    use {
        anyhow::{Context, Result, anyhow},
        async_channel::unbounded,
        tokio::runtime::Runtime,
    };

    use crate::{
        app::{AppChannels, AppState, build_broadcast_channels},
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

            let broadcast = build_broadcast_channels(Grid, Albums);

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
        let db = Path::new(":memory:");
        let settings = temp_dir().join(format!("oxhidifi-mock-settings-{}.json", id()));
        SqliteStorage::connect_with_settings_path(db, &settings)
            .await
            .context("Failed to create mock storage")
    }
}
