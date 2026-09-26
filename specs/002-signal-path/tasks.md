# Tasks: Audio Signal Path Inspector

**Input**: Design documents from `specs/002-signal-path/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/snapshot.md, contracts/dialog.md, quickstart.md
**Tests**: Included — required by constitution Principle II (every feature ships with tests; new contracts REQUIRE integration tests at the boundary) and asserted by `contracts/snapshot.md` invariants in `tests/signal_inspector.rs`.
**Organization**: Tasks grouped by user story for independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1–US4)
- Setup / Foundational / Polish phases carry no story label

## Path Conventions

- Single-binary desktop app: `src/`, `tests/` at repo root
- New stems (must stay unique codebase-wide): `signal_path`, `path_snapshot`, `path_verdict`, `stage_build`, `stage_output`, `stage_describe`, `signal_tab`, `signal_footer`, `signal_publish`, `signal_poll`, `signal_badge`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Register test target, verify naming/mod wiring preconditions, establish baseline

- [X] T001 Verify new file stems are unique codebase-wide (`signal_path`, `path_snapshot`, `path_verdict`, `stage_describe`, `signal_tab`, `signal_footer`, `signal_poll`, `signal_badge` vs `signal_handlers`) via ripgrep in repo root
- [X] T002 Register `[[test]] name = "signal_path" path = "tests/signal_inspector.rs"` in Cargo.toml after the `transitions` entry (default ungated target — the `verification-tests` feature gate does not apply)
- [X] T003 Create acceptance-test skeleton in tests/signal_inspector.rs with `//!` FR-001..FR-015 header mapping SC-003/SC-006 to contract invariants and SC-002 logic as proxy invariants (human 9/10 classification validated manually via quickstart)
- [X] T004 Run `cargo collate` baseline in repo root and record any pre-existing failures before feature work

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared immutable types, error type, and pure verdict classifier every story depends on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T005 Create parent index in src/playback/signal_path.rs with `//!` docs, shared value types (`SignalPathSnapshot` with `generation: u64`, `track_id: Option<i64>`, `zone_name: String`, `verdict: QualityVerdict`, `stages: Vec<PathStage>`, `devices: Vec<RenderingDevice>`, `processing_speed: Option<f64>`, `playback_status: PlaybackStatus` (reused from `crate::playback::state` — do NOT define a new enum); `PathStage` with `position: u32`, `kind: StageKind`, `title/detail/explanation: String`, `verdict: QualityVerdict`, `badge_icon: &'static str`; `QualityVerdict` variants `BitPerfect`/`Processed`/`Limited` with canonical labels exactly `Bit-Perfect`/`Processed`/`Limited`; `RenderingDevice` with `display_name: String`, `role: DeviceRole`, `brand_visual/illustration: String`, `manual_url: Option<String>`; `StageKind`, `StageFacts`, `DeviceRole`, `SnapshotInput` with `auth: Option<AuthFacts>` (MVP always `None` — authentication stage omitted; see data-model §5), `AuthFacts`), typed `SignalPathError` (`thiserror` variants `CatalogLookup`/`NoActiveTrack`/`SnapshotBuild` with `///` docs + `#[from]` sources), and `pub mod` wiring in src/playback.rs
- [X] T006 Implement total pure `resolve_verdict(stages: &[QualityVerdict]) -> QualityVerdict` with precedence `Limited > Processed > Bit-Perfect` in src/playback/signal_path/path_verdict.rs plus `#[cfg(test)] mod tests` unit tests (`anyhow::Result` + `ensure!`) covering empty/all-bit-perfect/mixed-limited cases
- [X] T007 [P] Add structured `tracing` fields spec for snapshot builds and verdict flips (snapshot generation, track id, verdict transition) in src/playback/signal_path.rs without touching the audio hot path
- [X] T008 [P] Add contract invariant tests 1–3 scaffolding (bit-perfect ⇒ `processing_speed.is_none()`; alteration incl. volume-only ⇒ verdict ≥ `Processed` + `processing_speed.is_some()`; shared-mixer/forced-downsample/lost-device ⇒ `Limited` even with enhancements) in tests/signal_inspector.rs using cloned `SnapshotInput` fixtures (no concurrency simulation)

