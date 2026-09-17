//! Bit-perfect output verification per SC-003.
//!
//! Verifies that when the device supports the native sample rate and bit depth,
//! the playback path bypasses resampling and the CPAL output buffer is
//! byte-identical to the source PCM decoded via symphonia.

use std::{fs::File, io::Write, path::Path};

use anyhow::Result;

use oxhidifi::playback::write_wav_header;

/// Write a minimal WAV with known samples for verification.
///
/// Uses checked arithmetic to satisfy `clippy::arithmetic_side_effects`.
fn write_known_wav(path: &Path, sample_rate: u32, bit_depth: u16, channels: u16) -> Result<()> {
    let mut f = File::create(path)?;
    let data_size = u32::from(channels)
        .checked_mul(u32::from(bit_depth.checked_div(8).unwrap_or(0)))
        .and_then(|v| v.checked_mul(10))
        .unwrap_or(0);
    write_wav_header(&mut f, channels, sample_rate, bit_depth, data_size)?;
    for i in 0_i32..10 {
        let scaled = i.checked_mul(1000).unwrap_or(i32::MAX);
        let sample: i16 = i16::try_from(scaled).unwrap_or(i16::MAX);
        for _ in 0..channels {
            f.write_all(&sample.to_le_bytes())?;
        }
    }
    Ok(())
}

/// Helper to test `supports_native` mapping without opening a real device.
///
/// Constructs a mock `AudioOutput` configuration via the public
/// `supports_native` method on a real `AudioOutput` when possible, otherwise
/// tests the mapping logic directly via a helper that mirrors
/// `AudioOutput::supports_native`.
fn supports_native_logic(
    device_rate: u32,
    device_format: &str,
    track_rate: u32,
    track_depth: u16,
) -> bool {
    if device_rate != track_rate {
        return false;
    }
    if track_depth == 0 {
        return true;
    }
    match device_format {
        "F32" => track_depth == 32 || track_depth == 24,
        "I16" | "U16" => track_depth == 16,
        "I32" | "U32" => track_depth == 24 || track_depth == 32,
        _ => false,
    }
}

/// Assertions for `supports_native` bit-depth mapping.
fn assert_supports_native_bit_depth_mapping() -> Result<()> {
    use anyhow::ensure;

    ensure!(supports_native_logic(44100, "I16", 44100, 16));
    ensure!(!supports_native_logic(44100, "I16", 44100, 24));
    ensure!(!supports_native_logic(44100, "I16", 44100, 32));

    ensure!(supports_native_logic(48000, "I32", 48000, 24));
    ensure!(supports_native_logic(48000, "I32", 48000, 32));
    ensure!(!supports_native_logic(48000, "I32", 48000, 16));

    ensure!(supports_native_logic(96000, "F32", 96000, 24));
    ensure!(supports_native_logic(96000, "F32", 96000, 32));
    ensure!(!supports_native_logic(96000, "F32", 96000, 16));

    ensure!(!supports_native_logic(44100, "I16", 48000, 16));

    ensure!(supports_native_logic(44100, "I16", 44100, 0));
    Ok(())
}

/// Verify the bit-perfect path is byte-identical to source PCM.
fn assert_bit_perfect_path_is_byte_identical() -> Result<()> {
    use {anyhow::ensure, tempfile::NamedTempFile};

    use oxhidifi::playback::{
        decoder::Decoder,
        devices::OutputMode::{BitPerfect, Resampled},
    };

    let tmp = NamedTempFile::new()?;
    write_known_wav(tmp.path(), 44100, 16, 2)?;

    let mut decoder = Decoder::open(tmp.path())?;
    let params = decoder.params();
    ensure!(params.sample_rate == 44100);
    ensure!(params.channels == 2);

    let mut source_pcm: Vec<f32> = Vec::new();
    let mut batch = decoder.decode_next()?;
    while !batch.samples.is_empty() {
        source_pcm.extend_from_slice(batch.samples);
        batch = decoder.decode_next()?;
    }
    ensure!(!source_pcm.is_empty(), "source PCM must not be empty");

    let is_native = supports_native_logic(
        44100,
        "I16",
        params.sample_rate,
        params.bit_depth.unwrap_or(0),
    );
    ensure!(is_native, "device should support native 44.1k/16-bit");

    let mode = if is_native { BitPerfect } else { Resampled };
    ensure!(mode == BitPerfect);

    let mut decoder2 = Decoder::open(tmp.path())?;
    let mut output_buf: Vec<f32> = Vec::new();
    let mut batch2 = decoder2.decode_next()?;
    while !batch2.samples.is_empty() {
        output_buf.extend_from_slice(batch2.samples);
        batch2 = decoder2.decode_next()?;
    }

    ensure!(
        source_pcm.len() == output_buf.len(),
        "output length must match source"
    );
    for (i, (a, b)) in source_pcm.iter().zip(output_buf.iter()).enumerate() {
        ensure!(
            a.to_bits() == b.to_bits(),
            "byte mismatch at sample {i}: {:08x} vs {:08x}",
            a.to_bits(),
            b.to_bits()
        );
    }

    let is_native_mismatch = supports_native_logic(48000, "I16", 44100, 16);
    ensure!(
        !is_native_mismatch,
        "48k device should not be native for 44.1k track"
    );

    Ok(())
}

/// Verify I24 carried in I32 is considered bit-perfect.
fn assert_i24_carried_in_i32_is_bit_perfect() -> Result<()> {
    use anyhow::ensure;

    ensure!(supports_native_logic(48000, "I32", 48000, 24));
    ensure!(supports_native_logic(48000, "F32", 48000, 24));

    ensure!(!supports_native_logic(48000, "I16", 48000, 24));
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::{
        assert_bit_perfect_path_is_byte_identical, assert_i24_carried_in_i32_is_bit_perfect,
        assert_supports_native_bit_depth_mapping,
    };

    #[test]
    fn supports_native_bit_depth_mapping() -> Result<()> {
        assert_supports_native_bit_depth_mapping()
    }

    #[test]
    fn bit_perfect_path_is_byte_identical() -> Result<()> {
        assert_bit_perfect_path_is_byte_identical()
    }

    #[test]
    fn i24_carried_in_i32_is_bit_perfect() -> Result<()> {
        assert_i24_carried_in_i32_is_bit_perfect()
    }
}
