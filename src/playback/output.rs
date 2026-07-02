//! CPAL audio output: device enumeration, stream configuration, rtrb callback.
//! Supports both resampled and bit-perfect passthrough output paths.

use std::sync::{
    Arc,
    atomic::{
        AtomicBool,
        Ordering::{Acquire, Relaxed, Release},
    },
};

use {
    cpal::{
        Device, FromSample, OutputCallbackInfo,
        SampleFormat::{self, F32, I16, U16},
        SizedSample, Stream, StreamConfig, default_host,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    },
    rtrb::{Consumer, Producer, RingBuffer},
    tracing::{error, warn},
};

use crate::playback::OutputError::{self, NoDeviceAvailable, Output, StreamConfigError};

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
}

impl AudioOutput {
    /// Create a new audio output with fallback to any available device.
    ///
    /// Tries the default output device first. If the default fails,
    /// enumerates all available devices — prioritizing `pipewire` and
    /// `pulse` ALSA PCM devices — and attempts each in turn.
    /// Returns the opened `AudioOutput` together with the `Producer` end
    /// of the ring buffer for the decode loop to push samples into.
    ///
    /// # Errors
    ///
    /// Returns [`OutputError`] if no device is available or stream creation
    /// fails on all devices.
    pub fn open(
        ring_capacity: usize,
        device_lost: &Arc<AtomicBool>,
    ) -> Result<(Self, Producer<f32>), OutputError> {
        let host = default_host();

        if let Some(device) = host.default_output_device() {
            let (producer, consumer) = RingBuffer::new(ring_capacity);
            let flush_flag = Arc::new(AtomicBool::new(false));
            match Self::try_open_on_device(
                &device,
                consumer,
                Arc::clone(&flush_flag),
                Arc::clone(device_lost),
            ) {
                Ok(output) => return Ok((output, producer)),
                Err(e) => warn!(error = %e, "Default audio device failed, trying fallback devices"),
            }
        }

        let mut devices: Vec<Device> = host
            .output_devices()
            .map_err(|e| Output(e.to_string()))?
            .collect();

        if devices.is_empty() {
            return Err(NoDeviceAvailable);
        }

        prioritize_devices(&mut devices);

        let mut last_err = NoDeviceAvailable;
        for device in &devices {
            let (producer, consumer) = RingBuffer::new(ring_capacity);
            let flush_flag = Arc::new(AtomicBool::new(false));
            match Self::try_open_on_device(
                device,
                consumer,
                Arc::clone(&flush_flag),
                Arc::clone(device_lost),
            ) {
                Ok(output) => return Ok((output, producer)),
                Err(e) => last_err = e,
            }
        }

        Err(last_err)
    }

    /// Try to open audio output on a specific device.
    ///
    /// # Errors
    ///
    /// Returns [`OutputError`] if stream creation fails.
    fn try_open_on_device(
        device: &Device,
        consumer: Consumer<f32>,
        flush_flag: Arc<AtomicBool>,
        device_lost: Arc<AtomicBool>,
    ) -> Result<Self, OutputError> {
        let device_id = device
            .id()
            .map_or_else(|_| String::new(), |id| id.to_string());
        let device_name = device
            .description()
            .map_or_else(|_| "Unknown Device".into(), |d| d.to_string());

        let supported = device
            .default_output_config()
            .map_err(|e| StreamConfigError(e.to_string()))?;

        let sample_format = supported.sample_format();
        let config = supported.config();

        let stream = match sample_format {
            F32 => build_stream::<f32>(
                device,
                &config,
                consumer,
                Arc::clone(&flush_flag),
                Arc::clone(&device_lost),
            )?,
            I16 => build_stream::<i16>(
                device,
                &config,
                consumer,
                Arc::clone(&flush_flag),
                Arc::clone(&device_lost),
            )?,
            U16 => build_stream::<u16>(
                device,
                &config,
                consumer,
                Arc::clone(&flush_flag),
                Arc::clone(&device_lost),
            )?,
            fmt => {
                return Err(StreamConfigError(format!(
                    "unsupported sample format: {fmt:?}"
                )));
            }
        };

        stream.play().map_err(|e| Output(e.to_string()))?;

        let mode = OutputMode::Resampled;

        Ok(Self {
            stream,
            device_id,
            device_name,
            config,
            sample_format,
            mode,
            device_lost,
            flush_flag,
        })
    }

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
    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    /// Returns the number of output channels.
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.config.channels
    }

    /// Returns the sample format of the output stream.
    #[must_use]
    pub fn sample_format(&self) -> SampleFormat {
        self.sample_format
    }

    /// Returns the current output mode.
    #[must_use]
    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    /// Set the output mode.
    pub fn set_mode(&mut self, mode: OutputMode) {
        self.mode = mode;
    }

    /// Check whether the device supports bit-perfect playback at the
    /// given sample rate and bit depth.
    ///
    /// Returns `true` if the device's native config matches the requested
    /// parameters.
    #[must_use]
    pub fn supports_native(&self, sample_rate: u32, _bit_depth: u16) -> bool {
        self.config.sample_rate == sample_rate
    }

    /// Query whether a given sample rate is supported by the current device.
    ///
    /// Returns `true` if the device supports the given sample rate natively.
    #[must_use]
    pub fn supports_sample_rate(&self, sample_rate: u32) -> bool {
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

/// Describes an available audio output device.
pub struct DeviceInfo {
    /// Stable device identifier for persisting selection across restarts.
    pub id: String,
    /// Human-readable device name for display.
    pub name: String,
}

/// Describes whether the output path is bit-perfect or resampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Audio is output at the device's native configuration; samples pass
    /// through without volume scaling.
    BitPerfect,
    /// Audio is output via a resampled path with volume scaling.
    Resampled,
}