**Checkpoint**: Foundation ready — `cargo test --test signal_path` compiles (failing assertions OK), user story implementation can now begin

---

## Phase 3: User Story 1 — View live signal path during playback (Priority: P1) 🎯 MVP

**Goal**: Vertical stage chain from Source to Output for the playing track, with empty-state and atomic gapless refresh

**Independent Test**: Play any track, open the `Signal` tab (via badge button or tab switcher), confirm chain Source → … → Output shows correct formats; stop playback → empty state explains no active path and how to start playback; queue 44.1kHz/16-bit → 96kHz/24-bit gapless transition shows no mixed-format chain (SC-006)

### Tests for User Story 1

- [X] T009 [P] [US1] Contract test for `build_snapshot` stage ordering (first stage `StageKind::Source`, last `Output`/`ExternalRenderer`, `stages` non-empty iff `track_id.is_some()`) in tests/signal_inspector.rs
- [X] T010 [P] [US1] Integration test for gapless atomic swap (two `SignalPathSnapshot` with different generations/track ids swap whole-instance, never mixed rows) in tests/signal_inspector.rs

### Implementation for User Story 1

- [X] T011 [US1] Implement synchronous `build_snapshot(input: &SnapshotInput) -> Result<SignalPathSnapshot, SignalPathError>` off-thread builder in src/playback/signal_path/path_snapshot.rs from pre-cloned facts only (the worker performs the async `SqliteStorage::get_track` lookup and all `parking_lot` clones before the call — no async I/O inside the builder or while holding a lock): cloned `Decoder::params`, resampler in/out rates + channels (`LoopCtx` in src/playback/pipeline.rs), `PlaybackState` volume/muted/output_mode/status/track, `AudioOutput` device id/name/rate/channels/mode, `track_audio: Option<TrackAudio>`; derive device nativeness for the bit-perfect rule by comparing track/decoder rate, depth, and channels against `device_sample_rate`/`device_channels` under `OutputMode::BitPerfect` (no separate native-capability input); bump `generation` per rebuild including `PlaybackStatus` changes; retain `track_id` + last snapshot with status flag when paused/stopped (empty state iff `track_id.is_none()`); MVP omissions (emit nothing): Authentication stage (`auth` always `None`), `Effect` stages and leveling/headroom volume variants (no engine source — `describe()` vocabulary only), network/streaming transport; single `Volume` stage iff DSP-volume scaling is active (`volume < 1.0` or muted) with dB via `format_volume_db`; external-renderer filter/modulator always `None` (title-only); exactly one output device from `device_id`/`device_name`; unknown source fields render as `unknown`; Decoder stage emitted iff live decoder facts are present (`decoder_params.is_some()`, otherwise omitted — the publisher passes `None` until the decode loop exposes them); a DSD source (codec/format label) undergoing volume scaling or resampling always yields an explicit `FormatConverter` DSD-to-PCM stage (silent omission is a contract violation per T023)
- [X] T012 [US1] Implement `summarize_text(snapshot: &SignalPathSnapshot) -> String` multi-line `Title — detail` chain with header verdict on first line in src/playback/signal_path/path_snapshot.rs
- [X] T013 [P] [US1] Build `Signal` tab page in src/ui/signal_view/signal_tab.rs as third `ViewStack` page named `signal` next to Albums/Artists (`ToolbarView`+`HeaderBar`, `ScrolledWindow` chain content scrolling inside the tab, programmatic widgets only, 6px spacing, theme-aware styling, no hardcoded radii/colors, no overlay dialog/popover/sheet) plus `ActiveTab::Signal` persistence (add `Signal` variant in src/storage/active_tab.rs, fix `is_albums()` exhaustiveness + add `is_signal()` helper, update every `ActiveTab` caller — src/ui/switching.rs `handle_tab_switch` + `wire_tab_tracking`, src/ui/navigation.rs `persist_active_tab` (map `"signal"`, no longer defaulting to `Albums`), src/ui/panes.rs `build_library_stack` third page + wire fns + `switch_mode_for_active_tab` + switcher tooltips, src/ui/header.rs `is_albums` binary sort-button logic, src/ui/preferences/display.rs tab `ComboRow` model + match, src/ui/gallery/rebuild_debounce.rs + album_grid.rs/artist_grid.rs guards, src/ui/toggle_popover.rs default, src/app/mocks.rs fixture, settings/persistence round-trip in src/storage/settings.rs + src/storage/config/persistence.rs + src/storage/database/user_prefs.rs, extend `active_tab_round_trips` + `persist_active_tab` tests) and `pub mod` wiring in src/ui/signal_view.rs and src/ui/panes.rs (if `signal_tab.rs` approaches 400 lines, pre-split header + chain rows into `signal_header.rs`/`signal_chain.rs`, stems stay unique)
- [X] T014 [US1] Render vertical `ListBox` chain in src/ui/signal_view/signal_tab.rs in `position` order source-at-top → output-at-bottom with continuous rail, circular `badge_icon` left, bold title + blue detail right, scrollable for 8+ stages with header/footer reachable
- [X] T015 [US1] Implement poll loop in src/ui/signal_view/signal_poll.rs via `timeout_add_local` 100–500ms draining per-subscriber `async-channel` mailbox with `try_recv` (newest wins), whole-snapshot swap on `generation`/track-id change (builder bumps `generation` on track/format/setting/device/status change), empty state iff `track_id` is `None`, paused/stopped status ribbon when `track_id.is_some()` + status paused/stopped (no `spawn_local`+`recv().await`, workers own I/O); add deterministic unit test for the generation guard (out-of-order/stale mailbox contents apply newest-`generation`-wins, never mixed rows) over a pure swap-decision function (simulation-style coverage for the concurrency-sensitive swap per constitution Principle II)
- [X] T016 [US1] Add unit tests for `build_snapshot` source/decoder facts and empty-state `NoActiveTrack` mapping in src/playback/signal_path/path_snapshot.rs (`#[cfg(test)] mod tests`, `anyhow::Result` + `ensure!`/`bail!`)
- [ ] T036 [US1] Implement off-thread snapshot publisher in src/ui/signal_view/signal_publish.rs (`spawn_snapshot_publisher` driving a per-subscriber `async-channel` mailbox from the playback event fan-out): initial publish for the current engine state, then rebuild on path-changing events only (`TrackStarted`/`TrackFinished`/`Paused`/`Resumed`/`Stopped`/`VolumeChanged`/`OutputModeChanged`/`DeviceLost` — position ticks, queue edits, seeks, and errors never rebuild); the worker clones engine facts under minimal lock scopes, runs the async `SqliteStorage::get_track` lookup after releasing every lock, captures `SnapshotInput` timing fields (`sampled_at_wall` + frame counters, owns the speed-EMA state per data-model §5), bumps `generation` per rebuild (including status-only changes), feeds `build_snapshot`, and `try_send`s the immutable snapshot (empty snapshot with retained zone name iff `track_id.is_none()`); add `pub mod` wiring in src/ui/signal_view.rs

