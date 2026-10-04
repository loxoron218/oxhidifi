//! Live audio signal path: immutable snapshots, verdicts, and stage facts.
//!
//! Read-only view from source to output. An off-thread builder copies facts
//! into one [`SignalPathSnapshot`] per generation; the UI swaps atomically.
//! The audio hot path is never instrumented.

pub mod describe_contract;
pub mod path_snapshot;
pub mod path_verdict;
pub mod stage_build;
pub mod stage_describe;
pub mod stage_output;
pub mod stage_volume;
use thiserror::Error;

use crate::{
    playback::{
        alsa_mixer::SystemMixerSource,
        decoder::AudioParams,
        devices::OutputMode,
        state::{MuteState, PlaybackStatus},
    },
    storage::{StorageError, catalog::TrackAudio},
};

/// Streaming-provider authentication facts for the playing source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthFacts {
    /// Provider name such as TIDAL or Qobuz.
    pub provider: String,
    /// Whether the provider stream is verified authentic.
    pub verified: bool,
}

/// Role of one [`RenderingDevice`] in the playback chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceRole {
    /// Network streamer or transport target feeding a separate DAC.
    TransportTarget,
    /// Device rendering audio to analog outputs or speakers.
    Output,
    /// External software or hardware renderer beyond the application.
    ExternalRenderer,
}

/// One node in the live chain, in execution order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathStage {
    /// Zero-based order in the snapshot stage list.
    pub position: u32,
    /// Coarse chain-order kind.
    pub kind: StageKind,
    /// Concise label such as `ALSA Direct Output`.
    pub title: String,
    /// Blue detail line with formats, input-to-output values, or dB.
    pub detail: String,
    /// Plain-language what-it-does text, never empty.
    pub explanation: String,
    /// Per-stage quality indicator with icon and text.
    pub verdict: QualityVerdict,
    /// Circular badge symbolic icon name for the codec, device, or effect.
    pub badge_icon: &'static str,
}

/// Whole-path or per-stage audio quality verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityVerdict {
    /// Audio data is unaltered from source to output.
    BitPerfect,
    /// Audio data is intentionally altered or enhanced.
    Processed,
    /// Audio data is constrained by the output path.
    Limited,
}

impl QualityVerdict {
    /// Canonical display label: exactly `Bit-Perfect`, `Processed`, or `Limited`.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::BitPerfect => "Bit-Perfect",
            Self::Processed => "Processed",
            Self::Limited => "Limited",
        }
    }
}

/// Hardware or external software endpoint producing sound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderingDevice {
    /// Display name from CPAL device info, falling back to the device id.
    pub display_name: String,
    /// Role of this device in the chain.
    pub role: DeviceRole,
    /// Generic symbolic icon name for the brand or device tile.
    pub brand_visual: String,
    /// Outline illustration icon name for the device card.
    pub illustration: String,
    /// Manual URL when known from explicit user config; `None` hides the link.
    pub manual_url: Option<String>,
}

/// Typed errors for signal path snapshot builds.
#[derive(Debug, Error)]
pub enum SignalPathError {
    /// Catalog lookup failed while fetching track audio facts.
    #[error("Catalog lookup failed: {0}")]
    CatalogLookup(#[from] StorageError),
    /// No active track; the UI shows the empty state instead of failing.
    #[error("No active track")]
    NoActiveTrack,
    /// Snapshot assembly failed after inputs were gathered.
    #[error("Snapshot build failed: {0}")]
    SnapshotBuild(String),
}

/// Complete live chain for the currently playing track.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalPathSnapshot {
    /// Monotonic counter; the UI re-renders only when it changes.
    pub generation: u64,
    /// Catalog id of the playing track (`None` shows the empty state).
    pub track_id: Option<i64>,
    /// Playing output or zone name shown in the header.
    pub zone_name: String,
    /// Whole-path verdict resolved by Limited over Processed over Bit-Perfect.
    pub verdict: QualityVerdict,
    /// Ordered stages from source at the top to output at the bottom.
    pub stages: Vec<PathStage>,
    /// Rendering devices in chain order.
    pub devices: Vec<RenderingDevice>,
    /// Throughput multiple of real time when in-app alteration is active.
    pub processing_speed: Option<f64>,
    /// Live playback status; paused or stopped shows a status ribbon.
    pub playback_status: PlaybackStatus,
}

