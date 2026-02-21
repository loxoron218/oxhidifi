//! High-quality sample rate conversion for audio playback.
//!
//! This module provides real-time sample rate conversion using the `rubato` crate,
//! which implements high-quality resampling algorithms suitable for professional audio.

use std::{
    error::Error,
    fmt::{Display, Formatter, Result as StdResult},
    sync::{
        Arc,
        atomic::{
            AtomicBool, AtomicU64,
            Ordering::{Relaxed, SeqCst},
        },
    },
    thread::{JoinHandle, sleep, spawn},
    time::Duration,
};

use {
    audioadapter_buffers::direct::InterleavedSlice,
    cpal::{
        OutputCallbackInfo, SampleFormat, Stream, StreamConfig,
        StreamError::{self, BackendSpecific},
        traits::DeviceTrait,
    },
    num_traits::cast::ToPrimitive,
    rtrb::{Consumer, PopError::Empty, Producer, PushError::Full},
    rubato::{Fft, FixedSync::Input, ResampleError, Resampler, ResamplerConstructionError},
    tracing::{debug, error, info, warn},
};

use crate::audio::{
    buffer_config::BufferConfig,
    decoder_types::AudioFormat,
    output::{
        AudioOutput,
        OutputError::{self, NoDeviceFound, UnsupportedSampleFormat},
    },
};

/// Sleep duration when target buffer is full.
const RESAMPLER_SLEEP_DURATION: Duration = Duration::from_micros(50);

/// Error type for resampling operations.
#[derive(Debug)]
pub enum ResamplingError {
    /// Rubato resampling error.
    RubatoError(String),
    /// Ring buffer error.
    RingBufferError(String),
    /// Invalid configuration.
    InvalidConfiguration(String),
}

impl Display for ResamplingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> StdResult {
        match self {
            Self::RubatoError(msg) => write!(f, "Rubato error: {msg}"),
            Self::RingBufferError(msg) => write!(f, "Ring buffer error: {msg}"),
            Self::InvalidConfiguration(msg) => {
                write!(f, "Invalid configuration: {msg}")
            }
        }
    }
}

impl Error for ResamplingError {}

/// Real-time audio resampler using rubato.
///
/// This struct handles sample rate conversion between source and target sample rates
/// in real-time, maintaining proper channel layout and timing.
pub struct AudioResampler {
    /// Rubato resampler instance.
    resampler: Fft<f32>,
    /// Source sample rate in Hz.
    source_rate: u32,
    /// Target sample rate in Hz.
    target_rate: u32,
    /// Number of channels.
    channels: usize,
    /// Accumulated interleaved input buffer (may hold partial frames).
    input_buffer: Vec<f32>,
    /// Output buffer for resampled data.
    output_buffer: Vec<f32>,
}

impl AudioResampler {
    /// Creates a new audio resampler.
    ///
    /// # Arguments
    ///
    /// * `source_rate` - Source sample rate in Hz.
    /// * `target_rate` - Target sample rate in Hz.
    /// * `channels` - Number of audio channels.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `AudioResampler` or a `ResamplingError`.
    ///
    /// # Errors
    ///
    /// Returns `ResamplingError` if the resampler cannot be created or if rates are invalid.
    pub fn new(
        source_rate: u32,
        target_rate: u32,
        channels: usize,
    ) -> Result<Self, ResamplingError> {
        if source_rate == 0 || target_rate == 0 {
            return Err(ResamplingError::InvalidConfiguration(
                "Sample rates must be greater than 0".to_string(),
            ));
        }

        if source_rate == target_rate {
            return Err(ResamplingError::InvalidConfiguration(
                "Source and target rates must be different".to_string(),
            ));
        }

        // Calculate chunk size based on sample rates
        // Use a reasonable default that balances latency and efficiency
        let chunk_size = calculate_chunk_size(source_rate, target_rate);

        let resampler = Fft::<f32>::new(
            source_rate as usize,
            target_rate as usize,
            chunk_size,
            1, // sub_chunks
            channels,
            Input,
        )
        .map_err(|e: ResamplerConstructionError| ResamplingError::RubatoError(e.to_string()))?;

        info!(
            "Created resampler: {} Hz -> {} Hz, {} channels, chunk size: {}",
            source_rate, target_rate, channels, chunk_size
        );

        Ok(Self {
            resampler,
            source_rate,
            target_rate,
            channels,
            input_buffer: Vec::with_capacity(chunk_size * channels),
            output_buffer: Vec::new(),
        })
    }