**Checkpoint**: US1 fully functional and testable independently — chain, empty state, atomic gapless swap, and publisher feeding the tab mailbox (T036) all work without badge/verdict/explainer/footer

---

## Phase 4: User Story 2 — Understand at a glance whether sound is untouched or processed (Priority: P1)

**Goal**: Player badge button + tab header verdict (`Bit-Perfect`/`Processed`/`Limited`) with non-color indicator, precedence Limited > Processed > Bit-Perfect

**Independent Test**: Play three situations — untouched, DSP-volume/resample active, shared-mixer/forced-downsample output — and confirm badge + header each show `Bit-Perfect`, `Processed`, `Limited` respectively with distinct icon/shape plus text (SC-002, 9/10 listeners classify correctly — human part manual-gated via quickstart scenarios 1–3)

### Tests for User Story 2

- [ ] T017 [P] [US2] Contract test for verdict precedence (`Limited` wins over `Processed` wins over `Bit-Perfect`; bit-perfect requires `OutputMode::BitPerfect` + no resampler + unity/unmuted or hardware-mixer volume + matching channels + device nativeness derived by comparing track/decoder rate/depth/channels against `device_sample_rate`/`device_channels` — no separate native-capability input) in tests/signal_inspector.rs
- [ ] T018 [P] [US2] GTK badge/header test (verdict text + dedicated icon per state, badge matches tab header) in src/ui/player/signal_badge.rs + src/ui/signal_view/signal_tab.rs via `libadwaita::gtk::{self, test}` + plain `#[test]`

