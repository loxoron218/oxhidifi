//! Ring buffer push helpers for the playback pipeline.
//!
//! Extracted from `pipeline.rs` to keep each file under 400 lines.

use std::{thread::sleep, time::Duration};

use rtrb::{Producer, PushError::Full};

use crate::playback::{
    resampler::converter::AudioResampler,
    state::PlaybackEvent::{self, Error},
};

/// Push a single sample, retrying on full.
pub fn push_sample(sample: f32, producer: &mut Producer<f32>) -> bool {
    let mut s = sample;
    loop {
        match producer.push(s) {
            Ok(()) => return true,
            Err(Full(val)) => {
                s = val;
            }
        }
        if producer.is_abandoned() {
            return false;
        }
        sleep(Duration::from_millis(1));
    }
}

/// Push samples to the ring buffer.
pub fn push_samples(samples: &[f32], producer: &mut Producer<f32>) {
    for sample in samples {
        if !push_sample(*sample, producer) {
            return;
        }
    }
}

/// Push samples through a resampler.
pub fn process_resampler(
    r: &mut AudioResampler,
    samples: &[f32],
    producer: &mut Producer<f32>,
) -> Option<String> {
    r.push_input(samples);
    while r.has_pending_output() {
        match r.process() {
            Ok(Some(output)) => push_samples(output, producer),
            Ok(None) => break,
            Err(e) => return Some(format!("Resampler error: {e}")),
        }
    }
    None
}

/// Push one decoded interleaved batch through the resampler (if any) into
/// the ring buffer producer.
///
/// Returns a [`PlaybackEvent::Error`] if resampling fails.
pub fn process_decoded_batch(
    samples: &[f32],
    resampler: &mut Option<AudioResampler>,
    producer: &mut Producer<f32>,
) -> Option<PlaybackEvent> {
    let error = if let Some(r) = resampler {
        process_resampler(r, samples, producer)
    } else {
        push_samples(samples, producer);
        None
    };
    error.map(|e| Error { error: e })
}
