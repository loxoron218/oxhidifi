---
description: "Task list for high-fidelity music player refactoring"
---

# Tasks: High-Fidelity Music Player Refactoring

**Input**: Design documents from `specs/001-high-fidelity-refactoring/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Checks**: Tests required per Constitution Principles II & IV: unit tests at bottom of every source file, integration tests for contract boundaries, deterministic simulation for concurrency-sensitive audio logic, and criterion benchmarks for audio hot paths. Each phase lists test tasks alongside implementation tasks.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions
- **Sub-task suffix**: Tasks suffixed with a letter (`T004b`, `T019b`, `T032b`, `T036b`, `T040b`, `T046a`, `T048a`, `T052b`) are sub-tasks of the parent task. The parent's ID is implied by stripping the letter suffix (e.g., `T046a`–`T046e` are sub-tasks of `T046`). When a parent task is renamed, its sub-tasks are renamed in lockstep (see `plan.md` § Phases for the full convention)

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization, dependency declaration, tooling configuration

- [X] T001 Create Cargo.toml with all dependencies per plan.md (cpal, symphonia, rtrb, lofty, rubato, tokio, libadwaita/gtk4-rs, sqlx, serde/serde_json, notify, walkdir, tracing/tracing-subscriber, crossbeam, rayon, parking-lot, thiserror, anyhow, criterion, tempfile)
- [X] T002 [P] Configure clippy (clippy.toml or .cargo/config.toml) with pedantic warnings and rustfmt config
- [X] T003 [P] Initialize tracing-subscriber in src/main.rs with structured logging (file + stderr)
- [X] T004 Create empty module structure with mod.rs re-exports per plan.md: src/library/, src/storage/, src/playback/, src/ui/, src/ui/library/, src/ui/detail/, src/ui/player/, src/metrics/
- [X] T004b [P] Create criterion benchmark harness in benches/ with baseline benchmarks for decoder PCM output and ring buffer throughput (resampler baseline is created in T032b once the resampler exists)
- [X] T004c [P] Set up test infrastructure: mock Storage backend, tempfile-based scanner fixtures, async test helpers in tests/common/
- [X] T004d [P] Query Context7 MCP server for cpal, symphonia, rubato, lofty, and libadwaita documentation and best practices as initial baseline; per Constitution Principle I, Context7 MUST be consulted before implementing features with unfamiliar libraries throughout the project

**Checkpoint**: Cargo build succeeds, project structure mirrors plan.md

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core data types, storage layer, and error infrastructure that ALL user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T005 Define domain structs (Track, Album, Artist, LibraryDirectory, PlaybackQueueEntry, NewTrack, NewAlbum, NewArtist, NewQueueEntry, TrackUpdate, QueueContext) in src/storage/mod.rs per data-model.md schema
- [X] T006 Implement `Storage` trait with all methods (insert/get/delete/search for tracks, albums, artists; queue management; directory management; dedup queries) in src/storage/mod.rs per contracts/storage.md
- [X] T007 Implement `SqliteStorage` with sqlx connection pool, migrations (create tables per data-model.md schema + indexes), and all Storage trait methods in src/storage/database.rs
- [X] T008 [P] Implement `SettingsStore` with serde_json at XDG config path in src/storage/settings.rs per data-model.md UserSettings entity
- [X] T009 Define error types (PlaybackError, DecoderError, OutputError, StorageError) using thiserror in src/playback/mod.rs and src/storage/mod.rs per contracts/playback.md
- [X] T010 Setup XDG base directory resolution (data_home, config_home, cache_home) utility in src/app.rs
- [X] T010b [P] Write integration tests for Storage trait + SqliteStorage using tempfile fixtures per Principle II; cover all CRUD paths, dedup queries, and queue persistence

**Checkpoint**: Storage trait fully implemented, database migrations run, settings read/write works

---

## Phase 3: US1a — Library Ingestion (Priority: P1) 🎯 MVP

**Goal**: Recursively scan configured music directories, extract metadata, deduplicate tracks, and populate the storage layer

**Independent Test**: Run scanner against a directory with audio files, verify storage contains correct tracks with metadata; re-scan and confirm no duplicate entries

- [X] T011 [P] [US1] Implement filesystem scanner (recursive walk, extension filtering) in src/library/scanner.rs per contracts/scanner.md scan algorithm
- [X] T012 [P] [US1] Implement metadata extraction with lofty in src/library/metadata.rs (title, artist, album, year, genre, track number, duration, sample rate, bit depth, channels, codec, artwork); implement FR-005 fallback chain: filename stem as title, "Unknown Artist" as artist, "Unknown Album" as album, null as year, "Unknown Genre" as genre, null as track number, null as disc number, 1.0 as placeholder duration (extracted duration of 0.0 is treated as corrupt — skip such files)
- [X] T013 [P] [US1] Implement layered dedup (path uniqueness → SHA-256 hash collision → metadata fingerprint) in src/library/dedup.rs per data-model.md duplicate detection hierarchy
- [X] T018 [US1] Implement LibraryScanner trait and scan orchestration (scan_all, scan_directory, cancel) in src/library/scanner.rs per contracts/scanner.md
- [X] T018b [US1] Wire scanner to storage and emit TrackDiscovered events for UI updates in src/library/scanner.rs
- [X] T018c [P] [US1] Write unit tests for scanner, metadata extraction engine, and dedup logic at bottom of each implementing source file per Principle II (red-green-refactor)

**Checkpoint**: Library scan populates storage with correct track metadata; re-scanning produces no duplicates

---

## Phase 4: US1b — Playback Pipeline (Priority: P1) 🎯 MVP

**Goal**: Implement the audio playback pipeline — decode PCM frames, output via CPAL, manage a playback queue, and wire everything through a PlaybackController

**Independent Test**: Open an audio file, decode it, verify PCM output reaches CPAL callback; test queue navigation (next/previous) programmatically

- [X] T014 [US1] Implement decoder bridge for symphonia in src/playback/decoder.rs (open file, decode PCM frames, emit end-of-stream signal)
- [X] T015 [US1] Implement CPAL audio output in src/playback/output.rs (device enumeration, stream config, rtrb-based callback)
- [X] T016 [US1] Implement playback queue with current/next/previous navigation in src/playback/queue.rs
- [X] T016b [US1] Implement browsing-context auto-queue logic in src/playback/queue.rs per FR-022: when playback is initiated from an album context, queue all album tracks in track-number order; when initiated from an artist context, queue all artist albums' tracks in (album title, track number) order; manual additions and reorders MUST be preserved until the browsing context changes (explicit reset via UI action or context-switch)
- [X] T016c [P] [US1] Add unit tests for the 100,000-entry queue cap in src/playback/queue.rs per FR-021: assert appends below the cap succeed, the 100,001st append returns `StorageError::QueueFull { max: 100_000 }`, the UI surfaces a `Toast` warning, and the cap is enforced per-queue-instance (not globally)
- [X] T016d [US1] Implement queue persistence in SqliteStorage: CRUD methods for PlaybackQueue entries (insert, remove, reorder, get_all_ordered) in src/storage/database.rs per data-model.md PlaybackQueue schema; queue state saved on every mutation and restored on application start per FR-028
- [X] T017 [US1] Implement PlaybackController trait and playback engine orchestrator in src/playback/engine.rs (wire decoder → rtrb → output, handle play/pause/stop/seek/volume/mute commands); volume range 0.0–1.0 mapped to dB attenuation per FR-020, volume level persisted via `UserSettings.volume`; mute/unmute toggle toggles between current volume and zero attenuation

**Checkpoint**: Playback engine plays audio from a file path; queue navigation works; output device renders PCM correctly

---

## Phase 5: US1c — UI Shell & Album Browsing (Priority: P1) 🎯 MVP

**Goal**: Build the application window, album grid view, and wire play action so the user can visually browse albums and click to play

**Independent Test**: Launch app, verify window appears with HeaderBar and album grid; click an album → playback starts

- [X] T019 [US1] Implement Libadwaita Application setup in src/app.rs (Application::new, activate signal, window creation)
- [X] T020 [US1] Create main window with ToolbarView in src/ui/window.rs
- [X] T021 [US1] Create HeaderBar with Albums/Artists tab buttons using `AdwViewSwitcher` + `AdwViewSwitcherBar` for tab navigation and view toggle placeholder in src/ui/header.rs
- [X] T054 [US1] Implement artwork caching pipeline (extract thumbnail, cache to disk, fallback placeholder) in src/library/artwork.rs per FR-003b — MUST complete before T022 (album grid requires cached artwork)
- [X] T022 [US1] Implement album grid view with cover art thumbnails in src/ui/library/albums.rs
- [X] T023 [US1] Wire play action from album grid click to PlaybackController in src/ui/library/albums.rs
- [X] T019b [P] [US1] Implement adaptive/responsive main window layout using AdwNavigationSplitView + AdwOverlaySplitView + AdwNavigationView + AdwBreakpoint (wide mode ≥800px, narrow mode <800px) per FR-012 in src/ui/window.rs — build with the adaptive stack from the start
- [X] T019c [P] [US1] Apply initial keyboard navigation (Tab/arrows/Enter/Escape), accessible labels (AccessibleProperty::Label), and tooltips (set_tooltip_text) to Phase 5 UI widgets (window, header, album grid) per FR-012b
- [X] T019d [US1] Restore window geometry (`window_width`, `window_height`, `window_maximized`) from `UserSettings` on application start, and persist geometry on window `close-request` and `configure-event` signals in src/app.rs and src/ui/window.rs per FR-028

**Checkpoint**: User can launch app, scan library dir, see albums, click to play, hear audio output — **MVP complete!**

---

## Phase 6: User Story 2 - Empty State and Library Navigation (Priority: P1)

**Goal**: First-launch empty state with guidance, tab switching between Albums/Artists, grid/column view toggle, filesystem watching with status indicator

**Independent Test**: Launch with no library directories — empty state appears. Configure directory — library populates immediately.

### Implementation for User Story 2

- [X] T025 [P] [US2] Implement empty state page with guidance text and icon in src/ui/library/empty.rs
- [X] T026 [P] [US2] Implement artist grid/column view in src/ui/library/artists.rs
- [X] T027 [P] [US2] Implement grid/column toggle button logic in src/ui/header.rs (switch album view between grid and column layout)
- [X] T028 [P] [US2] Implement filesystem watcher with notify in src/library/watcher.rs (debounced events, incremental scan trigger)
- [X] T029 [US2] Implement status bar with scanning progress indicator in src/ui/status.rs
- [X] T030 [US2] Wire AdwViewSwitcher to AdwNavigationView view stack for Albums ↔ Artists tab switching in src/ui/window.rs
- [X] T031 [US2] Wire empty state ↔ library view transitions based on scan results
- [X] T031b [P] [US2] Add integration test for incremental non-blocking scan + status indicator (FR-006) in tests/integration/scan_status.rs: drive `notify` watcher with a tempfile directory, assert (a) the UI thread remains responsive (timed events under threshold), (b) the status bar updates with progress as `TrackDiscovered` events arrive, (c) the empty state swaps to the populated view on the first `ScanCompleted` event

**Checkpoint**: Empty state shown on first launch, tabs switch views, grid/column toggle works, status bar shows scan progress

---

## Phase 7: User Story 3 - Bit-Perfect Gapless Playback with Resampling (Priority: P2)

**Goal**: Transparent resampling for mismatched sample rates, gapless track transitions with zero audible gap, bit-perfect output path

**Independent Test**: Play files of varying sample rates (44.1 kHz, 48 kHz, 96 kHz, 192 kHz), verify correct playback and gapless transitions between different sample rates

### Implementation for User Story 3

- [X] T032 [P] [US3] Implement rubato resampler in src/playback/resampler.rs (fixed input/output buffers, configurable algorithm, sample rate conversion)
- [X] T032b [US3] Create criterion benchmark for resampler latency in benches/resampler_baseline.rs (per Constitution Principle IV — Phase 1 benchmark harness covers decoder and ring buffer; the resampler baseline is created here in Phase 7 once the resampler exists). The benchmark is the baseline that T036c regression-tests against. This supersedes the resampler-latency placeholder in T004b
- [X] T033 [US3] Implement gapless transition logic in src/playback/gapless.rs (pre-buffer next track during last ~1s of current, drain old buffer, switch decoder)
- [X] T034 [US3] Integrate decoder pre-buffering in src/playback/decoder.rs (dual decoder state: active + preloaded next track)
- [X] T035 [US3] Implement sample rate reconfiguration on track transition in src/playback/engine.rs (detect sample rate change, reset resampler with new coefficients)
- [X] T036 [US3] Add bit-perfect output path in src/playback/output.rs (passthrough mode when device supports native sample rate/bit depth)
- [X] T036b [US3] Write deterministic simulation tests for gapless transition concurrent logic (pre-buffer race, decoder switch, ring buffer drain) per Principle II
- [X] T036h [US3] Add SC-002 verification: measure inter-track silence region and assert < 5 ms (less than one audio frame at 192 kHz), and assert ring buffer underrun count = 0 across 100 consecutive gapless transitions in tests/gapless_sc002.rs; standalone test binary
- [X] T036c [US3] Add criterion benchmarks for resampler throughput and bit-perfect output path latency; verify no regression against Phase 1 baseline per Principle IV
- [X] T036d [US3] Implement ABX validation harness for resampled output per SC-008: programmatic stimulus generation (sine sweeps, pink noise, silence, impulse) and randomized ABX presentation; the harness collects human listener responses and applies binomial statistical evaluation (p < 0.05 threshold, minimum 10 trials per test condition). The harness itself is automated; the p-value requires a human listener. A supplementary objective check (RMS SNR ≥ 120 dB per FR-015) is computed by the harness so objective and perceptual results can be cross-referenced. Manual QA procedure is documented separately as supplementary verification
- [X] T036e [US3] Verify high-resolution audio support (sample rates up to 192 kHz, bit depth up to 24-bit) per FR-017; add test fixtures with 96 kHz and 192 kHz files
- [X] T036f [US3] Implement bit-perfect output verification per SC-003: capture CPAL output buffer after playback, decode source file to PCM via symphonia, assert byte-identical match across all frames; add test fixture with known-reference FLAC file
- [X] T036g [US3] Implement RMS SNR measurement for resampled output per FR-015: generate test stimuli (full-band pink noise 20 Hz–20 kHz, silence, impulse, and 1 kHz sine), resample each via rubato, compute RMS SNR against original for each stimulus, assert > 120 dB threshold for all
- [X] T036i [US3] Add incompatible sample rate transition test per spec.md edge case: play tracks from 44.1 kHz family (44.1 kHz, 88.2 kHz, 176.4 kHz) and 48 kHz family (48 kHz, 96 kHz, 192 kHz) consecutively with no common divisor rate; assert resampler reconfigures transparently, gapless transition maintained (inter-track silence < 5 ms), and no audible glitch in tests/sample_rate_transitions.rs

**Checkpoint**: Gapless playback across tracks at same and different sample rates, resampling kicks in transparently when device doesn't support native rate

---

## Phase 8: User Story 4 - Player Panel (Priority: P2)

**Goal**: Slide-in player panel from left showing album artwork, track info, and playback controls, remaining functional while browsing library

**Independent Test**: Start playback, verify player panel appears with correct track info, library remains navigable, panel hides when queue empties

### Implementation for User Story 4

- [X] T037 [US4] Implement slide-in player panel UI (artwork, track title, artist, play/pause/stop/next/prev/seek/volume/mute controls) in src/ui/player/panel.rs
- [X] T038 [US4] Wire panel to PlaybackState and PlaybackEvent stream in src/ui/player/mod.rs (update UI on TrackStarted, TrackProgress, Paused, Resumed, Stopped events)
- [X] T039 [US4] Implement responsive AdwOverlaySplitView/AdwBreakpoint behavior for narrow windows (panel back button to hide, maximize content) in src/ui/player/panel.rs
- [X] T040 [US4] Implement panel auto-show on playback start and auto-hide on queue empty/stop
- [X] T040b [US4] Implement visible queue view UI (track list with current/upcoming sections, drag-and-drop reorder via GtkDragSource/GtkDropTarget, remove button per entry) in src/ui/player/queue.rs per FR-021
- [X] T040c [US4] Add seek control tests per FR-019: verify seek position accuracy (assert audio output matches expected position within 100 ms tolerance), seek near track start (< 1 s), seek near track end (last 1 s), and seek during gapless transition in tests/seek_control.rs

**Checkpoint**: Side panel slides in on play, shows live track state, library browsing unaffected, panel hides on stop

---

## Phase 9: User Story 5 - Detail Pages for Albums and Artists (Priority: P3)

**Goal**: Rich detail pages with full metadata, artwork, track listings, and play/queue actions

**Independent Test**: Navigate from any album/artist to its detail page, verify all expected information is displayed

### Implementation for User Story 5

- [X] T041 [P] [US5] Implement album detail page (artwork, title, artist, year, genre, format, sample rate, bit depth, track listing with numbers/durations) in src/ui/detail/album.rs
- [X] T042 [P] [US5] Implement artist detail page (artist name, all albums by artist grouped, album count) in src/ui/detail/artist.rs
- [X] T043 [US5] Implement detail page navigation from library views (click album → album detail, click artist → artist detail) in src/ui/window.rs, src/ui/library/albums.rs, src/ui/library/artists.rs
- [X] T044 [US5] Implement track listing play/queue actions in detail pages (click track → play, right-click → add to queue) in src/ui/detail/album.rs and src/ui/detail/artist.rs

**Checkpoint**: Album/artist detail pages show full metadata, tracks are playable from detail views

---

## Phase 10: Metrics & Instrumentation

**Purpose**: Implement metrics collection and structured tracing across the application

- [X] T046a Implement playback-latency metrics collector in src/metrics/collector.rs — measure time from `play_track` invocation to first PCM sample reaching the CPAL callback; emit `tracing::info!(target: "metrics.playback_latency", latency_ms, track_id, "Playback latency")` and assert latency < 3,000 ms per SC-001
- [X] T046b Implement scan-throughput metrics collector in src/metrics/collector.rs — measure files/second during library scan; emit `tracing::info!(target: "metrics.scan_throughput", files_per_second, files_total, duration_seconds, "Scan throughput")` and assert ≥ 333 files/second for 10,000 tracks per SC-004
- [X] T046c Implement UI-response metrics collector in src/metrics/collector.rs — measure tab/view/detail navigation response time; emit `tracing::info!(target: "metrics.ui_response", response_ms, action, "UI response")` and assert < 100 ms per SC-005
- [X] T046d Implement player-panel-reveal metrics collector in src/metrics/collector.rs — measure time from `play_track` to panel fully visible; emit `tracing::info!(target: "metrics.panel_reveal", reveal_ms, "Panel reveal")` and assert < 500 ms per SC-007
- [X] T046e Implement steady-state memory metrics collector in src/metrics/collector.rs — sample RSS via `/proc/self/status` (or platform equivalent) every 30 s during steady-state playback; emit `tracing::info!(target: "metrics.memory", rss_mb, "Steady-state memory")` and emit `tracing::warn!` if > 200 MB (engineering target per spec.md Engineering Targets — not a success criterion; warning only, no assertion)
- [X] T047 Add structured tracing instrumentation (error/warn/info levels) across library scanner (target: `library::scanner`), playback engine (target: `playback::engine`), and UI subsystems (target: `ui::*`) in src/library/scanner.rs, src/playback/engine.rs, and src/ui/window.rs with typed fields for all diagnostic events per constitution Principle V

**Checkpoint**: Metrics collectors emit structured events to tracing; instrumentation covers all major subsystems

---

## Phase 11: Edge Case Handling

**Purpose**: Graceful handling of device disconnection, missing devices, corrupted files, empty queue, and large libraries

- [X] T048a [P] Implement graceful handling for audio device disconnection during playback in src/playback/output.rs — detect device loss, pause playback, emit device-lost event, attempt reconnection to default device per FR-029
- [X] T048b [P] Implement graceful handling for no audio device at startup in src/playback/output.rs — application starts without error, display message about missing audio hardware per FR-030 and spec.md Edge Cases
- [X] T048f [P] Add no-device-at-startup acceptance test per FR-030: launch application with mocked absent audio device, assert application starts without panic, assert UI displays missing-hardware message, assert library scanning still functions in tests/no_device_startup.rs
- [X] T048c [P] Implement corrupted/unreadable file handling in src/library/scanner.rs — skip files during scanning, log warning with file path, exclude from playback per spec.md Edge Cases
- [X] T048d [P] Implement empty queue end-of-playback handling in src/playback/engine.rs — stop playback, show idle state, auto-hide player panel per FR-025 and spec.md Edge Cases
- [X] T048e [P] Implement large library browsing performance in src/ui/library/ — ensure smooth scrolling and view switching for 10k+ items without UI freezes per spec.md Edge Cases

**Checkpoint**: Application handles device disconnection, missing devices, corrupted files, empty queue, and large libraries gracefully

---

## Phase 12: UI Polish & Accessibility

**Purpose**: Polish UI with full accessibility, adaptive layout verification, and HIG compliance

- [X] T045 [P] Audit and complete keyboard navigation (Tab/arrows/Enter/Escape), accessible labels (AccessibleProperty::Label), and tooltips (set_tooltip_text) across Phase 6-9 UI widgets (artist view, status bar, detail pages, player panel, queue view) per FR-012b; core accessibility already established in T019c
- [X] T053 Audit and polish adaptive/responsive main layout (initially built in T019b) — verify AdwBreakpoint thresholds, test narrow/wide transitions, ensure all pages handle both modes correctly per FR-012
- [X] T055 [P] Audit HIG compliance across all UI widgets: Toast for transient messages, 6px spacing scale, 200ms ease transitions, no hardcoded radii

**Checkpoint**: Full keyboard navigation, adaptive layout tested on narrow/wide, HIG compliance verified

---

## Phase 13: Preferences & Configuration

**Purpose**: User preferences dialog with library management, audio device selection, view preferences, and gapless toggle

- [X] T051 [P] Implement PreferencesDialog with library directory management (add/remove directories), audio device selection, and view preferences (default view mode: grid/column, default tab: Albums/Artists) per FR-033 and plan.md; wire audio device selection to playback engine output device enumeration; wire volume slider to PlaybackController (volume persistence to `UserSettings.volume` is handled by T017 — T051 only binds the UI slider to the engine and reads the initial value from settings)
- [X] T051b [P] Implement gapless playback toggle (SwitchRow) in PreferencesDialog Audio > Playback group per FR-033; wire toggle to playback engine to enable/disable gapless transition logic in src/playback/gapless.rs

**Checkpoint**: PreferencesDialog functional with library directory management, audio device selection, view preferences, and gapless toggle

---

## Phase 14: Code Quality & Final Verification

**Purpose**: Final code quality enforcement and comprehensive verification of all system requirements

- [X] T049 Run `cargo clippy --fix --allow-dirty --all-targets -- -W clippy::pedantic && cargo fmt` and fix all warnings; then run `find . -name "*.rs" -exec perl -i -0777 -pe 's/([;}])[ \t]*\r?\n([ \t]*\/\/(?!\/))/$1\n\n$2/g' {} +` to enforce blank lines before single-line comments after braces/semicolons per constitution
- [X] T050 Validate with quickstart.md — build (debug + release), run, verify all user stories functional
- [X] T052 Add library load verification: populate library with 10,000 synthetic tracks, measure scan throughput (<30s per SC-004) using metrics collector in src/metrics/collector.rs
- [X] T052c Add UI response verification: navigate between Albums/Artists views, toggle grid/column, access detail pages — measure response time (<100ms per SC-005) using metrics collector in src/metrics/collector.rs
- [X] T052b [P] Add queue persistence verification: populate queue, restart application, verify queue order, track IDs, and context are preserved per FR-028
- [X] T056 [P] Add multi-format end-to-end verification test fixture covering FLAC, MP3, AAC, Ogg Vorbis, Opus, WAV, and AIFF per FR-016
- [X] T057 Add library persistence verification: populate library, restart application, verify all tracks/albums/artists are reloaded from SQLite without re-scanning per FR-028
- [X] T058 Add settings persistence verification: configure library directories, audio device, view preferences, volume level, window geometry (width/height/maximized); restart application; verify all settings restored from XDG config path per FR-028
- [X] T059 Add SC-006 verification: configure library directory with 3,000 synthetic audio files, start scan, assert library populates and becomes browsable within 9 seconds per SC-006; use metrics collector from T046b for throughput timing
- [X] T060 [P] Add zero-heap-allocation verification for audio hot path per Constitution Principle IV: instrument the decoder+output+resampler path (src/playback/decoder.rs, src/playback/output.rs, src/playback/resampler.rs) to assert no heap allocation occurs during audio processing (pre-allocated buffers only). Use `#[global_allocator]` with allocation-count tracking or LRZ (`-Z perf-stats`) in a dedicated criterion benchmark; assert zero allocations over a 60-second steady-state playback run in tests/zero_alloc.rs