### Implementation for User Story 2

- [ ] T019 [US2] Create player-area entry badge button in src/ui/player/signal_badge.rs showing current verdict text plus dedicated indicator icon/shape (never color alone), always available (empty state navigates to tab when idle), activating it switches the library to the `Signal` tab (no dialog/popover); keyboard-activatable with accessible name
- [ ] T020 [US2] Render tab header in src/ui/signal_view/signal_tab.rs with whole-path verdict label + indicator, output/zone name, hint text with the exact literal `Click on any stage of the path to learn more`
- [ ] T021 [US2] Assign per-stage quality indicators in src/playback/signal_path/path_snapshot.rs at build time (DSP-volume scaling [`volume < 1.0` or muted]/resample/bit-depth/DSD-to-PCM/channel-count conversion ⇒ `Processed`; shared-mixer/forced-downsample/lost-device ⇒ `Limited`; MVP emits no `Effect`/leveling/headroom stages — their `Processed` wording is `describe()`-only per T024) with icon/shape + text label per row in src/ui/signal_view/signal_tab.rs
- [ ] T022 [US2] Wire live verdict updates in src/ui/signal_view/signal_tab.rs + src/ui/player/signal_badge.rs so adjusting DSP volume or toggling output mode flips badge + header without leaving the tab, with `tracing` fields on verdict flips

**Checkpoint**: US1 + US2 both work independently — chain visible (US1) and verdict instantly classifiable (US2)

---

## Phase 5: User Story 3 — Inspect and learn from any single stage (Priority: P2)

**Goal**: Every stage selectable with plain-language explanation including input→output values, readable on narrow screens

**Independent Test**: Open path, select Source stage and one converter stage; explanation appears within 1s (SC-004) with input→output wording (e.g. `96kHz to 192kHz`); every row keyboard-selectable, announced as title + detail

### Tests for User Story 3

- [ ] T023 [P] [US3] Contract test for `describe` (never empty strings; converter `detail` always carries input→output; volume rows carry dB via `format_volume_db`; DSD + volume/DSP yields explicit `FormatConverter` DSD-to-PCM stage, zero silent alterations SC-003; effect/leveling/headroom wording asserted at the pure-`describe()` level — builder emission of those families is MVP-omitted per T011) in tests/signal_inspector.rs

### Implementation for User Story 3

- [ ] T024 [US3] Implement pure `describe(kind: &StageFacts) -> (String, String, String)` title/detail/explanation for every `StageKind` (source, authentication — MVP: always omitted since `auth` is always `None`, decoder, bit-depth/sample-rate/format converters, volume variants with dB via `volume_to_db`/`format_volume_db` in src/playback/volume.rs, EQ/crossover/crossfeed/channel-map effect vocabulary, transport with MVP Linux wording: ALSA direct-exclusive/shared-mixer/USB with network/streaming deferred, output, external renderer — MVP: always title-only since filter/modulator are always `None`) in src/playback/signal_path/stage_describe.rs plus unit tests; explanation = 1–2 plain sentences + input→output values where applicable, never empty (effect/leveling/headroom arms are `describe()`-only in MVP — the builder omits them per T011)
- [ ] T025 [US3] Make each `ListBoxRow` selectable in src/ui/signal_view/signal_tab.rs expanding inline directly under its title/detail lines to reveal its explanation within 1s without losing chain context on narrow screens (no popover/dialog/navigation; expanding one row MUST NOT hide surrounding stages); accessible name `title + detail` per row
- [ ] T026 [US3] Add GTK explainer interaction test (select Source + converter rows, assert inline explanation visible, keyboard activation path) in src/ui/signal_view/signal_tab.rs via `libadwaita::gtk::{self, test}` + plain `#[test]`

