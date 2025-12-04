//! Audio output management using the `cpal` crate.
//!
//! This module handles audio device enumeration, stream creation, and
//! bit-perfect playback configuration for high-fidelity audio output.

use std::time::Duration;

use {
    cpal::{
        BufferSize::Default as CpalDefault,
        BuildStreamError, ChannelCount, Device, Host, OutputCallbackInfo, PlayStreamError, Sample,
        SampleFormat::{self, F32, I16, U16},
        SampleRate, SizedSample, Stream, StreamConfig, default_host,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    },
    num_traits::{self, cast},
    rtrb::{Consumer, PopError::Empty},
    rubato::FftFixedIn,
    thiserror::Error,
};

use crate::audio::decoder::AudioFormat;

/// Error type for audio output operations.
#[derive(Error, Debug)]
pub enum OutputError {
    /// CPAL host or device error.
    #[error("Audio output error: {0}")]
    CpalError(#[from] BuildStreamError),
    /// Failed to start audio stream.
    #[error("Failed to start audio stream: {0}")]
    StreamStartError(#[from] PlayStreamError),
    /// No suitable audio device found.
    #[error("No suitable audio device found")]
    NoDeviceFound,
    /// Unsupported sample format.
    #[error("Unsupported sample format: {format:?}")]
    UnsupportedSampleFormat { format: SampleFormat },
    /// Resampling error.
    #[error("Resampling error: {0}")]
    ResamplingError(String),
    /// Ring buffer error.
    #[error("Ring buffer error: {0}")]
    RingBufferError(String),
}

/// Audio output configuration for bit-perfect playback.
#[derive(Debug, Clone)]
pub struct OutputConfig {
    /// Target sample rate for output (may differ from source).
    pub sample_rate: SampleRate,
    /// Number of output channels.
    pub channels: ChannelCount,
    /// Buffer duration in milliseconds.
    pub buffer_duration_ms: u32,
    /// Whether to use exclusive mode (bit-perfect).
    pub exclusive_mode: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            sample_rate: SampleRate(44100),
            channels: 2,
            buffer_duration_ms: 50,
            exclusive_mode: true,
        }
    }
}

/// Manages audio output devices and streams.
///
/// The `AudioOutput` struct handles device enumeration, stream creation,
/// and provides a consumer interface for receiving audio samples from
/// the decoder via ring buffers.
pub struct AudioOutput {
    /// The CPAL host instance.
    host: Host,
    /// The selected output device.
    device: Device,
    /// Current output configuration.
    config: OutputConfig,
    /// Whether resampling is currently active.
    pub is_resampling: bool,
}

impl AudioOutput {
    /// Creates a new audio output manager.
    ///
    /// # Arguments
    ///
    /// * `config` - Optional output configuration. Uses defaults if None.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `AudioOutput` or an `OutputError`.
    ///
    /// # Errors
    ///
    /// Returns `OutputError` if:
    /// - No suitable audio device is found
    /// - Device enumeration fails
    pub fn new(config: Option<OutputConfig>) -> Result<Self, OutputError> {
        let host = default_host();
        let device = host
            .default_output_device()
            .ok_or(OutputError::NoDeviceFound)?;

        Ok(AudioOutput {
            host,
            device,
            config: config.unwrap_or_default(),
            is_resampling: false,
        })
    }

    /// Gets the optimal stream configuration for bit-perfect playback.
    ///
    /// # Arguments
    ///
    /// * `source_format` - The audio format of the source material.
    ///
    /// # Returns
    ///
    /// A `Result` containing a tuple of the optimal `StreamConfig` and a boolean indicating
    /// whether resampling is needed, or an `OutputError`.
    ///
    /// # Errors
    ///
    /// Returns `OutputError` if device capabilities cannot be queried.
    pub fn get_optimal_config(
        &self,
        source_format: &AudioFormat,
    ) -> Result<(StreamConfig, bool), OutputError> {
        let supported_configs = self
            .device
            .supported_output_configs()
            .map_err(|_| OutputError::NoDeviceFound)?;

        // Try to find a configuration that matches our source exactly
        let mut best_config = None;
        let source_sample_rate = source_format.sample_rate;
        let source_channels = source_format.channels;

        for config in supported_configs {
            let sample_rate = config.max_sample_rate().0;
            let channels = config.channels();

            // Prefer exact match
            if sample_rate == source_sample_rate && u32::from(channels) == source_channels {
                best_config = Some(config.with_max_sample_rate());
                break;
            }

            // Fallback to compatible configurations
            if u32::from(channels) >= source_channels
                && (best_config.is_none()
                    || (config.max_sample_rate().0 > best_config.as_ref().unwrap().sample_rate().0))
            {
                best_config = Some(config.with_max_sample_rate());
            }
        }

        let config = best_config.ok_or(OutputError::NoDeviceFound)?;

        // Update our internal config based on what the device supports

        // Return whether resampling is needed as part of the result
        // The caller will need to handle this appropriately

        let is_resampling = config.sample_rate().0 != source_sample_rate
            || u32::from(config.channels()) != source_channels;

        Ok((
            StreamConfig {
                channels: config.channels(),
                sample_rate: config.sample_rate(),
                buffer_size: CpalDefault,
            },
            is_resampling,
        ))
    }

