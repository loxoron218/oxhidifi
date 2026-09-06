//! CPAL output stream construction and ring buffer draining.

use std::sync::{
    Arc,
    atomic::{
        AtomicBool, AtomicU32,
        Ordering::{Acquire, Relaxed},
    },
};

use {
    cpal::{
        Device, FromSample, OutputCallbackInfo, SizedSample, Stream, StreamConfig,
        traits::DeviceTrait,
    },
    rtrb::Consumer,
    tracing::error,
};

use crate::{
    metrics::GLOBAL_PLAYBACK_LATENCY,
    playback::{
        OutputError::{self, StreamConfigError},
        volume::volume_to_gain_f32,
    },
};

/// Drain all samples from the ring buffer consumer.
pub fn drain_consumer(consumer: &mut Consumer<f32>) {
    while consumer.pop().is_ok() {}
}

/// Fill a single output sample from the consumer, handling silence and latency.
fn fill_output_sample<T: SizedSample + FromSample<f32>>(
    sample: &mut T,
    consumer: &mut Consumer<f32>,
    first_sample: &mut bool,
    vol: f32,
) {
    let (value, is_ok) = consumer.pop().map_or((0.0, false), |v| (v, true));
    if is_ok && !*first_sample {
        *first_sample = true;
        GLOBAL_PLAYBACK_LATENCY.record_first_sample();
    }
    let out = if is_ok { value * vol } else { 0.0 };
    *sample = T::from_sample(out);
}

/// Build a cpal output stream for the given sample type.
///
/// # Errors
///
/// Returns [`OutputError`] if the output stream cannot be created.
pub fn build_stream<T: SizedSample + FromSample<f32>>(
    device: &Device,
    config: &StreamConfig,
    mut consumer: Consumer<f32>,
    flush_flag: Arc<AtomicBool>,
    device_lost: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
) -> Result<Stream, OutputError> {
    let stream = device
        .build_output_stream(
            *config,
            move |data: &mut [T], _: &OutputCallbackInfo| {
                if flush_flag.swap(false, Acquire) {
                    drain_consumer(&mut consumer);
                }
                let slider = f32::from_bits(volume.load(Relaxed));
                let gain = volume_to_gain_f32(slider);
                let mut first_sample = false;
                for sample in data.iter_mut() {
                    fill_output_sample(sample, &mut consumer, &mut first_sample, gain);
                }
            },
            move |err| {
                device_lost.store(true, Relaxed);
                error!(
                    error = %err,
                    "Audio output stream error \u{2014} device may be disconnected",
                );
            },
            None,
        )
        .map_err(|e| StreamConfigError(e.to_string()))?;

    Ok(stream)
}