    /// Resamples a block of audio samples.
    ///
    /// # Arguments
    ///
    /// * `input_samples` - Input samples in interleaved format [L, R, L, R, ...].
    ///
    /// # Returns
    ///
    /// A `Result` containing the resampled samples or a `ResamplingError`.
    ///
    /// # Errors
    ///
    /// Returns `ResamplingError` if resampling fails.
    pub fn resample_block(&mut self, input_samples: &[f32]) -> Result<Vec<f32>, ResamplingError> {
        if input_samples.is_empty() {
            return Ok(Vec::new());
        }

        // Accumulate input samples (interleaved)
        self.input_buffer.extend_from_slice(input_samples);
        self.output_buffer.clear();

        let ch = self.channels;
        let needed_frames = self.resampler.input_frames_next();
        let mut available_frames = self.input_buffer.len() / ch;

        while available_frames >= needed_frames {
            // Extract exactly needed_frames frames per channel as interleaved slice
            let samples_to_process = &self.input_buffer[..needed_frames * ch];
            let input_adapter = InterleavedSlice::new(samples_to_process, ch, needed_frames)
                .map_err(|e| ResamplingError::RubatoError(e.to_string()))?;

            let output_owned = self
                .resampler
                .process(&input_adapter, 0, None)
                .map_err(|e: ResampleError| ResamplingError::RubatoError(e.to_string()))?;

            self.output_buffer
                .extend_from_slice(&output_owned.take_data());

            // Remove processed interleaved samples from the accumulator
            let remove_count = needed_frames * ch;
            self.input_buffer.drain(0..remove_count);
            available_frames = self.input_buffer.len() / ch;
        }

        Ok(self.output_buffer.clone())
    }

    /// Gets the expected output size for a given input size.
    ///
    /// # Arguments
    ///
    /// * `input_size` - Number of input samples (per channel).
    ///
    /// # Returns
    ///
    /// Expected number of output samples (per channel).
    #[must_use]
    pub fn expected_output_size(&self, input_size: usize) -> usize {
        let in_rate = u64::from(self.source_rate);
        let out_rate = u64::from(self.target_rate);
        usize::try_from((input_size as u64 * out_rate) / in_rate).unwrap_or(usize::MAX)
    }
}

/// Calculates an appropriate chunk size for resampling based on sample rates.
fn calculate_chunk_size(source_rate: u32, target_rate: u32) -> usize {
    // Find GCD to get a reasonable chunk size
    let gcd = gcd(source_rate, target_rate);
    let lcm = (u64::from(source_rate) * u64::from(target_rate)) / u64::from(gcd);

    // Use a chunk size that's a multiple of both rates' relationship
    // but keep it reasonable for real-time processing
    let base_chunk = (lcm / u64::from(source_rate)).min(4096) as usize;

    // Ensure it's at least 256 samples for efficiency
    base_chunk.clamp(256, 8192)
}

/// Calculates the greatest common divisor of two numbers.
fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Resampling audio consumer that handles real-time sample rate conversion.
///
/// This consumer reads from a source ring buffer, resamples the audio data,
/// and writes to a target ring buffer that feeds the audio output.
pub struct ResamplingAudioConsumer {
    /// Running flag for the resampling thread.
    running: Arc<AtomicBool>,
    /// Resampling thread handle.
    thread_handle: Option<JoinHandle<()>>,
    /// Target stream configuration.
    target_config: StreamConfig,
}