**Checkpoint**: US1–US3 independently functional — chain (US1), verdict (US2), per-stage education (US3)

---

## Phase 6: User Story 4 — Identify the playback device and check processing headroom (Priority: P3)

**Goal**: Footer device card(s) with manual link + header processing-speed readout whenever in-app alteration is active (including volume-only)

**Independent Test**: Play to a named device → MVP shows exactly one footer card with the display name (manual link hidden — `manual_url` always `None`; multi-card streamer+DAC deferred); lower volume with no other processing → `Processing speed: {x}x` visible; bit-perfect playback → readout hidden

### Tests for User Story 4

- [ ] T027 [P] [US4] Contract test for device footer (exactly 1 device in MVP when track present — the active output device; streamer + separate DAC ≥2 cards in chain order is a deferred forward rule asserted with injected `RenderingDevice` fixtures only; cards never name a non-rendering device; lost device forces verdict off `Bit-Perfect`) in tests/signal_inspector.rs
- [ ] T028 [P] [US4] Contract test for processing-speed rule (`processing_speed.is_some()` iff any in-app alteration incl. volume-only; `None`/hidden when bit-perfect and when Limited-only with no in-app alteration; invariant 4 summarize lists every stage once in order) in tests/signal_inspector.rs

### Implementation for User Story 4

- [ ] T029 [US4] Compute snapshot-sampled `processing_speed: Option<f64>` throughput multiple in src/playback/signal_path/path_snapshot.rs (frames decoded + resampled per wall-clock ÷ device rate, smoothed via exponential moving average α=0.3 over snapshot samples; value is informational — contracts assert presence (`Some`/`None`) only, never the number; `Some` iff DSP-volume scaling (incl. volume-only), resample, or bit-depth/DSD conversion is active, else `None` — including Limited-only shared-mixer with no in-app alteration; effect/leveling/headroom families have no MVP inputs; never instrument the `rtrb`/CPAL callback; throughput counters + wall-clock samples arrive via the `SnapshotInput` timing fields and the EMA state lives in the publisher worker (see data-model §5 and T036) — `build_snapshot` stays pure over one input)
- [ ] T030 [US4] Render `Processing speed: {x.x}x` header-area readout (1 decimal, below verdict/zone line) in src/ui/signal_view/signal_tab.rs iff `processing_speed.is_some()`, fully hidden when `None` (bit-perfect and Limited-only without in-app alteration); readout exposed to assistive tech as `Processing speed {x.x} times real time`
- [ ] T031 [US4] Render exactly one footer device card in MVP for the active output device in src/ui/signal_view/signal_footer.rs (display name from CPAL `DeviceInfo` falling back to device id; multi-card chain order is a deferred forward rule); card carries generic brand/device symbolic visual + outline illustration with fallback icon when brand unknown, `View Product Manual` link iff `manual_url.is_some()` where MVP `manual_url` is always `None` (no device-manual config key exists yet — link hidden; tests inject `RenderingDevice` fixtures directly; future optional `device_manuals` user-config map may populate it) — never hardcoded or bundled; no card for non-rendering devices); lost-device mid-playback rebuilds Output row to lost state via `PlaybackEvent::DeviceLost`
- [ ] T032 [US4] Implement overflow three-dot `MenuButton` (sole allowed popover) in src/ui/signal_view/signal_tab.rs with exactly three read-only actions — Copy path summary (copies `summarize_text`, confirms with `Toast`, no dialog), open output/device settings elsewhere (presents the existing `show_preferences_dialog` PreferencesDialog with the audio page selected via page-selection support (T037), which includes the audio page per `src/ui/preferences/audio.rs:build_audio_page` — no inline DSP editing), About info (presents the existing PreferencesDialog default view per `src/ui/preferences.rs` — default landing page with no page selection; no dedicated About view exists, no new dialog built in the tab; dedicated About page is out-of-scope follow-up; acceptance per action is dialog presentation on the correct landing page: audio page for settings, default view for About) — all keyboard-reachable
- [ ] T037 [US4] Implement PreferencesDialog page-selection support in src/ui/preferences.rs (`show_preferences_dialog` gains a page-selection parameter or post-build visible-page call) so the Signal kebab settings action lands on the audio page while About keeps the default view; existing library/audio/view pages unchanged; GTK test asserting the landing page per action via `libadwaita::gtk::{self, test}` + plain `#[test]`

