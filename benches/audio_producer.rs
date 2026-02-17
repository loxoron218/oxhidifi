//! Benchmark for audio producer throttling performance.

use std::{hint::black_box, time::Duration};

use {
    criterion::{Criterion, criterion_group, criterion_main},
    num_traits::cast,
    rtrb::RingBuffer,
};

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
            let fill_ratio = if buffer_capacity == 0 {
                1.0
            } else {
                let available_f64 = cast::<usize, f64>(available).unwrap();
                let capacity_f64 = cast::<usize, f64>(buffer_capacity).unwrap();
                (1.0 - (available_f64 / capacity_f64)).clamp(0.0, 1.0)
            };

            // Dynamic throttling: 2x sleep for 50-75% buffer occupancy prevents premature buffer overflow
            let sleep_duration = if fill_ratio > 0.75 {
                Duration::from_micros(500)
            } else if fill_ratio > 0.5 {
                Duration::from_micros(200)
            } else {
                Duration::from_micros(100)
            };

            black_box(sleep_duration);
            black_box(fill_ratio);
        });
    });
}

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
            let fill_ratio = if buffer_capacity == 0 {
                1.0
            } else {
                let available_f64 = cast::<usize, f64>(available).unwrap();
                let capacity_f64 = cast::<usize, f64>(buffer_capacity).unwrap();
                (1.0 - (available_f64 / capacity_f64)).clamp(0.0, 1.0)
            };

            // Dynamic throttling: 5x sleep for 75%+ buffer occupancy provides aggressive backpressure
            let sleep_duration = if fill_ratio > 0.75 {
                Duration::from_micros(500)
            } else if fill_ratio > 0.5 {
                Duration::from_micros(200)
            } else {
                Duration::from_micros(100)
            };

            black_box(sleep_duration);
            black_box(fill_ratio);
        });
    });
}

fn benchmark_throttle_calculation(c: &mut Criterion) {
    c.bench_function("throttle_calculation", |b| {
        let buffer_capacity: usize = 65536;

        b.iter(|| {
            // Test different fill ratios to measure throttle calculation overhead
            let available = black_box(32768_usize);

            let fill_ratio = if buffer_capacity == 0 {
                1.0
            } else {
                let available_f64 = cast::<usize, f64>(available).unwrap();
                let capacity_f64 = cast::<usize, f64>(buffer_capacity).unwrap();
                (1.0 - (available_f64 / capacity_f64)).clamp(0.0, 1.0)
            };

            // Dynamic throttling: Scale sleep duration based on buffer fill ratio to balance throughput and memory pressure
            let sleep_duration = if fill_ratio > 0.75 {
                Duration::from_micros(500)
            } else if fill_ratio > 0.5 {
                Duration::from_micros(200)
            } else {
                Duration::from_micros(100)
            };

            black_box(sleep_duration);
            black_box(fill_ratio);
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