impl ResamplingAudioConsumer {
    /// Creates a new resampling audio consumer.
    ///
    /// # Arguments
    ///
    /// * `source_consumer` - Ring buffer consumer from the decoder.
    /// * `target_producer` - Ring buffer producer to the audio output.
    /// * `source_format` - Source audio format information.
    /// * `target_config` - Target stream configuration from the audio device.
    /// * `buffer_config` - Buffer size configuration.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `ResamplingAudioConsumer` or a `ResamplingError`.
    ///
    /// # Errors
    ///
    /// Returns `ResamplingError` if the resampler cannot be created.
    pub fn new(
        source_consumer: Consumer<f32>,
        target_producer: Producer<f32>,
        source_format: &AudioFormat,
        target_config: StreamConfig,
        buffer_config: &BufferConfig,
    ) -> Result<Self, ResamplingError> {
        let source_rate = source_format.sample_rate;
        let target_rate = target_config.sample_rate;
        let channels = target_config.channels as usize;

        let resampler = AudioResampler::new(source_rate, target_rate, channels)?;

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let input_buffer_size = buffer_config.input_buffer_size;

        // Start resampling thread
        let thread_handle = Some(spawn(move || {
            resampling_loop(
                source_consumer,
                target_producer,
                resampler,
                &running_clone,
                channels,
                input_buffer_size,
            );
        }));

        Ok(Self {
            running,
            thread_handle,
            target_config,
        })
    }

    /// Gets the target stream configuration.
    #[must_use]
    pub fn target_config(&self) -> &StreamConfig {
        &self.target_config
    }

    /// Stops the resampling consumer gracefully.
    pub fn stop(&mut self) {
        if let Some(handle) = self.thread_handle.take() {
            self.running.store(false, Relaxed);
            if let Err(e) = handle.join() {
                warn!(error = ?e, "Resampler thread panicked");
            }
        }
    }
}

impl Drop for ResamplingAudioConsumer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Checks if a ring buffer producer has been abandoned and returns early if so.
///
/// This macro ensures the resampling thread exits promptly when the stream is stopped,
/// preventing it from blocking indefinitely on abandoned ring buffers.
///
/// # Arguments
///
/// * `$producer` - A `Producer<T>` from the `rtrb` crate
macro_rules! check_abandonment {
    ($producer:expr) => {
        if $producer.is_abandoned() {
            return;
        }
    };
}

