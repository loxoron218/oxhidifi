//! High-quality sample rate conversion for audio playback.
//!
//! This module provides real-time sample rate conversion using the `rubato` crate,
//! which implements high-quality resampling algorithms suitable for professional audio.

use std::{
    error::Error,
    fmt::{Display, Formatter, Result as StdResult},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    thread::{JoinHandle, sleep, spawn, yield_now},
    time::Duration,
};

use {
    cpal::{OutputCallbackInfo, SampleFormat, Stream, StreamConfig, traits::DeviceTrait},
    rtrb::{Consumer, PopError::Empty, Producer, PushError::Full},
    rubato::{FftFixedIn, Resampler},
    tracing::{debug, error, info},
};

use crate::audio::{
    decoder::AudioFormat,
    output::{
        AudioOutput,
        OutputError::{self, NoDeviceFound, UnsupportedSampleFormat},
    },
};

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
            ResamplingError::RubatoError(msg) => write!(f, "Rubato error: {}", msg),
            ResamplingError::RingBufferError(msg) => write!(f, "Ring buffer error: {}", msg),
            ResamplingError::InvalidConfiguration(msg) => {
                write!(f, "Invalid configuration: {}", msg)
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
    resampler: FftFixedIn<f32>,
    /// Source sample rate in Hz.
    source_rate: u32,
    /// Target sample rate in Hz.
    target_rate: u32,
    /// Number of channels.
    channels: usize,
    /// Fixed input chunk size per channel expected by rubato.
    chunk_size: usize,
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

        let resampler = FftFixedIn::<f32>::new(
            source_rate as usize,
            target_rate as usize,
            chunk_size,
            1, // sub_chunks
            channels,
        )
        .map_err(|e| ResamplingError::RubatoError(e.to_string()))?;

        info!(
            "Created resampler: {} Hz -> {} Hz, {} channels, chunk size: {}",
            source_rate, target_rate, channels, chunk_size
        );

        Ok(AudioResampler {
            resampler,
            source_rate,
            target_rate,
            channels,
            chunk_size,
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
        let needed_frames = self.chunk_size;
        let mut available_frames = self.input_buffer.len() / ch;

        while available_frames >= needed_frames {
            // Deinterleave exactly chunk_size frames per channel using iterators
            let frame_samples = &self.input_buffer[..needed_frames * ch];
            let mut planar_in: Vec<Vec<f32>> =
                (0..ch).map(|_| Vec::with_capacity(needed_frames)).collect();
            for frame in frame_samples.chunks_exact(ch) {
                for (c, plane) in planar_in.iter_mut().enumerate() {
                    plane.push(frame[c]);
                }
            }

            let in_refs: Vec<&[f32]> = planar_in.iter().map(|v| v.as_slice()).collect();
            let planar_out = self
                .resampler
                .process(&in_refs, None)
                .map_err(|e| ResamplingError::RubatoError(e.to_string()))?;

            if !planar_out.is_empty() {
                // Reinterleave output without index-based loops
                let mut iters: Vec<_> = planar_out.iter().map(|v| v.iter()).collect();
                loop {
                    let mut frame_complete = true;
                    for it in iters.iter_mut() {
                        match it.next() {
                            Some(&s) => self.output_buffer.push(s),
                            None => {
                                frame_complete = false;
                                break;
                            }
                        }
                    }
                    if !frame_complete {
                        break;
                    }
                }
            }

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
    pub fn expected_output_size(&self, input_size: usize) -> usize {
        let in_rate = self.source_rate as u64;
        let out_rate = self.target_rate as u64;
        ((input_size as u64 * out_rate) / in_rate) as usize
    }
}

/// Calculates an appropriate chunk size for resampling based on sample rates.
fn calculate_chunk_size(source_rate: u32, target_rate: u32) -> usize {
    // Find GCD to get a reasonable chunk size
    let gcd = gcd(source_rate, target_rate);
    let lcm = (source_rate as u64 * target_rate as u64) / gcd as u64;

    // Use a chunk size that's a multiple of both rates' relationship
    // but keep it reasonable for real-time processing
    let base_chunk = (lcm / source_rate as u64).min(4096) as usize;

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
    ) -> Result<Self, ResamplingError> {
        let source_rate = source_format.sample_rate;
        let target_rate = target_config.sample_rate;
        let channels = target_config.channels as usize;

        let resampler = AudioResampler::new(source_rate, target_rate, channels)?;

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);

        // Start resampling thread
        let thread_handle = Some(spawn(move || {
            resampling_loop(
                source_consumer,
                target_producer,
                resampler,
                running_clone,
                channels,
            );
        }));

        Ok(ResamplingAudioConsumer {
            running,
            thread_handle,
            target_config,
        })
    }

    /// Gets the target stream configuration.
    pub fn target_config(&self) -> &StreamConfig {
        &self.target_config
    }

    /// Stops the resampling consumer gracefully.
    pub fn stop(&mut self) {
        if let Some(handle) = self.thread_handle.take() {
            self.running.store(false, Relaxed);
            let _ = handle.join();
        }
    }
}

impl Drop for ResamplingAudioConsumer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Main resampling loop that runs in a dedicated thread.
fn resampling_loop(
    mut source_consumer: Consumer<f32>,
    mut target_producer: Producer<f32>,
    mut resampler: AudioResampler,
    running: Arc<AtomicBool>,
    channels: usize,
) {
    const INPUT_BUFFER_SIZE: usize = 4096;
    let mut input_buffer = Vec::with_capacity(INPUT_BUFFER_SIZE);
    let mut output_buffer = Vec::new();

    while running.load(Relaxed) {
        // Read available samples from source
        input_buffer.clear();
        let mut samples_read = 0;

        while samples_read < INPUT_BUFFER_SIZE {
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
            yield_now();
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
                        match target_producer.push(sample) {
                            Ok(()) => break,
                            Err(Full(_)) => {
                                // Target buffer is full, wait briefly
                                sleep(Duration::from_micros(50));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Resampling error: {}", e);

                // Continue processing despite errors to avoid complete failure
                // Write silence to maintain timing
                let expected_output_size =
                    resampler.expected_output_size(input_buffer.len() / channels);
                for _ in 0..(expected_output_size * channels) {
                    loop {
                        match target_producer.push(0.0) {
                            Ok(()) => break,
                            Err(Full(_)) => {
                                sleep(Duration::from_micros(50));
                            }
                        }
                    }
                }
            }
        }
    }

    debug!("Resampling loop stopped");
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
///
/// # Returns
///
/// A `Result` containing the CPAL stream or an error.
pub fn create_resampling_stream(
    output: &AudioOutput,
    mut resampled_consumer: Consumer<f32>,
    target_config: StreamConfig,
) -> Result<Stream, OutputError> {
    let sample_format = output
        .device()
        .default_output_config()
        .map_err(|_| NoDeviceFound)?
        .sample_format();

    let err_fn = |err| {
        eprintln!("Audio stream error: {}", err);
    };

    let timeout = Duration::from_millis(output.config().buffer_duration_ms as u64);

    let stream = match sample_format {
        SampleFormat::F32 => output.device().build_output_stream(
            &target_config,
            move |data: &mut [f32], _: &OutputCallbackInfo| {
                for sample in data.iter_mut() {
                    match resampled_consumer.pop() {
                        Ok(value) => {
                            *sample = value.clamp(-1.0, 1.0);
                        }
                        Err(Empty) => {
                            *sample = 0.0;
                        }
                    }
                }
            },
            err_fn,
            Some(timeout),
        )?,
        SampleFormat::I16 => output.device().build_output_stream(
            &target_config,
            move |data: &mut [i16], _: &OutputCallbackInfo| {
                for sample in data.iter_mut() {
                    match resampled_consumer.pop() {
                        Ok(value) => {
                            let clamped = value.clamp(-1.0, 1.0);
                            *sample = (clamped * i16::MAX as f32) as i16;
                        }
                        Err(Empty) => {
                            *sample = 0;
                        }
                    }
                }
            },
            err_fn,
            Some(timeout),
        )?,
        SampleFormat::U16 => output.device().build_output_stream(
            &target_config,
            move |data: &mut [u16], _: &OutputCallbackInfo| {
                for sample in data.iter_mut() {
                    match resampled_consumer.pop() {
                        Ok(value) => {
                            let clamped = value.clamp(-1.0, 1.0);
                            *sample = ((clamped + 1.0) * (u16::MAX as f32) / 2.0) as u16;
                        }
                        Err(Empty) => {
                            *sample = 32768;
                        }
                    }
                }
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
    use crate::audio::resampler::{AudioResampler, ResamplingError::InvalidConfiguration};

    #[test]
    fn test_gcd_calculation() {
        assert_eq!(gcd(44100, 48000), 300);
        assert_eq!(gcd(96000, 48000), 48000);
        assert_eq!(gcd(192000, 48000), 48000);
    }

    #[test]
    fn test_chunk_size_calculation() {
        let chunk_size = calculate_chunk_size(44100, 48000);
        assert!(chunk_size >= 256 && chunk_size <= 8192);

        let chunk_size = calculate_chunk_size(192000, 48000);
        assert!(chunk_size >= 256 && chunk_size <= 8192);
    }

    #[test]
    fn test_invalid_resampler_creation() {
        let result = AudioResampler::new(0, 48000, 2);
        assert!(matches!(result, Err(InvalidConfiguration(_))));

        let result = AudioResampler::new(44100, 44100, 2);
        assert!(matches!(result, Err(InvalidConfiguration(_))));
    }
}
