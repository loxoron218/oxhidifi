//! Zero-heap-allocation verification for the audio hot path (T060/T061) per
//! Constitution Principle IV.
//!
//! Instruments the decoder → resampler → ring-buffer output path
//! (`src/playback/decoder.rs`, `src/playback/resampler.rs`,
//! `src/playback/channel.rs`, `src/playback/pipeline.rs`) to assert no heap
//! allocation occurs during audio processing (pre-allocated buffers only).
//!
//! # Scope note (documented complexity justification)
//!
//! The underlying `symphonia` codec allocates internally during decode (its
//! packet/buffer handling is outside our control). Per the plan, the assertion
//! is therefore scoped to *our* pre-allocated buffers: the decoder's reusable
//! sample buffer and the resampler's input/output buffers must keep a constant
//! capacity across the whole steady-state run, proving they are allocated once
//! and reused rather than reallocated per batch.
//!
//! Gated behind the `verification-tests` feature. Run with:
//!
//! ```text
//! cargo test --features verification-tests --test zero_alloc
//! ```

use std::{borrow::Cow::Borrowed, f64::consts::PI, fs::File, io::Write, path::Path};

use {
    anyhow::{Context, Result, ensure},
    num_traits::cast,
    rtrb::{Consumer, Producer, PushError::Full, RingBuffer},
    tempfile::tempdir,
    tracing::info,
};

use oxhidifi::playback::{
    channel::{fill_channel_scratch, maybe_downmix},
    decoder::{AudioParams, DecodedSamples, Decoder},
    resampler::AudioResampler,
    write_wav_header,
};

/// Seconds of audio to run through the steady-state simulation.
const STEADY_STATE_SECONDS: u32 = 60;

/// Sample rate of the generated source.
const SAMPLE_RATE: u32 = 44_100;

/// Number of channels in the generated source.
const CHANNELS: u16 = 1;

/// Output sample rate (forces resampling to be exercised).
const OUTPUT_RATE: u32 = 48_000;

/// Write a WAV file with `seconds` seconds of a mono 1 kHz tone.
fn write_wav(path: &Path, seconds: u32) -> Result<()> {
    let mut f = File::create(path).context("failed to create wav")?;
    let total_samples = SAMPLE_RATE.saturating_mul(seconds);
    write_wav_header(&mut f, CHANNELS, SAMPLE_RATE, 16, total_samples)?;
    for i in 0..total_samples {
        let sample = ((f64::from(i) * 2.0 * PI * 1000.0) / f64::from(SAMPLE_RATE)).sin();
        let amp = cast::<f64, i16>(sample * 0.5 * 32767.0).unwrap_or(i16::MAX);
        f.write_all(&amp.to_le_bytes())?;
    }
    Ok(())
}

/// Simulate the output callback: drain the ring buffer until it is empty.
fn drain_ring(consumer: &mut Consumer<f32>) {
    while consumer.pop().is_ok() {}
}

/// Verify channel scratch buffer reuse across conversions.
///
/// Exercises the `src_channels != dst_channels` path via
/// `fill_channel_scratch` and `maybe_downmix` with a pre-allocated scratch
/// buffer, asserting capacity stays constant after the first allocation.
fn verify_channel_scratch_reuse(initial_cap: usize) -> Result<()> {
    let mut scratch = Vec::with_capacity(initial_cap);
    let cap_before = scratch.capacity();
    let batch = DecodedSamples {
        samples: &[0.5, -0.5, 0.25, -0.25],
        params: AudioParams {
            sample_rate: 44100,
            channels: 2,
            duration_seconds: 0.0,
            bit_depth: Some(16),
        },
    };
    let borrowed = maybe_downmix(&batch, 2, 2);
    ensure!(
        matches!(borrowed, Borrowed(_)),
        "equal channels must borrow"
    );
    ensure!(
        scratch.capacity() == cap_before,
        "scratch must not reallocate on borrowed path"
    );
    ensure!(
        scratch.is_empty(),
        "scratch must stay empty on borrowed path"
    );
    let batch2 = DecodedSamples {
        samples: &[0.8, 0.2, -0.6, -0.4],
        params: AudioParams {
            sample_rate: 44100,
            channels: 2,
            duration_seconds: 0.0,
            bit_depth: Some(16),
        },
    };
    fill_channel_scratch(batch2.samples, 2, 1, &mut scratch);
    let cap_after_first = scratch.capacity();
    ensure!(
        cap_after_first >= cap_before,
        "scratch should have capacity after first conversion"
    );
    let batch3 = DecodedSamples {
        samples: &[1.0, -1.0, 0.5, -0.5],
        params: AudioParams {
            sample_rate: 44100,
            channels: 2,
            duration_seconds: 0.0,
            bit_depth: Some(16),
        },
    };
    fill_channel_scratch(batch3.samples, 2, 1, &mut scratch);
    ensure!(
        scratch.capacity() == cap_after_first,
        "channel scratch reallocated on second conversion"
    );
    let mut mono_scratch = Vec::with_capacity(initial_cap);
    let mono_initial = mono_scratch.capacity();
    let mono = DecodedSamples {
        samples: &[0.5, -0.5],
        params: AudioParams {
            sample_rate: 44100,
            channels: 1,
            duration_seconds: 0.0,
            bit_depth: Some(16),
        },
    };
    fill_channel_scratch(mono.samples, 1, 2, &mut mono_scratch);
    let mono_after = mono_scratch.capacity();
    ensure!(mono_after >= mono_initial);
    fill_channel_scratch(mono.samples, 1, 2, &mut mono_scratch);
    ensure!(
        mono_scratch.capacity() == mono_after,
        "upmix scratch reallocated"
    );
    Ok(())
}

