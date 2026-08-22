//! Library load verification (T052) per SC-004.
//!
//! Populates a library with 10,000 synthetic WAV files, scans it, asserts the
//! scan completes in under 30 seconds (≥ 333 files/second), and records the
//! measured throughput via the `ScanThroughput` metrics collector.
//!
//! Gated behind the `verification-tests` feature (not part of normal
//! `cargo test`). Run with:
//!
//! ```text
//! cargo test --features verification-tests --test load_verification
//! ```

use std::{
    fs::{File, create_dir_all},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use {
    anyhow::{Context, Result, ensure},
    async_channel::{Sender, unbounded},
    num_traits::cast,
    tempfile::{TempDir, tempdir},
    tokio::runtime::Runtime,
};

use oxhidifi::{
    library::scanner::{FsScanner, LibraryScanner, events::ScanEvent},
    metrics::ScanThroughput,
    playback::write_wav_header,
    storage::database::SqliteStorage,
};

/// Number of synthetic tracks to generate (SC-004: 10,000).
const TRACK_COUNT: u32 = 10_000;

/// Throughput threshold in files/second (SC-004: ≥ 333).
const MIN_FILES_PER_SECOND: f64 = 333.0;

/// Time budget for the full scan in seconds (SC-004: < 30 s).
const SCAN_TIME_BUDGET_SECS: f64 = 30.0;

/// Sample rate of the tiny WAV fixtures (mono 44.1 kHz, 16-bit).
const WAV_SAMPLE_RATE: u32 = 44_100;

/// Number of samples in each tiny WAV fixture.
const WAV_SAMPLE_COUNT: u32 = 200;

/// Write a minimal valid WAV file (mono, 44.1 kHz, 16-bit) with a short tone.
fn write_tiny_wav(path: &Path, sample: i16) -> Result<()> {
    let mut f = File::create(path).context("failed to create wav")?;
    let tone = sample.to_le_bytes();
    write_wav_header(&mut f, 1, WAV_SAMPLE_RATE, 16, WAV_SAMPLE_COUNT)?;
    for _ in 0..WAV_SAMPLE_COUNT {
        f.write_all(&tone)?;
    }
    Ok(())
}

/// Generate `TRACK_COUNT` tiny WAV files in `dir`, returning their paths.
fn generate_tracks(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::with_capacity(usize::try_from(TRACK_COUNT).unwrap_or(0));
    for i in 0..TRACK_COUNT {
        let path = dir.join(format!("track_{i:05}.wav"));

        write_tiny_wav(&path, i16::try_from(i).unwrap_or(i16::MAX))?;
        paths.push(path);
    }
    Ok(paths)
}

/// Create a temp directory with a `music` subdirectory for the fixtures.
fn music_temp_dir() -> Result<(TempDir, PathBuf)> {
    let dir = tempdir().context("failed to create temp dir")?;
    let music_dir = dir.path().join("music");
    create_dir_all(&music_dir).context("failed to create music dir")?;
    Ok((dir, music_dir))
}

/// Connect storage and build a scanner over the generated library.
fn scan_env(
    rt: &Runtime,
    dir: &Path,
    scan_event_tx: Sender<ScanEvent>,
) -> Result<(Arc<SqliteStorage>, Arc<FsScanner<SqliteStorage>>)> {
    let storage = Arc::new(
        rt.block_on(SqliteStorage::connect_with_settings_path(
            &dir.join("library.db"),
            &dir.join("settings.json"),
        ))
        .context("failed to connect to storage")?,
    );
    let scanner = Arc::new(FsScanner::new(Arc::clone(&storage), scan_event_tx, 8));
    Ok((storage, scanner))
}

/// Time a full directory scan and return the elapsed duration.
fn time_scan(
    rt: &Runtime,
    scanner: &Arc<FsScanner<SqliteStorage>>,
    dir: &Path,
) -> Result<Duration> {
    let start = Instant::now();
    rt.block_on(scanner.scan_directory(dir))
        .context("scan_directory failed")?;
    Ok(start.elapsed())
}

fn main_test() -> Result<()> {
    let rt = Runtime::new().context("failed to create tokio runtime")?;

    let (dir, music_dir) = music_temp_dir()?;

    let paths = generate_tracks(&music_dir)?;
    ensure!(
        paths.len() == usize::try_from(TRACK_COUNT).unwrap_or(0),
        "failed to generate all tracks"
    );

    let (scan_event_tx, _) = unbounded();
    let (storage, scanner) = scan_env(&rt, dir.path(), scan_event_tx)?;
    let elapsed = time_scan(&rt, &scanner, &music_dir)?;

    let files = u64::try_from(paths.len()).unwrap_or(0);
    let files_per_second =
        cast::<u64, f64>(files).unwrap_or(0.0) / elapsed.as_secs_f64().max(f64::EPSILON);

    ScanThroughput::record(files, elapsed);

    ensure!(
        elapsed < Duration::from_secs_f64(SCAN_TIME_BUDGET_SECS),
        "SC-004: scan of {files} files took {:.2}s (budget {SCAN_TIME_BUDGET_SECS}s)",
        elapsed.as_secs_f64()
    );
    ensure!(
        files_per_second >= MIN_FILES_PER_SECOND,
        "SC-004: throughput {files_per_second:.1} files/s below {MIN_FILES_PER_SECOND}"
    );

    drop(storage);
    drop(dir);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_ten_thousand_tracks_within_budget() -> Result<()> {
        super::main_test()
    }
}
