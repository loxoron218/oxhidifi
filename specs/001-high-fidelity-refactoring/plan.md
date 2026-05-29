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

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Code Quality Gate (Principle I)**: All committed code MUST pass `cargo clippy -- -W clippy::pedantic` and `cargo fmt`. No `#[allow]`, `unsafe`, or `unwrap()`/`expect()`/`panic!()` calls permitted. Each `.rs` file MUST NOT exceed 400 lines. Source files MUST be grouped by capability/domain — NEVER use `models/`/`handlers/`/`utils/`. Only `.rs` files permitted (no `.ui`/`.xml`/`.blp`). Status: **PASS** (project is empty scaffold; all new code will follow these rules).

**Testing Gate (Principle II)**: Unit tests at bottom of source files, integration tests for contract boundaries, `criterion` benchmarks for audio hot paths. Tests MUST be written and confirmed failing before implementation (red-green-refactor). Status: **PASS** (no existing violations; testing infrastructure to be established in Phase 1).

**UX Gate (Principle III)**: UI MUST follow GNOME HIG: `ToolbarView`/`HeaderBar`, programmatic Libadwaita widgets (never `GtkBox` layouts for navigation), `PreferencesDialog` for settings, accessible labels via `AccessibleProperty::Label`, keyboard navigation, `Toast` feedback, `AdwNavigationSplitView`/`AdwNavigationView`/`AdwOverlaySplitView` + `AdwBreakpoint` for responsiveness, 6px spacing scale, 200ms ease transitions, no hardcoded radii. Status: **PASS** (design will follow HIG from the start; research confirms the modern adaptive stack; constitution Principle III amended to match).

**Performance Gate (Principle IV)**: Audio pipeline changes MUST include `criterion` benchmarks showing no regression relative to baseline. Hot paths MUST be lock-free with zero heap allocation (pre-allocated buffers, `rtrb` for ring buffers). `tokio` for async I/O, `crossbeam` for message passing, `rayon` for CPU parallelism. `rubato` + `audioadapter-buffers` for resampling. Status: **PASS** (pipeline designed with these constraints).

**Observability Gate (Principle V)**: Structured `tracing` with typed fields for all diagnostic output. Library crates use `thiserror` with documented variants. Binary crate uses `anyhow` at top level only. NEVER use `let _`/`.ok()` — return errors with context. NEVER leak `anyhow::Error` across library boundaries. Status: **PASS** (error handling pattern established per constitution).

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

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| [e.g., 4th project] | [current need] | [why 3 projects insufficient] |
| [e.g., Repository pattern] | [specific problem] | [why direct DB access insufficient] |
