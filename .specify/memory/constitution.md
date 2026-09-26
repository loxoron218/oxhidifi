<!--
  Sync Impact Report
  ==================
  Version change: 1.1.0 → 1.2.0 (MINOR: materially expanded guidance + stack correction)
  Modified principles:
    - I. Code Quality (NON-NEGOTIABLE) → I. Code Quality (NON-NEGOTIABLE) (expanded: nursery lints, stem uniqueness, parent-index, import blocks, docs, no-hardcode)
    - II. Testing Standards → II. Testing Standards (expanded: anyhow Result/ensure!/bail!, GTK main-thread test, FR header, bench names)
    - III. User Experience Consistency → III. User Experience Consistency (expanded: builder pattern, mnemonics, 6-rule main-thread discipline)
    - IV. Performance Requirements → IV. Performance Requirements (expanded: 6-level concurrency ladder, lock-scope discipline, Send futures)
    - V. Observability & Error Handling → V. Observability & Error Handling (expanded: with_context, EnvFilter/JSON appender/stderr layers)
  Added sections: N/A (Technology Stack & Constraints and Development Workflow & Quality Gates refined in place)
  Removed sections: N/A
  Templates requiring updates:
    - .specify/templates/plan-template.md → ⚠ pending (verify Constitution Check references I–V)
    - .specify/templates/spec-template.md → ✅ no changes needed
    - .specify/templates/tasks-template.md → ✅ no changes needed
  Follow-up TODOs: none
-->

# oxhidifi Constitution

## Core Principles

### I. Code Quality (NON-NEGOTIABLE)

All Rust code MUST pass `cargo collate`, `cargo clippy --fix --allow-dirty
--all-targets --all-features && cargo fmt`, `cargo test`, and `cargo bench`
where applicable before commit. `clippy::pedantic` and `clippy::nursery` are
denied, as are the `[lints.rust]` table rules (`unsafe_code`, `missing_docs`,
`missing_debug_implementations`, `unreachable_pub`, `warnings` as deny, and
related hygiene lints). `unwrap_used`, `expect_used`, `panic`, `todo`,
`unimplemented`, `dbg_macro`, `print_stdout`, `print_stderr`,
`allow_attributes`, and `allow_attributes_without_reason` are denied; `#[allow]`
and `#[expect]` suppression attributes are forbidden. `rustfmt.toml`
(`style_edition = "2024"`, `imports_granularity = "One"`, `wrap_comments`,
`format_strings`, `reorder_impl_items`) plus `clippy.toml`
(`absolute-paths-max-segments = 1`, `excessive-nesting-threshold = 3`,
`source-item-ordering = ["module"]`) are normative.

Each `.rs` file MUST NOT exceed 400 lines; module nesting MUST NOT exceed depth
2. Modules MUST group by capability/domain; `models/`, `handlers/`, `utils/`,
`types/`, `common/` are forbidden. File stems MUST be unique codebase-wide
across `src/`, `tests/`, and `benches/` (case-folded, `-`/`_` equivalent,
singular/plural folded). Parent indexes MUST use modern style (`foo.rs` with
`pub mod` declarations plus `//!` docs, sibling `foo/` directory, shared types
or traits permitted in the parent). Imports MUST follow four blank-line
separated blocks (`std`, external crates in one `use { ... }`, own crate by
package name only in `tests/`/`benches/`, `crate::` internal with nested
re-imports) with one item per line; colliding names MUST be aliased at the
import, not qualified at use sites. Every file MUST begin with `//!` docs;
public items, struct fields, and enum variants MUST carry `///` docs with
`# Arguments` / `# Returns` where applicable. Abstractions and generics MUST be
preferred over boilerplate; values that should be configurable MUST NOT be
hardcoded. Refactoring that serves idiomatic, maintainable, high-performance
code MUST take precedence over preserving awkward APIs. Widgets MUST be built
programmatically; `.ui`, `.blp`, and `.xml` files are forbidden. Rationale:
compile-time guarantees and uniform layout prevent bug classes and keep a
bit-perfect audio codebase maintainable.

### II. Testing Standards

