# Research: Audio Signal Path Inspector (`002-signal-path`)

**Date**: 2026-09-26 | **Spec**: `specs/002-signal-path/spec.md`

All findings below are grounded in the current codebase (`src/playback/*`,
`src/storage/catalog.rs`, `src/ui/player/*`) and the constitution v1.2.0.
No new crates and no DB migration are required.

## R-01: Source of live pipeline truth (snapshot inputs)

- **Decision**: Build an immutable `SignalPathSnapshot` off the GTK main thread
  from already-existing shared state, under minimal explicit lock scopes:
  `Decoder::params()` (`sample_rate`, `channels`, `bit_depth`,
  `duration_seconds`), the active resampler's input/output rate + channel count
  (`src/playback/pipeline.rs` `LoopCtx`), `PlaybackState` (`volume`, `muted`,
  `output_mode`, `status`, `current_track_id`, `current_path`),
  `AudioOutput` (`device_id`, `device_name`, `sample_rate`, `channels`, `mode`),
  and `TrackAudio` from `SqliteStorage::get_track` (`codec`, `format`,
  `sample_rate`, `bit_depth`, `channels`, `lossless`, `file_path`).
- **Rationale**: Every required fact already exists in `EngineShared` or the
  catalog; a read-only builder adds no audio-hot-path work and no schema
  change. Short `{ ... }` lock blocks plus `drop` keep contention off the
  decode loop.
- **Alternatives considered**: Pushing per-frame events to the UI (rejected:
  floods the main thread, violates main-thread discipline); adding a SQLite
  `signal_path` table (rejected: the path is ephemeral live state, not
  persisted catalog data); instrumenting the `rtrb`/CPAL callback (rejected:
  risks hot-path allocation and lock use, forbidden by Principle IV).

## R-02: Whole-path and per-stage verdict computation

- **Decision**: Pure functions over the snapshot. Per-stage verdict is assigned
  at build time from stage kind + parameters; the whole-path verdict resolves
  by precedence **Limited > Processed > Bit-Perfect** (per spec clarification).
  Bit-Perfect requires ALL of: `OutputMode::BitPerfect`, no resampler, no
  software volume scaling (slider `1.0`/unmuted or hardware-mixer path only),
  no channel-count conversion, and device native support for the track format.
  Any resample, bit-depth/DSD conversion, channel map, or volume/EQ effect
  yields Processed; shared-mixer output, forced downsampling, or a lost device
  yields Limited (Limited wins over any enhancement present).
- **Rationale**: A pure, total function is trivially unit-testable
  (`anyhow::Result` + `ensure!`) and makes SC-002/SC-003 deterministic: every
  altering stage is classified, zero silent alterations.
- **Alternatives considered**: Storing verdicts in `PlaybackState`
  (rejected: derived display state would go stale on setting changes);
  single boolean bit-perfect flag (rejected: spec mandates three states).

## R-03: Processing-speed readout definition

- **Decision**: Snapshot-sampled throughput multiple of real time
  (frames decoded + resampled per wall-clock second ÷ device rate, smoothed),
  computed in the off-thread builder — never instrumented in the audio
  callback. Shown iff any in-app alteration is active (conversion, effect, or
  volume/leveling/headroom, including volume-only); hidden when bit-perfect
  AND when Limited-only with no in-app alteration (shared-mixer/forced-downsample/lost-device
  alone keeps `processing_speed` as `None` — see `contracts/snapshot.md` invariants 2–3).
- **Rationale**: Matches the clarified requirement (volume-only triggers the
  readout) without touching the lock-free hot path. Sampling at snapshot
  cadence is accurate enough for a headroom indicator.
- **Alternatives considered**: Callback-side cycle counting (rejected:
  Principle IV forbids hot-path bookkeeping); `cpal` stream-timing queries
  (rejected: backend-specific, not portable across ALSA/PipeWire/PulseAudio).

## R-04: Linux transport and output wording

- **Decision**: Derive wording from `OutputMode` + CPAL device id/name:
  `hw:`-style devices in `BitPerfect` mode → "ALSA direct/exclusive";
   `default`/PipeWire/PulseAudio-named devices or `Resampled` mode → "ALSA
   shared/system mixer"; USB-identifying names → "USB output"; streamer
   hand-off → "network/streaming transport" (MVP: ALSA direct-exclusive /
   shared-mixer / USB only — network/streaming wording deferred per FR-005,
   since no streaming-provider input exists). `OutputMode` already round-trips
  through serde (`snake_case`), so persisted settings stay compatible.
- **Rationale**: Directly implements the spec's Linux-terms clarification
  using the existing `devices.rs` vocabulary (`alsa_card_name`,
  `prioritize_devices`, `startup_device_check`).
- **Alternatives considered**: WASAPI/CoreAudio terms (rejected: spec settled
  on Linux terms); probing ALSA mixer topology per device (rejected:
  fragile, blocks on I/O that must stay off the main thread).

## R-05: Tab presentation (tab-only, no dialog)

- **Decision**: One programmatic tab page owned by
  `src/ui/signal_view/`: persistent third `ViewStack` page named `signal`
  next to Albums/Artists, shown at all window sizes with narrow adaptation
  via the existing `ViewSwitcher`/`ViewSwitcherBar` pattern. No `.ui`/`.blp`
  files, no overlay dialog/popover/sheet except the standard `MenuButton`
  kebab popover.
  Structure: header (verdict + zone name + hint + 3-dot `MenuButton` overflow),
  `ScrolledWindow` tab content (vertical `ListBox` chain with rail styling, safe
  for 8+ stages, rows expand inline for explanations), footer (one device card per rendering device, in chain
  order). Builder pattern with `css_classes`, `tooltip_text`, `can_focus`,
  `set_use_underline(true)` mnemonics; 6 px spacing scale; theme-aware
  colors via style manager (no hardcoded radii/colors) for dark/light parity.