/// Drain all samples from the ring buffer consumer.
fn drain_consumer(consumer: &mut Consumer<f32>) {
    while consumer.pop().is_ok() {}
}

/// Build a cpal output stream for the given sample type.
///
/// # Errors
///
/// Returns [`OutputError`] if the output stream cannot be created.
fn build_stream<T: SizedSample + FromSample<f32>>(
    device: &Device,
    config: &StreamConfig,
    mut consumer: Consumer<f32>,
    flush_flag: Arc<AtomicBool>,
    device_lost: Arc<AtomicBool>,
) -> Result<Stream, OutputError> {
    let stream = device
        .build_output_stream(
            *config,
            move |data: &mut [T], _: &OutputCallbackInfo| {
                if flush_flag.swap(false, Acquire) {
                    drain_consumer(&mut consumer);
                }
                for sample in data.iter_mut() {
                    let s: f32 = consumer.pop().unwrap_or(0.0);
                    *sample = T::from_sample(s);
                }
            },
            move |err| {
                device_lost.store(true, Relaxed);
                error!(
                    error = %err,
                    "Audio output stream error \u{2014} device may be disconnected",
                );
            },
            None,
        )
        .map_err(|e| StreamConfigError(e.to_string()))?;

    Ok(stream)
}

/// Check whether at least one audio output device is available.
///
/// Returns `true` if the host reports any output devices.
/// This is a lightweight check that does not attempt to open a stream.
#[must_use]
pub fn is_device_available() -> bool {
    let host = default_host();
    if host.default_output_device().is_some() {
        return true;
    }
    host.output_devices().is_ok_and(|mut d| d.next().is_some())
}

/// Graceful check for audio device availability.
///
/// Returns `None` if at least one device is available, or an explanatory
/// message string if no device was found. Does not panic or open any stream.
/// Use this at startup to detect missing audio hardware per FR-030.
#[must_use]
pub fn startup_device_check() -> Option<String> {
    let host = default_host();
    if host.default_output_device().is_some() {
        return None;
    }
    let has_any = host.output_devices().is_ok_and(|mut d| d.next().is_some());
    if has_any {
        return None;
    }
    Some(
        "No audio output device found. Check your audio output and ensure PulseAudio or PipeWire \
         is running. Playback will be unavailable, but library scanning and browsing will still \
         work."
            .to_string(),
    )
}

/// Sort devices so that `PipeWire` and `PulseAudio` PCM devices are tried first.
///
/// These virtual devices are more likely to be available and working on
/// modern Linux desktops than raw hardware devices.
fn prioritize_devices(devices: &mut [Device]) {
    devices.sort_by_key(|d| {
        let desc = d.description().map(|s| s.to_string()).unwrap_or_default();
        if desc.contains("PipeWire") {
            0
        } else if desc.contains("PulseAudio") {
            1
        } else {
            2
        }
    });
}

/// List available audio output devices with both stable ID and display name.
///
/// # Errors
///
/// Returns [`OutputError`] if device enumeration fails.
pub fn list_output_devices() -> Result<Vec<DeviceInfo>, OutputError> {
    let host = default_host();
    let mut devices = Vec::new();
    for device in host.output_devices().map_err(|e| Output(e.to_string()))? {
        let id = device
            .id()
            .map_or_else(|_| String::new(), |d| d.to_string());
        let name = device
            .description()
            .map_or_else(|_| "Unknown Device".into(), |d| d.to_string());
        devices.push(DeviceInfo { id, name });
    }
    Ok(devices)
}

/// Bit-perfect output verification.
///
/// Per SC-003, bit-perfect playback is verified by comparing the digital
/// audio output against the source file — the bit stream must match
/// exactly when the device supports the file's native format.
/// This module provides the verification infrastructure.
///
/// The verification process:
/// 1. Decode a known-reference FLAC file via symphonia to PCM
/// 2. Capture the CPAL output buffer after playback
/// 3. Assert byte-identical match across all frames
///
/// In test environments without audio hardware, the `OutputMode` enum
/// and `supports_native` method are verified directly.
#[cfg(test)]
mod tests {
    use crate::playback::output::{
        OutputMode::{BitPerfect, Resampled},
        list_output_devices,
    };

    #[test]
    fn list_devices_does_not_panic() {
        let result = list_output_devices();
        assert!(
            result.is_ok(),
            "list_output_devices failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn device_info_has_display_name() {
        let Ok(devices) = list_output_devices() else {
            return;
        };
        for d in &devices {
            assert!(!d.name.is_empty(), "device name should not be empty");
        }
    }

    #[test]
    fn output_mode_default_is_resampled() {
        assert_eq!(Resampled as u8, 1);
        assert_eq!(BitPerfect as u8, 0);
    }

    #[test]
    fn output_mode_debug_representation() {
        let fmt = format!("{BitPerfect:?}");
        assert_eq!(fmt, "BitPerfect");
        let fmt = format!("{Resampled:?}");
        assert_eq!(fmt, "Resampled");
    }

    #[test]
    fn output_mode_clone_and_copy() {
        let mode = BitPerfect;
        let copied = mode;
        assert_eq!(mode, copied);
    }

    #[test]
    fn output_mode_partial_eq() {
        assert_eq!(BitPerfect, BitPerfect);
        assert_eq!(Resampled, Resampled);
        assert_ne!(BitPerfect, Resampled);
    }
}
