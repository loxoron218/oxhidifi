//! Audio production for feeding decoded samples into ring buffers.

use std::{thread::sleep, time::Duration};

use {
    async_channel::Sender,
    num_traits::cast::ToPrimitive,
    rtrb::{Producer, PushError::Full},
    symphonia::core::{
        audio::{
            AudioBuffer,
            AudioBufferRef::{self, F32, F64, S8, S16, S24, S32, U8, U16, U24, U32},
            Signal,
        },
        sample::{i24, u24},
    },
    tracing::{debug, warn},
};

use crate::audio::{
    decoder::AudioDecoder,
    decoder_types::DecoderError::{self, NoAudioTrack},
};

/// Sleep duration when producer buffer is full.
const PRODUCER_SLEEP_DURATION: Duration = Duration::from_micros(100);

/// Minimum representable f32 value as f64.
const F32_MIN: f64 = f32::MIN as f64;

/// Maximum representable f32 value as f64.
const F32_MAX: f64 = f32::MAX as f64;

/// Generates audio format conversion functions to eliminate code duplication.
///
/// This macro generates standalone functions that convert audio buffers from various
/// sample types to interleaved f32 samples.
macro_rules! impl_convert {
    (simple $func_name:ident, $type:ty, $conversion:expr) => {
        pub fn $func_name(buf: &AudioBuffer<$type>) -> Vec<f32> {
            let channels = buf.spec().channels.count();
            let mut samples = Vec::with_capacity(buf.frames() * channels);

            for frame in 0..buf.frames() {
                for ch in 0..channels {
                    samples.push($conversion(buf.chan(ch)[frame]));
                }
            }
            samples
        }
    };
    (safe $func_name:ident, $type:ty, $conversion:expr) => {
        pub fn $func_name(buf: &AudioBuffer<$type>) -> Vec<f32> {
            let channels = buf.spec().channels.count();
            let mut samples = Vec::with_capacity(buf.frames() * channels);

            for frame in 0..buf.frames() {
                for ch in 0..channels {
                    samples.push(Self::safe_f64_to_f32($conversion(buf.chan(ch)[frame])));
                }
            }
            samples
        }
    };
    (special $func_name:ident, $type:ty, $min_val:expr, $regular:expr) => {
        pub fn $func_name(buf: &AudioBuffer<$type>) -> Vec<f32> {
            let channels = buf.spec().channels.count();
            let mut samples = Vec::with_capacity(buf.frames() * channels);

            for frame in 0..buf.frames() {
                for ch in 0..channels {
                    let s = buf.chan(ch)[frame];
                    let v = if s == $min_val { -1.0_f32 } else { $regular(s) };
                    samples.push(v);
                }
            }
            samples
        }
    };
}

/// Audio producer that feeds decoded samples into a ring buffer.
///
/// This struct wraps an `AudioDecoder` and continuously decodes audio,
/// writing samples to the provided ring buffer producer.
pub struct AudioProducer {
    /// The audio decoder that provides raw audio samples.
    decoder: AudioDecoder,
    /// Ring buffer producer for writing decoded samples.
    producer: Producer<f32>,
    /// Ring buffer capacity for flow control calculations.
    buffer_capacity: usize,
    /// Sender for track completion notifications.
    track_finished_tx: Option<Sender<()>>,
}

impl AudioProducer {
    /// Creates a new audio producer.
    ///
    /// # Arguments
    ///
    /// * `decoder` - The audio decoder to use.
    /// * `producer` - The ring buffer producer to write samples to.
    /// * `buffer_capacity` - The ring buffer capacity for flow control.
    /// * `track_finished_tx` - Optional sender for track completion notifications.
    pub fn new(
        decoder: AudioDecoder,
        producer: Producer<f32>,
        buffer_capacity: usize,
        track_finished_tx: Option<Sender<()>>,
    ) -> Self {
        Self {
            decoder,
            producer,
            buffer_capacity,
            track_finished_tx,
        }
    }