Every feature MUST ship with passing tests; all tests MUST pass before a
feature is complete. Unit tests MUST live in a `#[cfg(test)] mod tests` block
at the bottom of the implementing file. Functional tests MUST return
`anyhow::Result` and assert with `ensure!` / `bail!`; trivial tests MUST return
`()` and use `assert!`. `tempfile` MUST be used for filesystem fixtures.
Deterministic simulation testing MUST be used for concurrency-sensitive and
audio-pipeline logic. GTK-dependent tests MUST import
`libadwaita::gtk::{self, test}` and use plain `#[test]`. Integration and
acceptance tests MUST live in `tests/` with a `//!` header referencing the
relevant spec/FR (for example, `No-hardware-at-startup acceptance test
(FR-030)`). New library contracts and contract changes REQUIRE integration
tests at the contract boundary. Audio-pipeline changes REQUIRE `criterion`
benchmarks (`benches/` with `harness = false`, `throughput` and
`conversion_baseline`) demonstrating no regression. Rationale: bit-perfect
gapless playback tolerates no silent corruption; layered automated tests are
the primary defense.

### III. User Experience Consistency

All UI MUST follow GNOME Human Interface Guidelines. Navigation MUST use
`ToolbarView` with `HeaderBar`, never manual `GtkBox` layouts. Preferences
MUST use `PreferencesDialog` with `PreferencesPage`, `PreferencesGroup`, and
typed rows (`ActionRow`, `SwitchRow`, `ComboRow`, `EntryRow`,
`PasswordEntryRow`, `SpinRow`). Feedback MUST use `Toast` and
`suggested-action` / `destructive-action` classes. Responsive layouts MUST use
`AdwBreakpoint`, `AdwNavigationSplitView` plus `AdwNavigationView` for
collapsible panes and page stacks, `AdwOverlaySplitView` for overlay sidebars,
and `AdwViewSwitcher` plus `AdwViewSwitcherBar` for tabs. `AdwLeaflet` is
deprecated since Libadwaita 1.4 and MUST NOT be used. Spacing MUST follow the
6 px scale (6/12/18/24/30 px); radii MUST NEVER be hardcoded. Widgets MUST use
the builder pattern with `css_classes`, `tooltip_text`, `can_focus`, and
`set_use_underline(true)` mnemonics; accessible labels MUST be set via
`update_property(Label(...))`. The GTK main thread MUST NEVER be blocked:
I/O open/close/drop, heavy sync work (image decode, JSON parse, file read,
subprocess), `spawn_local` plus `recv().await` waits, shared broadcast with
mixed-speed subscribers, stale-thread writes, and `yield_now()` spins are
forbidden; workers MUST own I/O, decode off-thread, poll via
`timeout_add_local` (100–500 ms) with `try_recv` drains, use per-subscriber
channels (`Lagged` continues, `Closed` breaks), check id / `is_abandoned()`
before cleanup, and use `sleep(1 ms)`, `Condvar`, or blocking `recv`.
Rationale: HIG compliance plus main-thread discipline keeps the player native,
accessible, and freeze-free.

### IV. Performance Requirements

The audio rendering path MUST stay lock-free, MUST NOT allocate on the hot
path, and MUST use pre-allocated or statically sized buffers with `rtrb`
ring buffers for inter-thread audio transfer. Resampling MUST use `rubato`
with `fft_resampler`. Any audio-processing change REQUIRES a `criterion`
benchmark proving no regression from baseline. Concurrency MUST use the first
fitting abstraction and never reach lower for convenience: `thread::scope`
borrows by default, then `AtomicT` / `OnceLock` / `LazyLock`, then
`parking_lot::RwLock` (read-heavy) / `Mutex` (never `std::sync::*`), then
`Cow`, then `Box` only for large `Send` transfers, with `Arc` as last resort
for dynamic lifetimes only. Lock scopes MUST be minimal and explicit (`{ ...
}` blocks plus `drop(lock)`); I/O or event dispatch MUST NEVER run while
holding a lock. Async task traits MUST declare `Send + Sync + 'static` with
`Send` futures; Tokio errors in tasks MUST be `Send + Sync + 'static`.
`tokio` (async I/O), `async-channel` (channels), `rayon` (data parallelism),
and `parking_lot` (fast locks) are the approved runtime primitives. Rationale:
allocation stalls, lock contention, and main-thread work cause audible
glitches; deterministic low-latency execution is required for bit-perfect
gapless playback.

### V. Observability & Error Handling