/// Owned off-thread input facts for building one [`SignalPathSnapshot`].
#[derive(Debug, Clone)]
pub struct SnapshotInput {
    /// Requested generation; the caller bumps this on every rebuild.
    pub generation: u64,
    /// Catalog id of the playing track (`None` maps to `NoActiveTrack`).
    pub track_id: Option<i64>,
    /// Catalog audio facts for the track when known.
    pub track_audio: Option<TrackAudio>,
    /// Decoder stream facts when a decoder is active.
    pub decoder_params: Option<AudioParams>,
    /// Resampler input rate in Hz when sample-rate conversion is active.
    pub resampler_in_rate: Option<u32>,
    /// Resampler output rate in Hz when sample-rate conversion is active.
    pub resampler_out_rate: Option<u32>,
    /// Resampler channel count when sample-rate conversion is active.
    pub resampler_channels: Option<usize>,
    /// Linear volume slider value from 0.0 to 1.0.
    pub volume: f64,
    /// Mute state from the playback engine.
    pub muted: MuteState,
    /// Device/hardware read-back level (`None` falls back to the slider).
    pub device_volume: Option<f64>,
    /// Device/hardware mute switch state.
    pub device_muted: bool,
    /// System/application mixer level from 0.0 to 1.0 (`None` renders `unknown`).
    pub system_volume: Option<f64>,
    /// System/application mute switch state.
    pub system_muted: bool,
    /// Where the system level came from.
    pub system_source: SystemMixerSource,
    /// Output path mode from the playback engine.
    pub output_mode: OutputMode,
    /// Live playback status from the playback engine.
    pub status: PlaybackStatus,
    /// Stable output device identifier.
    pub device_id: String,
    /// Human-readable output device name.
    pub device_name: String,
    /// Device stream sample rate in Hz.
    pub device_sample_rate: u32,
    /// Device stream channel count.
    pub device_channels: u16,
    /// Whether the device was lost mid-playback.
    pub device_lost: bool,
    /// Playing output or zone name shown in the header.
    pub zone_name: String,
    /// Provider authentication facts; MVP always `None`, omitting the stage.
    pub auth: Option<AuthFacts>,
    /// Monotonic wall-clock sample time in nanos, captured by the publisher.
    pub sampled_at_wall: u64,
    /// Cumulative decoded frames at sample time, captured by the publisher.
    pub decoded_frames: u64,
    /// Cumulative resampled frames at sample time, captured by the publisher.
    pub resampled_frames: u64,
}

/// Owned parameters describing one stage for title, detail, and explanation.
#[derive(Debug, Clone, PartialEq)]
pub enum StageFacts {
    /// Source origin and format facts; unknown numeric fields are `None`.
    Source {
        /// Origin label such as local file or provider name.
        origin: String,
        /// Codec or container label such as FLAC or DSF.
        codec: String,
        /// Native sample rate in Hz when known.
        sample_rate: Option<u32>,
        /// Native bit depth when known (`None` for DSD or lossy).
        bit_depth: Option<u16>,
        /// Channel count when known.
        channels: Option<u16>,
    },
    /// Provider authentication or verification step.
    Authentication {
        /// Provider name.
        provider: String,
        /// Whether the provider stream is verified authentic.
        verified: bool,
    },
    /// Codec decode to PCM frames.
    Decoder {
        /// Codec label.
        codec: String,
        /// Stream sample rate in Hz.
        sample_rate: u32,
        /// Stream channel count.
        channels: u16,
    },
    /// Bit-depth conversion with input-to-output wording.
    BitDepthConverter {
        /// Input bit depth.
        input_bits: u16,
        /// Output bit depth.
        output_bits: u16,
    },
    /// Sample-rate conversion with input-to-output wording.
    SampleRateConverter {
        /// Input sample rate in Hz.
        input_rate: u32,
        /// Output sample rate in Hz.
        output_rate: u32,
    },
    /// Format conversion such as DSD-to-PCM.
    FormatConverter {
        /// Input format label.
        input_format: String,
        /// Output format label.
        output_format: String,
    },
    /// Volume handling with its decibel value; the label vocabulary is `DSP
    /// volume` for in-app scaling and `Device Volume` for hardware attenuation
    /// (leveling/headroom labels stay `describe()`-only in MVP).
    Volume {
        /// Linear volume slider value from 0.0 to 1.0.
        volume: f64,
        /// Volume family label such as DSP volume or Device Volume.
        label: String,
    },
    /// System/application volume with its OS-mixer level; `None` renders `unknown`.
    SystemVolume {
        /// OS-mixer level from 0.0 to 1.0 (`None` when unreadable).
        volume: Option<f64>,
        /// OS-mixer mute switch state.
        muted: bool,
        /// Where the system level came from.
        source: SystemMixerSource,
    },
    /// Effect with its setting summary; see the kind vocabulary below.
    Effect {
        /// Effect family such as Equalizer, Crossover, Crossfeed, or Channel map.
        kind: String,
        /// Human-readable setting summary with counts where applicable.
        summary: String,
    },
    /// Delivery transport with Linux output-mode wording.
    Transport {
        /// Transport mode label such as ALSA direct or shared mixer.
        mode: String,
        /// Device name carrying the transport.
        device_name: String,
    },
    /// Final destination device output.
    Output {
        /// Destination kind such as speakers or headphones.
        destination: String,
        /// Output mode wording.
        mode: String,
    },
    /// Hand-off beyond the application to an external renderer.
    ExternalRenderer {
        /// Renderer title.
        title: String,
        /// Filter description when known from output state.
        filter: Option<String>,
        /// Modulator description when known from output state.
        modulator: Option<String>,
    },
}