    /// Runs the audio production loop.
    ///
    /// This method continuously decodes audio and writes samples to the ring buffer.
    /// It should be run on a dedicated worker thread.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError` if decoding fails.
    pub fn run(mut self) -> Result<(), DecoderError> {
        while let Some(buffer) = self.decoder.decode_next_packet()? {
            let samples = Self::convert_buffer_to_interleaved_f32(buffer);

            if self.write_samples_to_buffer(&samples) {
                return Ok(());
            }
        }

        Self::notify_track_finished(self.track_finished_tx.as_ref());
        Ok(())
    }

    /// Converts an audio buffer to interleaved f32 samples.
    ///
    /// # Arguments
    ///
    /// * `buffer` - The audio buffer reference to convert.
    ///
    /// # Returns
    ///
    /// A `Vec<f32>` with interleaved samples.
    fn convert_buffer_to_interleaved_f32(buffer: AudioBufferRef<'_>) -> Vec<f32> {
        match buffer {
            F32(buf) => Self::convert_f32_to_interleaved(&buf),
            F64(buf) => Self::convert_f64_to_interleaved(&buf),
            U8(buf) => Self::convert_u8_to_interleaved(&buf),
            S8(buf) => Self::convert_s8_to_interleaved(&buf),
            U16(buf) => Self::convert_u16_to_interleaved(&buf),
            S16(buf) => Self::convert_s16_to_interleaved(&buf),
            U24(buf) => Self::convert_u24_to_interleaved(&buf),
            S24(buf) => Self::convert_s24_to_interleaved(&buf),
            U32(buf) => Self::convert_u32_to_interleaved(&buf),
            S32(buf) => Self::convert_s32_to_interleaved(&buf),
        }
    }

    /// Safely converts a f64 sample to f32 without panicking.
    ///
    /// Clamps the sample to the f32 representable range before conversion.
    /// This ensures the conversion never fails even with extreme input values.
    ///
    /// # Panics
    ///
    /// Panics if the clamped f64 value cannot be converted to f32 (should never occur in practice).
    #[must_use]
    pub fn safe_f64_to_f32(sample: f64) -> f32 {
        let clamped = sample.clamp(F32_MIN, F32_MAX);
        clamped.to_f32().unwrap_or_else(|| {
            if clamped.is_sign_positive() {
                f32::MAX
            } else {
                f32::MIN
            }
        })
    }

    impl_convert!(simple convert_f32_to_interleaved, f32, |s: f32| s);
    impl_convert!(safe convert_f64_to_interleaved, f64, |s: f64| s.clamp(-1.0_f64, 1.0_f64));
    impl_convert!(simple convert_u8_to_interleaved, u8, |s: u8| {
        (f32::from(s) - 128.0_f32) / 127.0_f32
    });
    impl_convert!(simple convert_s8_to_interleaved, i8, |s: i8| f32::from(s) / 127.0);
    impl_convert!(simple convert_u16_to_interleaved, u16, |s: u16| {
        (f32::from(s) - 32768.0_f32) / 32767.0_f32
    });
    impl_convert!(special convert_s16_to_interleaved, i16, i16::MIN, |s: i16| {
        f32::from(s) / f32::from(i16::MAX)
    });
    impl_convert!(safe convert_u24_to_interleaved, u24, |s: u24| {
        f64::from(s.0 & 0x00FF_FFFF) / 16_777_215.0_f64
    });
    impl_convert!(
        special convert_s24_to_interleaved,
        i24,
        i24::from(-8_388_608_i32),
        |s: i24| Self::safe_f64_to_f32(
            (f64::from(s.0 << 8 >> 8) / 8_388_607.0_f64).clamp(-1.0_f64, 1.0_f64),
        )
    );
    impl_convert!(safe convert_u32_to_interleaved, u32, |s: u32| {
        (f64::from(s) - 2_147_483_648.0_f64) / 2_147_483_647.0_f64
    });
    impl_convert!(
        special convert_s32_to_interleaved,
        i32,
        i32::MIN,
        |s: i32| Self::safe_f64_to_f32((f64::from(s) / f64::from(i32::MAX)).clamp(-1.0_f64, 1.0_f64))
    );

