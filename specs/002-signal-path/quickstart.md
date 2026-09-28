# Quickstart: Validate the Signal Path Inspector (`002-signal-path`)

**Spec**: `spec.md` | **Plan**: `plan.md` | **Contracts**: `contracts/`

## Prerequisites

- Linux desktop with GTK4 + Libadwaita 1.10 runtime; at least one audio
  output (PipeWire/PulseAudio virtual device is fine).
- Rust toolchain; repo at branch `002-signal-path`.
- Sample library: a lossless FLAC (e.g. 44.1 kHz/16-bit), a hi-res FLAC
  (e.g. 96 kHz/24-bit), and — if available — a DSF/DSD file.

## Setup

```bash
cargo collate
cargo build
```

## Validation scenarios

Map each scenario to its contract (`contracts/snapshot.md` = S,
`contracts/dialog.md` = D) and spec story.

### 1. Bit-perfect chain (Story 1 + 2; S §1, D badge/header)

1. Set output mode to bit-perfect, volume to maximum (0 dB, no DSP scaling).
2. Play the 44.1 kHz/16-bit FLAC; open the `Signal` tab from the player badge button (badge navigates to the tab, no dialog).
3. **Expect**: header verdict `Bit-Perfect`; chain Source → Decoder (iff live decoder facts are present, otherwise omitted) →
   transport (`ALSA direct/exclusive` wording) → Output; no
   processing-speed readout; badge matches header (SC-001, SC-002 — timing and
   human-classification parts are manual gates, not automated).

### 2. Processed chain with readout (Story 2 + 4; S §2, D readout)

1. Lower the volume slightly (or enable resampling via output mode), keep playing.
2. **Expect**: verdict flips to `Processed` live without leaving the tab;
   a volume row with dB value appears; `Processing speed: {x.x}x` readout (one
   decimal, header area below the verdict/zone line) is visible (volume-only
   MUST trigger it; hidden when bit-perfect and when Limited-only without
   in-app alteration).

### 3. Limited chain wins (Story 2; S §3)

1. Route playback through the shared system mixer (`default`/PipeWire device)
   or force a downsampling output.
2. **Expect**: verdict `Limited` even if a DSP-volume enhancement is also
   active (Limited > Processed > Bit-Perfect).

### 4. Stage explainer (Story 3; D stages)

1. Select the Source row, then a converter row (e.g. sample-rate).
2. **Expect**: row expands inline with its plain-language explanation within 1 s, including the
   input→output transformation; readable on a narrow window; every row is
   keyboard-selectable with a spoken title + detail (SC-004 — rendering asserted
   by GTK interaction test; paraphrase comprehension is a manual gate).

### 5. Device footer (Story 4; D footer)

1. Play to a named device; scroll to the footer.
2. **Expect**: MVP shows exactly one device card with the rendering device's
   display name; the `View Product Manual` link is hidden (`manual_url` always
   `None` — shown iff `Some` once the `device_manuals` map lands); streamer + DAC
   multi-card chains are deferred (SC-005 — naming asserted by contract test with
   injected fixtures; link/count targets apply once the deferred inputs land).

### 6. Gapless + edge cases (edge cases; D liveness)

1. Queue the 44.1 kHz track followed by the 96 kHz track with gapless on.
2. **Expect**: chain swaps atomically at the transition — no mixed-format
   frame (SC-006); long (8+) chains scroll with header/footer reachable.
3. Pause with the view open → last path stays with a paused ribbon; stop →
   stopped ribbon; close queue → empty state with start-playback guidance.
4. Unplug/disable the device mid-playback → Output row reflects the loss and
   the verdict is NOT `Bit-Perfect`. Play the DSD file with volume applied
   → an explicit DSD-to-PCM stage is shown.

## Known MVP limitations (validate as documented, do not file as regressions)

- Mute toggles emit no event (`apply_muted`): a newly-muted/unmuted state is
  classified on the next rebuild, not live (FR-011/T036).
- Manual device reselection that emits none of `DeviceLost`/`OutputModeChanged`/
  `TrackStarted` keeps the last rebuilt device until the next enumerated event
  (FR-011/T036).
- Kebab About action presents the existing PreferencesDialog default view as the
  About-equivalent info landing; a dedicated About page is a separately-tracked
  follow-up with no task in this feature (FR-014/T032).

## Automated checks

```bash
cargo clippy --fix --allow-dirty --all-targets --all-features && cargo fmt
cargo test --test signal_path   # acceptance: FR-001..FR-015, SC-002/SC-003/SC-006
cargo test                      # full suite must stay green
```

(`cargo bench` only if the audio pipeline itself changed — this feature is
display-only, so existing `throughput`/`conversion_baseline` just keep
passing. See `contracts/snapshot.md` invariants for what the acceptance
tests assert.)
