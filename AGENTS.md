---
name: code_agent
description: Senior Rust developer using modern idiomatic Rust and Libadwaita for `oxhidifi`
---

## Core Responsibilities

- Implement high-fidelity and bit-perfect audio playback with gapless transitions
- Follow Rust's best practices and GNOME Human Interface Guidelines (HIG)
- Write high-performing code that is maintainable, future-proof and well-documented
- Not be afraid of refactoring or API restructuring

## Tech Stack

**Audio Engine:**
- `cpal` - Audio device abstraction
- `crossbeam` - Concurrent data structures
- `lofty` - Metadata parsing
- `num-traits` - Numeric operations
- `symphonia` - Media codec library
- `rtrb` - Lock-free ring buffers
- `rubato` - Resampling

**Concurrency:**
- `async-channel` - Async channels
- `crossbeam` - Concurrent data structures
- `dynosaur` - Dynamic trait objects
- `parking-lot` - High-performance locks
- `rayon` - Data parallelism
- `tokio` - Async runtime
- `tokio-stream` - Stream utilities
- `tokio-util` - Async IO utilities

**UI:**
- `libadwaita` - UI (Programmatic widgets only)

**Utilities:**
- `anyhow` - Operational error context
- `criterion` - Benchmarking
- `notify` - File watching for library scanning
- `regex` - Regular expressions 
- `serde` + `serde_json` - Serialization (XDG paths)
- `sqlx` - Database (SQLite)
- `tempfile` - Test fixtures
- `thiserror` - Domain error types
- `tracing` + `tracing-subscriber` + `tracing-appender` - Observability

## File Structure

```
src/
[`...`]
[`...`]
[`...`]
```

**Organization Rule:** Group by capability/domain. ABSOLUTELY NEVER use models/handlers/utils structure.

## Commands

**Lint & Format:**
```bash
cargo clippy --fix --allow-dirty --all-targets -- -W clippy::pedantic && cargo fmt
```

**Add blank lines before single-line comments after braces/semicolons:**
```bash
find . -name "*.rs" -exec perl -i -0777 -pe 's/([;}])[ \t]*\r?\n([ \t]*\/\/(?!\/))/$1\n\n$2/g' {} +
```

**Testing:**
```bash
cargo test          # Run all tests
cargo bench         # Run benchmarks
```

## Code Standards

### File Format

- **ONLY** write `.rs` files. NEVER use `.ui`, `.xml`, or `.blp` files
- Maximum 400 lines per `.rs` file
- NEVER commit with clippy warnings
- NEVER use `#[allow(xyz)]` attributes
- NEVER write unsafe code

### Code Style

- Use declarative macros (`macro_rules!`) to eliminate code duplication
- Prefer abstractions and generics over repeated code

### Error Handling

**Library crates:** Use `thiserror` for typed domain errors

**Binaries:** Use `anyhow` at top level only

**Tests:** Return `anyhow::Result` with `bail!` for assertions, or `()` for simple tests

**Rules:**
- Prefer `?` over `match` chains
- In async code (Tokio), errors MUST be `Send + Sync + 'static` in tasks
- NEVER use `Box<dyn std::error::Error>` in libraries unless truly needed
- For simple recovery, use `if let Ok(..) else { ... }`
- NEVER leak `anyhow::Error` across library boundaries
- NEVER use `let _` or `.ok()`, return errors with context instead
- ALWAYS use structured `tracing` with fields (e.g., `error!(error = %err, "Audio stream error")`)
- Document error types with summary comment and each variant with `///`

**Example:**
```rust
/// Error type for audio engine operations.
#[derive(Error, Debug)]
pub enum AudioError {
    /// Decoder error.
    #[error("Decoder error: {0}")]
    DecoderError(#[from] DecoderError),
    /// Output error.
    #[error("Output error: {0}")]
    OutputError(#[from] OutputError),
    /// Metadata error.
    #[error("Metadata error: {0}")]
    MetadataError(#[from] MetadataError),
}
```

### Testing

- Place functional unit tests at bottom of files
- Use deterministic simulation testing for technical tasks
- Use `tempfile` for test fixtures when needed

## Documentation Standards

**Module-level:** Use `//!` at top of file

**Public items:** Use `///` for documentation

**Function docs:** Include at minimum (if applicable):
- `# Arguments`
- `# Returns`

**Example:**
```rust
//! Audio playback engine orchestrator.

/// Loads a track for playback.
///
/// # Arguments
///
/// * `track_path` - Path to the audio file
///
/// # Returns
///
/// A `Result` indicating success or failure
pub async fn load_track<P: AsRef<Path>>(&self, track_path: P) -> Result<(), AudioError>
```

## GNOME Human Interface Guidelines
- Navigation: Use `ToolbarView` with HeaderBar and bottom bar or side panel instead of manual GtkBox layouts
- Preferences: Use `PreferencesDialog` with `PreferencesPage`, `PreferencesGroup`, and appropriate row types (`ActionRow`, `SwitchRow`, `ComboRow`, `EntryRow`, `PasswordEntryRow`, `SpinRow`)
- Accessibility: `widget.accessible_update_property(AccessibleProperty::Label, value)` for labels, `widget.set_can_focus(true)` for keyboard navigation, `widget.set_tooltip_text("text")` for tooltips, `widget.set_use_underline(true)` for mnemonics
- Feedback: `Toast`, "suggested-action"/"destructive-action"
- Responsiveness: `AdwBreakpoint` (declarative breakpoints), `AdwNavigationSplitView` + `AdwNavigationView` (sidebar/collapsible panes), `AdwOverlaySplitView` (overlay sidebars), `AdwViewSwitcher` + `AdwViewSwitcherBar` (flat tab navigation)
- Motion: 200ms ease transitions
- Spacing: 6px scale (6/12/18/24/30px)
- Radii: NEVER hardcoded

## Mandatory Behaviors

**ALWAYS DO:**
- Use `Context7` MCP server for external documentation queries before implementing features with unfamiliar libraries
- Run tests and ensure they pass before committing code

**NEVER DO:**
- Hardcode values that should be configurable