**Checkpoint**: All four user stories independently functional

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Accessibility, theming, and validation across all stories

- [ ] T033 Verify dark/light appearances in src/ui/signal_view/signal_tab.rs + src/ui/signal_view/signal_footer.rs + src/ui/player/signal_badge.rs (near-black tab page/light text vs light tab page/dark text, theme accent/link role for detail links, legible rail/badges/indicators via theme-aware system palette and generic symbolic icons in both modes, no hardcoded colors/hex literals, no per-brand artwork; acceptance: side-by-side dark/light screenshot review in T034 with every indicator distinguishable by icon/shape + text without color vision) and full keyboard + screen-reader operability (FR-015: rows announced as title + detail, verdict + processing-speed readout announced as text)
- [ ] T034 Run quickstart.md validation scenarios 1–6 in order (bit-perfect chain, processed readout, limited-wins, stage explainer, device footer, gapless + edge cases incl. 8+ stage scroll, pause/stop ribbons, DSD-to-PCM explicit stage) and fix deviations in src/playback/signal_path/path_snapshot.rs or src/ui/signal_view/signal_tab.rs; include side-by-side dark/light screenshot review (FR-013: every indicator distinguishable by icon/shape + text without color vision) and record the manual gates (SC-001 timing, SC-002 9/10 classification, SC-004 paraphrase comprehension)
- [ ] T038 Verify new file stems `signal_publish`, `stage_build`, `stage_output` are unique codebase-wide (case-folded, `-`/`_` + singular/plural folded, vs `signal_handlers`) via ripgrep in repo root, extending the T001 record
- [ ] T035 Run `cargo collate`, `cargo clippy --fix --allow-dirty --all-targets --all-features && cargo fmt`, `cargo test` (incl. `cargo test --test signal_path`) in repo root; confirm zero clippy warnings (pedantic + nursery denied, no `#[allow]`/`#[expect]`), all files ≤400 lines with `//!`/`///` docs and 4-block imports, no new crates, no schema migration, no audio-hot-path changes

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Setup — BLOCKS all user stories
- **User Stories (Phase 3–6)**: All depend on Foundational completion
  - Sequential in priority order (US1 → US2 → US3 → US4), or parallel if staffed
- **Polish (Phase 7)**: Depends on all desired user stories being complete

### User Story Dependencies

- **US1 (P1)**: After Foundational — no dependencies on other stories
- **US2 (P1)**: After Foundational — integrates with US1 tab/builder but independently testable via badge/header
- **US3 (P2)**: After Foundational — integrates with US1 rows; `stage_describe.rs` independent of US2/US4
- **US4 (P3)**: After Foundational — integrates with US1 snapshot/tab; footer/readout/menu independently testable; T032 kebab acceptance depends on T037 page selection

### Within Each User Story