/// Position-independent kind of one [`PathStage`] in execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageKind {
    /// Origin file or stream with codec, rate, depth, and channels.
    Source,
    /// Provider authentication or verification step.
    Authentication,
    /// Codec decode to PCM frames.
    Decoder,
    /// Bit-depth conversion with input-to-output wording.
    BitDepthConverter,
    /// Sample-rate conversion with input-to-output wording.
    SampleRateConverter,
    /// Format conversion such as DSD-to-PCM.
    FormatConverter,
    /// Volume: DSP `Volume` plus hardware `Device Volume` (system is separate).
    Volume,
    /// System/application volume: OS-mixer attenuation outside the app.
    SystemVolume,
    /// Effect: equalizer, crossover, crossfeed, or channel map.
    Effect,
    /// Delivery transport with Linux output-mode wording.
    Transport,
    /// Final destination device output.
    Output,
    /// Hand-off beyond the application to an external renderer.
    ExternalRenderer,
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::playback::{
        alsa_mixer::SystemMixerSource::Unknown,
        devices::OutputMode::BitPerfect,
        signal_path::{
            QualityVerdict::{BitPerfect as VerdictBitPerfect, Limited, Processed},
            SignalPathSnapshot, SnapshotInput,
            stage_output::lab_test_device,
        },
        state::{MuteState::Unmuted, PlaybackStatus::Playing},
    };

    #[test]
    fn foundation_snapshot_and_types() -> Result<()> {
        let input = SnapshotInput {
            generation: 3,
            track_id: Some(7),
            track_audio: None,
            decoder_params: None,
            resampler_in_rate: Some(44100),
            resampler_out_rate: Some(48000),
            resampler_channels: Some(2),
            volume: 0.8,
            muted: Unmuted,
            device_volume: Some(0.8),
            device_muted: false,
            system_volume: None,
            system_muted: false,
            system_source: Unknown,
            output_mode: BitPerfect,
            status: Playing,
            device_id: String::from("hw:0"),
            device_name: String::from("Test DAC"),
            device_sample_rate: 44100,
            device_channels: 2,
            device_lost: false,
            zone_name: String::from("Test DAC"),
            auth: None,
            sampled_at_wall: 0,
            decoded_frames: 0,
            resampled_frames: 0,
        };
        let cloned = input.clone();
        ensure!(input.track_id == cloned.track_id, "clone keeps track");
        ensure!(cloned.auth.is_none(), "MVP omits auth");
        ensure!(cloned.system_volume.is_none(), "mixer facts start unknown");
        let snapshot = SignalPathSnapshot {
            generation: cloned.generation,
            track_id: cloned.track_id,
            zone_name: cloned.zone_name.clone(),
            verdict: VerdictBitPerfect,
            stages: Vec::new(),
            devices: vec![lab_test_device()],
            processing_speed: None,
            playback_status: cloned.status,
        };
        ensure!(
            snapshot.verdict == VerdictBitPerfect && snapshot.processing_speed.is_none(),
            "bit-perfect shape with hidden speed"
        );
        ensure!(Processed.label() == "Processed", "canonical processed");
        ensure!(Limited.label() == "Limited", "canonical limited");
        Ok(())
    }
}
