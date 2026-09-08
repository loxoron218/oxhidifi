//! CPAL audio output: stream configuration and rtrb callback.
//! Supports both resampled and bit-perfect passthrough output paths.

pub mod opener;

use std::{
    fmt::{Debug, Formatter, Result as FmtResult},
    sync::{
        Arc,
        atomic::{
            AtomicBool, AtomicU32,
            Ordering::{Relaxed, Release},
        },
    },
};

use {
    cpal::{SampleFormat, Stream, StreamConfig, traits::StreamTrait},
    num_traits::cast::AsPrimitive,
    tracing::{error, warn},
};

use crate::playback::{
    alsa_mixer::AlsaVolumeControl,
    devices::OutputMode::{self, BitPerfect, Resampled},
    native_support::{open_alsa_volume, supports_native},
};

/// Holds a CPAL output stream and its configuration.
///
/// Dropping this struct stops playback and releases the audio device.
pub struct AudioOutput {
    /// CPAL output stream (kept alive until dropped).
    stream: Stream,
    /// Stable device identifier for persisting device selection across restarts.
    device_id: String,
    /// Human-readable device name for display purposes.
    device_name: String,
    /// Stream configuration used for playback.
    config: StreamConfig,
    /// Sample format of the output stream.
    sample_format: SampleFormat,
    /// Whether the current output path is bit-perfect.
    mode: OutputMode,
    /// Shared flag set by the error callback when the device is lost.
    device_lost: Arc<AtomicBool>,
    /// Signalled by `flush()` to tell the audio callback to drain stale data
    /// after a seek. Transitions: `true` on seek request, `false` after drain.
    flush_flag: Arc<AtomicBool>,
    /// ALSA hardware volume control, present in bit-perfect mode.
    alsa_volume: Option<AlsaVolumeControl>,
    /// Lock-free volume scalar read by the audio callback on every frame.
    /// Stored as `f32::to_bits()` for lock-free atomic access.
    /// Initialised to 1.0 (no scaling); updated by `set_volume_atomic`.
    pub volume_atomic: Arc<AtomicU32>,
}

impl AudioOutput {
    /// Returns the stable device identifier for persisting device selection.
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Returns the human-readable name of the output device.
    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Returns the stream sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    /// Returns the number of output channels.
    #[must_use]
    pub const fn channels(&self) -> u16 {
        self.config.channels
    }

    /// Returns the sample format of the output stream.
    #[must_use]
    pub const fn sample_format(&self) -> SampleFormat {
        self.sample_format
    }

    /// Returns the current output mode.
    #[must_use]
    pub const fn mode(&self) -> OutputMode {
        self.mode
    }

    /// Set the output mode and initialise or tear down the ALSA mixer.
    ///
    /// When switching to bit-perfect mode the volume atomic is reset to 1.0
    /// (the audio callback passes samples through unscaled). The caller must
    /// sync the current volume to the ALSA mixer afterwards.
    pub fn set_mode(&mut self, mode: OutputMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        match mode {
            BitPerfect => {
                self.alsa_volume = open_alsa_volume(&self.device_id);
                self.volume_atomic.store(f32::to_bits(1.0), Relaxed);
            }
            Resampled => {
                self.alsa_volume = None;
            }
        }
    }

    /// Set hardware volume via ALSA mixer.
    ///
    /// No-op when not in bit-perfect mode or when the ALSA mixer is
    /// unavailable.
    pub fn set_hardware_volume(&self, volume: f64) {
        let Some(ctl) = self.alsa_volume.as_ref() else {
            return;
        };
        if let Err(e) = ctl.set_volume(volume) {
            warn!(error = %e, "Hardware volume control failed");
        }
    }

    /// Set the lock-free volume scalar read by the audio callback.
    ///
    /// The value is stored as `f32::to_bits()` so the audio callback
    /// can read it with a single relaxed atomic load. Changes take
    /// effect on the very next callback invocation (~10 ms latency).
    pub fn set_volume_atomic(&self, volume: f64) {
        self.volume_atomic
            .store(f32::to_bits(volume.as_()), Relaxed);
    }

    /// Check whether the device supports bit-perfect playback at the
    /// given sample rate and bit depth.
    #[must_use]
    pub const fn supports_native(&self, sample_rate: u32, bit_depth: u16) -> bool {
        supports_native(
            sample_rate,
            bit_depth,
            self.config.sample_rate,
            self.sample_format,
        )
    }

    /// Query whether a given sample rate is supported by the current device.
    #[must_use]
    pub const fn supports_sample_rate(&self, sample_rate: u32) -> bool {
        sample_rate == self.config.sample_rate
    }

    /// Whether the device has been detected as lost.
    #[must_use]
    pub fn is_device_lost(&self) -> bool {
        self.device_lost.load(Relaxed)
    }

    /// Pause the audio output stream instantly.
    pub fn pause(&self) {
        if let Err(e) = self.stream.pause() {
            error!(error = %e, "Failed to pause output stream");
        }
    }

    /// Resume the audio output stream.
    pub fn play(&self) {
        if let Err(e) = self.stream.play() {
            error!(error = %e, "Failed to play output stream");
        }
    }

    /// Signal the audio callback to discard all buffered audio data.
    ///
    /// The next callback invocation will drain the ring buffer, preventing
    /// stale pre-seek audio from reaching the output. The drain happens
    /// on the audio thread to avoid mutex contention.
    pub fn flush(&self) {
        self.flush_flag.store(true, Release);
    }
}

impl Debug for AudioOutput {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("AudioOutput")
            .field("device_id", &self.device_id)
            .field("device_name", &self.device_name)
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}
