# Research: High-Fidelity Music Player Refactoring

## Architecture Decisions

### Decision: Single-process GTK4/Libadwaita application with tokio async runtime

**Rationale**: The application is a desktop music player — no server component, no multi-process architecture. Libadwaita provides the native GNOME look-and-feel mandated by HIG. tokio handles async I/O for library scanning and file watching without blocking the UI thread.

**Alternatives considered**:
- Crossbeam channels + manual event loop: Would require reimplementing async I/O patterns that tokio provides out of the box
- GTK4's built-in `glib` event loop only: Not suitable for async I/O like filesystem scanning with cancellation
- Multi-process (separate player daemon + UI): Over-engineered for a single-user desktop app

### Decision: SQLite via `sqlx` for library catalog persistence

**Rationale**: Local single-user app with structured query patterns (lookup by album, artist, genre; full-text search). SQLite is zero-config, embedded, and `sqlx` provides compile-time query checking. The library catalog maps naturally to relational tables (tracks, albums, artists).

**Alternatives considered**:
- `serde_json` flat files: Would require reimplementing filtering, sorting, and joining in memory — poor for 10k+ tracks
- `sled` embedded database: Key-value store lacks relational query capabilities needed for library browsing

### Decision: Lock-free architecture with `rtrb` ring buffers for audio pipeline

**Rationale**: Audio hot path must never block. `rtrb` provides a wait-free single-producer single-consumer ring buffer that fits the decode → resample → output pipeline. Pre-allocated buffers avoid allocation on the hot path.

**Alternatives considered**:
- `crossbeam` channels: Bounded channels use mutexes internally, introducing potential contention
- `parking_lot` `Mutex` + `VecDeque`: Lock contention on every buffer swap — unacceptable for real-time audio

### Decision: `symphonia` for decoding, `cpal` for output

**Rationale**: `symphonia` is pure Rust, supports all required formats (FLAC, MP3, AAC, Ogg Vorbis, Opus, WAV, AIFF), and provides frame-level access for gapless transitions. `cpal` is the standard Rust audio output abstraction with Linux PulseAudio/PipeWire support.

**Alternatives considered**:
- `rodio`: Higher-level but lacks frame-level control needed for gapless transitions
- FFI to `libavcodec` (FFmpeg): Adds build complexity and C dependency; symphonia covers all needed formats in pure Rust

### Decision: `rubato` + `audioadapter-buffers` for resampling

**Rationale**: `rubato` provides high-quality asynchronous resampling with multiple algorithms (Linear, Sinc, FFT). `audioadapter-buffers` provides efficient buffer management for the resampler I/O. Fixed-size input/output buffers prevent allocation at resample time.

**Alternatives considered**:
- `sample` crate: Provides type conversion but not sample-rate conversion
- Custom sinc resampler: Unnecessary when `rubato` is battle-tested and benchmarked

### Decision: `lofty` for metadata parsing

**Rationale**: Pure Rust, supports ID3v2, Vorbis comments, APE tags, and embedded pictures across all target formats. Actively maintained.

**Alternatives considered**:
- `id3` crate: ID3-only, doesn't cover Vorbis comments or APE
- `metaflac`: FLAC-only, doesn't cover MP3, AAC, etc.

### Decision: `notify` for filesystem watching

**Rationale**: De facto Rust file-watching library. Cross-platform. Used for detecting new/deleted/modified audio files in library directories.

**Alternatives considered**:
- Polling-based scanning: Higher latency, wastes CPU cycles
- `inotify` directly: Linux-only, loses cross-platform compatibility

## Key Integration Patterns

### Audio Pipeline Thread Topology

```
[Main/UI Thread]          ↔ commands (play, pause, skip, seek)
[Scanner Thread]          ↔ discovered tracks (tokio task)
[Decoder Thread]          → decoded PCM frames (rtrb::RingBuffer)
[Resampler Thread]        → resampled PCM frames (rtrb::RingBuffer)
[Audio Output Thread]     → cpal callback (real-time priority)
```

### Scanning Flow

```
notify event / manual scan → scanner task → metadata extraction (lofty)
  → dedup check (path → hash → fingerprint) → SQLite insert
  → UI update signal (incremental)
```

### Gapless Transition

```
track N decode completes → decoder signals "end of stream"
  → output drains remaining buffer
  → decoder starts track N+1 decode (pre-buffered)
  → sample rate reconfig (resampler reset if needed)
  → next PCM frames enter ring buffer
```