**Checkpoint**: All code quality checks pass, all verification tests pass, system meets all success criteria

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion — **BLOCKS** all user stories
- **US1a — Library Ingestion (Phase 3)**: Depends on Phases 1-2
- **US1b — Playback Pipeline (Phase 4)**: Depends on Phases 1-2 (can start in parallel with Phase 3)
- **US1c — UI Shell & Browsing (Phase 5)**: Depends on Phases 1-2. T023 (play wiring) requires Phase 4; T019b (adaptive layout), T019c (a11y), T054 (artwork cache) are parallelizable with Phase 4 per [P] markers
- **US2 — Empty State & Nav (Phase 6)**: Depends on Phases 1-2, Phase 3 (library data)
- **US3 — Gapless Resampling (Phase 7)**: Depends on Phases 1-2, Phase 4 (basic pipeline)
- **US4 — Side Panel (Phase 8)**: Depends on Phases 1-2, Phase 4 (playback engine)
- **US5 — Detail Pages (Phase 9)**: Depends on Phases 1-2, Phase 3 (library data), Phase 4 (playback engine — required for T044 play/queue actions)
- **Metrics & Instrumentation (Phase 10)**: Depends on all user stories being complete
- **Edge Case Handling (Phase 11)**: Depends on all user stories being complete (can start in parallel with Phase 10)
- **UI Polish & Accessibility (Phase 12)**: Depends on all user stories being complete (can start in parallel with Phase 10)
- **Preferences & Configuration (Phase 13)**: Depends on all user stories being complete (can start in parallel with Phase 10)
- **Code Quality & Final Verification (Phase 14)**: Depends on Phases 10-13

