//! Audio output management using the `cpal` crate.
//!
//! This module handles audio device enumeration, stream creation, and
//! bit-perfect playback configuration for high-fidelity audio output.

use std::time::Duration;

use {
    cpal::{
        BufferSize::Default as CpalDefault,
        BuildStreamError, ChannelCount, Device, Host, OutputCallbackInfo, PlayStreamError,
        SampleFormat::{self, F32, I16, U16},
        Stream, StreamConfig, default_host,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    },
    rtrb::{Consumer, PopError::Empty},
    rubato::FftFixedIn,
    symphonia::core::audio::SignalSpec,
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
    pub sample_rate: u32,
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
            sample_rate: 44100,
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
    /// * `_source_spec` - The signal specification from symphonia with channel layout.
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
        _source_spec: &SignalSpec,
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
            let sample_rate = config.max_sample_rate();
            let channels = config.channels();

            // Prefer exact match for bit-perfect playback
            if sample_rate == source_sample_rate
                && <u32 as From<u16>>::from(channels) == source_channels
            {
                best_config = Some(config.with_max_sample_rate());
                break;
            }

            // Fallback to compatible configurations
            if <u32 as From<u16>>::from(channels) >= source_channels
                && (best_config.is_none()
                    || (config.max_sample_rate() > best_config.as_ref().unwrap().sample_rate()))
            {
                best_config = Some(config.with_max_sample_rate());
            }
        }

        let config = best_config.ok_or(OutputError::NoDeviceFound)?;

        let is_resampling = config.sample_rate() != source_sample_rate
            || <u32 as From<u16>>::from(config.channels()) != source_channels;

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
        mut consumer: Consumer<f32>,
    ) -> Result<Stream, OutputError> {
        let sample_format = self
            .device
            .default_output_config()
            .map_err(|_| OutputError::NoDeviceFound)?
            .sample_format();

        let err_fn = |err| {
            eprintln!("Audio stream error: {}", err);
        };

        let timeout = Duration::from_millis(self.config.buffer_duration_ms as u64);

        let stream = match sample_format {
            F32 => {
                self.device.build_output_stream(
                    &stream_config,
                    move |data: &mut [f32], _: &OutputCallbackInfo| {
                        for sample in data.iter_mut() {
                            match consumer.pop() {
                                Ok(value) => {
                                    // For f32, just clamp to valid range [-1.0, 1.0]
                                    *sample = value.clamp(-1.0, 1.0);
                                }
                                Err(Empty) => {
                                    // Buffer underrun - fill with silence
                                    *sample = 0.0;
                                }
                            }
                        }
                    },
                    err_fn,
                    Some(timeout),
                )?
            }
            I16 => {
                self.device.build_output_stream(
                    &stream_config,
                    move |data: &mut [i16], _: &OutputCallbackInfo| {
                        for sample in data.iter_mut() {
                            match consumer.pop() {
                                Ok(value) => {
                                    // Convert f32 [-1.0, 1.0] to i16 [-32768, 32767]
                                    // Clamp first to avoid overflow
                                    let clamped = value.clamp(-1.0, 1.0);
                                    *sample = (clamped * i16::MAX as f32) as i16;
                                }
                                Err(Empty) => {
                                    // Buffer underrun - fill with silence
                                    *sample = 0;
                                }
                            }
                        }
                    },
                    err_fn,
                    Some(timeout),
                )?
            }
            U16 => {
                self.device.build_output_stream(
                    &stream_config,
                    move |data: &mut [u16], _: &OutputCallbackInfo| {
                        for sample in data.iter_mut() {
                            match consumer.pop() {
                                Ok(value) => {
                                    // Convert f32 [-1.0, 1.0] to u16 [0, 65535]
                                    // Map [-1.0, 1.0] to [0, 65535] where 0.0 maps to 32768
                                    let clamped = value.clamp(-1.0, 1.0);
                                    *sample = ((clamped + 1.0) * (u16::MAX as f32) / 2.0) as u16;
                                }
                                Err(Empty) => {
                                    // Buffer underrun - fill with silence
                                    *sample = 32768; // Midpoint for u16
                                }
                            }
                        }
                    },
                    err_fn,
                    Some(timeout),
                )?
            }
            _ => {
                return Err(OutputError::UnsupportedSampleFormat {
                    format: sample_format,
                });
            }
        };

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
            Ok(devices) => devices
                .filter_map(|device| device.description().ok().map(|desc| desc.to_string()))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Gets the current device name.
    ///
    /// # Returns
    ///
    /// The name of the current output device, or "Unknown" if unavailable.
    pub fn get_current_device_name(&self) -> String {
        self.device
            .description()
            .map(|desc| desc.to_string())
            .unwrap_or_else(|_| "Unknown".to_string())
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
    /// * `source_spec` - The signal specification from symphonia.
    pub fn new(
        output: AudioOutput,
        consumer: Consumer<f32>,
        source_format: &AudioFormat,
        source_spec: &SignalSpec,
    ) -> Result<Self, OutputError> {
        // Determine if resampling is needed by checking optimal config
        let (_, is_resampling) = output.get_optimal_config(source_format, source_spec)?;
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
    pub fn run(
        self,
        source_format: &AudioFormat,
        source_spec: &SignalSpec,
    ) -> Result<Stream, OutputError> {
        let (stream_config, _) = self.output.get_optimal_config(source_format, source_spec)?;
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
        assert_eq!(config.sample_rate, 44100);
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