- Tests written FIRST and FAIL before implementation (per-story test tasks T009–T010, T017–T018, T023, T027–T028 precede their story's implementation)
- `build_snapshot` before tab rendering; `resolve_verdict` before badge/header; `describe` before explainer UI; `processing_speed`/devices before footer/readout; T036 publisher before T015 mailbox wiring is exercised end-to-end
- Story complete and checkpoint-validated before next priority

### Parallel Opportunities

- T007 + T008 (different files: `signal_path.rs` vs `tests/signal_inspector.rs`) in Foundational
- T009 + T010 (same test file — append-only separate sections; or serialize if conflicts arise)
- T013 (tab shell) parallel with T011–T012 (builder) — different files, builder API is contract-frozen
- T017 + T018 (contract vs GTK tests, different files); T027 + T028 (same test file — append-only separate sections; serialize if conflicts arise)
- T036 (new file `signal_publish.rs`) parallel with T011–T014; T037 (`preferences.rs` page selection) parallel with T029–T031, serial before T032
- US2/US3/US4 can proceed in parallel post-Foundational with separate owners (badge vs describe vs footer touch different files; tab edits need coordination)

---

## Parallel Example: User Story 1

```bash
# Contract-frozen builder and tab shell proceed in parallel (different files):
Task: "Implement build_snapshot + summarize_text in src/playback/signal_path/path_snapshot.rs"
Task: "Build Signal tab shell in src/ui/signal_view/signal_tab.rs"
# Tests together (same file, separate sections):
Task: "Contract test for build_snapshot ordering in tests/signal_inspector.rs"
Task: "Integration test for gapless atomic swap in tests/signal_inspector.rs"
```

## Parallel Example: User Story 4

```bash
Task: "Contract test for device footer in tests/signal_inspector.rs"
Task: "Contract test for processing-speed rule in tests/signal_inspector.rs"
# Then (different concerns, same tab file — serialize UI edits):
Task: "Compute processing_speed in src/playback/signal_path/path_snapshot.rs"
Task: "Render readout + footer cards + overflow menu in src/ui/signal_view/signal_tab.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001–T004)
2. Complete Phase 2: Foundational (T005–T008)
3. Complete Phase 3: US1 chain + empty state + atomic swap (T009–T016)
4. **STOP and VALIDATE**: play tracks of differing formats, confirm chain + empty state + gapless swap per US1 independent test
5. Deploy/demo if ready (playback transparency display delivers standalone value)

### Incremental Delivery

1. Setup + Foundational → types/verdict/test harness ready
2. + US1 → viewable chain (MVP!)
3. + US2 → trust badge/header (SC-001/SC-002, human parts manual-gated via quickstart)
4. + US3 → educational explainer (SC-004, rendering tested / paraphrase manual-gated)
5. + US4 → device footer + headroom readout + overflow menu (SC-005, MVP link hidden)
6. + Polish → a11y/theming/quickstart/gates green; each increment adds value without breaking prior stories

### Parallel Team Strategy

1. Team completes Setup + Foundational together
2. Then: Developer A → US1 builder + tab chain; Developer B → US2 badge/header (after T006); Developer C → US3 `describe` + explainer; Developer D → US4 speed/footer/menu
3. Coordinate edits to `signal_tab.rs` (shared UI file) — prefer serializing T014/T020/T025/T030–T032 or splitting by function; line budget: if `signal_tab.rs` approaches 400 lines (Constitution I gate, checked in T035), pre-split header + chain rows into `signal_header.rs`/`signal_chain.rs` (new stems must stay unique codebase-wide)

---

## Notes

- [P] tasks = different files, no dependencies; same-file UI tasks are intentionally sequential
- [USn] label maps each story task to spec.md user stories for traceability
- Data-model constraints quoted verbatim in tasks (non-empty iff track, Source-first/Output-last, canonical labels, `unknown` fields, explicit DSD-to-PCM, manual link iff known, speed iff alteration)
- FR-005 sub-coverage (single FR, many stage families): auth → T011/T024 (MVP: always `None`, omitted), converters (bit-depth/sample-rate/format incl. DSD-to-PCM) → T011/T023/T024, volume (single DSP volume + dB) → T011/T021/T023/T024, effects (EQ/crossover/crossfeed/channel-map) and leveling/headroom → T024 `describe()`-only in MVP (no builder emission per T011), transport (MVP Linux wording: ALSA direct-exclusive/shared-mixer/USB) → T011/T024; a stage family counts as done only when its mapped tasks pass
- No `unsafe`, no `#[allow]`/`#[expect]`, no new crates (`crossbeam`/`dynosaur`/`tokio-stream`/`tokio-util`/`regex` stay untouched), no migration, display-only (existing `throughput`/`conversion_baseline` benches just keep passing)
- Commit after each task or logical group; stop at any checkpoint to validate the story independently