- **Rationale**: Implements the spec tab-only clarification (FR-012) and
  Constitution III (ToolbarView/HeaderBar navigation,
  Toast feedback, builder widgets, responsive switcher primitives); badge
  button navigates to the tab instead of anchoring a popover.
- **Alternatives considered**: `AdwDialog` compact card / full-screen sheet
  (rejected: spec clarifications mandate tab-only + `MenuButton` kebab, no
  dialog); `PreferencesDialog` (rejected: this is an
  inspect-and-learn display, not settings); custom `GtkBox` window
  (rejected: violates ToolbarView/HeaderBar navigation rule); separate
  mobile/desktop implementations (rejected: one responsive tab is
  maintainable and HIG-compliant).

## R-06: Live, atomic updates without blocking the main thread

- **Decision**: UI polls via `timeout_add_local` (100–500 ms) and drains a
  per-subscriber `async-channel` snapshot mailbox with `try_recv` (newest
  wins); each poll swaps the whole immutable snapshot when its generation
  counter or track id differs. Heavy work (catalog lookup, explanation
  strings, summary text) happens on worker threads; the main thread only
  rebuilds rows from the ready snapshot.
- **Rationale**: This is the constitutionally prescribed pattern (poll +
  `try_recv` drain, per-subscriber channels, no `spawn_local`+`recv().await`,
  no shared broadcast with mixed-speed subscribers). Atomic whole-snapshot
  swap guarantees gapless format changes never show a mixed chain (SC-006),
  and paused/stopped states retain the last snapshot with a status ribbon.
- **Alternatives considered**: Subscribing the dialog directly to
  `PlaybackEvent` broadcast (rejected: mixed-speed subscriber problem);
  in-place row mutation (rejected: risks mixed/stale chains mid-transition).

## R-07: Per-stage explanations and input→output wording

- **Decision**: `const`-friendly pure describe function
  `describe(stage) -> { title, detail, explanation }` covering every `StageKind`
  (source, authentication, decoder, bit-depth/sample-rate/format converters,
  volume variants with dB via existing `volume_to_db`/`format_volume_db`,
  EQ/crossover/crossfeed/channel-map, transport, output, external renderer).
  Unknown source fields render as "unknown", never guessed. DSD sources
  (DSF/DSD codec, `bit_depth: None` from symphonia) always emit an explicit
  DSD-to-PCM conversion stage when volume/DSP applies.
- **Rationale**: Reuses the canonical dB curve (`src/playback/volume.rs`) so
  displayed decibels match the audio path; explicit unknown/DSD handling
  implements the spec edge cases.
- **Alternatives considered**: Storing explanation strings in the DB
  (rejected: static content, belongs in code with `///` docs); generating
  text with `format!` interpolation inside tracing (unrelated; logging stays
  structured-field style per Principle V).

## R-08: Device footer cards and manual links

- **Decision**: Footer renders one card per rendering device in chain order
  (streamer + DAC each get a card). Display name comes from CPAL `DeviceInfo`;
  the "View Product Manual" link appears only when manual info is known
  (optional mapping, e.g. user config/catalog); cards never name a device
  that is not rendering. A lost device mid-playback flips the Output stage to
  a lost-device state and forces the verdict off Bit-Perfect.
- **Rationale**: Implements FR-009 and the disappeared-device / double-device
  edge cases with data already available (`AudioOutput::device_id/name`,
  `PlaybackEvent::DeviceLost`).
- **Alternatives considered**: Bundling a device/manual database
  (rejected: out of scope, hardcoded data forbidden); hiding the footer for
  unknown devices (rejected: spec wants the name always, link when known).

## R-09: Accessibility and non-color verdict cues

- **Decision**: Each stage is a keyboard-activatable `ListBoxRow` whose
  accessible name is "title plus detail"; verdict shown as text label plus a
  distinct symbolic icon/shape per state (Bit-Perfect / Processed / Limited),
  never color alone; overflow actions and copy confirmation (`Toast`) are
  keyboard-reachable. GTK tests use `libadwaita::gtk::{self, test}` + plain
  `#[test]`.
- **Rationale**: Satisfies FR-015 and the assumption that verdicts must not
  rely on color vision, using only existing icon/HIG building blocks.
- **Alternatives considered**: Color-only indicator dots like the reference
  screenshots (rejected: fails the non-color-cue requirement).

## R-10: What is NOT needed

- **Decision**: No new dependencies (`crossbeam`, `dynosaur`, `tokio-stream`,
  `tokio-util`, `regex` stay untouched — several are commented out in
  `Cargo.toml` and MUST NOT be assumed), no `unsafe`, no `#[allow]`/`#[expect]`
  suppressions, no `pub use` re-exports, no schema migration, no DSP editing
  in the view (read-only; overflow menu navigates to settings elsewhere).
- **Rationale**: Keeps the change surface minimal and the quality gates green
  (`cargo collate`, pedantic+nursery clippy, `cargo test`; `cargo bench`
  only if the audio pipeline itself changes — it does not).
