//! CPAL audio output: stream configuration and rtrb callback.
//! Supports both resampled and bit-perfect passthrough output paths.

use std::sync::{
    Arc,
    atomic::{
        AtomicBool, AtomicU32,
        Ordering::{Relaxed, Release},
    },
};

use {
    cpal::{
        Device,
        SampleFormat::{self, F32, I16, U16},
        Stream, StreamConfig, default_host,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    },
    num_traits::cast::AsPrimitive,
    rtrb::{Consumer, Producer, RingBuffer},
    tracing::{error, info, warn},
};

use crate::playback::{
    OutputError::{self, NoDeviceAvailable, Output, StreamConfigError},
    alsa_volume::AlsaVolumeControl,
    devices::{OutputMode, alsa_card_name, prioritize_devices},
    stream::build_stream,
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
    ///
    /// # Errors
    ///
    /// Returns [`OutputError`] if the device cannot be opened.
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
