//! Shared application state, channels, and grid coordination types.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64},
    },
};

use {
    async_channel::{Receiver, Sender, unbounded},
    parking_lot::Mutex,
    tracing::warn,
};

use crate::{
    library::scanner::{FsScanner, events::ScanEvent},
    playback::engine::PlaybackEngine,
    storage::{
        active_tab::ActiveTab,
        catalog::{Album, Artist},
        database::SqliteStorage,
        formats::FormatInfo,
        sort_rules::{AlbumSortItem, ArtistSortItem},
        view_mode::ViewMode,
    },
    threading::ThreadManager,
    ui::{signal::ValueSignal, texture_pool::CoverArtCache},
};

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

/// Build the app's standard channel set with fresh pairs.
///
/// Shared by `main`, the test mock, and the integration verification tests so
/// all `AppState` constructions wire the same channels.
#[must_use]
pub fn build_app_channels() -> AppChannels {
    let (scan_event_tx, scan_event_rx) = unbounded();
    let (toast_tx, toast_rx) = unbounded();
    let (navigation_tx, navigation_rx) = unbounded();
    AppChannels {
        scan_event_tx,
        scan_event_rx,
        toast_tx,
        toast_rx,
        navigation_tx,
        navigation_rx,
    }
}

/// Build the app's broadcast channel set with the given initial UI state.
///
/// Shared by `main` and the test mock so both construct their channels
/// identically. `view_mode`/`active_tab` seed the respective channels with
/// the state the UI should present on startup.
#[must_use]
pub fn build_broadcast_channels(
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
