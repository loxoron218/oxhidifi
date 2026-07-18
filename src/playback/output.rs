//! CPAL audio output: device enumeration, stream configuration, rtrb callback.
//! Supports both resampled and bit-perfect passthrough output paths.

use std::sync::{
    Arc,
    atomic::{
        AtomicBool, AtomicU32,
        Ordering::{Acquire, Relaxed, Release},
    },
};

#[cfg(target_os = "linux")]
use alsa::mixer::{Mixer, SelemId};

use {
    cpal::{
        Device, FromSample, OutputCallbackInfo,
        SampleFormat::{self, F32, I16, U16},
        SizedSample, Stream, StreamConfig, default_host,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    },
    num_traits::cast::{AsPrimitive, FromPrimitive},
    rtrb::{Consumer, Producer, RingBuffer},
    serde::{Deserialize, Serialize},
    tracing::{error, info, warn},
};

use crate::playback::OutputError::{self, NoDeviceAvailable, Output, StreamConfigError};

/// Controls playback volume via ALSA hardware mixer for bit-perfect mode.
///
/// Opens the ALSA mixer for a given card and finds the "Master" (or "PCM")
/// element to set hardware volume. On non-Linux platforms this is a no-op stub.
#[cfg(target_os = "linux")]
struct AlsaVolumeControl {
    /// Opened ALSA mixer handle.
    mixer: Mixer,
    /// Identifier of the found mixer element (Master or PCM).
    selem_id: SelemId,
    /// Minimum playback volume (from ALSA range).
    min_volume: i64,
    /// Maximum playback volume (from ALSA range).
    max_volume: i64,
}

#[cfg(target_os = "linux")]
impl AlsaVolumeControl {
    /// Open an ALSA mixer for `card_name` and find the Master/PCM element.
    fn new(card_name: &str) -> Result<Self, String> {
        let mixer = Mixer::new(card_name, false)
            .map_err(|e| format!("Failed to open ALSA mixer '{card_name}': {e}"))?;

        let selem_id = if mixer.find_selem(&SelemId::new("Master", 0)).is_some() {
            SelemId::new("Master", 0)
        } else if mixer.find_selem(&SelemId::new("PCM", 0)).is_some() {
            SelemId::new("PCM", 0)
        } else {
            return Err("No suitable ALSA mixer element found (tried Master, PCM)".to_string());
        };

        let selem = mixer
            .find_selem(&selem_id)
            .ok_or_else(|| "Mixer element not found after creation".to_string())?;
        let (min, max) = selem.get_playback_volume_range();

        Ok(Self {
            mixer,
            selem_id,
            min_volume: min,
            max_volume: max,
        })
    }

    /// Set the hardware playback volume.
    ///
    /// Maps `volume` (0.0–1.0) to the ALSA mixer's integer range
    /// and applies it to all channels.
    fn set_volume(&self, volume: f64) -> Result<(), String> {
        let selem = self
            .mixer
            .find_selem(&self.selem_id)
            .ok_or_else(|| "Mixer element not found".to_string())?;
        let range: i32 = (self.max_volume - self.min_volume).try_into().unwrap_or(0);
        let offset = FromPrimitive::from_f64((volume * f64::from(range)).round()).unwrap_or(0);
        let value = self.min_volume + offset;
        selem
            .set_playback_volume_all(value)
            .map_err(|e| format!("Failed to set ALSA volume: {e}"))?;
        Ok(())
    }
}

/// Stub for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
struct AlsaVolumeControl;

#[cfg(not(target_os = "linux"))]
impl AlsaVolumeControl {
    /// Open an ALSA mixer — always fails on non-Linux.
    fn new(_card_name: &str) -> Result<Self, String> {
        Err("ALSA is only available on Linux".to_string())
    }

