//! Shared test infrastructure: fixture helpers and mock utilities.

use std::{
    fs::File,
    io::{Result, Write},
    path::Path,
};

use anyhow::{Context, Result as AnyhowResult};

use oxhidifi::playback::{decoder::Decoder, gapless::GaplessTransitioner, write_wav_header};

/// Write a minimal WAV file with the given sample rate, channel count, and
/// 16-bit samples.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written to.
pub fn write_wav(path: &Path, channels: u16, sample_rate: u32, samples: &[i16]) -> Result<()> {
    let mut f = File::create(path)?;
    let data_size = u32::try_from(samples.len()).unwrap_or(0).saturating_mul(2);
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
    _ = transitioner
        .prebuffer_next(current_id, next_id, next_path.to_path_buf())
        .context("Failed to pre-buffer next track")?;
    let mut decoder = transitioner.transition().context("Transition failed")?;
    let batch = decoder
        .decode_next()
        .context("Failed to decode next track")?;
    Ok(batch.samples.to_vec())
}

/// Count leading silence samples (consecutive zeros) in a buffer.
#[must_use]
pub fn leading_silence(samples: &[f32]) -> usize {
    samples.iter().take_while(|&&s| s == 0.0).count()
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Context, Result, ensure},
        tempfile::tempdir,
    };

    use crate::synth_wav::{drain_decoder, leading_silence, transition_and_decode, write_wav};

    #[test]
    fn helpers_roundtrip() -> Result<()> {
        let dir = tempdir().context("failed to create temp dir")?;
        let first_path = dir.path().join("first.wav");
        let next_path = dir.path().join("next.wav");
        write_wav(&first_path, 1, 44100, &[0, 0, 1000, -1000])
            .context("failed to write first wav")?;
        write_wav(&next_path, 1, 44100, &[500, -500, 0, 0]).context("failed to write next wav")?;
        drain_decoder(&first_path).context("failed to drain first wav")?;
        let samples = transition_and_decode(&first_path, &next_path, 1, 2)
            .context("failed transition decode")?;
        ensure!(!samples.is_empty(), "transition should yield samples");
        ensure!(
            leading_silence(&[0.0, 0.0, 0.5]) == 2,
            "leading silence should count zeros"
        );
        Ok(())
    }
}
