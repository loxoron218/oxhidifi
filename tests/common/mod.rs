//! Shared test infrastructure: fixture helpers and mock utilities.

use std::{
    fs::{File, write},
    io::{Error, Result, Write},
    path::{Path, PathBuf},
};

use {
    anyhow::{Context, Result as AnyhowResult, bail},
    tempfile::{TempDir, tempdir},
};

use oxhidifi_refactor::playback::{
    decoder::Decoder, gapless::GaplessTransitioner, write_wav_header,
};

/// Creates a temporary directory for test fixtures.
///
/// # Errors
///
/// Returns an error if the OS cannot create the directory.
pub fn create_fixture_dir() -> Result<TempDir> {
    tempdir().map_err(Error::other)
}

/// Creates a dummy audio file at the given path for scanner testing.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub fn create_dummy_audio_file(dir: &Path, name: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    write(&path, [])?;
    Ok(path)
}

/// Creates a minimal FLAC file for testing.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub fn create_minimal_flac(dir: &Path, name: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    let mut data = Vec::new();
    data.extend_from_slice(b"fLaC");

    data.extend_from_slice(&[0x00, 0x22, 0x00, 0x00]);
    data.extend_from_slice(&[0; 34]);
    write(&path, &data)?;
    Ok(path)
}

/// Write a minimal WAV file with the given sample rate, channel count, and
/// 16-bit samples.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written to.
pub fn write_wav(path: &Path, channels: u16, sample_rate: u32, samples: &[i16]) -> Result<()> {
    let mut f = File::create(path)?;
    let data_size = u32::try_from(samples.len()).unwrap_or(0) * 2;
    write_wav_header(&mut f, channels, sample_rate, 16, data_size)?;
    for s in samples {
        f.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

/// Drain all samples from a decoder until end-of-stream.
///
/// # Errors
///
/// Returns an error if the decoder fails to open or decode.
pub fn drain_decoder(path: &Path) -> AnyhowResult<()> {
    let mut decoder = Decoder::open(path).context("Failed to open decoder")?;
    loop {
        let batch = decoder.decode_next().context("Failed during decode")?;
        if batch.samples.is_empty() {
            break Ok(());
        }
    }
}

/// Perform a gapless transition between two audio files and return the decoded
/// samples of the next track's first batch.
///
/// # Errors
///
/// Returns an error if decoding, pre-buffering, or transition fails.
pub fn transition_and_decode(
    current_path: &Path,
    next_path: &Path,
    current_id: i64,
    next_id: i64,
) -> AnyhowResult<Vec<f32>> {
    drain_decoder(current_path)?;
    let mut transitioner = GaplessTransitioner::new();
    transitioner.start_playback(current_id);
    transitioner
        .prebuffer_next(current_id, next_id, next_path.to_path_buf())
        .context("Failed to pre-buffer next track")?;
    let mut decoder = transitioner.transition().context("Transition failed")?;
    let batch = decoder
        .decode_next()
        .context("Failed to decode next track")?;
    Ok(batch.samples)
}

/// Count leading silence samples (consecutive zeros) in a buffer.
#[must_use]
pub fn leading_silence(samples: &[f32]) -> usize {
    samples.iter().take_while(|&&s| s == 0.0).count()
}

/// Assert that the maximum silence found across transitions does not exceed
/// the threshold.
///
/// # Errors
///
/// Returns an error if `max_silence` exceeds `max_allowed`.
pub fn assert_max_silence(
    max_silence: usize,
    max_allowed: usize,
    context: &str,
) -> AnyhowResult<()> {
    if max_silence >= max_allowed {
        bail!("{context}: Max silence {max_silence} samples (exceeds {max_allowed})");
    }
    Ok(())
}