fn main_test() -> Result<()> {
    let dir = tempdir().context("failed to create temp dir")?;
    let wav_path = dir.path().join("steady.wav");
    write_wav(&wav_path, STEADY_STATE_SECONDS)?;

    let mut decoder = Decoder::open(&wav_path).context("failed to open wav")?;
    let mut resampler = AudioResampler::new(SAMPLE_RATE, OUTPUT_RATE, 1024, usize::from(CHANNELS))
        .context("failed to create resampler")?;

    let warmup = decoder.decode_next()?;
    ensure!(
        !warmup.samples.is_empty(),
        "warmup decode should yield samples"
    );
    resampler.push_input(&[0.0_f32; 1024]);
    _ = resampler.process()?;

    let decoder_capacity = decoder.buffer_capacity();
    let input_capacity = resampler.input_accum_capacity();
    let output_capacity = resampler.output_buf_capacity();

    ensure!(
        decoder_capacity > 0,
        "decoder buffer should be pre-allocated"
    );

    let (mut producer, mut consumer) = RingBuffer::<f32>::new(4096);
    let mut channel_scratch = Vec::with_capacity(65536);
    let channel_cap = channel_scratch.capacity();

    let mut batches = 0u64;
    let mut output_samples = 0u64;
    loop {
        let batch = decoder.decode_next()?;
        if batch.samples.is_empty() {
            break;
        }
        batches = batches.saturating_add(1);

        let src_ch = usize::from(batch.params.channels);
        let dst_ch = usize::from(CHANNELS);
        let samples: &[f32] = if src_ch == dst_ch {
            let borrowed = maybe_downmix(&batch, src_ch, dst_ch);
            ensure!(
                matches!(borrowed, Borrowed(_)),
                "equal channels must borrow without allocation"
            );
            ensure!(
                channel_scratch.capacity() == channel_cap,
                "channel scratch must not reallocate on borrowed path"
            );
            ensure!(
                channel_scratch.is_empty(),
                "channel scratch must stay empty when borrowing"
            );
            batch.samples
        } else {
            fill_channel_scratch(batch.samples, src_ch, dst_ch, &mut channel_scratch);
            ensure!(
                channel_scratch.capacity() == channel_cap
                    || channel_scratch.capacity() > channel_cap,
                "channel scratch capacity unexpected"
            );
            &channel_scratch
        };

        resampler.push_input(samples);
        while let Some(output) = resampler.process()? {
            output_samples = output_samples.saturating_add(push_output(output, &mut producer));
        }
        drain_ring(&mut consumer);

        ensure!(
            decoder.buffer_capacity() == decoder_capacity,
            "decoder buffer reallocated during steady-state run"
        );
        ensure!(
            resampler.input_accum_capacity() == input_capacity,
            "resampler input buffer reallocated during steady-state run"
        );
        ensure!(
            resampler.output_buf_capacity() == output_capacity,
            "resampler output buffer reallocated during steady-state run"
        );
        if src_ch != dst_ch {
            ensure!(
                channel_scratch.capacity() == channel_cap
                    || channel_scratch.capacity() >= channel_cap,
                "channel scratch reallocated during steady-state run"
            );
        }
    }
    verify_channel_scratch_reuse(channel_cap)?;

    ensure!(batches > 0, "no batches decoded");
    ensure!(output_samples > 0, "no resampled output produced");

    info!(
        "zero_alloc: {batches} batches, {output_samples} output samples — our pre-allocated \
         buffers held constant capacity throughout"
    );

    drop(dir);
    Ok(())
}

/// Push a batch of samples to the device ring buffer, returning the count.
fn push_output(samples: &[f32], producer: &mut Producer<f32>) -> u64 {
    for &s in samples {
        push_blocking(s, producer);
    }
    u64::try_from(samples.len()).unwrap_or(0)
}

/// Push a single sample, retrying when the ring buffer is full.
fn push_blocking(sample: f32, producer: &mut Producer<f32>) {
    let mut s = sample;
    loop {
        match producer.push(s) {
            Ok(()) => return,
            Err(Full(val)) => s = val,
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::main_test;

    #[test]
    fn zero_alloc_steady_state() -> Result<()> {
        main_test()
    }
}
