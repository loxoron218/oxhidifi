//! Pure channel-conversion DSP: downmixing and upmixing audio frames.

use std::{
    borrow::Cow::{self, Borrowed, Owned},
    iter::repeat_n,
};

use crate::playback::decoder::DecodedSamples;

/// Downmix interleaved frames from `src_channels` to fewer `dst_channels`
/// by averaging channel groups.
fn downsample_frames(samples: &[f32], src_channels: usize, dst_channels: usize) -> Vec<f32> {
    let frames = samples.len().checked_div(src_channels).unwrap_or(0);
    let mut out = Vec::with_capacity(frames.saturating_mul(dst_channels));
    for frame in samples.chunks_exact(src_channels) {
        push_downmixed_frame(frame, src_channels, dst_channels, &mut out);
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
/// When channel counts match, the returned [`Cow::Borrowed`] slice avoids any
/// heap allocation. When they differ, a downmix/upmix buffer is allocated.
/// This preserves the zero-allocation guarantee on the audio hot path for the
/// common case where source and device channel counts coincide.
#[must_use]
pub fn maybe_downmix<'a>(
    batch: &'a DecodedSamples<'a>,
    src_channels: usize,
    dst_channels: usize,
) -> Cow<'a, [f32]> {
    if src_channels == dst_channels {
        Borrowed(batch.samples)
    } else {
        Owned(downmix(batch.samples, src_channels, dst_channels))
    }
}

/// Push a single downmixed frame to `out`.
fn push_downmixed_frame(
    frame: &[f32],
    src_channels: usize,
    dst_channels: usize,
    out: &mut Vec<f32>,
) {
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

/// Fill `scratch` with channel-converted samples.
///
/// Clears `scratch` and populates it with the converted interleaved frames.
/// The caller must ensure `src_channels != dst_channels`; when equal, the
/// scratch buffer is left untouched.
pub fn fill_channel_scratch(
    samples: &[f32],
    src_channels: usize,
    dst_channels: usize,
    scratch: &mut Vec<f32>,
) {
    scratch.clear();
    if dst_channels > src_channels {
        let pad = dst_channels.saturating_sub(src_channels);
        let frames = samples.len().checked_div(src_channels).unwrap_or(0);
        scratch.reserve(frames.saturating_mul(dst_channels));
        for frame in samples.chunks_exact(src_channels) {
            scratch.extend_from_slice(frame);
            scratch.extend(repeat_n(0.0, pad));
        }
    } else {
        let frames = samples.len().checked_div(src_channels).unwrap_or(0);
        scratch.reserve(frames.saturating_mul(dst_channels));
        for frame in samples.chunks_exact(src_channels) {
            push_downmixed_frame(frame, src_channels, dst_channels, scratch);
        }
    }
}

/// Channel conversion with a reusable scratch buffer.
///
/// When `src_channels == dst_channels`, returns a borrowed slice without
/// touching `scratch`. Otherwise clears `scratch`, fills it with the
/// converted samples, and returns a borrowed view of `scratch`.
pub fn maybe_downmix_with_scratch<'a, 'b>(
    batch: &'a DecodedSamples<'a>,
    src_channels: usize,
    dst_channels: usize,
    scratch: &'b mut Vec<f32>,
) -> Cow<'a, [f32]>
where
    'b: 'a,
{
    if src_channels == dst_channels {
        Borrowed(batch.samples)
    } else {
        fill_channel_scratch(batch.samples, src_channels, dst_channels, scratch);
        Borrowed(scratch.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow::{Borrowed, Owned};

    use crate::playback::{
        channel::{downmix, maybe_downmix, maybe_downmix_with_scratch},
        decoder::{AudioParams, DecodedSamples},
    };

    fn stereo_samples() -> DecodedSamples<'static> {
        DecodedSamples {
            samples: &[0.5, -0.5, 0.25, -0.25],
            params: AudioParams {
                sample_rate: 44100,
                channels: 2,
                duration_seconds: 0.0,
                bit_depth: Some(16),
            },
        }
    }

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
        let batch = stereo_samples();
        let result = maybe_downmix(&batch, 2, 2);
        assert_eq!(result.as_ref(), &[0.5, -0.5, 0.25, -0.25]);
        assert!(
            matches!(result, Borrowed(_)),
            "equal channels must borrow without allocation"
        );
    }

    #[test]
    fn maybe_downmix_downmixes_when_channels_differ() {
        let batch = DecodedSamples {
            samples: &[0.8, 0.2],
            params: AudioParams {
                sample_rate: 44100,
                channels: 2,
                duration_seconds: 0.0,
                bit_depth: Some(16),
            },
        };
        let result = maybe_downmix(&batch, 2, 1);
        assert_eq!(result.len(), 1);
        assert_samples_close(result.as_ref(), &[0.5], f32::EPSILON);
        assert!(
            matches!(result, Owned(_)),
            "differing channels must allocate owned buffer"
        );
    }

    #[test]
    fn maybe_downmix_with_scratch_borrows_when_channels_match() {
        let batch = stereo_samples();
        let mut scratch = Vec::with_capacity(16);
        let cap_before = scratch.capacity();
        let result = maybe_downmix_with_scratch(&batch, 2, 2, &mut scratch);
        assert_eq!(result.as_ref(), &[0.5, -0.5, 0.25, -0.25]);
        assert_eq!(
            scratch.capacity(),
            cap_before,
            "scratch must not be touched when channels match"
        );
        assert!(scratch.is_empty(), "scratch must stay empty when borrowing");
    }

    #[test]
    fn maybe_downmix_with_scratch_reuses_allocation() {
        let batch = DecodedSamples {
            samples: &[0.8, 0.2, -0.6, -0.4],
            params: AudioParams {
                sample_rate: 44100,
                channels: 2,
                duration_seconds: 0.0,
                bit_depth: Some(16),
            },
        };
        let mut scratch = Vec::with_capacity(16);
        let result = maybe_downmix_with_scratch(&batch, 2, 1, &mut scratch);
        assert_samples_close(result.as_ref(), &[0.5, -0.5], f32::EPSILON);
        let cap_after_first = scratch.capacity();
        let batch2 = DecodedSamples {
            samples: &[1.0, -1.0, 0.5, -0.5],
            params: AudioParams {
                sample_rate: 44100,
                channels: 2,
                duration_seconds: 0.0,
                bit_depth: Some(16),
            },
        };
        let result2 = maybe_downmix_with_scratch(&batch2, 2, 1, &mut scratch);
        assert_samples_close(result2.as_ref(), &[0.0, 0.0], f32::EPSILON);
        assert_eq!(
            scratch.capacity(),
            cap_after_first,
            "scratch capacity must remain stable across conversions"
        );
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
