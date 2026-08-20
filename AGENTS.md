---
name: code_agent
description: Senior Rust developer using modern idiomatic Rust and Libadwaita for `oxhidifi`
---

# oxhidifi

High-fidelity music player focusing on bit-perfect audio playback and gapless transitions.

## Tech stack

- **Audio Engine:** alsa, cpal, lofty, num-traits, rtrb, rubato, symphonia
- **Concurrency:** async-channel, crossbeam, dynosaur, parking-lot, rayon, tokio, tokio-stream, tokio-util
- **UI:** libadwaita
- **Utilities:** anyhow, criterion, hex, notify, regex, serde + serde_json, sha2, sqlx, tempfile, thiserror, tracing + tracing-subscriber +
  tracing-appender

## Codebase map (src/)

- `benches/` — criterion benchmarks
- `specs/` — feature specs (FR-xxx)
- `src/app.rs` — bootstrap, runtime, lifecycle, XDG paths
- `src/library/` — media scanning (`scanner/`), metadata, artwork, dedup, file watcher
- `src/playback/` — audio engine, decoder, resampler, gapless transitions, queue, volume
- `src/storage/` — SQLite (catalog, config, migrations), settings, sort rules
- `src/threading.rs` — concurrency helpers
- `src/ui/` — Libadwaita UI: gallery, player, preferences, detail, navigation
- `tests/` — integration tests referencing them

## Conventions & workflow

- Read `CODING_STANDARDS.md` before writing code — single source of truth for style, error handling, concurrency, tracing, docs, UI/HIG, testing, and
  build commands (lint, format, test, bench).
