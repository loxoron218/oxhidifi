# Data Model: Audio Signal Path Inspector (`002-signal-path`)

**Spec**: `specs/002-signal-path/spec.md` | **Plan**: `plan.md`

All types are immutable in-memory values built off-thread per snapshot.
No persistence, no schema migration. Naming follows the spec's Key Entities.

## 1. SignalPathSnapshot (spec: Signal Path)

The complete live chain for the currently playing track. One instance per
poll generation; the UI swaps instances atomically, never mutates them.

| Field | Type | Description |
|---|---|---|
| `generation` | `u64` | Monotonic counter; UI re-renders only when it changes. |
| `track_id` | `Option<i64>` | Catalog id of the playing track (`None` ⇒ empty state). |
| `zone_name` | `String` | Playing output/zone name shown in the header. |
| `verdict` | `QualityVerdict` | Whole-path verdict (see §3 precedence). |
| `stages` | `Vec<PathStage>` | Ordered top (source) → bottom (output/external). |
| `devices` | `Vec<RenderingDevice>` | Rendering devices in chain order (1–n). |
| `processing_speed` | `Option<f64>` | Multiple of real time (snapshot-sampled; informational — presence only is asserted); `build_snapshot` returns the instantaneous presence signal (`Some` iff any in-app alteration incl. volume-only, else `None`/hidden) and the publisher worker's `SpeedEma` (α=0.3) smooths `Some` samples across rebuilds, resetting on `None`. |
| `playback_status` | `PlaybackStatus` (reused from `crate::playback::state`; do NOT redefine) | `Playing` / `Paused` / `Stopped` — retained last-known path shows a paused/stopped ribbon instead of vanishing. |

**Validation**: `stages` is non-empty iff `track_id.is_some()`; first stage
is always `StageKind::Source`; last stage is always `Output` or
`ExternalRenderer`; `processing_speed.is_some()` iff at least one stage reflects
an in-app alteration (resample, bit-depth/DSD conversion, or DSP-volume scaling
incl. volume-only) — `None` when bit-perfect and when Limited-only with no
in-app alteration. The value is informational (snapshot-sampled, EMA α=0.3);
contracts assert presence only, never the number.

**State transitions**: rebuilt from scratch on track change, format change,
setting change (DSP volume/output mode), device change, or
`PlaybackStatus` change (play/pause/stop); generation bumps each rebuild so the
UI re-renders the status ribbon. Trigger mapping (no dedicated device/format
events exist): track/format changes arrive as `TrackStarted` with new
resampler/device facts re-read on every rebuild; setting changes as
`VolumeChanged`/`OutputModeChanged`; device changes as
`DeviceLost`/`OutputModeChanged`/`TrackStarted`; status changes as
`Paused`/`Resumed`/`Stopped` (`PositionTick`, `QueueChanged`, `Seeked`,
`GaplessEnabledChanged`, and `Error` never rebuild). Manual device
reselection emitting none of these, and mute toggles (`apply_muted` emits no
event), are out-of-scope follow-ups. Paused/stopped retains `track_id` plus the
last snapshot + status flag; empty state applies iff `track_id.is_none()`.
Device loss rebuilds with the Output stage in lost-device state and verdict
forced off Bit-Perfect.

## 2. PathStage (spec: Path Stage)

One node in the chain, in execution order.

| Field | Type | Description |
|---|---|---|
| `position` | `u32` | Zero-based order in `stages`. |
| `kind` | `StageKind` | Source, Authentication (MVP: omitted), Decoder, BitDepthConverter, SampleRateConverter, FormatConverter (incl. DSD-to-PCM and source-vs-device Channel Conversion — both builder-emitted), Volume (MVP: DSP-volume only), Effect (MVP: `describe()`-only vocabulary, incl. channel-map — never builder-emitted), Transport (MVP: ALSA direct-exclusive/shared-mixer/USB), Output, ExternalRenderer (MVP: title-only). |
| `title` | `String` | Concise label, e.g. `Bit Depth Conversion 24bit to 64bit Float`, `ALSA Direct Output`. |
| `detail` | `String` | Blue detail line: formats, `96kHz to 192kHz`-style input→output, dB values (`format_volume_db`), Linux transport modes. Unknown source fields render as `unknown`. |
| `verdict` | `QualityVerdict` | Per-stage indicator (color/shape + text, never color alone). |
| `badge_icon` | `&'static str` | Circular badge symbolic icon name (codec/device/effect). |
| `explanation` | `String` | Plain-language what-it-does + effect-on-quality text, incl. input→output values where applicable. |

