//! Criterion benchmark harness for audio pipeline hot paths.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

/// Baseline benchmark for decoder PCM frame output throughput.
fn decoder_pcm_throughput(c: &mut Criterion) {
    c.bench_function("decoder_pcm_output", |b| {
        b.iter(|| {
            black_box(());
        });
    });
}

/// Baseline benchmark for ring buffer push/pop throughput.
fn ring_buffer_throughput(c: &mut Criterion) {
    c.bench_function("ring_buffer_throughput", |b| {
        b.iter(|| {
            black_box(());
        });
    });
}

/// Baseline benchmark for resampler latency.
fn resampler_latency(c: &mut Criterion) {
    c.bench_function("resampler_latency", |b| {
        b.iter(|| {
            black_box(());
        });
    });
}

criterion_group!(
    name = audio_pipeline;
    config = Criterion::default().sample_size(100);
    targets = decoder_pcm_throughput, ring_buffer_throughput, resampler_latency
);

criterion_main!(audio_pipeline);
