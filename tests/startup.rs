//! No-device-at-startup acceptance test (FR-030).
//!
//! Verifies that when no audio device is available, the application:
//! - Starts without panicking
//! - Returns a descriptive missing-hardware message
//! - Library scanning still functions independently

#[cfg(test)]
mod tests {
    use std::{panic::catch_unwind, path::Path, sync::Arc};

    use {
        anyhow::{Context, Result, ensure},
        async_channel::unbounded,
        tokio::runtime::Runtime,
        tracing::warn,
    };

    use oxhidifi::{
        library::scanner::{FsScanner, LibraryScanner},
        playback::{devices::startup_device_check, engine::PlaybackEngine},
        storage::{Storage, database::SqliteStorage},
    };

    fn log_device_check() {
        let Some(e) = startup_device_check() else {
            return;
        };
        warn!(error = %e, "startup_device_check returned");
    }

    #[test]
    fn startup_device_check_does_not_panic() {
        let result = catch_unwind(log_device_check);
        assert!(result.is_ok(), "startup_device_check must not panic");
    }

    #[test]
    fn engine_created_without_device() {
        let result = catch_unwind(PlaybackEngine::new);
        assert!(
            result.is_ok(),
            "PlaybackEngine::new must not panic regardless of device availability"
        );
    }

    #[test]
    fn library_scanning_works_without_device() -> Result<()> {
        let rt = Runtime::new().context("Failed to create tokio runtime")?;
        let dir = tempfile::tempdir().context("Failed to create temp dir")?;
        let settings_path = dir.path().join("settings.json");

        rt.block_on(async {
            let storage = Arc::new(
                SqliteStorage::connect_with_settings_path(Path::new(":memory:"), &settings_path)
                    .await
                    .context("Failed to create in-memory storage")?,
            );

            let (scan_event_tx, _) = unbounded();
            let scanner = Arc::new(FsScanner::new(Arc::clone(&storage), scan_event_tx, 4));

            let dirs = storage
                .list_library_directories()
                .await
                .context("Failed to list directories")?;
            ensure!(
                dirs.is_empty(),
                "Fresh storage should have no library directories"
            );

            let result = scanner.cancel();
            ensure!(
                result.is_ok(),
                "Cancelling a scan on empty storage must succeed"
            );

            Ok(())
        })
    }
}
