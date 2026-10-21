//! Mock `AppState` construction, fresh storage, and widget fixtures for tests.

use std::{
    env::temp_dir,
    io::Error,
    path::Path,
    process::id,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicU64, Ordering::SeqCst},
    },
};

use {
    anyhow::{Context, Result, anyhow},
    libadwaita::{
        glib::object::{Cast, IsA},
        gtk::{
            Adjustment, Box, Button, Label,
            Orientation::{Horizontal, Vertical},
            Picture, Scale, Widget, Window,
            accessible::Property::Label as PropertyLabel,
        },
        prelude::{AccessibleExtManual, WidgetExt},
    },
    tokio::runtime::Runtime,
};

use crate::{
    app::runtime::{AppState, build_app_channels, build_broadcast_channels},
    library::scanner::FsScanner,
    playback::engine::PlaybackEngine,
    storage::{active_tab::ActiveTab::Albums, database::SqliteStorage, view_mode::ViewMode::Grid},
    threading::ThreadManager,
    ui::player::panel_widgets::{PlaybackWidgets, TrackLabels},
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
static TEST_RUNTIME: LazyLock<Result<Runtime, Error>> = LazyLock::new(Runtime::new);

/// Counter for unique isolated settings files.
static ISOLATED_SETTINGS_COUNTER: AtomicU64 = AtomicU64::new(0);

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

impl PlaybackWidgets {
    /// Build deterministic widgets for listener tests.
    #[must_use]
    pub fn test_fixture() -> Self {
        let seek_adjustment = Adjustment::new(0.0, 0.0, 100.0, 1.0, 10.0, 0.0);
        let volume_adjustment = Adjustment::new(0.0, 0.0, 1.0, 0.01, 0.1, 0.0);
        let artwork_image = Picture::builder()
            .css_classes(["test-fixture"])
            .tooltip_text("Test artwork image")
            .build();
        artwork_image.update_property(&[PropertyLabel("Test artwork image")]);
        let play_button = Button::builder()
            .css_classes(["test-fixture"])
            .tooltip_text("Test play button")
            .build();
        let seek_scale = Scale::builder()
            .orientation(Horizontal)
            .adjustment(&seek_adjustment)
            .css_classes(["test-fixture"])
            .tooltip_text("Test seek scale")
            .build();
        let current_time = Label::builder()
            .label("00:00")
            .css_classes(["test-fixture"])
            .tooltip_text("Test current time")
            .build();
        let total_time = Label::builder()
            .label("00:00")
            .css_classes(["test-fixture"])
            .tooltip_text("Test total time")
            .build();
        let output_mode_btn = Button::builder()
            .css_classes(["test-fixture"])
            .tooltip_text("Test output mode button")
            .build();
        let volume_scale = Scale::builder()
            .orientation(Horizontal)
            .adjustment(&volume_adjustment)
            .css_classes(["test-fixture"])
            .tooltip_text("Test volume scale")
            .build();
        Self {
            labels: TrackLabels::test_fixture(),
            artwork_image,
            play_button,
            seek_scale,
            current_time,
            total_time,
            output_mode_btn,
            volume_scale,
        }
    }
}

impl TrackLabels {
    /// Build deterministic labels for widget tests.
    #[must_use]
    pub fn test_fixture() -> Self {
        let title = Label::builder()
            .label("Title")
            .css_classes(["test-fixture"])
            .tooltip_text("Test title label")
            .build();
        title.update_property(&[PropertyLabel("Test title label")]);
        let artist = Label::builder()
            .label("Artist")
            .css_classes(["test-fixture"])
            .tooltip_text("Test artist label")
            .build();
        let album = Label::builder()
            .label("Album")
            .css_classes(["test-fixture"])
            .tooltip_text("Test album label")
            .build();
        let format = Label::builder()
            .label("FLAC")
            .css_classes(["test-fixture"])
            .tooltip_text("Test format label")
            .build();
        Self {
            title,
            artist,
            album,
            format,
        }
    }
}

/// Build an `AppState` around existing storage, using fresh channels.
pub fn build_app_state(storage: Arc<SqliteStorage>) -> AppState {
    let scanner_storage = Arc::clone(&storage);

    let channels = build_app_channels();

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

/// Build an `AppState` with independent in-memory storage for tests.
///
/// Unlike [`AppState::mock`], which shares one global backend, each call
/// returns state with its own storage backend so parallel tests that mutate
/// settings cannot interfere with each other.
///
/// # Errors
///
/// Returns an error if the storage backend fails to connect.
pub fn isolated_app_state() -> Result<AppState> {
    let index = ISOLATED_SETTINGS_COUNTER.fetch_add(1, SeqCst);
    let settings_path =
        temp_dir().join(format!("oxhidifi-isolated-settings-{}-{index}.json", id()));
    let rt = Runtime::new().context("Failed to create tokio runtime")?;
    let backend = rt
        .block_on(SqliteStorage::connect_with_settings_path(
            Path::new(":memory:"),
            &settings_path,
        ))
        .context("Failed to create isolated storage")?;
    Ok(build_app_state(Arc::new(backend)))
}

/// Build a shared in-memory mock storage for tests.
fn init_mock_storage() -> Result<Arc<SqliteStorage>> {
    let rt = Runtime::new().context("Failed to create tokio runtime")?;
    let storage = rt.block_on(create_mock_storage())?;
    Ok(Arc::new(storage))
}

/// Create an in-memory `SqliteStorage` with a unique settings file.
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

/// Collect all descendants of type `T` under `root` in tree order.
///
/// Shared by focus-chain and CSS-class regression tests to avoid copy-pasted
/// tree walkers across drag, gallery, and preferences modules.
///
/// # Arguments
///
/// * `root` - Widget subtree to walk.
///
/// # Returns
///
/// * `Vec<T>` - All matching descendants in tree order.
#[must_use]
pub fn find_descendants<T>(root: &Widget) -> Vec<T>
where
    T: IsA<Widget>,
{
    let mut found = Vec::new();
    let mut next = root.first_child();
    while let Some(child) = next {
        next = child.next_sibling();
        found.extend(child.clone().downcast::<T>());
        found.extend(find_descendants::<T>(&child));
    }
    found
}

/// Build a vertical test card for gallery overlay tests.
///
/// Shared by grid-fit and grid-flow regression tests to avoid copy-pasted
/// card builders.
///
/// # Returns
///
/// * `Box` - Card with `test-fixture` class and accessibility label.
#[must_use]
pub fn test_card() -> Box {
    let card = Box::builder()
        .orientation(Vertical)
        .spacing(0)
        .css_classes(["test-fixture"])
        .tooltip_text("Test card")
        .build();
    card.update_property(&[PropertyLabel("Test card")]);
    card
}

/// Build a parent window for dialog tests.
///
/// Shared by dialog regression tests to avoid copy-pasted parent scaffolds.
///
/// # Returns
///
/// * `Window` - Window with `test-parent` class and accessibility label.
#[must_use]
pub fn test_parent_window() -> Window {
    let parent = Window::builder()
        .css_classes(["test-parent", "background"])
        .tooltip_text("Test parent window")
        .build();
    parent.update_property(&[PropertyLabel("Test parent window")]);
    parent
}