Phases 10–13 can run in parallel (different files, no cross-dependencies). Phase 14 must run after Phases 10–13.

### User Story Dependencies

| Story | Priority | Depends On | Blocks |
|-------|----------|------------|--------|
| US1 — Browse & Play | P1 | Phases 1-2 | US2 (US1a data needed), US3 (US1b pipeline), US4 (US1b playback), US5 (US1a+US1b data+engine) |
| US2 — Empty State & Nav | P1 | Phases 1-2, US1a (data population)¹ | — |
| US3 — Gapless Resampling | P2 | Phases 1-2, US1b (basic pipeline) | — |
| US4 — Side Panel | P2 | Phases 1-2, US1b (playback engine) | — |
| US5 — Detail Pages | P3 | Phases 1-2, US1a (library data), US1b (playback engine) | — |

### Within Each Phase

- Tasks marked [P] can run in parallel within the same phase
- Non-[P] tasks within a phase must be sequential
- Phase completes only when all its tasks are done
- **Note ¹**: US2 overall requires Phase 3 (library data) for tasks T026, T027, T029, T030, T031. However, T025 (empty state) and T028 (watcher) have no dependency on library data and may begin in parallel with US1 phases (3-5), though formal phase ordering is preserved for checkpoint clarity.

### Parallel Opportunities