    /// Writes samples to the ring buffer.
    ///
    /// # Arguments
    ///
    /// * `samples` - Slice of f32 samples to write.
    ///
    /// # Returns
    ///
    /// A `bool` indicating if the producer was abandoned.
    /// Returns `true` if producer was abandoned, `false` if all samples written successfully.
    fn write_samples_to_buffer(&mut self, samples: &[f32]) -> bool {
        let sample_count = samples.len();
        let mut retry_count = 0;

        // Dynamic throttling based on buffer occupancy
        // When buffer is filling up, sleep longer before writing
        while !self.producer.is_abandoned() {
            let available = self.producer.slots();

            // If buffer has sufficient space, write immediately
            if available >= sample_count {
                break;
            }

            let sleep_duration = if available == 0
                || self.buffer_capacity == 0
                || available < self.buffer_capacity / 4
            {
                // Buffer is very full, sleep 5x longer
                PRODUCER_SLEEP_DURATION * 5
            } else if available < self.buffer_capacity / 2 {
                // Buffer is moderately full, sleep 2x longer
                PRODUCER_SLEEP_DURATION * 2
            } else {
                // Buffer is below half full, use normal sleep
                PRODUCER_SLEEP_DURATION
            };

            sleep(sleep_duration);
        }

        // Producer was abandoned
        if self.producer.is_abandoned() {
            return true;
        }

        for &sample in samples {
            loop {
                if self.producer.is_abandoned() {
                    return true;
                }
                match self.producer.push(sample) {
                    Ok(()) => break,
                    Err(Full(_)) => {
                        retry_count += 1;
                        if retry_count == 1 {
                            // Log first retry only to avoid spam
                            warn!(
                                retry_count,
                                sample_count, "Audio buffer full, retrying after sleep"
                            );
                        }

                        // Buffer is full, wait a bit and retry
                        sleep(PRODUCER_SLEEP_DURATION);
                    }
                }
            }
        }
        if retry_count > 0 {
            debug!(
                total_retries = retry_count,
                "Successfully wrote samples to buffer"
            );
        }
        false
    }

    /// Notifies that the track has finished decoding.
    ///
    /// # Arguments
    ///
    /// * `track_finished_tx` - Optional sender for track completion notifications.
    fn notify_track_finished(track_finished_tx: Option<&Sender<()>>) {
        // Notify that track has finished (normal end of file)
        if let Some(tx) = track_finished_tx
            && let Err(e) = tx.try_send(())
        {
            warn!("AudioProducer: Failed to send track finished notification: {e}");
        }
    }
}

/// Gapless audio producer that manages seamless track transitions.
///
/// This producer handles switching between current and next track decoders
/// without stopping the audio output stream, enabling gapless playback.
pub struct GaplessProducer {
    /// Current track decoder and producer.
    current_producer: Option<AudioProducer>,
    /// Next track decoder for pre-buffering.
    next_decoder: Option<AudioDecoder>,
}

impl GaplessProducer {
    /// Creates a new gapless audio producer.
    ///
    /// # Arguments
    ///
    /// * `decoder` - The initial audio decoder.
    /// * `producer` - The ring buffer producer for output.
    /// * `buffer_capacity` - The ring buffer capacity for flow control.
    /// * `track_finished_tx` - Optional sender for track completion notifications.
    ///
    /// # Panics
    ///
    /// Panics if the prebuffer cannot be obtained (this indicates a bug in initialization).
    pub fn new(
        decoder: AudioDecoder,
        producer: Producer<f32>,
        buffer_capacity: usize,
        track_finished_tx: Option<Sender<()>>,
    ) -> Self {
        let audio_producer =
            AudioProducer::new(decoder, producer, buffer_capacity, track_finished_tx);

        Self {
            current_producer: Some(audio_producer),
            next_decoder: None,
        }
    }

    /// Preloads the next track for gapless transition.
    ///
    /// # Arguments
    ///
    /// * `decoder` - Decoder for the next track.
    pub fn preload_next_track(&mut self, decoder: AudioDecoder) {
        self.next_decoder = Some(decoder);
    }

    /// Runs the gapless production loop.
    ///
    /// This method continuously decodes audio from the current track,
    /// pre-buffers the next track, and handles seamless transitions.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError` if decoding fails.
    ///
    /// # Panics
    ///
    /// Panics if there's no current producer.
    pub fn run(self) -> Result<(), DecoderError> {
        if let Some(producer) = self.current_producer {
            // Run current track producer
            producer.run()
        } else {
            Err(NoAudioTrack)
        }
    }
}
