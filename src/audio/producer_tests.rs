//! Tests for audio producer and sample conversion.

use std::io::{Error, ErrorKind::NotFound};

use symphonia::core::{
    audio::{AudioBuffer, Channels, Signal, SignalSpec},
    sample::{i24, u24},
    units::Duration,
};

use crate::audio::{
    decoder_types::{
        AudioFormat,
        DecoderError::{IoError, UnsupportedFormat},
    },
    producer::AudioProducer,
};

macro_rules! assert_float_eq {
    ($left:expr, $right:expr) => {
        assert!(
            ($left - $right).abs() < f32::EPSILON,
            "Expected {}, got {}",
            $right,
            $left
        );
    };
}

#[test]
fn test_decoder_error_display() {
    let io_error = Error::new(NotFound, "File not found");
    let decoder_error = IoError(io_error);
    assert!(decoder_error.to_string().contains("IO error"));

    let unsupported_error = UnsupportedFormat;
    assert_eq!(unsupported_error.to_string(), "Unsupported audio format");
}

#[test]
fn test_audio_format_creation() {
    let format = AudioFormat {
        sample_rate: 96000,
        channels: 2,
        bits_per_sample: 24,
        channel_mask: 0x3,
    };
    assert_eq!(format.sample_rate, 96000);
    assert_eq!(format.channels, 2);
}