Structured `tracing` with typed fields MUST be used everywhere; interpolated
strings MUST NOT replace fields. The binary entry point MUST initialize
`tracing-subscriber` with `EnvFilter`, a JSON file appender, and a
human-readable stderr layer. Library code MUST define typed
`#[derive(Debug, Error)]` errors with a summary doc, `///` on every variant,
`#[error("...")]` messages, and `#[from]` sources; `Box<dyn std::error::Error>`
MUST NOT be used unless no alternative exists. Binary and application
boundaries MUST use `anyhow::{Context, Result}` with `.context()` /
`.with_context()` and MUST NEVER leak `anyhow::Error` across library
boundaries. Callers MUST prefer `?` over `match` chains, use
`if let Ok(..) else { ... }` for simple recovery, and MUST NEVER use `let _`
or `.ok()` to discard errors. Error types MUST document the enum and each
variant. Rationale: diagnosing dropouts, resampling faults, or catalog
failures demands precise structured context, not discarded or opaque errors.

## Technology Stack & Constraints

Edition 2024, `autotests = false` with explicit `[[test]]` and `[[bench]]`
entries, `default-features = false` throughout, `verification-tests` feature
gating `load_verification`, `response_time`, `sc006`, and `zero_alloc`.

**Audio Engine:** `alsa` (Linux-only target dep), `cpal` (device
abstraction), `symphonia` with `all` (decoding), `rtrb` (lock-free transport),
`lofty` with `id3v2_compression_support` (metadata), `rubato` with
`fft_resampler` (resampling), `num-traits`.

**Concurrency:** `tokio` (`macros`, `rt-multi-thread`, `signal`; `test-util`
in dev), `async-channel` with `std`, `parking_lot`, `rayon`. `crossbeam`,
`dynosaur`, `tokio-stream`, and `tokio-util` are currently commented out in
`Cargo.toml` and MUST NOT be assumed available without re-adding them.

**Data & Persistence:** `sqlx` (`macros`, `runtime-tokio`, `sqlite`) for the
catalog, config, and migrations; `serde` plus `serde_json` for serialization;
`sha2` and `hex` with `alloc` for hashing and dedup; XDG paths owned by
`src/app.rs` bootstrap.

**UI:** `libadwaita` (`gio_v2_80`, `gtk_v4_24`, `v1_10`), programmatic widgets
only.

**Utilities:** `anyhow`, `thiserror`, `notify` (file watcher), `tracing` plus
`tracing-subscriber` (`ansi`, `env-filter`, `json`) plus `tracing-appender`;
`regex` is commented out and MUST NOT be assumed available. Test and bench
only: `tempfile`, `criterion` (also a main utility for pipeline benchmarks).

## Development Workflow & Quality Gates

`CODING_STANDARDS.md` is the single source of truth for style, error handling,
concurrency, tracing, docs, UI/HIG, testing, and build commands; this
constitution restates its non-negotiables for governance. Every commit MUST
pass:

```bash
cargo collate # project hygiene checks (must pass before committing)
cargo clippy --fix --allow-dirty --all-targets --all-features && cargo fmt
cargo test    # all tests must pass before committing
cargo bench   # required for audio pipeline changes
```

Commits with clippy warnings MUST be rejected; warnings are errors. Lint
suppression via `#[allow]` or `#[expect]` is forbidden. Before using unfamiliar
libraries, official docs MUST be queried via the `Context7` MCP server. Source
layout rules (capability grouping, stem uniqueness, parent-index modules,
400-line and depth-2 limits, four-block imports, `//!`/`///` docs) are quality
gates, not suggestions. `src/app.rs` owns bootstrap, runtime, lifecycle, and
XDG paths; `src/library/` owns scanning, metadata, artwork, dedup, and file
watching; `src/playback/` owns engine, decoder, resampler, gapless
transitions, queue, and volume; `src/storage/` owns SQLite catalog, config,
migrations, settings, and sort rules; `src/ui/` owns gallery, player,
preferences, detail, and navigation.

## Governance

This constitution supersedes all other development practices in this
repository. Amendments REQUIRE a documented proposal, team approval, and a
migration plan for existing code. Versioning follows semantic rules: MAJOR for
incompatible principle removals or redefinitions, MINOR for new principles or
materially expanded guidance, PATCH for clarifications and wording fixes. All
pull requests and reviews MUST verify compliance; any violation of a
NON-NEGOTIABLE principle MUST carry documented justification accepted by the
team. `AGENTS.md` provides runtime agent guidance; `CODING_STANDARDS.md`,
`rustfmt.toml`, `clippy.toml`, and the `[lints.*]` tables in `Cargo.toml`
provide enforced style truth.

**Version**: 1.2.0 | **Ratified**: 2026-05-22 | **Last Amended**: 2026-09-26
