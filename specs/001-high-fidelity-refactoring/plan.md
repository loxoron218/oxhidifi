# Implementation Plan: High-Fidelity Music Player Refactoring

**Branch**: `001-high-fidelity-refactoring` | **Date**: 2026-05-22 | **Spec**: `specs/001-high-fidelity-refactoring/spec.md`

**Input**: Feature specification from `specs/001-high-fidelity-refactoring/spec.md`

## Summary

Refactor oxhidifi into a high-fidelity GTK4/Libadwaita desktop music player with gapless bit-perfect playback. The application provides library management (metadata extraction, filesystem watching, dedup), browsable Albums/Artists views with grid/column toggles and detail pages, a slide-in player panel, and robust observability — all built with modern idiomatic Rust.

## Technical Context

**Language/Version**: Rust Edition 2024 (stable)

**Primary Dependencies**: `cpal` (audio device abstraction), `symphonia` (codec decoding), `rtrb` (lock-free ring buffers), `lofty` (metadata parsing), `rubato` + `audioadapter-buffers` (resampling), `tokio` (async runtime), `libadwaita`/`gtk4-rs` (UI), `sqlx` (SQLite), `serde` + `serde_json` (settings persistence), `notify` (file watching), `tracing` + `tracing-subscriber` (observability), `crossbeam` (concurrency), `rayon` (data parallelism), `parking-lot` (fast mutexes)

**Storage**: SQLite via `sqlx` for library catalog, queue state, and metadata cache. JSON files via `serde` at XDG config/data paths for user settings.

**Testing**: `cargo test` — unit tests at bottom of source files, integration tests for contract boundaries (audio pipeline, storage layer), `criterion` benchmarks for audio hot paths

**Target Platform**: Linux (GNOME desktop environment), PulseAudio/PipeWire audio server

**Project Type**: desktop-app (GTK4/Libadwaita, programmatic widgets only)

**Performance Goals**: < 3s play initiation (click-to-audio), gapless track transitions (zero audible gap), bit-perfect output (bit-identical compare), < 100ms UI response, < 500ms player panel reveal

**Constraints**: Lock-free audio pipeline, zero heap allocation on audio hot path (pre-allocated buffers only), < 200MB steady-state memory, offline-only (local filesystem), stereo output only

**Scale/Scope**: 10k+ tracks per library, single user, local filesystem paths

## Constitution Check

*GATE: Must pass before research phase (research.md created as Phase 0 output). Re-check after Phase 1 design.*

| Gate | Check | Verifier | Status |
|------|-------|----------|--------|
| **Code Quality (Principle I)** | `cargo clippy --fix --allow-dirty --all-targets -- -W clippy::pedantic` exits 0 | CI | PASS (no existing code) |
| **Code Quality (Principle I)** | `cargo fmt --check` exits 0 | CI | PASS (no existing code) |
| **Code Quality (Principle I)** | No `#[allow(...)]` attributes in `src/` | `rg '#\[allow\(' src/` returns empty | PASS (no existing code) |
| **Code Quality (Principle I)** | No `unsafe` blocks in `src/` | `rg 'unsafe ' src/` returns empty | PASS (no existing code) |
| **Code Quality (Principle I)** | No `unwrap()`/`expect()`/`panic!()` in `src/` | `rg '\.(unwrap\|expect)\|panic!' src/` returns empty | PASS (no existing code) |
| **Code Quality (Principle I)** | No file in `src/` exceeds 400 lines | `awk 'length>400' src/**/*.rs` returns empty | PASS (no existing code) |
| **Code Quality (Principle I)** | No `models/`/`handlers/`/`utils/` directories | `ls src/` | PASS (no existing code) |
| **Code Quality (Principle I)** | No `.ui`/`.xml`/`.blp` files in repo | `find . -name '*.ui' -o -name '*.xml' -o -name '*.blp'` returns empty | PASS (no existing code) |
| **Testing (Principle II)** | Unit tests at bottom of every source file | Code review | PASS (new code only) |
| **Testing (Principle II)** | `criterion` benchmark for every audio hot path | `cargo bench --no-run` succeeds | PASS (new code only) |
| **Testing (Principle II)** | Integration tests for contract boundaries | `cargo test --test '*'` succeeds | PASS (new code only) |
| **UX (Principle III)** | Navigation uses `AdwNavigationSplitView`/`AdwNavigationView`/`AdwOverlaySplitView` + `AdwBreakpoint` | Code review + `rg 'AdwLeaflet' src/` returns empty | PASS (amended Principle III) |
| **UX (Principle III)** | 6px spacing scale (no hardcoded radii) | `rg 'border-radius' src/` returns empty | PASS (no existing code) |
| **UX (Principle III)** | 200ms ease transitions for motion | Code review | PASS (no existing code) |
| **Performance (Principle IV)** | Hot paths use `rtrb` (lock-free ring buffers) | Code review | PASS (designed in) |
| **Performance (Principle IV)** | Resampling uses `rubato` + `audioadapter-buffers` | Code review | PASS (designed in) |
| **Observability (Principle V)** | Library crates use `thiserror` with documented variants | Code review | PASS (designed in) |
| **Observability (Principle V)** | No `let _` / `.ok()` in `src/` | `rg 'let _\|\.ok\(\)' src/` returns empty | PASS (no existing code) |

## Phases

The plan is decomposed into 14 phases, each producing a checkpoint. Phase dependencies are listed in `tasks.md` § "Phase Dependencies".

