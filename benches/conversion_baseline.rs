//! Criterion benchmark baseline for rubato resampler latency.

use criterion::{Criterion, criterion_group, criterion_main};

use oxhidifi::playback::resampler::AudioResampler;

/// Drain all pending output from a resampler.
fn drain_resampler(resampler: &mut AudioResampler) {
    while let Ok(Some(_)) = resampler.process() {}
}

/// Benchmark resampler latency for 44.1kHz → 48kHz conversion.
fn resampler_44100_to_48000(c: &mut Criterion) {
    let Ok(mut resampler) = AudioResampler::new(44100, 48000, 1024, 2) else {
        return;
    };
    let input = vec![0.5_f32; 1024 * 2];

    c.bench_function("resampler_44100_to_48000", |b| {
        b.iter(|| {
            resampler.push_input(&input);
            drain_resampler(&mut resampler);
        });
    });
}

/// Benchmark resampler latency for 48kHz → 96kHz upsampling.
fn resampler_48000_to_96000(c: &mut Criterion) {
    let Ok(mut resampler) = AudioResampler::new(48000, 96000, 1024, 2) else {
        return;
    };
    let input = vec![0.5_f32; 1024 * 2];

    c.bench_function("resampler_48000_to_96000", |b| {
        b.iter(|| {
            resampler.push_input(&input);
            drain_resampler(&mut resampler);
        });
    });
}

/// Benchmark resampler latency for 96kHz → 44.1kHz downsampling.
fn resampler_96000_to_44100(c: &mut Criterion) {
    let Ok(mut resampler) = AudioResampler::new(96000, 44100, 1024, 2) else {
        return;
    };
    let input = vec![0.5_f32; 1024 * 2];

    c.bench_function("resampler_96000_to_44100", |b| {
        b.iter(|| {
            resampler.push_input(&input);
            drain_resampler(&mut resampler);
        });
    });
}

/// Benchmark resampler for mono channel (1 channel).
fn resampler_mono_44100_to_48000(c: &mut Criterion) {
    let Ok(mut resampler) = AudioResampler::new(44100, 48000, 1024, 1) else {
        return;
    };
    let input = vec![0.5_f32; 1024];

    c.bench_function("resampler_mono_44100_to_48000", |b| {
        b.iter(|| {
            resampler.push_input(&input);
            drain_resampler(&mut resampler);
        });
    });
}

/// Benchmark resampler for 192kHz high-resolution input.
fn resampler_192000_to_48000(c: &mut Criterion) {
    let Ok(mut resampler) = AudioResampler::new(192_000, 48000, 1024, 2) else {
        return;
    };
    let input = vec![0.5_f32; 1024 * 2];

    c.bench_function("resampler_192000_to_48000", |b| {
        b.iter(|| {
            resampler.push_input(&input);
            drain_resampler(&mut resampler);
        });
    });
}

criterion_group!(
    name = resampler;
    config = Criterion::default().sample_size(100);
    targets =
        resampler_44100_to_48000,
        resampler_48000_to_96000,
        resampler_96000_to_44100,
        resampler_mono_44100_to_48000,
        resampler_192000_to_48000,
);

criterion_main!(resampler);
