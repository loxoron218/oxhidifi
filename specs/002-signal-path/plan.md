# Implementation Plan: Audio Signal Path Inspector

**Branch**: `002-signal-path` | **Date**: 2026-09-26 | **Spec**: `specs/002-signal-path/spec.md`

**Input**: Feature specification from `/specs/002-signal-path/spec.md`

## Summary

Recreate Roon's Signal Path as a read-only, inspect-and-learn view for
`oxhidifi`: a player-area quality badge button (Bit-Perfect / Processed / Limited)
plus a persistent third library tab named `Signal` showing the live vertical
stage chain from source file to output device, per-stage inline expandable
explanations, a single-device footer card (manual link hidden in MVP), and a
processing-speed readout whenever in-app alteration is active. MVP scope: single
active output device, no EQ/leveling/headroom engine inputs (display vocabulary
with `describe()` coverage only), `auth` always `None`, `manual_url` always
`None`, external-renderer title-only; multi-card chains, device-manual config,
provider auth, and dedicated About content are deferred follow-ups. Approach from
research: an immutable `SignalPathSnapshot` built off-thread from existing
`EngineShared`/catalog state, classified by pure verdict/describe functions,
polled atomically by a single responsive `ViewStack` tab page in
`src/ui/signal_view/` (badge navigates to the tab) — no new crates, no schema
migration, no audio-hot-path changes, no overlay dialog/popover/sheet except
the standard `MenuButton` kebab popover.

## Technical Context

**Language/Version**: Rust, Edition 2024 (`src/`, `tests/`, `benches/` file
stems unique codebase-wide; ≤400 lines/file; module depth ≤2).

**Primary Dependencies**: `libadwaita 0.9.2` (`gio_v2_80`, `gtk_v4_24`,
`v1_10`, programmatic widgets only), `cpal 0.18.2`, `alsa` (Linux target),
`symphonia 0.6.1` (`all`), `rtrb 0.4`, `rubato` (`fft_resampler`), `lofty`
(`id3v2_compression_support`), `num-traits`; `tokio` (`macros`,
`rt-multi-thread`, `signal`), `async-channel` (`std`), `parking_lot`,
`rayon`; `sqlx` (`macros`, `runtime-tokio`, `sqlite`), `serde` + `serde_json`,
`sha2`, `hex` (`alloc`); `anyhow`, `thiserror`, `notify`, `tracing` +
`tracing-subscriber` (`ansi`, `env-filter`, `json`) + `tracing-appender`.
`crossbeam`, `dynosaur`, `tokio-stream`, `tokio-util`, `regex` are commented
out in `Cargo.toml` and are NOT used.

**Storage**: SQLite via `sqlx` (`Track`/`TrackAudio` catalog records as
snapshot input). No migration: the signal path is ephemeral live state, not
persisted data. N/A for new tables.

**Testing**: `cargo test` — unit tests in `#[cfg(test)] mod tests` per file
(`anyhow::Result` + `ensure!`/`bail!`; trivial `()` + `assert!`); GTK tests
via `libadwaita::gtk::{self, test}` + plain `#[test]`; integration/acceptance
in `tests/` with `//!` FR headers (new: `tests/signal_inspector.rs`, target `signal_path`,
default ungated — the `verification-tests` feature gate does not apply); deterministic
newest-wins unit test for the poll generation guard (T015, constitution Principle II
simulation-style coverage for the concurrency-sensitive swap); `criterion`
benches (`throughput`, `conversion_baseline`) only if the audio pipeline
changes (it does not — display-only feature).

**Target Platform**: Linux desktop (GTK4 + Libadwaita 1.10), ALSA
direct/exclusive and shared/system-mixer outputs, USB output (MVP transports);
network/streaming transports are deferred (no streaming-provider input — see FR-005 MVP scope).

**Project Type**: Single-binary desktop app (`src/main.rs`, `src/lib.rs`).

**Performance Goals**: Stage-selection explanation renders <1 s (SC-004);
chain swaps atomically on gapless format change with zero mixed-frame display
(SC-006); verdict identifiable <10 s on first use (SC-001); GTK main thread
never blocked (poll 100–500 ms `timeout_add_local` + `try_recv` drain;
workers own I/O/decode); audio hot path untouched (zero-alloc, lock-free,
`rtrb` transport preserved).

**Constraints**: Constitution v1.2.0 gates — `cargo collate` clean;
`clippy::pedantic` + `clippy::nursery` denied, zero warnings, no
`#[allow]`/`#[expect]`; 4-block imports; `//!`/`///` docs; no `unsafe`;
HIG layout (`ToolbarView`+`HeaderBar`, `Toast` feedback, 6 px spacing, no
hardcoded radii/colors); verdicts distinguishable without color vision;
full keyboard + screen-reader operability.

**Scale/Scope**: FR-001..FR-015 (15 requirements), 4 entities
(`SignalPathSnapshot`, `PathStage`, `QualityVerdict`, `RenderingDevice`),
chains of 1–8+ stages, footer with exactly one device card in MVP (multi-card chains deferred), one library tab + one player
badge button navigating to it.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Code Quality**: PASS — new modules follow capability grouping
  (`playback::signal_path`, `ui::signal_view`), unique stems
   (`signal_path`, `path_snapshot`, `path_verdict`, `stage_build`,
   `stage_output`, `stage_describe`, `signal_tab`, `signal_footer`,
   `signal_publish`, `signal_poll`, `signal_badge`), parent-index style, ≤400 lines/file,
  depth ≤2, programmatic widgets, 4-block imports, `//!`/`///` docs, no
  hardcoding (device/manual data from runtime state, theme-aware styling).
