//! Application-level utilities including XDG base directory resolution and
//! Libadwaita `AdwApplication` setup.

use std::{
    env::{var, var_os},
    fs::create_dir_all,
    path::PathBuf,
    sync::Arc,
};

use {
    anyhow::{Context, Result},
    async_channel::{Receiver, Sender, unbounded},
    libadwaita::{
        Application,
        prelude::{ApplicationExt, ApplicationExtManual, GtkWindowExt},
    },
    tokio::sync::watch::{Sender as TokioSender, channel},
    tracing::info,
};

use crate::{
    library::scanner::{FsScanner, ScanEvent, ScannerConfig},
    playback::engine::PlaybackEngine,
    storage::{database::SqliteStorage, settings::ViewMode},
    ui::window::build_window,
};

/// Application identifier for D-Bus and resource paths.
const APP_ID: &str = "com.github.oxhidifi";

/// Shared application state passed to the window.
pub struct AppState {
    /// The playback engine controlling audio output.
    pub playback: Arc<PlaybackEngine>,
    /// The storage backend for library data.
    pub storage: Arc<SqliteStorage>,
    /// The library scanner for discovering audio files.
    pub scanner: Arc<FsScanner<SqliteStorage>>,
    /// Signal sender to notify the UI when the library changes.
    pub refresh_tx: TokioSender<()>,
    /// Broadcasts view mode changes (grid/column) to the UI.
    pub view_mode_tx: TokioSender<ViewMode>,
    /// Channel sender for forwarding scan events to the UI (status bar).
    pub scan_event_tx: Sender<ScanEvent>,
    /// Channel receiver for consuming scan events (cloned for each subscriber).
    pub scan_event_rx: Receiver<ScanEvent>,
    /// Channel sender for toast notifications displayed to the user.
    pub toast_tx: Sender<String>,
    /// Channel receiver for toast notifications.
    pub toast_rx: Receiver<String>,
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
        ScannerConfig::default(),
        scan_event_tx.clone(),
    ));

    let initial_view_mode = storage.get_view_mode();

    let state = Arc::new(AppState {
        playback,
        storage,
        scanner,
        refresh_tx: channel(()).0,
        view_mode_tx: channel(initial_view_mode).0,
        scan_event_tx,
        scan_event_rx,
        toast_tx,
        toast_rx,
    });

    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(move |app| {
        let window = build_window(app, &state);
        window.present();
    });

    info!("Starting application");
    app.run();

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
        app::AppState,
        library::scanner::{FsScanner, ScannerConfig},
        playback::engine::PlaybackEngine,
        storage::{database::SqliteStorage, settings::ViewMode::Grid},
    };

    impl AppState {
        /// Creates a mock `AppState` with an in-memory `SQLite` database for testing.
        ///
        /// # Errors
        ///
        /// Returns an error if the in-memory database cannot be initialized.
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

            Ok(Self {
                playback: Arc::new(PlaybackEngine::new()),
                storage,
                scanner: Arc::new(FsScanner::new(
                    scanner_storage,
                    ScannerConfig::default(),
                    scan_event_tx.clone(),
                )),
                refresh_tx: channel(()).0,
                view_mode_tx: channel(Grid).0,
                scan_event_tx,
                scan_event_rx,
                toast_tx,
                toast_rx,
            })
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
