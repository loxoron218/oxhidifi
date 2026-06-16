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

- [X] T001 Create Cargo.toml with all dependencies per plan.md (cpal, symphonia, rtrb, lofty, rubato, audioadapter-buffers, tokio, libadwaita/gtk4-rs, sqlx, serde/serde_json, notify, walkdir, tracing/tracing-subscriber, crossbeam, rayon, parking-lot, thiserror, anyhow, criterion, tempfile)
- [X] T002 [P] Configure clippy (clippy.toml or .cargo/config.toml) with pedantic warnings and rustfmt config
- [X] T003 [P] Initialize tracing-subscriber in src/main.rs with structured logging (file + stderr)
- [X] T004 Create empty module structure with mod.rs re-exports per plan.md: src/library/, src/storage/, src/playback/, src/ui/, src/ui/library/, src/ui/detail/, src/ui/player/, src/metrics/
- [X] T004b [P] Create criterion benchmark harness in benches/ with baseline benchmarks for decoder PCM output and ring buffer throughput (resampler baseline is created in T032b once the resampler exists)
- [X] T004c [P] Set up test infrastructure: mock Storage backend, tempfile-based scanner fixtures, async test helpers in tests/common/
- [X] T004d [P] Query Context7 MCP server for cpal, symphonia, rubato, lofty, and libadwaita documentation and best practices before implementing any features using these libraries

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
- [X] T012 [P] [US1] Implement metadata extraction with lofty in src/library/metadata.rs (title, artist, album, year, genre, track number, duration, sample rate, bit depth, channels, codec, artwork); implement FR-005 fallback chain: filename stem as title, "Unknown Artist" as artist, "Unknown Album" as album, 0 as year, "Unknown Genre" as genre, null as track/disc number, 0 as duration (skip files with 0 duration as corrupt)
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
- [X] T017 [US1] Implement PlaybackController trait and playback engine orchestrator in src/playback/engine.rs (wire decoder → rtrb → output, handle play/pause/stop/seek/volume commands); volume range 0.0–1.0 mapped to dB attenuation per FR-020, volume level persisted via `UserSettings.volume`

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
- [X] T019b [P] [US1] Implement adaptive/responsive main window layout using AdwNavigationSplitView + AdwNavigationView + AdwBreakpoint (wide mode ≥800px, narrow mode <800px) per FR-012 in src/ui/window.rs — build with the adaptive stack from the start
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
- [X] T030 [US2] Implement tab switching logic (Albums ↔ Artists) with view content swap in src/ui/window.rs
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

- [X] T037 [US4] Implement slide-in player panel UI (artwork, track title, artist, play/pause/next/prev/seek/volume/mute controls) in src/ui/player/panel.rs
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

## Phase 10: Polish & Cross-Cutting Concerns

**Purpose**: Non-functional improvements across the entire application

