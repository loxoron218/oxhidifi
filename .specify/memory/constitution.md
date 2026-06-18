<!--
  Sync Impact Report
  ==================
  Version change: 1.0.0 → 1.1.0 (amended Principle III)
  Modified principles: III (Leaflet → NavigationSplitView/OverlaySplitView stack)
  Added sections:
    - I. Code Quality (NON-NEGOTIABLE)
    - II. Testing Standards
    - III. User Experience Consistency
    - IV. Performance Requirements
    - V. Observability & Error Handling
    - Technology Stack & Constraints
    - Development Workflow & Quality Gates
    - Governance
  Removed sections: N/A
  Templates requiring updates:
    - .specify/templates/plan-template.md → ⚠ pending (update Constitution Check to reference principle numbers)
    - .specify/templates/spec-template.md → ✅ no changes needed
    - .specify/templates/tasks-template.md → ✅ no changes needed
  Follow-up TODOs:
    - 2026-05-22: Original adoption date estimated from feature branch creation
-->

# oxhidifi-refactor Constitution

## Core Principles

### I. Code Quality (NON-NEGOTIABLE)

All Rust code MUST pass `cargo clippy -- -W clippy::pedantic` and `cargo fmt` before
commit. `#[allow]` attributes, `unsafe` blocks, and calls to `unwrap()`/`expect()`/`panic!()`
are strictly forbidden. Each `.rs` file MUST NOT exceed 400 lines. Source files MUST be
grouped by capability/domain — the `models/`, `handlers/`, `utils/` organizational pattern
is NEVER permitted. Declarative macros (`macro_rules!`) MUST be used to eliminate code
duplication; prefer abstractions and generics over repeated code. Only `.rs` files are
permitted — NEVER use `.ui`, `.xml`, or `.blp` files. Rationale: Strict compile-time
guarantees and consistent style prevent entire categories of bugs and reduce maintenance
overhead in a high-fidelity audio application.

### II. Testing Standards

Every feature MUST have passing unit tests placed at the bottom of the implementing source
file. Tests MUST be written and confirmed failing before implementation (red-green-refactor
cycle). New library contracts and contract changes REQUIRE integration tests covering the
contract boundary. Deterministic simulation testing MUST be used for concurrency-sensitive
audio pipeline logic. `tempfile` MUST be used for filesystem test fixtures. Performance
regression tests using `criterion` MUST accompany any audio pipeline changes. All tests
MUST pass before a feature is considered complete. Rationale: Bit-perfect audio playback tolerates
no regressions; automated testing is the primary defense against silent corruption.

### III. User Experience Consistency

All UI MUST follow GNOME Human Interface Guidelines (HIG). Navigation MUST use
`ToolbarView` with `HeaderBar` — never manual `GtkBox` layouts. Preferences MUST use
`PreferencesDialog` with `PreferencesPage`, `PreferencesGroup`, and typed rows
(`ActionRow`, `SwitchRow`, `ComboRow`, `EntryRow`, `PasswordEntryRow`, `SpinRow`).
Accessibility is mandatory: every widget MUST set an accessible label via
`accessible_update_property(AccessibleProperty::Label, value)`, enable keyboard navigation
via `set_can_focus(true)`, and provide tooltip text via `set_tooltip_text()`. User
feedback MUST use `Toast` for transient messages and `suggested-action`/`destructive-action`
CSS classes for emphasis. Responsive layouts MUST use `AdwNavigationSplitView` for sidebar/content navigation, `AdwOverlaySplitView` for overlay panels (e.g., player panel), and `AdwBreakpoint` for responsive breakpoint sizing (≥800px wide mode, <800px narrow mode). `AdwNavigationView` MUST be used for push/pop page stacks, and `AdwViewSwitcher`/`AdwViewSwitcherBar` for tab navigation. `AdwLeaflet` is deprecated as of Libadwaita 1.4 and MUST NOT be used. Motion
animations MUST use 200ms ease transitions. Spacing MUST follow the 6px scale
(6/12/18/24/30px). Border radii MUST NEVER be hardcoded. Rationale: GNOME HIG compliance
ensures the application feels native, accessible, and professional across all desktop
environments.