**Validation**: `title`/`detail`/`explanation` are non-empty (`explanation` = 1–2 plain sentences + input→output values where applicable); converter
stages always carry both input and output values in `detail`; volume stages
always carry the dB value; authentication stages appear only when provider auth
facts are present, otherwise omitted (MVP: `auth` always `None`, stage omitted);
external-renderer stages are title-only in MVP (filter/modulator always `None` —
no output/device source fields exist); DSD sources undergoing volume/DSP always produce
an explicit `FormatConverter` (DSD-to-PCM) stage — silent omission is a
contract violation (SC-003: zero silent alterations). MVP omission: the builder
never emits `Effect` stages or leveling/headroom `Volume` variants (no engine
source); the `Volume` stage covers the single DSP volume control only, while
`describe()` still covers every `StageFacts` variant at the pure-function level.

## 3. QualityVerdict (spec: Quality Verdict)

| Variant | Meaning | Indicator |
|---|---|---|
| `BitPerfect` | Unaltered, bit-perfect. | Dedicated icon + label `Bit-Perfect`. |
| `Processed` | Intentionally altered/enhanced. | Dedicated icon + label `Processed`. |
| `Limited` | High quality but constrained (shared mixer, forced downsample, lost device). | Dedicated icon + label `Limited`. |

**Resolution (whole path)**: `Limited > Processed > Bit-Perfect` — any
`Limited` stage forces `Limited`; else any `Processed` stage forces
`Processed`; else `Bit-Perfect`. Canonical labels are exactly
`Bit-Perfect` / `Processed` / `Limited` in both header and player badge.

## 4. RenderingDevice (spec: Rendering Device)

| Field | Type | Description |
|---|---|---|
| `display_name` | `String` | From CPAL `DeviceInfo` (never empty; falls back to device id). |
| `role` | `DeviceRole` | `TransportTarget` / `Output` / `ExternalRenderer`. |
| `brand_visual` | `String` | Generic symbolic icon name for the brand/device tile; fallback icon when the brand is unknown (never hardcoded per-brand artwork). |
| `illustration` | `String` | Outline illustration icon name (fallback when device-specific art is unknown). |
| `manual_url` | `Option<String>` | `Some` only when manual info is known from explicit optional user config/catalog; card shows `View Product Manual` link iff `Some`. Never hardcoded or bundled. MVP: always `None` (no device-manual config key exists yet — link hidden; contract tests inject `RenderingDevice` fixtures directly). Future: optional `device_manuals` user-config map (device id/name → manual URL) may populate it. |

**Validation**: every snapshot with a track lists exactly 1 device in MVP (the
active output device); streamer + separate DAC producing ≥2 cards in chain order
is a deferred forward rule (no second-device input facts exist in MVP; contract
tests inject `RenderingDevice` fixtures directly); cards never name a device that
is not rendering (SC-005). The manual link is hidden in MVP (`manual_url` always
`None`); it appears iff `Some` once the optional `device_manuals` user-config map lands.

## 5. Relationships

```text
SignalPathSnapshot 1 ── * PathStage   (ordered by position)
SignalPathSnapshot 1 ── * RenderingDevice (chain order, roles reference
                         Transport/Output/ExternalRenderer stages)
PathStage * ── 1 QualityVerdict
SignalPathSnapshot 1 ── 1 QualityVerdict (resolved, not stored per-device)
```

Inputs (read-only, owned elsewhere; the worker performs the async catalog
`get_track` lookup and all `parking_lot` clones *before* calling the synchronous
`build_snapshot` — no async I/O runs inside the builder or while holding a lock):
`Decoder::params` + `TrackAudio` (Source/Decoder facts), resampler in/out rates +
channel counts (converter stages), `PlaybackState::volume/muted/output_mode`
(DSP-volume + verdict facts; there are no leveling/headroom/EQ/crossover inputs —
effect vocabulary is `describe()`-only in MVP), `AudioOutput` id/name/rate/channels/mode
(transport/output/device facts; device nativeness for the bit-perfect rule is derived
by comparing track/decoder rate, depth, and channels against device rate/channels
under `OutputMode::BitPerfect` — no separate native-capability input),
`PlaybackEvent::DeviceLost` (lost-device state), external-renderer filter/modulator
always `None` in MVP (title-only),
`SnapshotInput::auth: Option<AuthFacts>` (`AuthFacts { provider: String, verified: bool }`;
MVP always `None`, so the Authentication stage is omitted — populated only when
streaming-provider auth facts exist for the playing source). Snapshot-sample
timing for the processing-speed estimate arrives via `SnapshotInput` timing
fields (`sampled_at_wall: u64` monotonic wall-clock nanos plus cumulative
decoded/resampled frame counters, all captured by the publisher worker before
the call); the EMA (α=0.3) state lives in the publisher worker across polls,
never in the immutable snapshot — `build_snapshot` stays pure over one input.

## 6. Error type

`SignalPathError` (`thiserror`, documented enum + variants + `#[from]`
sources): `CatalogLookup`, `NoActiveTrack` (empty state, not a failure for
the UI — maps to the empty-state view), `SnapshotBuild`. UI boundary maps
these to `anyhow::Result` with `.context()`.
