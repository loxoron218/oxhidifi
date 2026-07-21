//! Pure channel-conversion DSP: downmixing and upmixing audio frames.

use std::iter::repeat_n;

use crate::playback::decoder::DecodedSamples;

/// Downmix interleaved frames from `src_channels` to fewer `dst_channels`
/// by averaging channel groups.
fn downsample_frames(samples: &[f32], src_channels: usize, dst_channels: usize) -> Vec<f32> {
    let frames = samples.len() / src_channels;
    let mut out = Vec::with_capacity(frames * dst_channels);
    for frame in samples.chunks_exact(src_channels) {
        for out_ch in 0..dst_channels {
            let start_ch = (out_ch * src_channels) / dst_channels;
            let end_ch = ((out_ch + 1) * src_channels) / dst_channels;
            let count = u8::try_from(end_ch - start_ch).unwrap_or(1);
            out.push(frame[start_ch..end_ch].iter().sum::<f32>() / f32::from(count));
        }
    }
    out
}

/// Upmix interleaved frames from `src_channels` to more `dst_channels` by
/// padding extra channels with silence.
fn upsample_frames(samples: &[f32], src_channels: usize, dst_channels: usize) -> Vec<f32> {
    let pad = dst_channels - src_channels;
    let frames = samples.len() / src_channels;
    let mut out = Vec::with_capacity(frames * dst_channels);
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
#[must_use]
pub fn maybe_downmix(batch: DecodedSamples, src_channels: usize, dst_channels: usize) -> Vec<f32> {
    if src_channels == dst_channels {
        batch.samples
    } else {
        downmix(&batch.samples, src_channels, dst_channels)
    }
}

#[cfg(test)]
mod tests {
    use crate::playback::{
        channel::{downmix, maybe_downmix},
        decoder::{AudioParams, DecodedSamples},
    };

    fn assert_approx_eq(a: f32, b: f32) {
        assert!((a - b).abs() < f32::EPSILON, "{a} != {b}");
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
        assert_approx_eq(result[0], 0.5);
        assert_approx_eq(result[1], -0.5);
    }

    #[test]
    fn downmix_downmix_51_to_stereo_averages_groups() {
        let samples = vec![1.0, 0.5, 0.0, 0.0, -1.0, -0.5];
        let result = downmix(&samples, 6, 2);
        assert_approx_eq(result[0], 0.5);
        assert_approx_eq(result[1], -0.5);
    }

    #[test]
    fn downmix_downmix_7ch_to_3ch_distributes_evenly() {
        let samples = vec![1.0, 2.0, 10.0, 20.0, 100.0, 200.0, 0.5];
        let result = downmix(&samples, 7, 3);
        assert_approx_eq(result[0], 1.5);
        assert_approx_eq(result[1], 15.0);
        assert!((result[2] - 100.166_67).abs() < 0.001);
    }

    #[test]
    fn downmix_downmix_5ch_to_2ch_uneven_groups() {
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = downmix(&samples, 5, 2);
        assert_approx_eq(result[0], 1.5);
        assert_approx_eq(result[1], 4.0);
    }

    #[test]
    fn maybe_downmix_no_downmix_when_channels_match() {
        let batch = DecodedSamples {
            samples: vec![0.5, -0.5, 0.25, -0.25],
            params: AudioParams {
                sample_rate: 44100,
                channels: 2,
                duration_seconds: 0.0,
            },
        };
        let result = maybe_downmix(batch, 2, 2);
        assert_eq!(result, vec![0.5, -0.5, 0.25, -0.25]);
    }

    #[test]
    fn maybe_downmix_downmixes_when_channels_differ() {
        let batch = DecodedSamples {
            samples: vec![0.8, 0.2],
            params: AudioParams {
                sample_rate: 44100,
                channels: 2,
                duration_seconds: 0.0,
            },
        };
        let result = maybe_downmix(batch, 2, 1);
        assert_eq!(result.len(), 1);
        assert!((result[0] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn downmix_multiple_frames_preserves_frame_boundaries() {
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let result = downmix(&samples, 4, 2);
        assert_eq!(result.len(), 4);
        assert!((result[0] - 1.5).abs() < f32::EPSILON);
        assert!((result[1] - 3.5).abs() < f32::EPSILON);
        assert!((result[2] - 5.5).abs() < f32::EPSILON);
        assert!((result[3] - 7.5).abs() < f32::EPSILON);
    }

    #[test]
    fn downmix_empty_input_returns_empty_output() {
        let result = downmix(&[], 2, 1);
        assert!(result.is_empty());
        let result = downmix(&[], 1, 6);
        assert!(result.is_empty());
    }
}
