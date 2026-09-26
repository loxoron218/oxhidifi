# Contract: Snapshot Builder API (`002-signal-path`)

**Boundary**: `src/playback/signal_path/` (builder + classifiers) ↔
`src/ui/signal_view/signal_tab.rs` (renderer) and `tests/signal_inspector.rs`
(test target `signal_path`).
This is the integration-test boundary for the feature: any contract change
REQUIRES updating the acceptance tests.

## API

```rust
/// Build one immutable snapshot from cloned engine/catalog state.
///
/// # Arguments
/// * `input` - Cloned track facts (`Track.audio` / `TrackAudio` via async
///   `get_track`), decoder params (`AudioParams`), pipeline facts (resampler
///   in/out rates, channel counts), playback facts (volume, mute, output
///   mode, status), and output facts (device id/name/rate/mode). Provider
///   auth facts are optional input (`SnapshotInput::auth: Option<AuthFacts>`
///   with `AuthFacts { provider: String, verified: bool }`; MVP always `None`):
///   no Authentication stage is emitted when absent.
///
/// # Returns
/// * `Result<SignalPathSnapshot, SignalPathError>` - Ready-to-render
///   snapshot, or a typed build error.
pub fn build_snapshot(input: &SnapshotInput) -> Result<SignalPathSnapshot, SignalPathError>;

/// Classify the whole path from per-stage verdicts.
///
/// Precedence: Limited > Processed > Bit-Perfect. Total function — every
/// stage list, including empty, maps to exactly one verdict (`None`-track
/// snapshots never reach this function; the UI shows the empty state).
///
/// # Arguments
/// * `stages` - Per-stage verdicts in chain order.
///
/// # Returns
/// * `QualityVerdict` - Resolved whole-path verdict.
pub fn resolve_verdict(stages: &[QualityVerdict]) -> QualityVerdict;

/// Render the overflow-menu "Copy path summary" plain-text summary.
///
/// # Arguments
/// * `snapshot` - Snapshot to summarize.
///
/// # Returns
/// * `String` - Multi-line `Title — detail` chain with header verdict.
pub fn summarize_text(snapshot: &SignalPathSnapshot) -> String;

/// Describe one stage kind with input→output facts.
///
/// # Arguments
/// * `kind` - Stage kind plus its numeric/string parameters.
///
/// # Returns
/// * `(String, String, String)` - (title, detail, plain-language explanation).
pub fn describe(kind: &StageFacts) -> (String, String, String);
```

## Invariants (MUST hold; asserted in `tests/signal_inspector.rs`, target `signal_path`)

1. Bit-perfect input (native format, `BitPerfect` mode, unity/unmuted volume,
   matching channels, device matching derived nativeness — track/decoder rate,
   depth, and channels equal device rate/channels under `OutputMode::BitPerfect`,
   no separate native-capability input) ⇒ verdict `Bit-Perfect` and
   `processing_speed.is_none()`.
2. Any DSP-volume scaling (incl. volume-only), resample, bit-depth or
   DSD-to-PCM conversion ⇒ verdict ≥ `Processed` and
   `processing_speed.is_some()`. (Leveling/headroom/EQ/effect inputs do not
   exist in MVP; their `describe()` wording is covered by invariant 5, not by
   builder emission.)
3. Shared-mixer transport, forced downsampling, or lost device ⇒ verdict
   `Limited` even when enhancements are also present (`processing_speed`
   still follows invariant 2: `Some` iff an in-app alteration is present,
   else `None` for Limited-only paths).
4. `summarize_text` lists every stage exactly once, in order, with the header
   verdict on the first line.
5. `describe` never returns empty strings (`explanation` = 1–2 plain sentences
   + input→output values where applicable); unknown source fields render as
   `unknown`; authentication stages are emitted only when provider auth facts
   are present (MVP: always omitted); external-renderer stages are title-only
   in MVP (filter/modulator always `None` — no source fields exist);
   DSD + volume/DSP input always yields a `FormatConverter` stage.
