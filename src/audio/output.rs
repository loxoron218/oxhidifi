//! Audio output management using the `cpal` crate.
//!
//! This module handles audio device enumeration, stream creation, and
//! bit-perfect playback configuration for high-fidelity audio output.

use std::{
    convert::identity,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::SeqCst},
    },
    time::Duration,
};

use {
    cpal::{
        BufferSize::Default as CpalDefault,
        BuildStreamError, ChannelCount, Device, Host, OutputCallbackInfo, PlayStreamError,
        SampleFormat::{self, F32, F64, I8, I16, I24, I32, I64, U8, U16, U24, U32, U64},
        Stream, StreamConfig,
        StreamError::{self, BackendSpecific},
        SupportedStreamConfig,
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

use crate::audio::{
    buffer_config::BufferConfig,
    constants::{DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE},
    decoder::MS_PER_SEC,
    decoder_types::AudioFormat,
    resampler::{ResamplingAudioConsumer, create_resampling_stream},
};

/// Builds a sample output stream with format-specific conversion.
///
/// # Macro Parameters
///
/// * `$device` - The CPAL device to build the stream on
/// * `$stream_config` - The stream configuration
/// * `$consumer` - The ring buffer consumer to read samples from
/// * `$err_fn` - Error handler callback
/// * `$timeout` - Buffer timeout duration
/// * `$type` - Type of samples for the output stream (e.g., `i16`, `f32`)
/// * `$convert` - Closure expression to convert f32 samples to target type
/// * `$silent` - Silence value for buffer underruns
/// * `$position` - Atomic position counter to update with consumed samples
/// * `$channels` - Number of audio channels
/// * `$sample_rate` - Sample rate for position calculation
macro_rules! build_sample_stream {
    ($device:expr, $stream_config:expr, $consumer:expr, $err_fn:expr, $timeout:expr, $type:ty, $convert:expr, $silent:expr, $position:expr, $channels:expr, $sample_rate:expr) => {{
        let position = Arc::clone(&$position);
        let channels = $channels as usize;
        let sample_rate = u64::from($sample_rate);

        $device
            .build_output_stream(
                $stream_config,
                move |data: &mut [$type], _: &OutputCallbackInfo| {
                    let mut samples_consumed = 0;
                    for sample in data.iter_mut() {
                        match $consumer.pop() {
                            Ok(value) => {
                                *sample = $convert(value);
                                samples_consumed += 1;
                            }
                            Err(Empty) => *sample = $silent,
                        }
                    }

                    // Update position based on samples actually consumed and played
                    if samples_consumed > 0 {
                        let frames = samples_consumed / channels;
                        let duration_ms = (frames as u64 * MS_PER_SEC) / sample_rate;
                        position.fetch_add(duration_ms, SeqCst);
                    }
                },
                $err_fn,
                Some($timeout),
            )
            .map_err(|e| OutputError::CpalError(e))
    }};
}

/// Generates stream builder methods for each sample format.
///
/// # Macro Parameters
///
/// * `$name` - Function name to generate (e.g., `build_stream_f32`)
/// * `$type` - Sample type for the stream (e.g., `f32`, `i16`)
/// * `$silent` - Silence value for buffer underruns
/// * `$convert` - Closure expression to convert f32 samples to target type
macro_rules! generate_stream_builders {
    ($name:ident, $type:ty, $silent:expr, $convert:expr) => {
        /// Builds an audio stream for the sample format.
        ///
        /// # Errors
        ///
        /// Returns `OutputError::StreamBuildError` if the audio stream cannot be created.
        fn $name(mut ctx: Self) -> Result<Stream, OutputError> {
            build_sample_stream!(
                ctx.device,
                ctx.stream_config,
                ctx.consumer,
                ctx.err_fn,
                ctx.timeout,
                $type,
                $convert,
                $silent,
                ctx.current_position,
                ctx.channels,
                ctx.sample_rate
            )
        }
    };
}

/// Maximum i64 value as f64 for scaling.
const I64_MAX_F64: f64 = 9_223_372_036_854_776_000.0;

/// Minimum i64 value as f64 for scaling.
const I64_MIN_F64: f64 = -9_223_372_036_854_776_000.0;

/// Maximum u64 value as f64 for scaling.
const U64_MAX_F64: f64 = 18_446_744_073_709_552_000_f64;

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
    /// Exclusive mode requirements not met.
    #[error("Exclusive mode requires bit-perfect playback: {reason}")]
    ExclusiveModeFailed { reason: String },
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
    /// Output device name (if available).
    pub device_name: Option<String>,
    /// Bits per sample for output format.
    pub bits_per_sample: u32,
    /// Whether resampling is currently active.
    pub is_resampling: bool,
    /// Buffer size configuration for ring buffers.
    pub buffer_config: BufferConfig,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            sample_rate: DEFAULT_SAMPLE_RATE,
            channels: u16::try_from(DEFAULT_CHANNELS).unwrap_or(2),
            buffer_duration_ms: 500,
            exclusive_mode: true,
            device_name: None,
            bits_per_sample: 24,
            is_resampling: false,
            buffer_config: BufferConfig::default(),
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

/// Context for building audio streams with all required parameters.
///
/// This struct groups all parameters needed to create an audio stream,
/// reducing the number of arguments passed to helper functions.
struct StreamBuildContext<'a, F>
where
    F: Fn(StreamError) + Send + 'static,
{
    /// The audio device to build the stream on.
    device: &'a Device,
    /// The stream configuration.
    stream_config: &'a StreamConfig,
    /// The ring buffer consumer.
    consumer: Consumer<f32>,
    /// Error handler callback.
    err_fn: F,
    /// Buffer timeout duration.
    timeout: Duration,
    /// Shared atomic for tracking playback position.
    current_position: &'a Arc<AtomicU64>,
    /// Number of audio channels.
    channels: ChannelCount,
    /// Sample rate for position calculation.
    sample_rate: cpal::SampleRate,
}

/// Associated functions for building audio streams.
impl<F> StreamBuildContext<'_, F>
where
    F: Fn(StreamError) + Send + 'static,
{
    generate_stream_builders!(build_stream_f32, f32, 0.0, |value: f32| {
        value.clamp(-1.0, 1.0)
    });

    generate_stream_builders!(build_stream_f64, f64, 0.0, |value: f32| {
        f64::from(value.clamp(-1.0, 1.0))
    });

    generate_stream_builders!(build_stream_i8, i8, 0, |value: f32| {
        let clamped = value.clamp(-1.0, 1.0);
        let scaled = clamped * f32::from(i8::MAX);
        let clamped_scaled = scaled.clamp(f32::from(i8::MIN), f32::from(i8::MAX));

        clamped_scaled.to_i8().unwrap_or(0)
    });

    generate_stream_builders!(build_stream_i16, i16, 0, |value: f32| {
        let clamped = value.clamp(-1.0, 1.0);
        let scaled = clamped * f32::from(i16::MAX);
        let clamped_scaled = scaled.clamp(f32::from(i16::MIN), f32::from(i16::MAX));

        clamped_scaled.to_i16().unwrap_or(0)
    });

    generate_stream_builders!(build_stream_i32, i32, 0, |value: f32| {
        let clamped = f64::from(value.clamp(-1.0, 1.0));
        let scaled = clamped * f64::from(i32::MAX);
        let clamped_scaled = scaled.clamp(f64::from(i32::MIN), f64::from(i32::MAX));

        clamped_scaled.to_i32().unwrap_or(0)
    });

    generate_stream_builders!(build_stream_i64, i64, 0, |value: f32| {
        let clamped = f64::from(value.clamp(-1.0, 1.0));
        let scaled = clamped * I64_MAX_F64;
        let clamped_scaled = scaled.clamp(I64_MIN_F64, I64_MAX_F64);

        clamped_scaled.to_i64().unwrap_or(0)
    });

    generate_stream_builders!(build_stream_u8, u8, 1_u8 << 7, |value: f32| {
        let clamped = value.clamp(-1.0, 1.0);
        let scaled = (clamped + 1.0) * f32::from(u8::MAX) / 2.0;
        let clamped_scaled = scaled.clamp(0.0, f32::from(u8::MAX));

        clamped_scaled.to_u8().unwrap_or(128)
    });

    generate_stream_builders!(build_stream_u16, u16, 1_u16 << 15, |value: f32| {
        let clamped = value.clamp(-1.0, 1.0);
        let scaled = (clamped + 1.0) * f32::from(u16::MAX) / 2.0;
        let clamped_scaled = scaled.clamp(0.0, f32::from(u16::MAX));

        clamped_scaled.to_u16().unwrap_or(32768)
    });

    generate_stream_builders!(build_stream_i24, i32, 0, |value: f32| {
        let clamped = f64::from(value.clamp(-1.0, 1.0));
        let scaled = clamped * f64::from((1_i32 << 23) - 1);
        let clamped_scaled = scaled.clamp(f64::from(-(1_i32 << 23)), f64::from((1_i32 << 23) - 1));

        clamped_scaled.to_i32().unwrap_or(0)
    });

    generate_stream_builders!(build_stream_u24, u32, 1_u32 << 23, |value: f32| {
        let clamped = f64::from(value.clamp(-1.0, 1.0));
        let scaled = (clamped + 1.0) * f64::from((1_u32 << 24) - 1) / 2.0;
        let clamped_scaled = scaled.clamp(0.0, f64::from((1_u32 << 24) - 1));

        clamped_scaled.to_u32().unwrap_or(1_u32 << 23)
    });

    generate_stream_builders!(build_stream_u32, u32, 1_u32 << 31, |value: f32| {
        let clamped = f64::from(value.clamp(-1.0, 1.0));
        let scaled = (clamped + 1.0) * f64::from(u32::MAX) / 2.0;
        let clamped_scaled = scaled.clamp(0.0, f64::from(u32::MAX));

        clamped_scaled.to_u32().unwrap_or(1_u32 << 31)
    });

    generate_stream_builders!(build_stream_u64, u64, 1_u64 << 63, |value: f32| {
        let clamped = f64::from(value.clamp(-1.0, 1.0));
        let scaled = (clamped + 1.0) * U64_MAX_F64 / 2.0;
        let clamped_scaled = scaled.clamp(0.0, U64_MAX_F64);

        clamped_scaled.to_u64().unwrap_or(1_u64 << 63)
    });
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

        let output_config = config.map_or_else(
            || {
                debug!("No output config provided, using defaults");
                OutputConfig::default()
            },
            identity,
        );

        let exclusive_mode = output_config.exclusive_mode;

        // Try multiple hosts in order of preference
        // For PipeWire systems, try Jack first (PipeWire has Jack compatibility)
        // Then fall back to Alsa
        let hosts_to_try = vec![Jack, Alsa];

        for host_id in hosts_to_try {
            if !all_hosts.contains(&host_id) {
                debug!("Host {:?} not available, skipping", host_id);
                continue;
            }

            match Self::try_host(host_id, exclusive_mode) {
                Ok((host, device)) => {
                    let device_name = device
                        .description()
                        .map_or_else(|_| "Unknown".to_string(), |d| d.to_string());
                    info!(
                        "Successfully initialized host {:?} with device: {}, exclusive_mode: {}",
                        host_id, device_name, exclusive_mode
                    );

                    return Ok(Self {
                        host,
                        device,
                        config: output_config,
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
    /// * `exclusive_mode` - Whether to try exclusive mode
    ///
    /// # Returns
    ///
    /// A `Result` containing a tuple of the host and device, or an `OutputError`.
    ///
    /// # Errors
    ///
    /// Returns `OutputError` if the host cannot be initialized or no device found.
    fn try_host(host_id: HostId, exclusive_mode: bool) -> Result<(Host, Device), OutputError> {
        debug!(
            "Instantiating host: {:?}, exclusive_mode: {}",
            host_id, exclusive_mode
        );

        let host = host_from_id(host_id).map_err(|e| {
            error!(host = ?host_id, error = ?e, "Failed to instantiate host");
            OutputError::HostInitFailed { host: host_id }
        })?;

        debug!("Getting output device for host: {:?}", host_id);

        if exclusive_mode {
            info!(
                "Attempting exclusive mode with host: {:?} (may fail if device is busy)",
                host_id
            );
        }

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
    /// Returns `OutputError` if device capabilities cannot be queried or exclusive mode requires unsupported format.
    ///
    /// # Panics
    ///
    /// Panics if no compatible configuration is found (should not happen with valid audio devices).
    pub fn get_target_config(
        &self,
        source_format: &AudioFormat,
        _source_spec: &SignalSpec,
    ) -> Result<(StreamConfig, bool), OutputError> {
        let exclusive_mode = self.config.exclusive_mode;

        // Get all supported output configurations
        let supported_configs = self
            .device
            .supported_output_configs()
            .map_err(|_err| OutputError::NoDeviceFound)?;

        let source_sample_rate = source_format.sample_rate;
        let source_channels = source_format.channels;
        let source_bits = source_format.bits_per_sample;

        // Try to find the best matching configuration
        let mut best_config = None;
        let mut rate_match = false;

        for config in supported_configs {
            let config_sample_rate = config.max_sample_rate();
            let config_channels = config.channels();
            let config_sample_format = config.sample_format();

            // Convert target sample format to bits for comparison
            let target_bits = match config_sample_format {
                U8 | I8 => 8,
                I16 | U16 => 16,
                I24 | U24 => 24,
                F64 | I64 | U64 => 64,
                _ => 32,
            };

            // Check for exact match (bit-perfect)
            // Requires matching sample rate, channels, and bit depth
            if config_sample_rate == source_sample_rate
                && <u32 as From<u16>>::from(config_channels) == source_channels
                && target_bits == source_bits
            {
                best_config = Some(config.with_max_sample_rate());
                rate_match = true;
                break;
            }

            // Otherwise, find the best compatible configuration
            if <u32 as From<u16>>::from(config_channels) >= source_channels {
                let should_update = best_config.is_none()
                    || config_sample_rate
                        > best_config
                            .as_ref()
                            .map_or(0, SupportedStreamConfig::sample_rate);
                if should_update {
                    best_config = Some(config.with_max_sample_rate());
                    if config_sample_rate == source_sample_rate {
                        rate_match = true;
                    }
                }
            }
        }

        // In exclusive mode, reject playback if no exact match is found
        if exclusive_mode && !rate_match {
            let reason = format!(
                "Device doesn't support {source_sample_rate} Hz / {source_channels} ch / {source_bits}-bit audio"
            );
            warn!("Exclusive mode requires bit-perfect playback: {}", reason);
            return Err(OutputError::ExclusiveModeFailed { reason });
        }

        let config = best_config.ok_or(OutputError::NoDeviceFound)?;
        let target_sample_rate = config.sample_rate();
        let target_channels = config.channels();
        let target_sample_format = config.sample_format();

        // Only use resampler if sample rates differ
        // Bit-depth conversion is handled automatically in stream creation
        let needs_resampling = !rate_match;

        // Create stream config using the selected configuration
        let stream_config = StreamConfig {
            channels: target_channels,
            sample_rate: target_sample_rate,
            buffer_size: CpalDefault,
        };

        info!(
            "Target config: {} Hz, {} channels, {:?} format, resampling needed: {}, exclusive_mode: {}",
            target_sample_rate,
            target_channels,
            target_sample_format,
            needs_resampling,
            exclusive_mode
        );

        Ok((stream_config, needs_resampling))
    }

    /// Creates an audio stream with the specified configuration.
    ///
    /// # Arguments
    ///
    /// * `stream_config` - The stream configuration to use.
    /// * `consumer` - The ring buffer consumer to read samples from.
    /// * `current_position` - Shared atomic for tracking actual playback position.
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
        stream_config: &StreamConfig,
        consumer: Consumer<f32>,
        current_position: &Arc<AtomicU64>,
    ) -> Result<Stream, OutputError> {
        let sample_format = self
            .device
            .default_output_config()
            .map_err(|_err| OutputError::NoDeviceFound)?
            .sample_format();

        let err_fn = Self::create_error_handler();
        let timeout = Duration::from_millis(u64::from(self.config.buffer_duration_ms));
        let channels = stream_config.channels;
        let sample_rate = stream_config.sample_rate;

        let ctx = StreamBuildContext {
            device: &self.device,
            stream_config,
            consumer,
            err_fn,
            timeout,
            current_position,
            channels,
            sample_rate,
        };

        let stream = Self::create_stream_for_format(sample_format, ctx)?;

        Ok(stream)
    }

    /// Creates an error handler callback for stream errors.
    ///
    /// # Returns
    ///
    /// A closure that handles stream errors by logging them appropriately.
    fn create_error_handler() -> impl Fn(StreamError) {
        |err: StreamError| {
            if let BackendSpecific { err } = err {
                let err_str = err.to_string();
                if err_str.contains("buffer size changed") {
                    info!(message = %err_str, "Audio buffer size changed");
                } else {
                    error!(error = %err_str, "Audio backend error");
                }
            } else {
                error!(error = %err, "Audio stream error");
            }
        }
    }

    /// Creates a stream for the specified sample format.
    ///
    /// # Arguments
    ///
    /// * `sample_format` - The sample format to use for the stream.
    /// * `ctx` - Stream build context containing all required parameters.
    ///
    /// # Returns
    ///
    /// A `Result` containing the CPAL stream or an `OutputError`.
    ///
    /// # Errors
    ///
    /// Returns `OutputError` if the sample format is unsupported.
    fn create_stream_for_format<F>(
        sample_format: SampleFormat,
        ctx: StreamBuildContext<F>,
    ) -> Result<Stream, OutputError>
    where
        F: Fn(StreamError) + Send + 'static,
    {
        match sample_format {
            F32 => StreamBuildContext::build_stream_f32(ctx),
            I8 => StreamBuildContext::build_stream_i8(ctx),
            I16 => StreamBuildContext::build_stream_i16(ctx),
            I24 => StreamBuildContext::build_stream_i24(ctx),
            I32 => StreamBuildContext::build_stream_i32(ctx),
            I64 => StreamBuildContext::build_stream_i64(ctx),
            U8 => StreamBuildContext::build_stream_u8(ctx),
            U16 => StreamBuildContext::build_stream_u16(ctx),
            U24 => StreamBuildContext::build_stream_u24(ctx),
            U32 => StreamBuildContext::build_stream_u32(ctx),
            U64 => StreamBuildContext::build_stream_u64(ctx),
            F64 => StreamBuildContext::build_stream_f64(ctx),
            _ => Err(OutputError::UnsupportedSampleFormat {
                format: sample_format,
            }),
        }
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
            4096, // chunk_size_in - larger for better throughput
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
    pub fn get_available_devices(&self) -> Vec<String> {
        match self.host.output_devices() {
            Ok(devices) => devices
                .filter_map(|device| match device.description() {
                    Ok(desc) => Some(desc.to_string()),
                    Err(e) => {
                        warn!(error = %e, "Failed to get device description");
                        None
                    }
                })
                .collect(),
            Err(e) => {
                warn!(error = %e, "Failed to enumerate output devices");
                Vec::new()
            }
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
        current_position: Arc<AtomicU64>,
    },
    /// Resampling consumer (resampling needed).
    Resampling {
        output: AudioOutput,
        resampling_consumer: ResamplingAudioConsumer,
        resampled_consumer: Consumer<f32>,
        current_position: Arc<AtomicU64>,
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
    /// * `current_position` - Shared atomic for tracking actual playback position.
    ///
    /// # Returns
    ///
    /// A tuple of the `AudioConsumer` and the actual target `OutputConfig`.
    ///
    /// # Errors
    ///
    /// Returns `OutputError` if the output configuration cannot be determined.
    pub fn new(
        mut output: AudioOutput,
        consumer: Consumer<f32>,
        source_format: &AudioFormat,
        source_spec: &SignalSpec,
        current_position: Arc<AtomicU64>,
    ) -> Result<(Self, OutputConfig), OutputError> {
        let (target_config, needs_resampling) =
            output.get_target_config(source_format, source_spec)?;

        let sample_format = output
            .device
            .default_output_config()
            .map_err(|_err| OutputError::NoDeviceFound)?
            .sample_format();

        let bits_per_sample = match sample_format {
            U8 | I8 => 8,
            I16 | U16 => 16,
            I24 | U24 => 24,
            F64 | I64 | U64 => 64,
            _ => 32,
        };

        let device_name = output.device.description().map(|d| d.to_string()).ok();

        let target_output_config = OutputConfig {
            sample_rate: target_config.sample_rate,
            channels: target_config.channels,
            buffer_duration_ms: output.config.buffer_duration_ms,
            exclusive_mode: output.config.exclusive_mode,
            device_name,
            bits_per_sample,
            is_resampling: needs_resampling,
            buffer_config: output.config.buffer_config.clone(),
        };

        output.is_resampling = needs_resampling;

        let consumer = if needs_resampling {
            // Create ring buffers for resampling
            // Use very large buffer to handle resampling expansion, rate mismatches, and playback buffer management
            let buffer_size = output.config.buffer_config.resampler_buffer_size;
            let (resampled_producer, resampled_consumer) = RingBuffer::<f32>::new(buffer_size);

            // Create resampling consumer
            let resampling_consumer = ResamplingAudioConsumer::new(
                consumer,
                resampled_producer,
                source_format,
                target_config,
                &output.config.buffer_config,
            )
            .map_err(|e| OutputError::ResamplingError(e.to_string()))?;

            info!("Created resampling audio consumer");
            Self::Resampling {
                output,
                resampling_consumer,
                resampled_consumer,
                current_position,
            }
        } else {
            info!("Created direct audio consumer (no resampling needed)");
            Self::Direct {
                output,
                consumer,
                current_position,
            }
        };

        Ok((consumer, target_output_config))
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
            Self::Direct {
                output,
                consumer,
                current_position,
            } => {
                let (stream_config, _) = output.get_target_config(source_format, source_spec)?;
                let stream = output.create_stream(&stream_config, consumer, &current_position)?;
                stream.play()?;
                Ok((stream, None))
            }
            Self::Resampling {
                output,
                resampling_consumer,
                resampled_consumer,
                current_position,
            } => {
                let stream_config = resampling_consumer.target_config().clone();
                let stream = create_resampling_stream(
                    &output,
                    resampled_consumer,
                    &stream_config,
                    &current_position,
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
        assert_eq!(config.buffer_duration_ms, 500);
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