#[test]
fn test_convert_f32_to_interleaved() {
    let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
    let mut buf = AudioBuffer::<f32>::new(Duration::from(2u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = 0.5;
    buf.chan_mut(1)[0] = -0.5;
    buf.chan_mut(0)[1] = 1.0;
    buf.chan_mut(1)[1] = -1.0;

    let samples = AudioProducer::convert_f32_to_interleaved(&buf);

    assert_eq!(samples.len(), 4);
    assert_float_eq!(samples[0], 0.5);
    assert_float_eq!(samples[1], -0.5);
    assert_float_eq!(samples[2], 1.0);
    assert_float_eq!(samples[3], -1.0);
}

#[test]
fn test_convert_f32_to_interleaved_zero_samples() {
    let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
    let buf = AudioBuffer::<f32>::new(Duration::from(0u64), spec);

    let samples = AudioProducer::convert_f32_to_interleaved(&buf);

    assert_eq!(samples.len(), 0);
}

#[test]
fn test_convert_f64_to_interleaved() {
    let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
    let mut buf = AudioBuffer::<f64>::new(Duration::from(2u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = 0.25_f64;
    buf.chan_mut(1)[0] = -0.75_f64;
    buf.chan_mut(0)[1] = 0.0_f64;
    buf.chan_mut(1)[1] = 1.0_f64;

    let samples = AudioProducer::convert_f64_to_interleaved(&buf);

    assert_eq!(samples.len(), 4);
    assert_float_eq!(samples[0], 0.25);
    assert_float_eq!(samples[1], -0.75);
    assert_float_eq!(samples[2], 0.0);
    assert_float_eq!(samples[3], 1.0);
}

#[test]
fn test_convert_f64_to_interleaved_clamping() {
    let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
    let mut buf = AudioBuffer::<f64>::new(Duration::from(2u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = 2.0_f64;
    buf.chan_mut(0)[1] = -2.0_f64;

    let samples = AudioProducer::convert_f64_to_interleaved(&buf);

    assert_eq!(samples.len(), 2);
    assert_float_eq!(samples[0], 1.0);
    assert_float_eq!(samples[1], -1.0);
}

#[test]
fn test_convert_u8_to_interleaved() {
    let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
    let mut buf = AudioBuffer::<u8>::new(Duration::from(2u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = 255;
    buf.chan_mut(1)[0] = 0;
    buf.chan_mut(0)[1] = 128;
    buf.chan_mut(1)[1] = 129;

    let samples = AudioProducer::convert_u8_to_interleaved(&buf);

    assert_eq!(samples.len(), 4);
    assert_float_eq!(samples[0], 1.0);
    assert_float_eq!(samples[1], -1.007_874);
    assert_float_eq!(samples[2], 0.0);
    assert_float_eq!(samples[3], 0.007_874_016);
}

#[test]
fn test_convert_s8_to_interleaved() {
    let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
    let mut buf = AudioBuffer::<i8>::new(Duration::from(3u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = 127;
    buf.chan_mut(0)[1] = -128;
    buf.chan_mut(0)[2] = 0;

    let samples = AudioProducer::convert_s8_to_interleaved(&buf);

    assert_eq!(samples.len(), 3);
    assert_float_eq!(samples[0], 1.0);
    assert_float_eq!(samples[1], -1.007_874);
    assert_float_eq!(samples[2], 0.0);
}

#[test]
fn test_convert_u16_to_interleaved() {
    let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
    let mut buf = AudioBuffer::<u16>::new(Duration::from(2u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = 65535;
    buf.chan_mut(1)[0] = 0;
    buf.chan_mut(0)[1] = 32768;
    buf.chan_mut(1)[1] = 32767;

    let samples = AudioProducer::convert_u16_to_interleaved(&buf);

    assert_eq!(samples.len(), 4);
    assert_float_eq!(samples[0], 1.0);
    assert_float_eq!(samples[1], -1.000_030_5);
    assert_float_eq!(samples[2], 0.0);
    assert_float_eq!(samples[3], -0.000_030_518_51);
}

#[test]
fn test_convert_s16_to_interleaved_edge_cases() {
    let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
    let mut buf = AudioBuffer::<i16>::new(Duration::from(4u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = i16::MIN;
    buf.chan_mut(0)[1] = i16::MAX;
    buf.chan_mut(0)[2] = 0;
    buf.chan_mut(0)[3] = -1000;

    let samples = AudioProducer::convert_s16_to_interleaved(&buf);

    assert_eq!(samples.len(), 4);
    assert_float_eq!(samples[0], -1.0);
    assert_float_eq!(samples[1], 1.0);
    assert_float_eq!(samples[2], 0.0);
    assert_float_eq!(samples[3], -0.030_518_51);
}

#[test]
fn test_convert_s16_to_interleaved_stereo() {
    let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
    let mut buf = AudioBuffer::<i16>::new(Duration::from(2u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = 1000;
    buf.chan_mut(1)[0] = -1000;
    buf.chan_mut(0)[1] = i16::MAX;
    buf.chan_mut(1)[1] = i16::MIN;

    let samples = AudioProducer::convert_s16_to_interleaved(&buf);

    assert_eq!(samples.len(), 4);
    assert_float_eq!(samples[0], 0.030_518_51);
    assert_float_eq!(samples[1], -0.030_518_51);
    assert_float_eq!(samples[2], 1.0);
    assert_float_eq!(samples[3], -1.0);
}

#[test]
fn test_convert_u24_to_interleaved() {
    let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
    let mut buf = AudioBuffer::<u24>::new(Duration::from(3u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = u24(0x00FF_FFFF);
    buf.chan_mut(0)[1] = u24(0x0000_0000);
    buf.chan_mut(0)[2] = u24(0x0080_0000);

    let samples = AudioProducer::convert_u24_to_interleaved(&buf);

    assert_eq!(samples.len(), 3);
    assert_float_eq!(samples[0], 1.0);
    assert_float_eq!(samples[1], 0.0);
    assert_float_eq!(samples[2], 0.500_000_06);
}

#[test]
fn test_convert_s24_to_interleaved() {
    let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
    let mut buf = AudioBuffer::<i24>::new(Duration::from(3u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = i24(0x007F_FFFF);
    buf.chan_mut(0)[1] = i24(0x0080_0000u32.cast_signed());
    buf.chan_mut(0)[2] = i24(0x0000_0000);

    let samples = AudioProducer::convert_s24_to_interleaved(&buf);

    assert_eq!(samples.len(), 3);
    assert_float_eq!(samples[0], 1.0);
    assert_float_eq!(samples[1], -1.0);
    assert_float_eq!(samples[2], 0.0);
}

#[test]
fn test_convert_u32_to_interleaved() {
    let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
    let mut buf = AudioBuffer::<u32>::new(Duration::from(3u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = 4_294_967_295;
    buf.chan_mut(0)[1] = 2_147_483_648;
    buf.chan_mut(0)[2] = 0;

    let samples = AudioProducer::convert_u32_to_interleaved(&buf);

    assert_eq!(samples.len(), 3);
    assert_float_eq!(samples[0], 1.0);
    assert_float_eq!(samples[1], 0.0);
    assert_float_eq!(samples[2], -1.0);
}

#[test]
fn test_convert_s32_to_interleaved_edge_cases() {
    let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
    let mut buf = AudioBuffer::<i32>::new(Duration::from(4u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = i32::MIN;
    buf.chan_mut(0)[1] = i32::MAX;
    buf.chan_mut(0)[2] = 0;
    buf.chan_mut(0)[3] = 100_000_000;

    let samples = AudioProducer::convert_s32_to_interleaved(&buf);

    assert_eq!(samples.len(), 4);
    assert_float_eq!(samples[0], -1.0);
    assert_float_eq!(samples[1], 1.0);
    assert_float_eq!(samples[2], 0.0);
    assert_float_eq!(samples[3], 0.046_566_13);
}

#[test]
fn test_convert_s32_to_interleaved_stereo() {
    let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
    let mut buf = AudioBuffer::<i32>::new(Duration::from(2u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = 500_000;
    buf.chan_mut(1)[0] = -500_000;
    buf.chan_mut(0)[1] = i32::MAX;
    buf.chan_mut(1)[1] = i32::MIN;

    let samples = AudioProducer::convert_s32_to_interleaved(&buf);

    assert_eq!(samples.len(), 4);
    assert_float_eq!(samples[0], 0.000_232_830_65);
    assert_float_eq!(samples[1], -0.000_232_830_65);
    assert_float_eq!(samples[2], 1.0);
    assert_float_eq!(samples[3], -1.0);
}

#[test]
fn test_convert_all_formats_interleaved_ordering() {
    let spec = SignalSpec::new(
        44100,
        Channels::FRONT_LEFT | Channels::FRONT_RIGHT | Channels::FRONT_CENTRE,
    );

    let mut buf_f32 = AudioBuffer::<f32>::new(Duration::from(1u64), spec);

    buf_f32.render_silence(None);

    buf_f32.chan_mut(0)[0] = 0.1;
    buf_f32.chan_mut(1)[0] = 0.2;
    buf_f32.chan_mut(2)[0] = 0.3;

    let samples = AudioProducer::convert_f32_to_interleaved(&buf_f32);

    assert_eq!(samples.len(), 3);
    assert_float_eq!(samples[0], 0.1);
    assert_float_eq!(samples[1], 0.2);
    assert_float_eq!(samples[2], 0.3);
}

#[test]
fn test_convert_multichannel_interleaving() {
    let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
    let mut buf = AudioBuffer::<f32>::new(Duration::from(3u64), spec);

    buf.render_silence(None);

    buf.chan_mut(0)[0] = 1.0;
    buf.chan_mut(1)[0] = -1.0;
    buf.chan_mut(0)[1] = 0.5;
    buf.chan_mut(1)[1] = -0.5;
    buf.chan_mut(0)[2] = 0.0;
    buf.chan_mut(1)[2] = 0.0;
    let samples = AudioProducer::convert_f32_to_interleaved(&buf);

    assert_eq!(samples.len(), 6);
    assert_float_eq!(samples[0], 1.0);
    assert_float_eq!(samples[1], -1.0);
    assert_float_eq!(samples[2], 0.5);
    assert_float_eq!(samples[3], -0.5);
    assert_float_eq!(samples[4], 0.0);
    assert_float_eq!(samples[5], 0.0);
}