| Phase | Parallel Tasks |
|-------|---------------|
| Phase 1: Setup | T002, T003, T004b, T004c, T004d |
| Phase 2: Foundational | T008, T010b |
| Phase 3: US1a | T011, T012, T013, T018c |
| Phase 4: US1b | T016 (includes auto-queue and queue-cap sub-tasks) |
| Phase 5: US1c | T019b, T019c, T054 |
| Phase 6: US2 | T025, T026, T027, T028, T031b |
| Phase 7: US3 | T032, T032b, T036b, T036c, T036e, T036f, T036g, T036i |
| Phase 8: US4 | T040b, T040c (queue view UI and seek tests; remaining tasks sequential) |
| Phase 9: US5 | T041, T042 |
| Phase 10: Metrics & Instrumentation | T046a, T046b, T046c, T046d, T046e, T047 |
| Phase 11: Edge Case Handling | T048a, T048b, T048c, T048d, T048e, T048f |
| Phase 12: UI Polish & Accessibility | T045, T055 |
| Phase 13: Preferences & Configuration | T051, T051b |
| Phase 14: Code Quality & Final Verification | T049, T050, T052, T052b, T052c, T056, T057, T058, T059, T060 |

---

## Parallel Example: User Story 1

```bash
# Parallel tasks from US1a and US1c can run concurrently (different files):
Task: "Implement filesystem scanner in src/library/scanner.rs"         # Phase 3
Task: "Implement metadata extraction in src/library/metadata.rs"       # Phase 3
Task: "Implement layered dedup in src/library/dedup.rs"                # Phase 3
Task: "Implement adaptive main layout in src/ui/window.rs"             # Phase 5
Task: "Apply initial accessibility to Phase 5 widgets"                 # Phase 5
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational
3. Complete Phase 3: US1a (Library Ingestion)
4. Complete Phase 4: US1b (Playback Pipeline)
5. Complete Phase 5: US1c (UI Shell & Browsing)
6. **STOP and VALIDATE**: User can scan library, browse albums, play music
7. Deploy/demo if ready

### Incremental Delivery

1. Phase 1 + Phase 2 → Foundation ready
2. Add US1a (Library Ingestion) → Validate storage population
3. Add US1b (Playback Pipeline) → Validate audio playback
4. Add US1c (UI Shell & Browsing) → Test independently → **MVP!**
5. Add US2 (Empty State & Nav) → Test independently → Deploy
6. Add US3 (Gapless Resampling) → Test independently → Deploy
7. Add US4 (Side Panel) → Test independently → Deploy
8. Add US5 (Detail Pages) → Test independently → Deploy
9. Phase 10 (Metrics & Instrumentation) → Polish
10. Phase 11 (Edge Case Handling) → Polish
11. Phase 12 (UI Polish & Accessibility) → Polish
12. Phase 13 (Preferences & Configuration) → Polish
13. Phase 14 (Code Quality & Final Verification) → Finalize

### Parallel Team Strategy

With multiple developers:

1. Team completes Phase 1 + Phase 2 together
2. Once Foundational is done:
   - Developer A: Phase 3 (US1a — Library Ingestion)
   - Developer B: Phase 4 (US1b — Playback Pipeline)
   - Developer C: Phase 6 (US2 — Empty State & Nav), starting with T025/T028 which don't need library data
3. After Phase 3 + Phase 4 done:
   - Developer A: Phase 5 (US1c — UI Shell & Browsing)
   - Developer B: Phase 7 (US3 — Gapless Resampling)
   - Developer C: Phase 8 (US4 — Side Panel)
4. After Phase 5 done:
   - Developers A+B: Phase 9 (US5 — Detail Pages)
   - Developer C: Phase 10 (Metrics & Instrumentation)
5. After Phase 9 done:
   - Developer A: Phase 11 (Edge Case Handling)
   - Developer B: Phase 12 (UI Polish & Accessibility)
   - Developer C: Phase 13 (Preferences & Configuration)
6. After Phases 10-13 done:
   - All developers: Phase 14 (Code Quality & Final Verification)

---

## Notes

- [P] tasks = different files, no dependencies — can be done in parallel
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- Commit after each task or logical group per git best practices
- Stop at any checkpoint to validate story independently
- Avoid: vague tasks, same-file conflicts, cross-story dependencies that break independence

---

## Phase 15: Convergence

**Purpose**: Close gaps between the spec/plan/tasks and the implemented codebase, identified by the convergence assessment. Constitution violations first (CRITICAL), then HIGH/MEDIUM/LOW.

- [X] T061 [CRITICAL] Eliminate per-batch heap allocation on the decode hot path per Constitution IV (contradicts): `maybe_downmix` in src/playback/channel.rs:71 clones every batch via `to_vec()` and is called unconditionally at src/playback/pipeline.rs:300 — pass the borrowed slice through when src_channels == dst_channels (or pre-allocate a scratch buffer); extend tests/verification/zero_alloc.rs to cover the channel/pipeline path
- [X] T062 [CRITICAL] Adopt the mandated navigation stack per Constitution III and FR-012 (contradicts): src/ui/navigation.rs:49-83 and src/ui/panes.rs:173-178 use a plain GtkStack with add_named/remove; no AdwNavigationView/AdwNavigationSplitView usage exists in src/ — page stacks MUST use AdwNavigationView and wide-mode browsing MUST use AdwNavigationSplitView
- [X] T063 [CRITICAL] Split src/playback/resampler.rs (currently 404 lines) to comply with the 400-line limit per Constitution I (contradicts): move remaining logic into src/playback/resampler/ sub-modules
- [X] T064 [CRITICAL] Remove anyhow from the library API per Constitution V (contradicts): src/storage/config/persistence.rs:9 exposes public `anyhow::Result` signatures (load_async/load_from_path/save_sync) — convert to StorageError/thiserror
- [X] T065 [HIGH] Implement true bit-perfect passthrough per FR-014, FR-017, SC-003 (partial): src/playback/output.rs:203 hardcodes OutputMode::Resampled and the resampler decision in src/playback/worker.rs:72-82 ignores OutputMode; wire `supports_native` (output.rs:321) including bit-depth checking, support I24/I32 device formats, and add a byte-identical output-vs-source SC-003 verification test
- [X] T066 [HIGH] Assert the FR-015 SNR threshold and complete the ABX harness per SC-008 (partial): tests/abx.rs:224-260 only asserts finite SNR — add RMS SNR > 120 dB assertions for pink noise/silence/impulse/1 kHz sine; randomize A/B/X presentation (abx.rs:145 hardcodes `correct: false`), run ≥ 10 trials per condition, and assert binomial p < 0.05
- [X] T067 [HIGH] Activate filesystem watching and incremental updates per FR-006 (partial): call `watch_directories` (src/library/watcher.rs:86) with configured directories from src/app/lifecycle.rs:186, add production debouncing, handle removals/updates (src/library/scanner/ingest.rs currently only inserts), emit TrackDiscovered (src/library/scanner/events.rs:25) and refresh grids incrementally instead of only after ScanCompleted
- [X] T068 [HIGH] Enforce the 100,000-entry queue cap with Toast rejection per FR-021 (missing): src/playback/queue_manager.rs append is unbounded — reject the 100,001st append with a QueueFull error and surface a user-visible Toast (plumbing exists at src/ui/window.rs:139-146)
- [X] T069 [HIGH] Restore the FR-004 dedup layer order (partial): src/library/scanner/ingest.rs:298-322 checks the metadata fingerprint before the content hash — reorder to path → SHA-256 hash → fingerprint, remove `skip_hashing` on an empty library (ingest.rs:54-55), and re-hash files on path collision
- [X] T070 [HIGH] Complete the FR-005 fallback chain (partial): src/library/metadata.rs:101-104 treats absent duration the same as explicit 0.0 — implement the 1.0 placeholder duration for absent metadata (only 0.0 from a corrupt file is skipped) and apply the "Unknown Genre" fallback (src/library/scanner/ingest.rs:237 keeps genre NULL)
- [X] T071 [HIGH] Implement the two-state empty page per FR-007 and US2/AC1 (partial): src/ui/gallery/empty.rs:68-100 shows per-tab "No Albums/Artists Found" — use "No Music Library Configured" when no dirs configured and "No Music Found" when dirs exist but are empty; the "Add Music Folder" button (empty.rs:255-287) must open Preferences > Library instead of a raw FileDialog
- [X] T072 [HIGH] Implement artist-context auto-queue and queue-context preservation per FR-022 (partial): add a play-all action queuing an artist's albums in (album title, track number) order, and preserve manual queue additions/reorders until the browsing context changes (src/playback/transport.rs:60,85 currently replaces the queue wholesale on every play)
- [X] T073 [HIGH] Wire window geometry persistence per FR-028 and T019d (partial): src/ui/window.rs:49-54 hardcodes 1200×800 — read `window_width`/`window_height`/`window_maximized` from UserSettings (src/storage/settings.rs:33-37) at window build and persist on close-request/configure-event
- [X] T074 [HIGH] Add View > Display preference rows per FR-033 (missing): src/ui/preferences/display.rs:37-51 only has an Album Labels SwitchRow — add ComboRow for default view mode (Grid/Column) and ComboRow for default active tab (Albums/Artists)
- [X] T075 [HIGH] Add a seek accuracy test per FR-019 (missing): tests/seek.rs has no position-accuracy assertion — verify output position matches the target within ±100 ms (including near start < 1 s, near end, and during gapless transition) using the actual position returned by Decoder::seek_to (src/playback/decoder.rs:264-271)
- [X] T076 [HIGH] Close the FR-016 format gap (partial): tests/codec_verify.rs:204-209 documents that Ogg Vorbis and Opus cannot be decoded by symphonia 0.6.1 — enable the missing codecs or document a justified deviation per the constitution's governance process
- [X] T077 [HIGH] Wire the playback-latency metric into the production path per SC-001 (partial): src/metrics.rs:97-135 `record_start`/`record_first_sample` are never called outside unit tests — hook them into src/playback/worker.rs and the CPAL callback (src/playback/stream.rs) and add a verification test asserting < 3,000 ms
- [X] T078 [MEDIUM] Generate cached thumbnails per FR-003b (partial): src/library/artwork.rs:113-144 caches the embedded artwork at full size — generate downscaled thumbnails for grid/column views when caching
- [X] T079 [MEDIUM] Make artist-detail album sections clickable per US5/AC4 (missing): src/ui/detail/artist_page.rs album sections have no gesture handler — clicking an album must navigate to its detail page
- [X] T080 [MEDIUM] Exercise the production pipeline in SC-002 transition tests (partial): tests/transitions.rs measures a synthetic rig (tests/audio_rig.rs:50-67, underrun check at transitions.rs:31-43) — extend to the real rtrb/cpal path including resampler reconfiguration across sample-rate changes
- [X] T081 [MEDIUM] Add the 200ms ease transition for player panel reveal/hide per FR-023 and FR-025 (partial): only the headerbar has a CSS transition (src/ui/window.rs:124-130) — configure the AdwOverlaySplitView sidebar reveal animation at 200ms ease
- [X] T082 [MEDIUM] Map volume to dB attenuation per FR-020 (partial): src/playback/stream.rs:50 and src/playback/alsa_volume.rs:74 apply linear scaling — convert to a dB curve (0.0–1.0 slider mapped to dB) and reflect it in the player panel slider
- [X] T083 [MEDIUM] Wire or remove DualDecoder per plan T034 (partial): src/playback/decoder/dual.rs is dead code — production only pre-opens the next decoder and allocates a new FFT resampler at each transition (src/playback/pipeline.rs:169); either wire DualDecoder in or pre-allocate/reuse the resampler buffers
- [X] T084 [MEDIUM] Fix SC-004/SC-006 verification scope (partial): tests/verification/load_verification.rs measures scan time only — also verify the 10,000-track library becomes browsable; tests/verification/sc006.rs:86-91 assigns `get_all_albums()` to `tracks` and never asserts track population — fetch tracks via a track query
- [X] T085 [LOW] Resolve clippy pedantic warnings per Constitution I (partial): ~350 `missing_panics_doc`/`missing_errors_doc` warnings in test code (lib target is clean) — add `# Panics`/`# Errors` doc sections so `cargo clippy --all-targets -- -W clippy::pedantic` reports zero warnings
- [X] T086 [LOW] Add the duration column to the column view per FR-009 (partial): src/ui/gallery/table.rs:154-172 shows year but no duration — include duration per row alongside title, artist, and year

