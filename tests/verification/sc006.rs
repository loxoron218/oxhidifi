//! SC-006 verification (T059).
//!
//! Configures a library directory with 3,000 synthetic audio files, starts a
//! scan, and asserts the library populates and becomes browsable within 9
//! seconds. Uses the `ScanThroughput` metrics collector from T046b for
//! throughput timing.
//!
//! Gated behind the `verification-tests` feature. Run with:
//!
//! ```text
//! cargo test --features verification-tests --test sc006
//! ```

use std::{
    fs::{File, create_dir_all},
    io::Write,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use {
    anyhow::{Context, Result, ensure},
    async_channel::unbounded,
    tempfile::tempdir,
    tokio::runtime::Runtime,
};

use oxhidifi::{
    library::scanner::{FsScanner, LibraryScanner},
    metrics::ScanThroughput,
    playback::write_wav_header,
    storage::{Storage, database::SqliteStorage},
};

/// Number of synthetic audio files (SC-006: 3,000).
const TRACK_COUNT: u32 = 3_000;

/// Time budget for the library to become browsable (SC-006: 9 s).
const BROWSABLE_BUDGET_SECS: f64 = 9.0;

/// Write a minimal valid WAV file.
fn write_tiny_wav(path: &Path, sample: i16) -> Result<()> {
    let mut f = File::create(path).context("failed to create wav")?;
    let sample_rate = 44100u32;
    let samples = 200u32;
    write_wav_header(&mut f, 1, sample_rate, 16, samples)?;
    for _ in 0..samples {
        f.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}

fn main_test() -> Result<()> {
    let rt = Runtime::new().context("failed to create tokio runtime")?;

    let dir = tempdir().context("failed to create temp dir")?;
    let music_dir = dir.path().join("music");
    create_dir_all(&music_dir).context("failed to create music dir")?;

    for i in 0..TRACK_COUNT {
        let path = music_dir.join(format!("track_{i:04}.wav"));
        write_tiny_wav(&path, i16::try_from(i).unwrap_or(i16::MAX))?;
    }

    let settings_path = dir.path().join("settings.json");
    let storage = Arc::new(
        rt.block_on(SqliteStorage::connect_with_settings_path(
            &dir.path().join("library.db"),
            &settings_path,
        ))
        .context("failed to connect to storage")?,
    );

    let (scan_event_tx, _) = unbounded();
    let scanner = Arc::new(FsScanner::new(Arc::clone(&storage), scan_event_tx, 8));

    let start = Instant::now();
    rt.block_on(scanner.scan_directory(&music_dir))
        .context("scan_directory failed")?;
    let elapsed = start.elapsed();

    let files = u64::from(TRACK_COUNT);
    ScanThroughput::record(files, elapsed);

    let tracks = rt
        .block_on(storage.get_all_albums())
        .context("get_all_albums failed")?;
    let albums = rt
        .block_on(storage.get_all_albums())
        .context("get_all_albums failed")?;
    let artists = rt
        .block_on(storage.get_all_artists())
        .context("get_all_artists failed")?;

    ensure!(
        elapsed < Duration::from_secs_f64(BROWSABLE_BUDGET_SECS),
        "SC-006: library took {:.2}s to populate (budget {BROWSABLE_BUDGET_SECS}s)",
        elapsed.as_secs_f64()
    );
    ensure!(!tracks.is_empty(), "SC-006: no tracks inserted");
    ensure!(!albums.is_empty(), "SC-006: no albums inserted");
    ensure!(!artists.is_empty(), "SC-006: no artists inserted");

    drop(storage);
    drop(dir);
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::main_test;

    #[test]
    fn sc006_library_browsable_within_budget() -> Result<()> {
        main_test()
    }
}
