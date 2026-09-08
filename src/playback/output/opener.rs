//! Device establishment for CPAL audio output.
//!
//! Opens the default output device with fallback to prioritized devices,
//! creates the ring buffer and stream, and selects bit-perfect or resampled
//! mode. Lives in a child module so `output.rs` stays under the file-size
//! limit; as a child it sees the parent's private fields.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering::Relaxed},
};

use {
    cpal::{
        Device,
        SampleFormat::{F32, I16, I32, U16, U32},
        default_host,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    },
    rtrb::{Consumer, Producer, RingBuffer},
    tracing::warn,
};

use crate::playback::{
    OutputError::{self, NoDeviceAvailable, Output, StreamConfigError},
    devices::{
        OutputMode::{self, BitPerfect, Resampled},
        prioritize_devices,
    },
    native_support::open_alsa_volume,
    output::AudioOutput,
    stream::build_stream,
};

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
        desired_mode: OutputMode,
    ) -> Result<(Self, Producer<f32>), OutputError> {
        let volume_atomic = Arc::new(AtomicU32::new(f32::to_bits(1.0)));
        let host = default_host();

        if let Some(device) = host.default_output_device() {
            match Self::try_open_device(
                &device,
                ring_capacity,
                device_lost,
                &volume_atomic,
                desired_mode,
            ) {
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
            match Self::try_open_device(
                device,
                ring_capacity,
                device_lost,
                &volume_atomic,
                desired_mode,
            ) {
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
        desired_mode: OutputMode,
    ) -> Result<(Self, Producer<f32>), OutputError> {
        let (producer, consumer) = RingBuffer::new(ring_capacity);
        let flush_flag = Arc::new(AtomicBool::new(false));
        Self::try_open_on_device(
            device,
            consumer,
            flush_flag,
            Arc::clone(device_lost),
            Arc::clone(volume_atomic),
            desired_mode,
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
        desired_mode: OutputMode,
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
            I32 => build_stream::<i32>(
                device,
                &config,
                consumer,
                Arc::clone(&flush_flag),
                Arc::clone(&device_lost),
                Arc::clone(&volume_atomic),
            )?,
            U32 => build_stream::<u32>(
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

        let (mode, alsa_volume) = match desired_mode {
            BitPerfect => {
                let alsa = open_alsa_volume(
                    &device
                        .id()
                        .map_or_else(|_| String::new(), |id| id.to_string()),
                );
                volume_atomic.store(f32::to_bits(1.0), Relaxed);
                (BitPerfect, alsa)
            }
            Resampled => (Resampled, None),
        };

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
}
