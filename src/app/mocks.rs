//! Mock `AppState` construction and fresh storage test scaffolding.

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
    app::runtime::{AppChannels, AppState, build_broadcast_channels},
    library::scanner::FsScanner,
    playback::engine::PlaybackEngine,
    storage::{active_tab::ActiveTab::Albums, database::SqliteStorage, view_mode::ViewMode::Grid},
    threading::ThreadManager,
};

/// Shared Tokio runtime used to drive the `GLib` main context in tests.
///
/// Production runs the `GLib` main loop inside [`Runtime::block_on`], so
/// local futures spawned on it may await Tokio/SQLx operations. Tests that
/// iterate the process-wide default main context must enter this runtime
/// too: futures spawned by earlier tests (settings persistence, detail
/// fetches) are polled from that shared context on whichever thread pumps
/// it, and would otherwise panic on the missing Tokio context. Sharing one
/// runtime keeps timer registrations valid across pump calls.
static TEST_RUNTIME: LazyLock<Result<Runtime, std::io::Error>> = LazyLock::new(Runtime::new);

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

        Ok(build_app_state(storage))
    }
}

/// Build an `AppState` around existing storage, using fresh channels.
pub fn build_app_state(storage: Arc<SqliteStorage>) -> AppState {
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

    AppState::new(
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
    )
}

/// Create a fresh `SqliteStorage` backed by a temp database and settings file.
///
/// # Errors
///
/// Returns an error if the storage backend fails to connect.
pub async fn fresh_storage(dir: &Path) -> Result<Arc<SqliteStorage>> {
    let db = dir.join("library.db");
    let settings = dir.join("settings.json");
    Ok(Arc::new(
        SqliteStorage::connect_with_settings_path(&db, &settings)
            .await
            .context("Failed to create fresh storage")?,
    ))
}

/// Build a shared in-memory mock storage for tests.
///
/// # Errors
///
/// Returns an error if the mock storage cannot be initialized.
fn init_mock_storage() -> Result<Arc<SqliteStorage>> {
    let rt = Runtime::new().context("Failed to create tokio runtime")?;
    let storage = rt.block_on(create_mock_storage())?;
    Ok(Arc::new(storage))
}

/// Create an in-memory `SqliteStorage` with a unique settings file.
///
/// # Errors
///
/// Returns an error if the storage backend fails to connect.
async fn create_mock_storage() -> Result<SqliteStorage> {
    let db = Path::new(":memory:");
    let settings = temp_dir().join(format!("oxhidifi-mock-settings-{}.json", id()));
    SqliteStorage::connect_with_settings_path(db, &settings)
        .await
        .context("Failed to create mock storage")
}

/// Run `pump` inside the shared test Tokio runtime context.
///
/// # Arguments
///
/// * `pump` - Closure that iterates the `GLib` main context
///
/// # Errors
///
/// Returns an error if the shared test runtime failed to initialize.
pub fn pump_in_test_runtime<T>(pump: impl FnOnce() -> T) -> Result<T> {
    let runtime = &*TEST_RUNTIME;
    let rt = match runtime {
        Ok(rt) => rt,
        Err(e) => return Err(anyhow!("Failed to create shared test runtime: {e}")),
    };
    let guard = rt.enter();
    let result = pump();
    drop(guard);
    Ok(result)
}
