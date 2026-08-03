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

use crate::playback::OutputError::{self, StreamConfigError};

/// Drain all samples from the ring buffer consumer.
pub fn drain_consumer(consumer: &mut Consumer<f32>) {
    while consumer.pop().is_ok() {}
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
                let vol = f32::from_bits(volume.load(Relaxed));
                for sample in data.iter_mut() {
                    let s: f32 = consumer.pop().unwrap_or(0.0);
                    *sample = T::from_sample(s * vol);
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