/// Main resampling loop that runs in a dedicated thread.
fn resampling_loop(
    mut source_consumer: Consumer<f32>,
    mut target_producer: Producer<f32>,
    mut resampler: AudioResampler,
    running: &Arc<AtomicBool>,
    channels: usize,
    input_buffer_size: usize,
) {
    let mut input_buffer = Vec::with_capacity(input_buffer_size);
    let mut output_buffer = Vec::new();

    while running.load(Relaxed) {
        // Read available samples from source
        input_buffer.clear();
        let mut samples_read = 0;

        // Flow control: limit reads based on available target buffer space
        // This prevents producer from overwhelming consumer during rate conversion
        let available_space = target_producer.slots();
        let samples_per_frame = channels;
        let max_read = if available_space < input_buffer_size / 2 {
            // If target buffer is less than half full, limit reads
            (available_space / samples_per_frame) * samples_per_frame
        } else {
            // Otherwise, read full chunk
            input_buffer_size
        };

        while samples_read < max_read {
            check_abandonment!(source_consumer);
            match source_consumer.pop() {
                Ok(sample) => {
                    input_buffer.push(sample);
                    samples_read += 1;
                }
                Err(Empty) => {
                    // No more samples available right now
                    break;
                }
            }
        }

        if input_buffer.is_empty() {
            sleep(Duration::from_micros(100));
            continue;
        }

        // Resample the input buffer
        match resampler.resample_block(&input_buffer) {
            Ok(resampled_samples) => {
                output_buffer.clear();
                output_buffer.extend_from_slice(&resampled_samples);

                // Write resampled samples to target
                for &sample in &output_buffer {
                    loop {
                        check_abandonment!(target_producer);
                        match target_producer.push(sample) {
                            Ok(()) => break,
                            Err(Full(_)) => {
                                // Target buffer is full, wait briefly
                                sleep(RESAMPLER_SLEEP_DURATION);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "Resampling error");

                // Continue processing despite errors to avoid complete failure
                // Write silence to maintain timing
                let expected_output_size =
                    resampler.expected_output_size(input_buffer.len() / channels);
                for _ in 0..(expected_output_size * channels) {
                    loop {
                        check_abandonment!(target_producer);
                        match target_producer.push(0.0) {
                            Ok(()) => break,
                            Err(Full(_)) => {
                                sleep(RESAMPLER_SLEEP_DURATION);
                            }
                        }
                    }
                }
            }
        }
    }

    debug!("Resampling loop stopped");
}

/// Gets the sample format from the audio output device.
///
/// # Arguments
///
/// * `output` - The audio output device.
///
/// # Returns
///
/// A `Result` containing the sample format or an `OutputError`.
///
/// # Errors
///
/// Returns `NoDeviceFound` if the device configuration cannot be queried.
fn get_sample_format(output: &AudioOutput) -> Result<SampleFormat, OutputError> {
    Ok(output
        .device()
        .default_output_config()
        .map_err(|_err| NoDeviceFound)?
        .sample_format())
}

/// Creates an error callback function for audio stream errors.
///
/// # Returns
///
/// A function that handles audio stream errors.
fn create_error_callback() -> impl Fn(StreamError) {
    move |err: StreamError| {
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

/// Updates the playback position based on consumed samples.
///
/// # Arguments
///
/// * `samples_consumed` - Number of samples actually consumed.
/// * `channels` - Number of audio channels.
/// * `sample_rate` - Sample rate in Hz.
/// * `position` - Shared atomic for tracking playback position.
fn update_position(
    samples_consumed: usize,
    channels: usize,
    sample_rate: u64,
    position: &Arc<AtomicU64>,
) {
    if samples_consumed > 0 {
        let frames = samples_consumed / channels;
        let duration_ms = (frames as u64 * 1000) / sample_rate;
        position.fetch_add(duration_ms, SeqCst);
    }
}

/// Processes samples in F32 format and fills the output buffer.
///
/// # Arguments
///
/// * `data` - Output buffer to fill.
/// * `consumer` - Ring buffer consumer for audio samples.
/// * `channels` - Number of audio channels.
/// * `sample_rate` - Sample rate in Hz.
/// * `position` - Shared atomic for tracking playback position.
///
/// # Returns
///
/// The number of samples consumed.
fn process_samples_f32(
    data: &mut [f32],
    consumer: &mut Consumer<f32>,
    channels: usize,
    sample_rate: u64,
    position: &Arc<AtomicU64>,
) -> usize {
    let mut samples_consumed = 0;
    for sample in data.iter_mut() {
        match consumer.pop() {
            Ok(value) => {
                *sample = value.clamp(-1.0, 1.0);
                samples_consumed += 1;
            }
            Err(Empty) => {
                *sample = 0.0;
            }
        }
    }

    update_position(samples_consumed, channels, sample_rate, position);
    samples_consumed
}

/// Processes samples in I16 format and fills the output buffer.
///
/// # Arguments
///
/// * `data` - Output buffer to fill.
/// * `consumer` - Ring buffer consumer for audio samples.
/// * `channels` - Number of audio channels.
/// * `sample_rate` - Sample rate in Hz.
/// * `position` - Shared atomic for tracking playback position.
///
/// # Returns
///
/// The number of samples consumed.
fn process_samples_i16(
    data: &mut [i16],
    consumer: &mut Consumer<f32>,
    channels: usize,
    sample_rate: u64,
    position: &Arc<AtomicU64>,
) -> usize {
    let mut samples_consumed = 0;
    for sample in data.iter_mut() {
        match consumer.pop() {
            Ok(value) => {
                let clamped = value.clamp(-1.0, 1.0);
                let scaled = clamped * f32::from(i16::MAX);
                *sample = scaled
                    .clamp(f32::from(i16::MIN), f32::from(i16::MAX))
                    .to_i16()
                    .unwrap_or(0);
                samples_consumed += 1;
            }
            Err(Empty) => {
                *sample = 0;
            }
        }
    }

    update_position(samples_consumed, channels, sample_rate, position);
    samples_consumed
}

/// Processes samples in U16 format and fills the output buffer.
///
/// # Arguments
///
/// * `data` - Output buffer to fill.
/// * `consumer` - Ring buffer consumer for audio samples.
/// * `channels` - Number of audio channels.
/// * `sample_rate` - Sample rate in Hz.
/// * `position` - Shared atomic for tracking playback position.
///
/// # Returns
///
/// The number of samples consumed.
fn process_samples_u16(
    data: &mut [u16],
    consumer: &mut Consumer<f32>,
    channels: usize,
    sample_rate: u64,
    position: &Arc<AtomicU64>,
) -> usize {
    let mut samples_consumed = 0;
    for sample in data.iter_mut() {
        match consumer.pop() {
            Ok(value) => {
                let clamped = value.clamp(-1.0, 1.0);

                // Formula (clamped + 1.0) * f32::from(u16::MAX) / 2.0 always yields [0.0, 65535.0]
                // Clamp to valid range and use to_u16 with fallback for safe conversion
                let scaled = (clamped + 1.0) * f32::from(u16::MAX) / 2.0;
                let clamped_scaled = scaled.clamp(0.0, f32::from(u16::MAX));
                *sample = clamped_scaled.to_u16().unwrap_or(32768);
                samples_consumed += 1;
            }
            Err(Empty) => {
                *sample = 32768;
            }
        }
    }

    update_position(samples_consumed, channels, sample_rate, position);
    samples_consumed
}

/// Creates a resampling stream for audio output.
///
/// This function creates a CPAL audio stream that reads from a resampled ring buffer.
///
/// # Arguments
///
/// * `output` - The audio output device.
/// * `resampled_consumer` - Ring buffer consumer for resampled audio data.
/// * `target_config` - Target stream configuration.
/// * `current_position` - Shared atomic for tracking actual playback position.
///
/// # Returns
///
/// A `Result` containing the CPAL stream or an error.
///
/// # Errors
///
/// Returns `OutputError` if the device configuration cannot be queried or the stream cannot be created.
pub fn create_resampling_stream(
    output: &AudioOutput,
    mut resampled_consumer: Consumer<f32>,
    target_config: &StreamConfig,
    current_position: &Arc<AtomicU64>,
) -> Result<Stream, OutputError> {
    let sample_format = get_sample_format(output)?;
    let err_fn = create_error_callback();
    let timeout = Duration::from_millis(u64::from(output.config().buffer_duration_ms));
    let channels = target_config.channels as usize;
    let sample_rate = u64::from(target_config.sample_rate);
    let position = Arc::clone(current_position);

    let stream = match sample_format {
        SampleFormat::F32 => output.device().build_output_stream(
            target_config,
            move |data: &mut [f32], _: &OutputCallbackInfo| {
                process_samples_f32(
                    data,
                    &mut resampled_consumer,
                    channels,
                    sample_rate,
                    &position,
                );
            },
            err_fn,
            Some(timeout),
        )?,
        SampleFormat::I16 => output.device().build_output_stream(
            target_config,
            move |data: &mut [i16], _: &OutputCallbackInfo| {
                process_samples_i16(
                    data,
                    &mut resampled_consumer,
                    channels,
                    sample_rate,
                    &position,
                );
            },
            err_fn,
            Some(timeout),
        )?,
        SampleFormat::U16 => output.device().build_output_stream(
            target_config,
            move |data: &mut [u16], _: &OutputCallbackInfo| {
                process_samples_u16(
                    data,
                    &mut resampled_consumer,
                    channels,
                    sample_rate,
                    &position,
                );
            },
            err_fn,
            Some(timeout),
        )?,
        _ => {
            return Err(UnsupportedSampleFormat {
                format: sample_format,
            });
        }
    };

    Ok(stream)
}

#[cfg(test)]
mod tests {
    use crate::audio::resampler::{
        AudioResampler, ResamplingError::InvalidConfiguration, calculate_chunk_size, gcd,
    };

    #[test]
    fn test_gcd_calculation() {
        assert_eq!(gcd(44100, 48000), 300);
        assert_eq!(gcd(96000, 48000), 48000);
        assert_eq!(gcd(192_000, 48000), 48000);
    }

    #[test]
    fn test_chunk_size_calculation() {
        let chunk_size = calculate_chunk_size(44100, 48000);
        assert!((256..=8192).contains(&chunk_size));

        let chunk_size = calculate_chunk_size(192_000, 48000);
        assert!((256..=8192).contains(&chunk_size));
    }

    #[test]
    fn test_invalid_resampler_creation() {
        let result = AudioResampler::new(0, 48000, 2);
        assert!(matches!(result, Err(InvalidConfiguration(_))));

        let result = AudioResampler::new(44100, 44100, 2);
        assert!(matches!(result, Err(InvalidConfiguration(_))));
    }
}
