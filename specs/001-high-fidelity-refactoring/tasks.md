---
description: "Task list for high-fidelity music player refactoring"
---

# Tasks: High-Fidelity Music Player Refactoring

**Input**: Design documents from `specs/001-high-fidelity-refactoring/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Checks**: The feature specification does not explicitly request unit test tasks, so no test tasks are generated. Each phase includes acceptance criteria as independent test verification.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization, dependency declaration, tooling configuration

- [ ] T001 Create Cargo.toml with all dependencies per plan.md (cpal, symphonia, rtrb, lofty, rubato, audioadapter-buffers, tokio, libadwaita/gtk4-rs, sqlx, serde/serde_json, notify, tracing/tracing-subscriber, crossbeam, rayon, parking-lot, thiserror, anyhow, criterion, tempfile)
- [ ] T002 [P] Configure clippy (clippy.toml or .cargo/config.toml) with pedantic warnings and rustfmt config
- [ ] T003 [P] Initialize tracing-subscriber in src/main.rs with structured logging (file + stderr)
- [ ] T004 Create empty module structure with mod.rs re-exports per plan.md: src/library/, src/storage/, src/playback/, src/ui/, src/ui/library/, src/ui/detail/, src/ui/player/, src/metrics/

**Checkpoint**: Cargo build succeeds, project structure mirrors plan.md

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core data types, storage layer, and error infrastructure that ALL user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [ ] T005 Define domain structs (Track, Album, Artist, LibraryDirectory, PlaybackQueueEntry, NewTrack, NewAlbum, NewArtist, NewQueueEntry, TrackUpdate, QueueContext) in src/storage/mod.rs per data-model.md schema
- [ ] T006 Implement `Storage` trait with all methods (insert/get/delete/search for tracks, albums, artists; queue management; directory management; dedup queries) in src/storage/mod.rs per contracts/storage.md
- [ ] T007 Implement `SqliteStorage` with sqlx connection pool, migrations (create tables per data-model.md schema + indexes), and all Storage trait methods in src/storage/database.rs
- [ ] T008 [P] Implement `SettingsStore` with serde_json at XDG config path in src/storage/settings.rs per data-model.md UserSettings entity
- [ ] T009 Define error types (PlaybackError, DecoderError, OutputError, ScanError, StorageError) using thiserror in src/playback/mod.rs and src/storage/mod.rs per contracts/playback.md
- [ ] T010 Setup XDG base directory resolution (data_home, config_home, cache_home) utility in src/app.rs

**Checkpoint**: Storage trait fully implemented, database migrations run, settings read/write works

---

## Phase 3: User Story 1 - Browse and Play Music from Library (Priority: P1) 🎯 MVP

**Goal**: User can scan a configured music directory, see albums in a grid, and play tracks with basic gapless audio output

**Independent Test**: Point application to a directory with audio files, verify library populates, select a track, confirm audible playback starts

### Implementation for User Story 1

- [ ] T011 [P] [US1] Implement filesystem scanner (recursive walk, extension filtering) in src/library/scanner.rs per contracts/scanner.md scan algorithm
- [ ] T012 [P] [US1] Implement metadata extraction with lofty in src/library/metadata.rs (title, artist, album, year, genre, track number, duration, sample rate, bit depth, channels, codec, artwork)
- [ ] T013 [P] [US1] Implement layered dedup (path uniqueness → SHA-256 hash collision → metadata fingerprint) in src/library/dedup.rs per data-model.md duplicate detection hierarchy
- [ ] T014 [US1] Implement decoder bridge for symphonia in src/playback/decoder.rs (open file, decode PCM frames, emit end-of-stream signal)
- [ ] T015 [US1] Implement CPAL audio output in src/playback/output.rs (device enumeration, stream config, rtrb-based callback)
- [ ] T016 [US1] Implement playback queue with current/next/previous navigation in src/playback/queue.rs
- [ ] T017 [US1] Implement PlaybackController trait and playback engine orchestrator in src/playback/engine.rs (wire decoder → rtrb → output, handle play/pause/stop/seek/volume commands)
- [ ] T018 [US1] Implement LibraryScanner trait and scan orchestration (scan_all, scan_directory, cancel) in src/library/scanner.rs per contracts/scanner.md
- [ ] T019 [US1] Implement Libadwaita Application setup in src/app.rs (Application::new, activate signal, window creation)
- [ ] T020 [US1] Create main window with ToolbarView in src/ui/window.rs
- [ ] T021 [US1] Create HeaderBar with Albums/Artists tab buttons and view toggle placeholder in src/ui/header.rs
- [ ] T022 [US1] Implement album grid view with cover art thumbnails in src/ui/library/albums.rs
- [ ] T023 [US1] Wire play action from album grid click to PlaybackController in src/ui/library/albums.rs
- [ ] T024 [US1] Wire scanner to storage and emit TrackDiscovered events for UI updates in src/library/scanner.rs

**Checkpoint**: User can launch app, scan library dir, see albums, click to play, hear audio output

---

## Phase 4: User Story 2 - Empty State and Library Navigation (Priority: P1)

**Goal**: First-launch empty state with guidance, tab switching between Albums/Artists, grid/column view toggle, filesystem watching with status indicator

**Independent Test**: Launch with no library directories — empty state appears. Configure directory — library populates immediately.

### Implementation for User Story 2

- [ ] T025 [P] [US2] Implement empty state page with guidance text and icon in src/ui/library/empty.rs
- [ ] T026 [P] [US2] Implement artist grid/column view in src/ui/library/artists.rs
- [ ] T027 [P] [US2] Implement grid/column toggle button logic in src/ui/header.rs (switch AlbumFlowBox between grid and list)
- [ ] T028 [P] [US2] Implement filesystem watcher with notify in src/library/watcher.rs (debounced events, incremental scan trigger)
- [ ] T029 [US2] Implement status bar with scanning progress indicator in src/ui/status.rs
- [ ] T030 [US2] Implement tab switching logic (Albums ↔ Artists) with view content swap in src/ui/window.rs
- [ ] T031 [US2] Wire empty state ↔ library view transitions based on scan results

**Checkpoint**: Empty state shown on first launch, tabs switch views, grid/column toggle works, status bar shows scan progress

---

## Phase 5: User Story 3 - Bit-Perfect Gapless Playback with Resampling (Priority: P2)

**Goal**: Transparent resampling for mismatched sample rates, gapless track transitions with zero audible gap, bit-perfect output path

**Independent Test**: Play files of varying sample rates (44.1 kHz, 48 kHz, 96 kHz, 192 kHz), verify correct playback and gapless transitions between different sample rates

### Implementation for User Story 3

- [ ] T032 [P] [US3] Implement rubato resampler in src/playback/resampler.rs (fixed input/output buffers, configurable algorithm, sample rate conversion)
- [ ] T033 [US3] Implement gapless transition logic in src/playback/gapless.rs (pre-buffer next track during last ~1s of current, drain old buffer, switch decoder)
- [ ] T034 [US3] Integrate decoder pre-buffering in src/playback/decoder.rs (dual decoder state: active + preloaded next track)
- [ ] T035 [US3] Implement sample rate reconfiguration on track transition in src/playback/engine.rs (detect sample rate change, reset resampler with new coefficients)
- [ ] T036 [US3] Add bit-perfect output path in src/playback/output.rs (passthrough mode when device supports native sample rate/bit depth)

**Checkpoint**: Gapless playback across tracks at same and different sample rates, resampling kicks in transparently when device doesn't support native rate

---

## Phase 6: User Story 4 - Side Panel Player (Priority: P2)

**Goal**: Slide-in side panel from left showing album artwork, track info, and playback controls, remaining functional while browsing library

**Independent Test**: Start playback, verify side panel appears with correct track info, library remains navigable, panel hides when queue empties

### Implementation for User Story 4

- [ ] T037 [US4] Implement slide-in side player panel UI (artwork, track title, artist, play/pause/next/prev/seek/volume controls) in src/ui/player/panel.rs
- [ ] T038 [US4] Wire panel to PlaybackState and PlaybackEvent stream in src/ui/player/mod.rs (update UI on TrackStarted, TrackProgress, Paused, Resumed, Stopped events)
- [ ] T039 [US4] Implement responsive Leaflet/Breakpoint behavior for narrow windows (panel back button to hide, maximize content) in src/ui/player/panel.rs
- [ ] T040 [US4] Implement panel auto-show on playback start and auto-hide on queue empty/stop

**Checkpoint**: Side panel slides in on play, shows live track state, library browsing unaffected, panel hides on stop

---

## Phase 7: User Story 5 - Detail Pages for Albums and Artists (Priority: P3)

**Goal**: Rich detail pages with full metadata, artwork, track listings, and play/queue actions

**Independent Test**: Navigate from any album/artist to its detail page, verify all expected information is displayed

### Implementation for User Story 5

- [ ] T041 [P] [US5] Implement album detail page (artwork, title, artist, year, genre, format, sample rate, bit depth, track listing with numbers/durations) in src/ui/detail/album.rs
- [ ] T042 [P] [US5] Implement artist detail page (artist name, all albums by artist grouped, album count) in src/ui/detail/artist.rs
- [ ] T043 [US5] Implement detail page navigation from library views (click album → album detail, click artist → artist detail)
- [ ] T044 [US5] Implement track listing play/queue actions in detail pages (click track → play, right-click → add to queue)

**Checkpoint**: Album/artist detail pages show full metadata, tracks are playable from detail views

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Non-functional improvements across the entire application

- [ ] T045 [P] Add keyboard navigation (Tab/arrows/Enter/Escape) and accessible labels (AccessibleProperty::Label) across all UI widgets per GNOME HIG
- [ ] T046 Implement performance metrics collector (playback latency, scan throughput, memory usage, UI response) with tracing in src/metrics/collector.rs
- [ ] T047 Add structured tracing instrumentation (error/warn/info levels) across library scanner, playback engine, and UI subsystems
- [ ] T048 [P] Add graceful error handling for edge cases per spec.md Edge Cases section (device disconnection, no device at startup, corrupt files, empty queue)
- [ ] T049 Run `cargo clippy --fix --allow-dirty --all-targets -- -W clippy::pedantic && cargo fmt` and fix all warnings
- [ ] T050 Validate with quickstart.md — build (debug + release), run, verify all user stories functional

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion — **BLOCKS** all user stories
- **User Stories (Phase 3-7)**: All depend on Foundational phase completion
- **Polish (Phase 8)**: Depends on all user stories being complete

### User Story Dependencies

| Story | Priority | Depends On | Blocks |
|-------|----------|------------|--------|
| US1 — Browse & Play | P1 | Phases 1-2 | US2 (data needed), US3 (pipeline), US4 (playback), US5 (data) |
| US2 — Empty State & Nav | P1 | Phases 1-2, US1 (data population) | — |
| US3 — Gapless Resampling | P2 | Phases 1-2, US1 (basic pipeline) | — |
| US4 — Side Panel | P2 | Phases 1-2, US1 (playback engine) | — |
| US5 — Detail Pages | P3 | Phases 1-2, US1 (library data) | — |

### Within Each Phase

- Tasks marked [P] can run in parallel within the same phase
- Non-[P] tasks within a phase must be sequential
- Phase completes only when all its tasks are done

### Parallel Opportunities

| Phase | Parallel Tasks |
|-------|---------------|
| Phase 1: Setup | T002, T003 |
| Phase 2: Foundational | T008 |
| Phase 3: US1 | T011, T012, T013 |
| Phase 4: US2 | T025, T026, T027, T028 |
| Phase 5: US3 | T032 |
| Phase 6: US4 | — (mostly sequential) |
| Phase 7: US5 | T041, T042 |
| Phase 8: Polish | T045, T048 |

---

## Parallel Example: User Story 1

```bash
# Launch all scanner/metadata/dedup tasks together:
Task: "Implement filesystem scanner in src/library/scanner.rs"
Task: "Implement metadata extraction in src/library/metadata.rs"
Task: "Implement layered dedup in src/library/dedup.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational
3. Complete Phase 3: User Story 1 (Browse & Play)
4. **STOP and VALIDATE**: User can scan library, browse albums, play music
5. Deploy/demo if ready

### Incremental Delivery

1. Phase 1 + Phase 2 → Foundation ready
2. Add US1 (Browse & Play) → Test independently → **MVP!**
3. Add US2 (Empty State & Nav) → Test independently → Deploy
4. Add US3 (Gapless Resampling) → Test independently → Deploy
5. Add US4 (Side Panel) → Test independently → Deploy
6. Add US5 (Detail Pages) → Test independently → Deploy
7. Phase 8 (Polish) → Finalize

### Parallel Team Strategy

With multiple developers:

1. Team completes Phase 1 + Phase 2 together
2. Once Foundational is done:
   - Developer A: US1 (Browse & Play) — largest scope
   - Developer B: US2 (Empty State & Nav) — UI-focused, parallel to US1
   - Developer C: Standby for US1 integration help, then US3/US4
3. After US1 done:
   - Developer A: US3 (Gapless Resampling)
   - Developer B: US4 (Side Panel)
   - Developer C: US5 (Detail Pages)
4. Team completes Phase 8 together

---

## Notes

- [P] tasks = different files, no dependencies — can be done in parallel
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- Commit after each task or logical group per git best practices
- Stop at any checkpoint to validate story independently
- Avoid: vague tasks, same-file conflicts, cross-story dependencies that break independence