| # | Phase | Priority | Output | Tasks |
|---|-------|----------|--------|-------|
| 1 | Setup | — | Cargo scaffold, lint config, test infra, criterion harness | T001–T004d |
| 2 | Foundational | — | Storage trait, SQLite impl, settings, error types, XDG | T005–T010b |
| 3 | US1a Library Ingestion | P1 (MVP) | Scanner, metadata, dedup, events | T011–T013, T018–T018c |
| 4 | US1b Playback Pipeline | P1 (MVP) | Decoder, output, queue, engine | T014–T017 |
| 5 | US1c UI Shell & Browsing | P1 (MVP) | App, window, header, album grid, artwork cache, play wiring, adaptive layout, a11y, window-geometry restore | T019–T023, T054, T019b–T019d |
| 6 | US2 Empty State & Nav | P1 | Empty state, artist view, grid/column toggle, watcher, status bar, tab switching, scan+status integration test | T025–T031, T031b |
| 7 | US3 Gapless Resampling | P2 | Resampler (with criterion baseline), gapless, pre-buffer, sample-rate reconfig, bit-perfect path, ABX harness, hi-res, SNR, bit-perfect verify, SC-002 verification, incompatible SR transitions | T032–T036, T032b, T036b–T036i |
| 8 | US4 Side Panel | P2 | Slide-in panel, state wiring, narrow-mode back nav, auto show/hide, queue view UI | T037–T040, T040b |
| 9 | US5 Detail Pages | P3 | Album detail, artist detail, navigation, play/queue actions | T041–T044 |
| 10 | Metrics & Instrumentation | — | Playback-latency, scan-throughput, UI-response, panel-reveal, memory metrics collectors; structured tracing instrumentation | T046a–T046e, T047 |
| 11 | Edge Case Handling | — | Device disconnection, no-device startup, corrupted files, empty queue, large library handling | T048a–T048f |
| 12 | UI Polish & Accessibility | — | Full a11y audit, adaptive layout polish, HIG compliance audit | T045, T053, T055 |
| 13 | Preferences & Configuration | — | PreferencesDialog, gapless playback toggle | T051, T051b |
| 14 | Code Quality & Final Verification | — | Clippy/fmt, quickstart validate, 10k library load verification, UI response verification, queue persistence, multi-format e2e test, library persistence, settings persistence, SC-006 verification | T049, T050, T052–T052c, T056–T059 |

**Sub-task suffix convention**: tasks suffixed with a letter (`T004b`, `T019b`, `T032b`, `T036b`, `T040b`, `T046a`, `T048a`, `T052b`) are sub-tasks of the parent task. The parent's ID is implied by stripping the letter suffix (e.g., `T046a`–`T046e` are sub-tasks of `T046`). When a parent task is renamed, its sub-tasks are renamed in lockstep (e.g., the original T024 was renamed to T018b and its sub-task T024b became T018c; the Phase 5 tasks originally labelled T024c/T024d became T019b/T019c; T024 no longer exists in the current task list).

## Project Structure

### Documentation (this feature)

```text
specs/001-high-fidelity-refactoring/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
└── tasks.md             # Phase 2 output (created by /speckit.tasks)
```

### Source Code (repository root)

```text
src/
├── main.rs              # Application entry point
├── app.rs               # Libadwaita Application setup
├── library/             # Library scanning & metadata
│   ├── mod.rs
│   ├── scanner.rs       # Filesystem scan & index
│   ├── metadata.rs      # Metadata extraction (lofty)
│   ├── artwork.rs       # Artwork extraction, caching, thumbnails
│   ├── dedup.rs         # Layered duplicate detection
│   └── watcher.rs       # Filesystem change monitoring (notify)
├── storage/             # Persistence layer
│   ├── mod.rs
│   ├── database.rs      # SQLite schema & queries (sqlx)
│   └── settings.rs      # XDG-based settings (serde_json)
├── playback/            # Audio pipeline
│   ├── mod.rs
│   ├── engine.rs        # Playback orchestrator
│   ├── decoder.rs       # Symphonia decoder bridge
│   ├── output.rs        # CPAL device output
│   ├── resampler.rs     # Rubato resampling
│   ├── queue.rs         # Playback queue
│   └── gapless.rs       # Gapless transition logic
├── ui/                  # Libadwaita UI
│   ├── mod.rs
│   ├── window.rs        # Main ToolbarView window
│   ├── header.rs        # HeaderBar (tabs, view toggle)
│   ├── library/
│   │   ├── mod.rs
│   │   ├── albums.rs    # Album grid/column view
│   │   ├── artists.rs   # Artist grid/column view
│   │   └── empty.rs     # Empty state page
│   ├── detail/
│   │   ├── mod.rs
│   │   ├── album.rs     # Album detail page
│   │   └── artist.rs    # Artist detail page
│   ├── player/
│   │   ├── mod.rs
│   │   ├── panel.rs     # Slide-in player panel
│   │   └── queue.rs     # Queue view UI (track list, drag-and-drop, remove)
│   ├── settings.rs      # PreferencesDialog
│   └── status.rs        # Status bar (scanning indicator)
└── metrics/             # Observability & metrics
    ├── mod.rs
    └── collector.rs     # Performance metrics (tracing)
```

**Structure Decision**: Selected single-project layout grouped by capability/domain per Principle I. Each top-level module (`library/`, `storage/`, `playback/`, `ui/`, `metrics/`) represents a bounded domain capability. No `models/`, `handlers/`, or `utils/` directories.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified.**
> No violations currently exist — the design follows every principle. If a future change
> requires relaxing a MUST principle, document the violation, the simpler alternative that
> was rejected, and obtain team approval per the constitution's Governance section before
> proceeding.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _(none)_ | — | — |