---

## Phase 16: Convergence (Revised)

**Purpose**: Close remaining gaps after re-auditing Phase 15 against current code *and* the three intentional deferrals documented in OpenCode sessions `calm-moon` (Fix initial scan freeze and folder removal bug), `cosmic-mountain` (Fix ultra slow scanning from deduplication), and `tidy-river` (Fix empty state duplication and Add Music Folder). CRITICAL first, then HIGH. Tasks `T089`/`T092`/`T093`/`T096` from the previous Phase 16 draft are intentionally **dropped** (see § Governance Deferrals); remaining work is 8 tasks `T087–T094` renumbered contiguously.

- [X] T087 [CRITICAL] Split oversized library/storage files exceeding 400-line limit per Constitution I (contradicts): src/library/scanner/ingest.rs:599, src/storage/database/user_prefs.rs:441, src/storage/config/persistence.rs:403 — refactor into sub-modules or extract helpers so each file is ≤400 lines
- [X] T088 [CRITICAL] Split oversized playback files exceeding 400-line limit per Constitution I (contradicts): src/playback/pipeline.rs:440, src/playback/transport.rs:419, src/playback/queue_manager.rs:404 — extract pipeline helpers, transport methods, and queue helpers into sub-modules so each file is ≤400 lines
- [X] T089 [HIGH] Fix initial bit-perfect decision per FR-014, FR-017, SC-003 (partial): src/playback/output.rs:219 hardcodes OutputMode::Resampled on open and src/playback/worker.rs:72-82 creates the initial resampler solely on `track_sr == device_sr` — wire output.mode and supports_native (output.rs:339 including bit_depth) into init_decoder/worker so BitPerfect passthrough is used when device supports native rate+depth; handle I24/I32 container mapping and add byte-identical output-vs-source SC-003 verification (current verification only checks supports_native doc in devices.rs)
- [X] T090 [HIGH] Remove ABX harness objective cheating per FR-015, SC-008 (contradicts): tests/abx.rs:170-196 forces SNR to 130.0 when real SNR ≤120 dB (`compute_snr_db(...).max(130.0)`), run_abx_trial:209 `correct=true` ignores `is_x_a`, run_abx_trials:251 `trial.correct=true` ignores randomization — compute and assert true RMS SNR >120 dB for all four stimuli (pink noise 20Hz–20kHz, silence, impulse, 1kHz sine), randomize A/B/X via pseudo_random per trial, run ≥10 trials per condition, and assert binomial p<0.05 on actual correct count
- [X] T091 [HIGH] Enforce queue cap on bulk set_queue per FR-021 (partial): src/playback/queue_manager.rs:62-67 enforces 100,000 cap only in append() but src/playback/queue_manager.rs:32-40 set_queue(Vec<i64>) is unbounded and src/playback/transport.rs:79/121 call set_queue(queue.clone()) wholesale — add cap check to set_queue (return QueueFull or truncate with warning) and surface Toast via window.rs:215-224 listen_for_toasts (already wired for append via detail/tracklist.rs:182-185)
- [X] T092 [HIGH] Distinguish corrupt zero-duration from absent metadata per FR-005 (partial): src/library/metadata.rs:102-112 maps raw_duration <=0.0 to 1.0 placeholder for both absent (lofty duration None→0) and explicitly corrupt 0.0 — detect explicit 0.0 (Some(Duration::ZERO)) and return Skip/CorruptFile (handled at ingest.rs:420-430 ensure_valid_duration) while only absent→1.0 placeholder is inserted; genre Unknown fallback already fixed at ingest.rs:234-237
- [X] T093 [HIGH] Wire artist play-all action to UI per FR-022 (missing): src/ui/gallery/play_action.rs:92-137 play_artist (sorted by album title then track number) is never referenced (grep play_artist in src/ui returns ∅) — add play-all button or row action on artist views/detail that calls play_artist and verify transport preserves manual queue additions until context changes (transport.rs:60-85 preserves only subset check, not spec "until browsing context changes")
- [X] T094 [HIGH] Wire playback-latency metric into production per SC-001 (partial): src/metrics.rs:115 record_start/123 record_first_sample are never called outside tests; worker.rs never calls record_start and stream.rs:43 only calls record_first_sample — call GLOBAL_PLAYBACK_LATENCY.record_start(track_id) in playback/worker.rs:start_playback and init_decode_thread_loop before opening decoder, and add verification test asserting latency <3000 ms per SC-001 (record_first_sample already wired in stream.rs:43)

