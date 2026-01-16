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
        Stream, StreamConfig,
        StreamError::{self, BackendSpecific},
        platform::{
            HostId::{self, Alsa, Jack},
            available_hosts, host_from_id,
        },
        traits::{DeviceTrait, HostTrait, StreamTrait},
    },
    num_traits::cast::ToPrimitive,
    rtrb::{Consumer, PopError::Empty, RingBuffer},
    rubato::{Fft, FixedSync::Input, ResamplerConstructionError},
    symphonia::core::audio::SignalSpec,
    thiserror::Error,
    tracing::{debug, error, info, warn},
};

use crate::audio::{decoder::AudioFormat, resampler::ResamplingAudioConsumer};

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
    /// Failed to initialize a specific audio host.
    #[error("Failed to initialize audio host: {host:?}")]
    HostInitFailed { host: HostId },
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
        let all_hosts = available_hosts();

        // Try multiple hosts in order of preference
        // For PipeWire systems, try Jack first (PipeWire has Jack compatibility)
        // Then fall back to Alsa
        let hosts_to_try = vec![Jack, Alsa];

        for host_id in hosts_to_try {
            if !all_hosts.contains(&host_id) {
                debug!("Host {:?} not available, skipping", host_id);
                continue;
            }

            match Self::try_host(host_id) {
                Ok((host, device)) => {
                    let device_name = device
                        .description()
                        .map_or_else(|_| "Unknown".to_string(), |d| d.to_string());
                    info!(
                        "Successfully initialized host {:?} with device: {}",
                        host_id, device_name
                    );

                    return Ok(AudioOutput {
                        host,
                        device,
                        config: config.unwrap_or_default(),
                        is_resampling: false,
                    });
                }
                Err(e) => {
                    warn!(host = ?host_id, error = %e, "Failed to initialize host");
                }
            }
        }

        error!("Failed to initialize any audio host");
        Err(OutputError::NoDeviceFound)
    }

    /// Tries to initialize audio output with a specific host.
    ///
    /// # Arguments
    ///
    /// * `host_id` - The host ID to try
    ///
    /// # Returns
    ///
    /// A `Result` containing a tuple of the host and device, or an `OutputError`.
    ///
    /// # Errors
    ///
    /// Returns `OutputError` if the host cannot be initialized or no device found.
    fn try_host(host_id: HostId) -> Result<(Host, Device), OutputError> {
        debug!("Instantiating host: {:?}", host_id);

        let host = host_from_id(host_id).map_err(|e| {
            error!(host = ?host_id, error = ?e, "Failed to instantiate host");
            OutputError::HostInitFailed { host: host_id }
        })?;

        debug!("Getting default output device for host: {:?}", host_id);

        let device = host.default_output_device().ok_or_else(|| {
            warn!("No default output device found for host: {:?}", host_id);
            OutputError::NoDeviceFound
        })?;

        info!("Successfully got device for host: {:?}", host_id);

        Ok((host, device))
    }

    /// Gets the target stream configuration based on system capabilities.
    ///
    /// This method respects the actual audio backend constraints by examining
    /// all supported configurations and selecting the most appropriate one.
    /// It returns the actual sample rate that will be used for playback, which may
    /// differ from what the source provides.
    ///
    /// # Arguments
    ///
    /// * `source_format` - The audio format of the source material.
    /// * `_source_spec` - The signal specification from symphonia with channel layout.
    ///
    /// # Returns
    ///
    /// A `Result` containing a tuple of the target `StreamConfig` and a boolean indicating
    /// whether resampling is needed, or an `OutputError`.
    ///
    /// # Errors
    ///
    /// Returns `OutputError` if device capabilities cannot be queried.
    ///
    /// # Panics
    ///
    /// Panics if no compatible configuration is found (should not happen with valid audio devices).
    pub fn get_target_config(
        &self,
        source_format: &AudioFormat,
        _source_spec: &SignalSpec,
    ) -> Result<(StreamConfig, bool), OutputError> {
        // Get all supported output configurations
        let supported_configs = self
            .device
            .supported_output_configs()
            .map_err(|_| OutputError::NoDeviceFound)?;

        let source_sample_rate = source_format.sample_rate;
        let source_channels = source_format.channels;

        // Try to find the best matching configuration
        let mut best_config = None;
        let mut exact_match = false;

        for config in supported_configs {
            let config_sample_rate = config.max_sample_rate();
            let config_channels = config.channels();

            // Check for exact match (bit-perfect)
            if config_sample_rate == source_sample_rate
                && <u32 as From<u16>>::from(config_channels) == source_channels
            {
                best_config = Some(config.with_max_sample_rate());
                exact_match = true;
                break;
            }

            // Otherwise, find the best compatible configuration
            if <u32 as From<u16>>::from(config_channels) >= source_channels
                && (best_config.is_none()
                    || config_sample_rate > best_config.as_ref().unwrap().sample_rate())
            {
                best_config = Some(config.with_max_sample_rate());
            }
        }

        let config = best_config.ok_or(OutputError::NoDeviceFound)?;
        let target_sample_rate = config.sample_rate();
        let target_channels = config.channels();
        let target_sample_format = config.sample_format();

        let needs_resampling = !exact_match;

        // Create stream config using the selected configuration
        let stream_config = StreamConfig {
            channels: target_channels,
            sample_rate: target_sample_rate,
            buffer_size: CpalDefault,
        };

        info!(
            "Target config: {} Hz, {} channels, {:?} format, resampling needed: {}",
            target_sample_rate, target_channels, target_sample_format, needs_resampling
        );

        Ok((stream_config, needs_resampling))
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
    ///
    /// # Panics
    ///
    /// Panics if the audio sample conversion fails. This is guaranteed never to happen because:
    /// 1. Input values are clamped to [-1.0, 1.0]
    /// 2. The conversion formula `(clamped + 1.0) * u16::MAX / 2.0` always yields [0.0, 65535.0]
    /// 3. Converting via i32 and using `try_from().unwrap()` ensures the value fits in u16 range
    pub fn create_stream(
        &self,
        stream_config: &StreamConfig,
        mut consumer: Consumer<f32>,
    ) -> Result<Stream, OutputError> {
        let sample_format = self
            .device
            .default_output_config()
            .map_err(|_| OutputError::NoDeviceFound)?
            .sample_format();

        let err_fn = |err: StreamError| {
            if let BackendSpecific { err } = err {
                let err_str = err.to_string();
                if err_str.contains("buffer size changed") {
                    info!("Audio buffer size changed: {}", err_str);
                } else {
                    error!("Audio backend error: {}", err_str);
                }
            } else {
                error!("Audio stream error: {}", err);
            }
        };

        let timeout = Duration::from_millis(u64::from(self.config.buffer_duration_ms));

        let stream = match sample_format {
            F32 => {
                self.device.build_output_stream(
                    stream_config,
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
                    stream_config,
                    move |data: &mut [i16], _: &OutputCallbackInfo| {
                        for sample in data.iter_mut() {
                            match consumer.pop() {
                                Ok(value) => {
                                    // Convert f32 [-1.0, 1.0] to i16 [-32768, 32767]
                                    // Clamp first to avoid overflow
                                    let clamped = value.clamp(-1.0, 1.0);
                                    let scaled = clamped * f32::from(i16::MAX);
                                    *sample = scaled
                                        .clamp(f32::from(i16::MIN), f32::from(i16::MAX))
                                        .to_i16()
                                        .unwrap();
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
                    stream_config,
                    move |data: &mut [u16], _: &OutputCallbackInfo| {
                        for sample in data.iter_mut() {
                            match consumer.pop() {
                                Ok(value) => {
                                    // Convert f32 [-1.0, 1.0] to u16 [0, 65535]
                                    // Map [-1.0, 1.0] to [0, 65535] where 0.0 maps to 32768
                                    let clamped = value.clamp(-1.0, 1.0);

                                    // Formula (clamped + 1.0) * 65535.0 / 2.0 always yields [0.0, 65535.0]
                                    // Clamp to valid range and use try_from for safe conversion
                                    let scaled = (clamped + 1.0) * f32::from(u16::MAX) / 2.0;
                                    let clamped_scaled = scaled.clamp(0.0, f32::from(u16::MAX));
                                    *sample =
                                        u16::try_from(clamped_scaled.to_i32().unwrap()).unwrap();
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
    ) -> Result<Fft<f32>, OutputError> {
        if source_rate == target_rate {
            return Err(OutputError::ResamplingError(
                "Source and target rates are identical".to_string(),
            ));
        }

        let resampler = Fft::<f32>::new(
            source_rate as usize,
            target_rate as usize,
            1024, // chunk_size_in - reasonable default
            1,    // sub_chunks
            channels,
            Input,
        )
        .map_err(|e: ResamplerConstructionError| OutputError::ResamplingError(e.to_string()))?;

        Ok(resampler)
    }

    /// Gets information about available audio devices.
    ///
    /// # Returns
    ///
    /// A vector of device names.
    #[must_use]
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
    #[must_use]
    pub fn get_current_device_name(&self) -> String {
        self.device
            .description()
            .map_or_else(|_| "Unknown".to_string(), |desc| desc.to_string())
    }
}

impl AudioOutput {
    /// Gets a reference to the CPAL device.
    ///
    /// # Returns
    /// A reference to the selected CPAL output device. The lifetime is tied to `self`.
    #[must_use]
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Gets a reference to the output configuration.
    ///
    /// # Returns
    /// A reference to the current output configuration used to build streams.
    #[must_use]
    pub fn config(&self) -> &OutputConfig {
        &self.config
    }
}

/// Audio consumer that reads samples from a ring buffer and feeds them to the output.
///
/// This struct wraps an `AudioOutput` and continuously reads samples from the
/// provided ring buffer consumer. When resampling is needed, it uses a
/// `ResamplingAudioConsumer` to handle sample rate conversion.
pub enum AudioConsumer {
    /// Direct consumer (no resampling needed).
    Direct {
        output: AudioOutput,
        consumer: Consumer<f32>,
    },
    /// Resampling consumer (resampling needed).
    Resampling {
        output: AudioOutput,
        resampling_consumer: ResamplingAudioConsumer,
        resampled_consumer: Consumer<f32>,
    },
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
    ///
    /// # Errors
    ///
    /// Returns `OutputError` if the output configuration cannot be determined.
    pub fn new(
        output: AudioOutput,
        consumer: Consumer<f32>,
        source_format: &AudioFormat,
        source_spec: &SignalSpec,
    ) -> Result<Self, OutputError> {
        let (target_config, needs_resampling) =
            output.get_target_config(source_format, source_spec)?;

        let mut output = output;
        output.is_resampling = needs_resampling;

        if needs_resampling {
            // Create ring buffers for resampling
            let buffer_size = 8192; // Larger buffer for resampling
            let (resampled_producer, resampled_consumer) = RingBuffer::<f32>::new(buffer_size);

            // Create resampling consumer
            let resampling_consumer = ResamplingAudioConsumer::new(
                consumer,
                resampled_producer,
                source_format,
                target_config,
            )
            .map_err(|e| OutputError::ResamplingError(e.to_string()))?;

            info!("Created resampling audio consumer");
            Ok(AudioConsumer::Resampling {
                output,
                resampling_consumer,
                resampled_consumer,
            })
        } else {
            info!("Created direct audio consumer (no resampling needed)");
            Ok(AudioConsumer::Direct { output, consumer })
        }
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
    ) -> Result<(Stream, Option<ResamplingAudioConsumer>), OutputError> {
        match self {
            AudioConsumer::Direct { output, consumer } => {
                let (stream_config, _) = output.get_target_config(source_format, source_spec)?;
                let stream = output.create_stream(&stream_config, consumer)?;
                stream.play()?;
                Ok((stream, None))
            }
            AudioConsumer::Resampling {
                output,
                resampling_consumer,
                resampled_consumer,
            } => {
                let stream_config = resampling_consumer.target_config().clone();
                let stream = crate::audio::resampler::create_resampling_stream(
                    &output,
                    resampled_consumer,
                    &stream_config,
                )?;
                stream.play()?;

                // Return the resampling consumer for RAII-based lifetime management.
                // It's dropped when the stream stops, cleanly shutting down the resampling thread.
                Ok((stream, Some(resampling_consumer)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::audio::output::{OutputConfig, OutputError::NoDeviceFound};

    #[test]
    fn test_output_config_default() {
        let config = OutputConfig::default();
        assert_eq!(config.sample_rate, 44100);
        assert_eq!(config.channels, 2);
        assert_eq!(config.buffer_duration_ms, 50);
        assert!(config.exclusive_mode);
    }

    #[test]
    fn test_output_error_display() {
        let no_device_error = NoDeviceFound;
        assert_eq!(
            no_device_error.to_string(),
            "No suitable audio device found"
        );
    }
}