    /// Creates an audio stream with the specified configuration.
    ///
    /// # Arguments
    ///
    /// * `stream_config` - The stream configuration to use.
    /// * `consumer` - The ring buffer consumer to read samples from.
    ///
    /// # Returns
    ///
    /// A `Result` containing the CPAL stream or an `OutputError`.
    ///
    /// # Errors
    ///
    /// Returns `OutputError` if stream creation fails.
    pub fn create_stream(
        &self,
        stream_config: StreamConfig,
        consumer: Consumer<f32>,
    ) -> Result<Stream, OutputError> {
        let sample_format = self
            .device
            .default_output_config()
            .map_err(|_| OutputError::NoDeviceFound)?
            .sample_format();

        let stream = match sample_format {
            F32 => self.build_stream::<f32>(stream_config, consumer),
            I16 => self.build_stream::<i16>(stream_config, consumer),
            U16 => self.build_stream::<u16>(stream_config, consumer),
            _ => {
                return Err(OutputError::UnsupportedSampleFormat {
                    format: sample_format,
                });
            }
        }?;

        Ok(stream)
    }

    /// Builds a typed audio stream.
    fn build_stream<T>(
        &self,
        config: StreamConfig,
        mut consumer: Consumer<f32>,
    ) -> Result<Stream, OutputError>
    where
        T: Sample + SizedSample + Copy + num_traits::NumCast + Default,
    {
        let err_fn = |err| {
            eprintln!("Audio stream error: {}", err);
        };

        let timeout = Duration::from_millis(self.config.buffer_duration_ms as u64);

        let stream = self.device.build_output_stream(
            &config,
            move |data: &mut [T], _: &OutputCallbackInfo| {
                for sample in data.iter_mut() {
                    match consumer.pop() {
                        Ok(value) => {
                            // Convert f32 to the target sample type
                            *sample = match value {
                                v if v >= 1.0 => {
                                    // Use the maximum positive value for the type
                                    cast(1.0f32).unwrap_or(T::default())
                                }
                                v if v <= -1.0 => {
                                    // Use the minimum negative value for the type
                                    cast(-1.0f32).unwrap_or(T::default())
                                }
                                v => {
                                    // Direct cast from f32 to target type
                                    cast(v).unwrap_or(T::default())
                                }
                            };
                        }
                        Err(Empty) => {
                            // Buffer underrun - fill with silence
                            *sample = T::default();
                        }
                    }
                }
            },
            err_fn,
            Some(timeout),
        )?;

        Ok(stream)
    }

    /// Creates a resampler for sample rate conversion.
    ///
    /// # Arguments
    ///
    /// * `source_rate` - Source sample rate.
    /// * `target_rate` - Target sample rate.
    /// * `channels` - Number of channels.
    ///
    /// # Returns
    ///
    /// A `Result` containing the resampler or an `OutputError`.
    ///
    /// # Errors
    ///
    /// Returns `OutputError` if resampler creation fails.
    pub fn create_resampler(
        &self,
        source_rate: u32,
        target_rate: u32,
        channels: usize,
    ) -> Result<FftFixedIn<f32>, OutputError> {
        if source_rate == target_rate {
            return Err(OutputError::ResamplingError(
                "Source and target rates are identical".to_string(),
            ));
        }

        let resampler = FftFixedIn::<f32>::new(
            source_rate as usize,
            target_rate as usize,
            1024, // chunk_size_in - reasonable default
            1,    // sub_chunks
            channels,
        )
        .map_err(|e| OutputError::ResamplingError(e.to_string()))?;

        Ok(resampler)
    }

    /// Gets information about available audio devices.
    ///
    /// # Returns
    ///
    /// A vector of device names.
    pub fn get_available_devices(&self) -> Vec<String> {
        match self.host.output_devices() {
            Ok(devices) => devices.filter_map(|device| device.name().ok()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Gets the current device name.
    ///
    /// # Returns
    ///
    /// The name of the current output device, or "Unknown" if unavailable.
    pub fn get_current_device_name(&self) -> String {
        self.device.name().unwrap_or_else(|_| "Unknown".to_string())
    }
}

/// Audio consumer that reads samples from a ring buffer and feeds them to the output.
///
/// This struct wraps an `AudioOutput` and continuously reads samples from the
/// provided ring buffer consumer.
pub struct AudioConsumer {
    output: AudioOutput,
    consumer: Consumer<f32>,
}

impl AudioConsumer {
    /// Creates a new audio consumer.
    ///
    /// # Arguments
    ///
    /// * `output` - The audio output to use.
    /// * `consumer` - The ring buffer consumer to read samples from.
    /// * `source_format` - The source audio format.
    pub fn new(
        output: AudioOutput,
        consumer: Consumer<f32>,
        source_format: &AudioFormat,
    ) -> Result<Self, OutputError> {
        // Determine if resampling is needed by checking optimal config
        let (_, is_resampling) = output.get_optimal_config(source_format)?;
        let mut output = output;
        output.is_resampling = is_resampling;

        Ok(AudioConsumer { output, consumer })
    }

    /// Runs the audio consumption loop.
    ///
    /// This method should be called to start the audio output stream.
    /// It handles resampling if configured and manages the audio stream lifecycle.
    ///
    /// # Returns
    ///
    /// A `Result` containing the running stream or an `OutputError`.
    ///
    /// # Errors
    ///
    /// Returns `OutputError` if stream creation or startup fails.
    pub fn run(self, source_format: &AudioFormat) -> Result<Stream, OutputError> {
        let (stream_config, _) = self.output.get_optimal_config(source_format)?;
        let stream = self.output.create_stream(stream_config, self.consumer)?;
        stream.play()?;
        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use crate::audio::output::{OutputConfig, OutputError};

    #[test]
    fn test_output_config_default() {
        let config = OutputConfig::default();
        assert_eq!(config.sample_rate.0, 44100);
        assert_eq!(config.channels, 2);
        assert_eq!(config.buffer_duration_ms, 50);
        assert_eq!(config.exclusive_mode, true);
    }

    #[test]
    fn test_output_error_display() {
        let no_device_error = OutputError::NoDeviceFound;
        assert_eq!(
            no_device_error.to_string(),
            "No suitable audio device found"
        );
    }
}
