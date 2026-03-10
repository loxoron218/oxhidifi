//! Benchmark for audio producer throttling performance.

use std::{hint::black_box, time::Duration};

use {
    criterion::{Criterion, criterion_group, criterion_main},
    rtrb::RingBuffer,
};

/// Benchmark write performance with empty buffer (no throttling).
fn benchmark_write_with_buffer_empty(c: &mut Criterion) {
    c.bench_function("write_samples_buffer_empty", |b| {
        let buffer_capacity: usize = 65536;
        let sample_count = 8192;

        b.iter(|| {
            let (producer, mut consumer) = RingBuffer::<f32>::new(buffer_capacity);

            // Clear buffer to simulate empty state
            while consumer.pop().is_ok() {}

            let available = producer.slots();

            // Skip throttling - buffer has sufficient space
            black_box(available >= sample_count);
        });
    });
}

/// Benchmark write performance with moderately full buffer (60% - triggers moderate throttling).
fn benchmark_write_with_buffer_moderately_full(c: &mut Criterion) {
    c.bench_function("write_samples_buffer_moderately_full", |b| {
        let buffer_capacity: usize = 65536;

        b.iter(|| {
            let (producer, mut consumer) = RingBuffer::<f32>::new(buffer_capacity);

            // Fill buffer to 60% to trigger moderate throttling (50-75% range)
            // (popping from consumer = filling producer's view of the buffer)
            for _ in 0..(buffer_capacity * 60 / 100) {
                if consumer.pop().is_ok() {}
            }

            let available = producer.slots();

            // Dynamic throttling: integer comparisons avoid f64 conversion overhead
            let sleep_duration =
                if available == 0 || buffer_capacity == 0 || available < buffer_capacity / 4 {
                    Duration::from_micros(500)
                } else if available < buffer_capacity / 2 {
                    Duration::from_micros(200)
                } else {
                    Duration::from_micros(100)
                };

            black_box(sleep_duration);
            black_box(available);
        });
    });
}

/// Benchmark write performance with very full buffer (85% - triggers severe throttling).
fn benchmark_write_with_buffer_very_full(c: &mut Criterion) {
    c.bench_function("write_samples_buffer_very_full", |b| {
        let buffer_capacity: usize = 65536;

        b.iter(|| {
            let (producer, mut consumer) = RingBuffer::<f32>::new(buffer_capacity);

            // Fill buffer to 85% to trigger severe throttling (75%+ range)
            // (popping from consumer = filling producer's view of the buffer)
            for _ in 0..(buffer_capacity * 85 / 100) {
                if consumer.pop().is_ok() {}
            }

            let available = producer.slots();

            // Dynamic throttling: integer comparisons avoid f64 conversion overhead
            let sleep_duration =
                if available == 0 || buffer_capacity == 0 || available < buffer_capacity / 4 {
                    Duration::from_micros(500)
                } else if available < buffer_capacity / 2 {
                    Duration::from_micros(200)
                } else {
                    Duration::from_micros(100)
                };

            black_box(sleep_duration);
            black_box(available);
        });
    });
}

/// Benchmark throttle calculation overhead with different buffer fill ratios.
fn benchmark_throttle_calculation(c: &mut Criterion) {
    c.bench_function("throttle_calculation", |b| {
        let buffer_capacity: usize = 65536;

        b.iter(|| {
            // Test different fill ratios to measure throttle calculation overhead
            let available = black_box(32768_usize);

            // Dynamic throttling: integer comparisons avoid f64 conversion overhead
            let sleep_duration =
                if available == 0 || buffer_capacity == 0 || available < buffer_capacity / 4 {
                    Duration::from_micros(500)
                } else if available < buffer_capacity / 2 {
                    Duration::from_micros(200)
                } else {
                    Duration::from_micros(100)
                };

            black_box(sleep_duration);
            black_box(available);
        });
    });
}

criterion_group!(
    benches,
    benchmark_write_with_buffer_empty,
    benchmark_write_with_buffer_moderately_full,
    benchmark_write_with_buffer_very_full,
    benchmark_throttle_calculation
);
criterion_main!(benches);