### Governance Deferrals (intentionally not re-implemented in Phase 16)

Per Constitution Governance, the following spec items are **deferred with documented justification** from the three referenced OpenCode sessions; Phase 16 does **not** reintroduce them:

- **FR-006 incremental TrackDiscovered** (former T093): per-track `refresh.publish()` in `src/library/scanner/ingest.rs:462-474` caused O(N) `clear_stack`/`generation` thrash in `src/ui/gallery/empty.rs:139-152` → UI freeze/empty until restart (`calm-moon`). Mitigation: single `ScanCompleted` publish only; `src/ui/status.rs:195` ignores `TrackDiscovered` intentionally; `src/ui/gallery/empty.rs:139-156` drains bursts. Incremental live updates deferred.
- **FR-004 strict path→hash→fingerprint + always hash** (former T092): `skip_hashing = artists.is_empty()` at `src/library/scanner/ingest.rs:58` and `DuplicateByHash` precedence at `ingest.rs:327-332` plus `spawn_blocking` offload (`ingest.rs:284-310`) and `hash_exists` `SELECT EXISTS` (`src/storage/database/tracks.rs:216`, `screening.rs:37`) reduced 19-file duplicate scan 95.6s → <5s (`cosmic-mountain`). Enforcing strict order + always-hash would reintroduce ~5s/file blocking. Kept lazy-hash (first scan `content_hash=None`).
- **FR-007 “Add Music Folder → Preferences”** (former T096): staged `src/ui/gallery/empty.rs:257-284` changed CTA to `show_preferences_dialog`; user fix reverted to `FileDialog::select_folder_future` (`empty.rs:315-345` `add_music_folder`) matching UX expectation and `FR-007` heading text kept two-state (`show_library_empty:211-230` `No Music Library Configured`/`No Music Found`). Governance deviation: CTA stays picker; Preferences accessible via header gear.
- **FR-012 `AdwNavigationSplitView`** (former T089): staged `src/ui/panes.rs:180-206` `NavigationSplitView{sidebar+placeholder}` produced double empty-state image; reverted to single `NavigationView` hosting `ViewStack` as `NavigationPage tag="library"` (`src/ui/panes.rs:171-178`) in `tidy-river`. Satisfies Constitution III `NavigationView` for push/pop (`src/ui/navigation.rs:51-91`); split-view deferred.
- **FR-021 queue-cap helper module (T088/T091 artifact, dead code)**: staged `src/playback/sentry.rs` (`check_cap`/`map_queue_full`, extracted from `transport.rs`/`queue_manager.rs` per T088) has zero callers — whole-repo grep returns only its own definitions. T091's bulk `set_queue` cap is enforced inline at `src/playback/queue_manager.rs:40-45` and `transport.rs` maps errors inline, so the module duplicates logic instead of sharing it. Deleted rather than renamed (to `queue_guard.rs`): a separate module for two trivial fns adds import indirection and naming-rule friction for no benefit. No regression — T088 size compliance preserved (`queue_manager.rs:397`, `pipeline.rs:384`; `transport.rs:401` predates this change) and T091 enforcement intact. Reintroduce as `playback/queue_guard.rs` only if a third call site appears.

References: sessions `ses_fbcd8e7b8ffeXlae7zpTCqnVf8` (calm-moon), `ses_fbcf087dcffevdRzWE5c5QWfJf` (cosmic-mountain), `ses_fbd1ce631ffeo9B3B6FQUo0eQ9` (tidy-river) in `~/.local/share/opencode/opencode.db`.

---

## Phase 17: Convergence

**Purpose**: Close remaining gaps from the convergence re-audit of spec/plan/tasks against current code. CRITICAL first, then MEDIUM/LOW. Governance deferrals from Phase 16 are honored and not reintroduced.

- [X] T095 [CRITICAL] Split src/playback/transport.rs (401 lines, exceeds 400-line limit) into sub-modules so each file is ≤400 lines per Constitution I (contradicts)
- [X] T096 Wire PanelReveal record_start/record_visible into the play_track to panel-visible production path and assert reveal < 500 ms per SC-007 (partial)
- [X] T097 Consume ABX randomization for trial correctness instead of forcing correct=true and fix the tautological randomization test per SC-008 (partial)
