//! CPAL audio output: device enumeration, stream configuration, rtrb callback.

use {
    cpal::{
        Device, FromSample, OutputCallbackInfo,
        SampleFormat::{self, F32, I16, U16},
        SizedSample, Stream, StreamConfig, default_host,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    },
    rtrb::Consumer,
    tracing::error,
};

use crate::playback::OutputError::{self, NoDeviceAvailable, Output, StreamConfigError};

/// Holds a CPAL output stream and its configuration.
///
/// Dropping this struct stops playback and releases the audio device.
pub struct AudioOutput {
    /// CPAL output stream (kept alive until dropped).
    _stream: Stream,
    /// Name of the output device.
    device_name: String,
    /// Stream configuration used for playback.
    config: StreamConfig,
    /// Sample format of the output stream.
    sample_format: SampleFormat,
}

impl AudioOutput {
    /// Create a new audio output on the default device.
    ///
    /// The `consumer` reads interleaved f32 PCM samples from the ring buffer.
    /// The cpal callback pulls samples and silences when the buffer runs dry.
    ///
    /// # Errors
    ///
    /// Returns [`OutputError`] if no device is available or stream creation
    /// fails.
    pub fn open(consumer: Consumer<f32>) -> Result<Self, OutputError> {
        let host = default_host();
        let device = host.default_output_device().ok_or(NoDeviceAvailable)?;

        let device_name = device
            .description()
            .map_or_else(|_| "Unknown Device".into(), |d| d.to_string());

        let supported = device
            .default_output_config()
            .map_err(|e| StreamConfigError(e.to_string()))?;

        let sample_format = supported.sample_format();
        let config = supported.config();

        let stream = match sample_format {
            F32 => build_stream::<f32>(&device, &config, consumer)?,
            I16 => build_stream::<i16>(&device, &config, consumer)?,
            U16 => build_stream::<u16>(&device, &config, consumer)?,
            fmt => {
                return Err(StreamConfigError(format!(
                    "unsupported sample format: {fmt:?}"
                )));
            }
        };

        stream.play().map_err(|e| Output(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            device_name,
            config,
            sample_format,
        })
    }

    /// Returns the name of the output device.
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

/// List available audio output device names.
///
/// # Errors
///
/// Returns [`OutputError`] if device enumeration fails.
pub fn list_output_devices() -> Result<Vec<String>, OutputError> {
    let host = default_host();
    let mut names = Vec::new();
    for device in host.output_devices().map_err(|e| Output(e.to_string()))? {
        if let Ok(desc) = device.description() {
            names.push(desc.to_string());
        }
    }
    Ok(names)
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
}