### IV. Performance Requirements

The audio rendering pipeline MUST remain lock-free and MUST NOT allocate on the hot path.
Lock-free ring buffers (`rtrb`) MUST be used for all audio data transfer between threads.
Any change to audio processing code REQUIRES a `criterion` benchmark demonstrating no
regression relative to the previous baseline. Hot paths MUST use zero heap allocation —
all buffers MUST be pre-allocated at initialization or use statically-sized arrays.
Concurrent work MUST prefer `tokio` for async I/O, `crossbeam` for high-throughput
message passing, and `rayon` for data-parallel CPU workloads. Resampling MUST use `rubato`
(with its `rubato::audioadapter_buffers` sub-module) for efficient buffer management. Rationale: Bit-perfect gapless
playback demands deterministic low-latency execution; allocation stalls or lock contention
directly causes audible glitches.

### V. Observability & Error Handling

Structured `tracing` MUST be used for all diagnostic output with typed fields (e.g.,
`error!(error = %err, "Audio stream error")`). Library crates MUST use `thiserror` for
typed domain errors with a summary doc comment and `///` documentation on each variant.
Binary crates MUST use `anyhow` at the top level only, NEVER leaking `anyhow::Error` across
library boundaries. In async code, errors MUST implement `Send + Sync + 'static`. Prefer
the `?` operator over manual `match` chains. For simple recovery use `if let Ok(..) else { ... }`.
NEVER use `let _` or `.ok()` — return errors with context instead.
NEVER use `Box<dyn std::error::Error>` in library code unless no alternative exists.
Error types MUST be fully documented with summary comments and per-variant `///` docs.
Rationale: Diagnosing audio dropouts or playback failures requires precise, structured context —
not discarded errors or opaque dynamic dispatch.

## Technology Stack & Constraints

**Audio Engine:** `cpal` (device abstraction), `symphonia` (codec), `rtrb` (lock-free
ring buffers), `lofty` (metadata), `rubato` (with its `rubato::audioadapter_buffers` sub-module) (resampling),
`crossbeam`, `num-traits`.

**Concurrency:** `tokio` (async runtime), `tokio-stream`, `tokio-util` (async I/O),
`async-channel` (channels), `dynosaur` (dynamic traits), `parking-lot` (fast locks),
`rayon` (data parallelism), `crossbeam`.

**Data & Persistence:** `sqlx` (SQLite database), `serde` + `serde_json` (XDG-path
serialization).

**UI:** `libadwaita` (programmatic widgets only — no blueprint or XML).

**Utilities:** `notify` (file watching), `regex`, `thiserror`, `anyhow`, `criterion`
(benchmarking), `tempfile` (test fixtures), `tracing` + `tracing-subscriber`.

## Development Workflow & Quality Gates

Every commit MUST pass the full lint suite before push. The required commands are:

```bash
cargo clippy --fix --allow-dirty --all-targets -- -W clippy::pedantic && cargo fmt
```

Blank lines before single-line comments after braces/semicolons MUST be enforced:

```bash
find . -name "*.rs" -exec perl -i -0777 -pe \
  's/([;}])[ \t]*\r?\n([ \t]*\/\/(?!\/))/$1\n\n$2/g' {} +
```

All tests MUST pass before commit:

```bash
cargo test
```

Audio pipeline changes REQUIRE passing benchmarks:

```bash
cargo bench
```

Before implementing features with unfamiliar libraries, the `Context7` MCP server MUST
be queried for official documentation and best practices. Values that should be
configurable MUST NOT be hardcoded.

## Governance

This constitution supersedes all other development practices in this repository.
Amendments require a documented proposal, team approval, and a migration plan for
existing code. All pull requests and code reviews MUST verify compliance with the
principles herein. Any violation of a NON-NEGOTIABLE principle MUST be accompanied by
a documented justification of complexity accepted by the team. Use `AGENTS.md` for
runtime agent development guidance.

**Version**: 1.1.0 | **Ratified**: 2026-05-22 | **Last Amended**: 2026-05-23