- **II. Testing Standards**: PASS — unit tests per new file; `tests/`
   acceptance with `//!` FR-001..FR-015 header; `tempfile` fixtures where
   files are needed; deterministic snapshot/verdict tests (pure functions over
   cloned builder input, so the builder itself needs no concurrency simulation)
   plus a deterministic newest-wins unit test for the poll generation guard (T015 —
   the one concurrency-sensitive swap rule); no audio-pipeline change ⇒ no new `criterion` bench required
  (existing `throughput`/`conversion_baseline` keep passing).
- **III. UX Consistency**: PASS — HIG tab (`ToolbarView`+`HeaderBar`, `ListBox`
  chain, `MenuButton` kebab popover as sole popover exception), overflow
  menu with exactly the 3 read-only actions, `Toast` on copy, adaptive
  `ViewStack` + `ViewSwitcher`/`ViewSwitcherBar` tab pattern (no overlay
  dialog/popover/sheet), main-thread discipline (poll + `try_recv`
  per-subscriber mailbox, workers own I/O), non-color verdict cues +
  keyboard/AT support.
- **IV. Performance**: PASS — display-only; snapshot built off-thread from
  cloned state under minimal lock scopes; no hot-path allocation, locks, or
  callback instrumentation; approved primitives only
  (`parking_lot`, `async-channel`, `tokio`); processing speed is
  snapshot-sampled, not callback-measured.
- **V. Observability & Error Handling**: PASS — structured `tracing` fields
  on snapshot builds/verdict flips; typed `thiserror` error enum for the
  builder (documented variants, `#[from]` sources); UI boundary uses
  `anyhow::Result` + `.context()`; no discarded errors.

*Post-design re-check (after Phase 1): no violations introduced — data model
is in-memory value types, contracts are Rust API + UI-behavior docs,
quickstart is a validation guide. No Complexity Tracking entries needed.*

## Project Structure

### Documentation (this feature)

```text
specs/002-signal-path/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
│   ├── snapshot.md      # Builder API + verdict precedence contract
│   └── dialog.md        # Signal tab UI-behavior contract (tab-only; historic name, covers tab + badge)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── playback/
│   ├── signal_path.rs            # Parent index: snapshot/verdict/describe API + error type
│   └── signal_path/
│       ├── path_snapshot.rs      # Immutable snapshot builder from EngineShared + catalog
│       ├── path_verdict.rs       # Per-stage + whole-path verdict (Limited > Processed > Bit-Perfect)
│       ├── stage_build.rs        # Source/decoder/converter/volume stage constructors behind build_snapshot
│       ├── stage_output.rs       # Transport/output/footer-device stage constructors behind build_snapshot
│       └── stage_describe.rs     # Titles, details, plain-language explanations per StageKind
└── ui/
    ├── signal_view.rs            # Parent index: Signal tab capability (ViewStack page `signal`)
    ├── signal_view/
    │   ├── signal_tab.rs         # Tab page: header verdict, ListBox chain with inline explainers, MenuButton kebab
    │   ├── signal_footer.rs      # Device footer card(s), generic art + device name
    │   ├── signal_publish.rs     # Off-thread publisher: event subscription, rebuilds, per-subscriber mailbox feed
    │   └── signal_poll.rs        # timeout_add_local poll + try_recv drain, whole-snapshot swap
    └── player/
        └── signal_badge.rs       # Player-area verdict badge button navigating to the Signal tab

tests/
└── signal_inspector.rs         # Acceptance tests (test target `signal_path`)
```

**Structure Decision**: Single-project desktop-app layout. Name map: `playback::signal_path`
(snapshot/verdict/describe truth) vs `ui::signal_view` (tab presentation: `signal_tab`,
`signal_footer`, `signal_poll`) vs `ui::player::signal_badge` (entry badge) vs
`tests/signal_inspector.rs` (acceptance file) run as test target `signal_path` vs
`contracts/dialog.md` (historic name — specifies the tab-only view, no dialog is built).
Playback truth stays in `src/playback/` (engine/decoder/resampler/volume/output owners
unchanged; new `signal_path/` capability group reads them). Presentation
is a third library tab: `src/ui/signal_view/` tab page (`signal_tab`,
`signal_footer`, `signal_poll`) added to the existing `ViewStack` in
`src/ui/panes.rs` next to Albums/Artists with `ActiveTab::Signal` persistence
(new `Signal` variant in `src/storage/active_tab.rs` plus settings/persistence
round-trip — see tasks T013; T013 is authoritative for the full existing-caller list),
plus a verdict badge button in `src/ui/player/` that navigates to the tab.
No new top-level domains, no `models/`/`utils/` groupings, all new stems
verified unique (`signal_handlers` is the only near-collision and is
distinct). If `signal_tab.rs` approaches the 400-line gate, pre-split header +
chain rows into `signal_header.rs` / `signal_chain.rs` (new stems must stay
unique codebase-wide).

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — (none) | — | — |