- [ ] T045 [P] Audit and complete keyboard navigation (Tab/arrows/Enter/Escape), accessible labels (AccessibleProperty::Label), and tooltips (set_tooltip_text) across Phase 6-9 UI widgets (artist view, status bar, detail pages, player panel, queue view) per FR-012b; core accessibility already established in T019c
- [ ] T046a Implement playback-latency metrics collector in src/metrics/collector.rs — measure time from `play_track` invocation to first PCM sample reaching the CPAL callback; emit `tracing::info!(target: "metrics.playback_latency", latency_ms, track_id, "Playback latency")` and assert latency < 3,000 ms per SC-001
- [ ] T046b Implement scan-throughput metrics collector in src/metrics/collector.rs — measure files/second during library scan; emit `tracing::info!(target: "metrics.scan_throughput", files_per_second, files_total, duration_seconds, "Scan throughput")` and assert ≥ 333 files/second for 10,000 tracks per SC-004
- [ ] T046c Implement UI-response metrics collector in src/metrics/collector.rs — measure tab/view/detail navigation response time; emit `tracing::info!(target: "metrics.ui_response", response_ms, action, "UI response")` and assert < 100 ms per SC-005
- [ ] T046d Implement player-panel-reveal metrics collector in src/metrics/collector.rs — measure time from `play_track` to panel fully visible; emit `tracing::info!(target: "metrics.panel_reveal", reveal_ms, "Panel reveal")` and assert < 500 ms per SC-007
- [ ] T046e Implement steady-state memory metrics collector in src/metrics/collector.rs — sample RSS via `/proc/self/status` (or platform equivalent) every 30 s during steady-state playback; emit `tracing::info!(target: "metrics.memory", rss_mb, "Steady-state memory")` and assert < 200 MB engineering target (per plan.md constraint; not a SC)
- [ ] T047 Add structured tracing instrumentation (error/warn/info levels) across library scanner (target: `library::scanner`), playback engine (target: `playback::engine`), and UI subsystems (target: `ui::*`) in src/library/scanner.rs, src/playback/engine.rs, and src/ui/window.rs with typed fields for all diagnostic events per constitution Principle V
- [ ] T048a [P] Implement graceful handling for audio device disconnection during playback in src/playback/output.rs — detect device loss, pause playback, emit device-lost event, attempt reconnection to default device per FR-029
- [ ] T048b [P] Implement graceful handling for no audio device at startup in src/playback/output.rs — application starts without error, display message about missing audio hardware per FR-030 and spec.md Edge Cases
- [ ] T048f [P] Add no-device-at-startup acceptance test per FR-030: launch application with mocked absent audio device, assert application starts without panic, assert UI displays missing-hardware message, assert library scanning still functions in tests/no_device_startup.rs
- [ ] T048c [P] Implement corrupted/unreadable file handling in src/library/scanner.rs — skip files during scanning, log warning with file path, exclude from playback per spec.md Edge Cases
- [ ] T048d [P] Implement empty queue end-of-playback handling in src/playback/engine.rs — stop playback, show idle state, auto-hide player panel per FR-025 and spec.md Edge Cases
- [ ] T048e [P] Implement large library browsing performance in src/ui/library/ — ensure smooth scrolling and view switching for 10k+ items without UI freezes per spec.md Edge Cases
- [ ] T049 Run `cargo clippy --fix --allow-dirty --all-targets -- -W clippy::pedantic && cargo fmt` and fix all warnings; then run `find . -name "*.rs" -exec perl -i -0777 -pe 's/([;}])[ \t]*\r?\n([ \t]*\/\/(?!\/))/$1\n\n$2/g' {} +` to enforce blank lines before single-line comments after braces/semicolons per constitution
- [ ] T050 Validate with quickstart.md — build (debug + release), run, verify all user stories functional
- [ ] T051 [P] Implement PreferencesDialog with library directory management (add/remove directories), audio device selection, and view preferences (default view mode: grid/column, default tab: Albums/Artists) per FR-033 and plan.md; wire audio device selection to playback engine output device enumeration; wire volume slider to PlaybackController (volume persistence to `UserSettings.volume` is handled by T017 — T051 only binds the UI slider to the engine and reads the initial value from settings)
- [ ] T051b [P] Implement gapless playback toggle (SwitchRow) in PreferencesDialog Audio > Playback group per FR-033; wire toggle to playback engine to enable/disable gapless transition logic in src/playback/gapless.rs
- [ ] T052 Add library load verification: populate library with 10,000 synthetic tracks, measure scan throughput (<30s per SC-004) using metrics collector in src/metrics/collector.rs
- [ ] T052c Add UI response verification: navigate between Albums/Artists views, toggle grid/column, access detail pages — measure response time (<100ms per SC-005) using metrics collector in src/metrics/collector.rs
- [ ] T052b [P] Add queue persistence verification: populate queue, restart application, verify queue order, track IDs, and context are preserved per FR-028
- [ ] T053 Audit and polish adaptive/responsive main layout (initially built in T019b) — verify AdwBreakpoint thresholds, test narrow/wide transitions, ensure all pages handle both modes correctly per FR-012
- [ ] T055 [P] Audit HIG compliance across all UI widgets: Toast for transient messages, 6px spacing scale, 200ms ease transitions, no hardcoded radii
- [ ] T056 [P] Add multi-format end-to-end verification test fixture covering FLAC, MP3, AAC, Ogg Vorbis, Opus, WAV, and AIFF per FR-016
- [ ] T057 Add library persistence verification: populate library, restart application, verify all tracks/albums/artists are reloaded from SQLite without re-scanning per FR-028
- [ ] T058 Add settings persistence verification: configure library directories, audio device, view preferences, volume level, window geometry (width/height/maximized); restart application; verify all settings restored from XDG config path per FR-028
- [ ] T059 Add SC-006 verification: configure library directory with 3,000 synthetic audio files, start scan, assert library populates and becomes browsable within 9 seconds per SC-006; use metrics collector from T046b for throughput timing

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
- **Polish (Phase 10)**: Depends on all user stories being complete

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
| Phase 10: Polish | T045, T046a, T046b, T046c, T046d, T046e, T047, T048a, T048b, T048c, T048d, T048e, T048f, T051, T051b, T055, T056 |

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
9. Phase 10 (Polish) → Finalize

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
   - Developer C: Phase 10 (Polish)

---

## Notes

- [P] tasks = different files, no dependencies — can be done in parallel
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- Commit after each task or logical group per git best practices
- Stop at any checkpoint to validate story independently
- Avoid: vague tasks, same-file conflicts, cross-story dependencies that break independence
