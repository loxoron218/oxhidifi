//! CPAL audio output: device enumeration, stream configuration, rtrb callback.

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
    _stream: Stream,
    /// Stable device identifier for persisting device selection across restarts.
    device_id: String,
    /// Human-readable device name for display purposes.
    device_name: String,
    /// Stream configuration used for playback.
    config: StreamConfig,
    /// Sample format of the output stream.
    sample_format: SampleFormat,
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
    pub fn open(ring_capacity: usize) -> Result<(Self, Producer<f32>), OutputError> {
        let host = default_host();

        if let Some(device) = host.default_output_device() {
            let (producer, consumer) = RingBuffer::new(ring_capacity);
            match Self::try_open_on_device(&device, consumer) {
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
            match Self::try_open_on_device(device, consumer) {
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
    fn try_open_on_device(device: &Device, consumer: Consumer<f32>) -> Result<Self, OutputError> {
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
            F32 => build_stream::<f32>(device, &config, consumer)?,
            I16 => build_stream::<i16>(device, &config, consumer)?,
            U16 => build_stream::<u16>(device, &config, consumer)?,
            fmt => {
                return Err(StreamConfigError(format!(
                    "unsupported sample format: {fmt:?}"
                )));
            }
        };

        stream.play().map_err(|e| Output(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            device_id,
            device_name,
            config,
            sample_format,
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
}

/// Describes an available audio output device.
pub struct DeviceInfo {
    /// Stable device identifier for persisting selection across restarts.
    pub id: String,
    /// Human-readable device name for display.
    pub name: String,
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
) -> Result<Stream, OutputError> {
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &OutputCallbackInfo| {
                for sample in data.iter_mut() {
                    let s: f32 = consumer.pop().unwrap_or(0.0);
                    *sample = T::from_sample(s);
                }
            },
            |err| {
                error!(error = %err, "Audio output stream error");
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

#[cfg(test)]
mod tests {
    use crate::playback::output::list_output_devices;

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
}
