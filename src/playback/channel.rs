//! Pure channel-conversion DSP: downmixing and upmixing audio frames.

use std::iter::repeat_n;

use crate::playback::decoder::DecodedSamples;

/// Downmix interleaved frames from `src_channels` to fewer `dst_channels`
/// by averaging channel groups.
fn downsample_frames(samples: &[f32], src_channels: usize, dst_channels: usize) -> Vec<f32> {
    let frames = samples.len().checked_div(src_channels).unwrap_or(0);
    let mut out = Vec::with_capacity(frames.saturating_mul(dst_channels));
    for frame in samples.chunks_exact(src_channels) {
        for out_ch in 0..dst_channels {
            let start_ch = out_ch
                .saturating_mul(src_channels)
                .checked_div(dst_channels)
                .unwrap_or(0);
            let end_ch = out_ch
                .saturating_add(1)
                .saturating_mul(src_channels)
                .checked_div(dst_channels)
                .unwrap_or(0);
            let group_len = end_ch.saturating_sub(start_ch);
            let count = u8::try_from(group_len).unwrap_or(1);
            out.push(frame.iter().skip(start_ch).take(group_len).sum::<f32>() / f32::from(count));
        }
    }
    out
}

/// Upmix interleaved frames from `src_channels` to more `dst_channels` by
/// padding extra channels with silence.
fn upsample_frames(samples: &[f32], src_channels: usize, dst_channels: usize) -> Vec<f32> {
    let pad = dst_channels.saturating_sub(src_channels);
    let frames = samples.len().checked_div(src_channels).unwrap_or(0);
    let mut out = Vec::with_capacity(frames.saturating_mul(dst_channels));
    for frame in samples.chunks_exact(src_channels) {
        out.extend_from_slice(frame);
        out.extend(repeat_n(0.0, pad));
    }
    out
}

/// Downmix interleaved samples from `src_channels` to `dst_channels`.
///
/// When `src_channels > dst_channels`, source channels are averaged into
/// groups to produce the output channels. When `src_channels < dst_channels`,
/// extra output channels are filled with silence.
fn downmix(samples: &[f32], src_channels: usize, dst_channels: usize) -> Vec<f32> {
    if src_channels == dst_channels {
        return samples.to_vec();
    }
    if dst_channels > src_channels {
        upsample_frames(samples, src_channels, dst_channels)
    } else {
        downsample_frames(samples, src_channels, dst_channels)
    }
}

/// Return `batch.samples` as-is if channel counts match, otherwise downmix.
///
/// When channel counts match, the returned buffer is a fresh allocation owned
/// by the caller. When they differ, a downmix/upmix buffer is allocated.
#[must_use]
pub fn maybe_downmix(
    batch: &DecodedSamples<'_>,
    src_channels: usize,
    dst_channels: usize,
) -> Vec<f32> {
    if src_channels == dst_channels {
        batch.samples.to_vec()
    } else {
        downmix(batch.samples, src_channels, dst_channels)
    }
}

#[cfg(test)]
mod tests {
    use crate::playback::{
        channel::{downmix, maybe_downmix},
        decoder::{AudioParams, DecodedSamples},
    };

    fn assert_samples_close(actual: &[f32], expected: &[f32], tolerance: f32) {
        assert_eq!(
            actual.len(),
            expected.len(),
            "length mismatch: {actual:?} != {expected:?}"
        );
        for (a, b) in actual.iter().zip(expected) {
            assert!(
                (a - b).abs() < tolerance,
                "{a} != {b} (tolerance {tolerance})"
            );
        }
    }

    #[test]
    fn downmix_equal_channels_returns_copy() {
        let samples = vec![1.0, -0.5, 0.25, -1.0];
        let result = downmix(&samples, 2, 2);
        assert_eq!(result, samples);
    }

    #[test]
    fn downmix_upmix_mono_to_stereo_pads_with_silence() {
        let samples = vec![0.75, -0.25];
        let result = downmix(&samples, 1, 2);
        assert_eq!(result, vec![0.75, 0.0, -0.25, 0.0]);
    }

    #[test]
    fn downmix_upmix_stereo_to_51_pads_extra_channels() {
        let samples = vec![0.5, -0.5, 1.0, -1.0];
        let result = downmix(&samples, 2, 6);
        assert_eq!(
            result,
            vec![0.5, -0.5, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn downmix_downmix_stereo_to_mono_averages() {
        let samples = vec![0.8, 0.2, -0.6, -0.4];
        let result = downmix(&samples, 2, 1);
        assert_samples_close(&result, &[0.5, -0.5], f32::EPSILON);
    }

    #[test]
    fn downmix_downmix_51_to_stereo_averages_groups() {
        let samples = vec![1.0, 0.5, 0.0, 0.0, -1.0, -0.5];
        let result = downmix(&samples, 6, 2);
        assert_samples_close(&result, &[0.5, -0.5], f32::EPSILON);
    }

    #[test]
    fn downmix_downmix_7ch_to_3ch_distributes_evenly() {
        let samples = vec![1.0, 2.0, 10.0, 20.0, 100.0, 200.0, 0.5];
        let result = downmix(&samples, 7, 3);
        assert_samples_close(&result, &[1.5, 15.0, 100.166_67], 0.001);
    }

    #[test]
    fn downmix_downmix_5ch_to_2ch_uneven_groups() {
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = downmix(&samples, 5, 2);
        assert_samples_close(&result, &[1.5, 4.0], f32::EPSILON);
    }

    #[test]
    fn maybe_downmix_no_downmix_when_channels_match() {
        let batch = DecodedSamples {
            samples: &[0.5, -0.5, 0.25, -0.25],
            params: AudioParams {
                sample_rate: 44100,
                channels: 2,
                duration_seconds: 0.0,
            },
        };
        let result = maybe_downmix(&batch, 2, 2);
        assert_eq!(result, vec![0.5, -0.5, 0.25, -0.25]);
    }

    #[test]
    fn maybe_downmix_downmixes_when_channels_differ() {
        let batch = DecodedSamples {
            samples: &[0.8, 0.2],
            params: AudioParams {
                sample_rate: 44100,
                channels: 2,
                duration_seconds: 0.0,
            },
        };
        let result = maybe_downmix(&batch, 2, 1);
        assert_eq!(result.len(), 1);
        assert_samples_close(&result, &[0.5], f32::EPSILON);
    }

    #[test]
    fn downmix_multiple_frames_preserves_frame_boundaries() {
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let result = downmix(&samples, 4, 2);
        assert_eq!(result.len(), 4);
        assert_samples_close(&result, &[1.5, 3.5, 5.5, 7.5], f32::EPSILON);
    }

    #[test]
    fn downmix_empty_input_returns_empty_output() {
        let result = downmix(&[], 2, 1);
        assert_eq!(result, Vec::<f32>::new());
        let result = downmix(&[], 1, 6);
        assert_eq!(result, Vec::<f32>::new());
    }
}
