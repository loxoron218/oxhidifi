//! Shared fixtures for the `signal_path` test target.
//!
//! Deterministic `SnapshotInput` builders owned by the target rooted at
//! `tests/signal_inspector.rs` and shared with its `signal_invariants` and
//! `signal_us4` submodules. Mirrors the `tests/abx` sharing pattern: public
//! fixtures with docs in a public submodule, explicit `crate::` imports.

use anyhow::{Result, bail};

use oxhidifi::{
    playback::{
        decoder::AudioParams,
        devices::OutputMode::BitPerfect,
        signal_path::{DeviceRole::Output, RenderingDevice, SnapshotInput},
        state::{MuteState::Unmuted, PlaybackStatus::Playing},
    },
    storage::catalog::TrackAudio,
};

/// Bit-perfect baseline input: unity volume, direct mode, matching device.
#[must_use]
pub fn bit_perfect_input() -> SnapshotInput {
    SnapshotInput {
        device_id: String::from("hw:1"),
        device_name: String::from("Lab DAC"),
        device_sample_rate: 48000,
        device_channels: 2,
        device_lost: false,
        generation: 1,
        track_id: Some(12),
        track_audio: None,
        decoder_params: None,
        resampler_in_rate: None,
        resampler_out_rate: None,
        resampler_channels: None,
        volume: 1.0,
        muted: Unmuted,
        output_mode: BitPerfect,
        status: Playing,
        zone_name: String::from("Lab DAC"),
        auth: None,
        sampled_at_wall: 0,
        decoded_frames: 0,
        resampled_frames: 0,
    }
}

/// Single rendering-device fixture for the Lab DAC output.
#[must_use]
pub fn make_device() -> RenderingDevice {
    RenderingDevice {
        display_name: String::from("Lab DAC"),
        role: Output,
        brand_visual: String::from("audio-card-symbolic"),
        illustration: String::from("audio-speakers-symbolic"),
        manual_url: None,
    }
}

/// Catalog audio fixture for one lossless FLAC track.
#[must_use]
pub fn flac_audio(sample_rate: i32, bit_depth: Option<i32>) -> TrackAudio {
    TrackAudio {
        file_path: format!("/music/track-{sample_rate}.flac"),
        content_hash: None,
        format: String::from("FLAC"),
        sample_rate,
        bit_depth,
        channels: 2,
        codec: String::from("FLAC"),
        lossless: true,
        bitrate: None,
        album_id: None,
        artist_id: None,
        file_size: 1024,
        last_modified: String::from("2026-01-01T00:00:00Z"),
    }
}

/// Playing-track input with catalog audio plus live decoder facts.
#[must_use]
pub fn playing_input(
    track_id: i64,
    generation: u64,
    sample_rate: i32,
    bit_depth: Option<i32>,
) -> SnapshotInput {
    let mut input = bit_perfect_input();
    input.track_id = Some(track_id);
    input.generation = generation;
    input.track_audio = Some(flac_audio(sample_rate, bit_depth));
    input.decoder_params = Some(AudioParams {
        sample_rate: u32::try_from(sample_rate).unwrap_or(0),
        channels: 2,
        duration_seconds: 180.0,
        bit_depth: bit_depth.map(|bits| u16::try_from(bits).unwrap_or(0)),
    });
    input
}

/// DSD track input under volume scaling for explicit conversion coverage.
///
/// # Errors
///
/// Returns an error when the catalog audio fixture is unexpectedly missing.
pub fn dsd_input(track_id: i64, generation: u64) -> Result<SnapshotInput> {
    let mut input = playing_input(track_id, generation, 2_822_400, None);
    input.volume = 0.5;
    let Some(audio) = input.track_audio.as_mut() else {
        bail!("DSD fixture needs audio")
    };
    audio.codec = String::from("DSF");
    audio.format = String::from("DSF");
    Ok(input)
}