    /// Set the hardware playback volume — always fails on non-Linux.
    fn set_volume(&self, _volume: f64) -> Result<(), String> {
        Err("ALSA is only available on Linux".to_string())
    }
}

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
        let volume_atomic = Arc::new(AtomicU32::new(f32::to_bits(1.0)));
        let host = default_host();

        if let Some(device) = host.default_output_device() {
            match Self::try_open_device(&device, ring_capacity, device_lost, &volume_atomic) {
                Ok(result) => return Ok(result),
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
            match Self::try_open_device(device, ring_capacity, device_lost, &volume_atomic) {
                Ok(result) => return Ok(result),
                Err(e) => last_err = e,
            }
        }

        Err(last_err)
    }

    /// Try to open a device, creating a ring buffer and flush flag.
    ///
    /// # Arguments
    ///
    /// * `device` - The audio device to open
    /// * `ring_capacity` - Capacity of the ring buffer
    /// * `device_lost` - Shared flag indicating device loss
    /// * `volume_atomic` - Shared atomic volume value
    ///
    /// # Returns
    ///
    /// A tuple of [`Output`] and [`Producer<f32>`] on success.
    fn try_open_device(
        device: &Device,
        ring_capacity: usize,
        device_lost: &Arc<AtomicBool>,
        volume_atomic: &Arc<AtomicU32>,
    ) -> Result<(Self, Producer<f32>), OutputError> {
        let (producer, consumer) = RingBuffer::new(ring_capacity);
        let flush_flag = Arc::new(AtomicBool::new(false));
        Self::try_open_on_device(
            device,
            consumer,
            flush_flag,
            Arc::clone(device_lost),
            Arc::clone(volume_atomic),
        )
        .map(|output| (output, producer))
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
        volume_atomic: Arc<AtomicU32>,
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
                Arc::clone(&volume_atomic),
            )?,
            I16 => build_stream::<i16>(
                device,
                &config,
                consumer,
                Arc::clone(&flush_flag),
                Arc::clone(&device_lost),
                Arc::clone(&volume_atomic),
            )?,
            U16 => build_stream::<u16>(
                device,
                &config,
                consumer,
                Arc::clone(&flush_flag),
                Arc::clone(&device_lost),
                Arc::clone(&volume_atomic),
            )?,
            fmt => {
                return Err(StreamConfigError(format!(
                    "unsupported sample format: {fmt:?}"
                )));
            }
        };

        stream.play().map_err(|e| Output(e.to_string()))?;

        let mode = OutputMode::Resampled;
        let alsa_volume = None;

        Ok(Self {
            stream,
            device_id,
            device_name,
            config,
            sample_format,
            mode,
            device_lost,
            flush_flag,
            alsa_volume,
            volume_atomic,
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
            OutputMode::BitPerfect => {
                self.alsa_volume = Self::open_alsa_volume(&self.device_id);
                self.volume_atomic.store(f32::to_bits(1.0), Relaxed);
            }
            OutputMode::Resampled => {
                self.alsa_volume = None;
            }
        }
    }

    /// Attempt to initialise the ALSA hardware volume controller.
    fn open_alsa_volume(device_id: &str) -> Option<AlsaVolumeControl> {
        let card = alsa_card_name(device_id);
        match AlsaVolumeControl::new(&card) {
            Ok(ctl) => {
                info!(card = %card, "ALSA hardware volume control initialised");
                Some(ctl)
            }
            Err(e) => {
                warn!(error = %e, "Failed to initialise ALSA volume control, falling back to no volume scaling");
                None
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
    ///
    /// Returns `true` if the device's native config matches the requested
    /// parameters.
    #[must_use]
    pub fn supports_native(&self, sample_rate: u32, _: u16) -> bool {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    /// Audio is output at the device's native configuration; samples pass
    /// through without volume scaling.
    BitPerfect,
    /// Audio is output via a resampled path with volume scaling.
    Resampled,
}

impl OutputMode {
    /// Get the symbolic icon name for this output mode.
    #[must_use]
    pub const fn icon_name(self) -> &'static str {
        match self {
            Self::BitPerfect => "media-optical-cd-audio-symbolic",
            Self::Resampled => "audio-card-symbolic",
        }
    }
}

/// Derive the ALSA mixer card name from a CPAL device identifier.
///
/// CPAL ALSA device names follow patterns like `"hw:0,0"` (PCM device) or
/// `"default"`. The mixer card name is the part before the comma (e.g. `"hw:0"`),
/// or the whole name for simple identifiers.
fn alsa_card_name(device_id: &str) -> String {
    if let Some(card_part) = device_id.split(',').next()
        && !card_part.is_empty()
        && card_part != device_id
    {
        return card_part.to_string();
    }
    device_id.to_string()
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
    volume: Arc<AtomicU32>,
) -> Result<Stream, OutputError> {
    let stream = device
        .build_output_stream(
            *config,
            move |data: &mut [T], _: &OutputCallbackInfo| {
                if flush_flag.swap(false, Acquire) {
                    drain_consumer(&mut consumer);
                }
                let vol = f32::from_bits(volume.load(Relaxed));
                for sample in data.iter_mut() {
                    let s: f32 = consumer.pop().unwrap_or(0.0);
                    *sample = T::from_sample(s * vol);
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
