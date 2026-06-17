# Data Model: High-Fidelity Music Player

## Entity: Track

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `id` | `i64` (SQLite AUTOINCREMENT) | PRIMARY KEY | Unique track identifier |
| `title` | `TEXT` | NOT NULL | Track title (filename stem as fallback) |
| `track_number` | `INTEGER` | NULLABLE | Disc/track number from metadata |
| `disc_number` | `INTEGER` | NULLABLE | Disc number |
| `duration` | `REAL` | NOT NULL, >= 0 | Duration in seconds (floating-point) |
| `file_path` | `TEXT` | NOT NULL, UNIQUE | Absolute path to audio file |
| `content_hash` | `TEXT` | NULLABLE | SHA-256 hex digest (computed on scan) |
| `format` | `TEXT` | NOT NULL | File format (FLAC, MP3, AAC, Ogg, Opus, WAV, AIFF) |
| `sample_rate` | `INTEGER` | NOT NULL, > 0 | Native sample rate in Hz |
| `bit_depth` | `INTEGER` | NULLABLE | Bit depth (none for lossy formats) |
| `channels` | `INTEGER` | NOT NULL, > 0 | Number of audio channels |
| `codec` | `TEXT` | NOT NULL | Codec identifier from symphonia |
| `lossless` | `INTEGER` (bool) | NOT NULL | 1 if lossless, 0 if lossy |
| `bitrate` | `INTEGER` | NULLABLE | Average bitrate in kbps (lossy formats) |
| `album_id` | `INTEGER` | FOREIGN KEY → Album(id), NULLABLE | Parent album |
| `artist_id` | `INTEGER` | FOREIGN KEY → Artist(id), NULLABLE | Track artist (may differ from album artist) |
| `file_size` | `INTEGER` | NOT NULL, > 0 | File size in bytes |
| `last_modified` | `TEXT` (ISO 8601) | NOT NULL | Filesystem mtime at scan time |
| `created_at` | `TEXT` (ISO 8601) | NOT NULL, DEFAULT CURRENT_TIMESTAMP | Database insertion time |

**Validation rules**:
- `file_path` must be a valid absolute path to an existing file at scan time
- `duration` must be > 0 when populated from decoded metadata; files whose extracted metadata explicitly reports 0.0 are treated as corrupt and skipped. Files with absent metadata receive a fallback value of 1.0 (see FR-005).
- `sample_rate` must be one of: 8000, 11025, 16000, 22050, 44100, 48000, 88200, 96000, 176400, 192000
- `channels` must be 1 or 2 (stereo-only for initial release)

**Duplicate detection hierarchy**:
1. `file_path` uniqueness (primary key constraint)
2. `content_hash` collision → confirm same file at different path
3. Metadata fingerprint (artist + album + title + track_number) → probable duplicate with different hash/path

**State transitions**: Track exists in one state — `cataloged`. Removal is by filesystem deletion detected by watcher.

---

## Entity: Album

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `id` | `i64` (SQLite AUTOINCREMENT) | PRIMARY KEY | Unique album identifier |
| `title` | `TEXT` | NOT NULL | Album title (fallback: "Unknown Album") |
| `artist_id` | `INTEGER` | FOREIGN KEY → Artist(id), NOT NULL | Album artist |
| `year` | `INTEGER` | NULLABLE | Release year |
| `genre` | `TEXT` | NULLABLE | Genre tag |
| `artwork_path` | `TEXT` | NULLABLE | Path to extracted/cached album art |
| `track_count` | `INTEGER` | NOT NULL, DEFAULT 0 | Number of tracks in album |
| `total_duration` | `REAL` | NOT NULL, DEFAULT 0.0 | Sum of track durations |
| `format_summary` | `TEXT` | NOT NULL | e.g., "FLAC 24-bit/96kHz" |
| `lossless` | `INTEGER` (bool) | NOT NULL | 1 if all tracks lossless, 0 otherwise |

**Validation rules**:
- Album title must not be empty (use "Unknown Album" fallback)
- `total_duration` = SUM(tracks.duration) for the album, maintained by trigger/application logic
- `format_summary` derived from track formats; mixed-format albums show e.g., "FLAC + MP3"

---

## Entity: Artist

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `id` | `i64` (SQLite AUTOINCREMENT) | PRIMARY KEY | Unique artist identifier |
| `name` | `TEXT` | NOT NULL, UNIQUE | Artist name (fallback: "Unknown Artist") |
| `album_count` | `INTEGER` | NOT NULL, DEFAULT 0 | Number of albums by this artist |

**Validation rules**:
- Name must not be empty (use "Unknown Artist" fallback)
- Name uniqueness is case-insensitive for display but case-preserving

---

## Entity: LibraryDirectory

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `id` | `i64` (SQLite AUTOINCREMENT) | PRIMARY KEY | Unique directory identifier |
| `path` | `TEXT` | NOT NULL, UNIQUE | Absolute filesystem path |
| `enabled` | `INTEGER` (bool) | NOT NULL, DEFAULT 1 | Whether this directory is actively watched |
| `last_scanned` | `TEXT` (ISO 8601) | NULLABLE | Timestamp of last completed scan |
| `added_at` | `TEXT` (ISO 8601) | NOT NULL, DEFAULT CURRENT_TIMESTAMP | When directory was added |

---

## Entity: PlaybackQueue

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| `id` | `i64` (SQLite AUTOINCREMENT) | PRIMARY KEY | Unique queue entry identifier |
| `track_id` | `INTEGER` | FOREIGN KEY → Track(id), NOT NULL | Track in queue |
| `position` | `INTEGER` | NOT NULL, >= 0 | Order in queue (0 = next to play) |
| `context_type` | `TEXT` | NULLABLE | How track was queued: "album", "artist", "manual" |
| `context_id` | `INTEGER` | NULLABLE | ID of album/artist if auto-queued |
| `added_at` | `TEXT` (ISO 8601) | NOT NULL, DEFAULT CURRENT_TIMESTAMP | When track was added to queue |

**Persistence**: Queue state saved to SQLite on every mutation (add, remove, reorder) and restored on application start.

---

## Entity: UserSettings

Stored as JSON file at XDG config path (`$XDG_CONFIG_HOME/oxhidifi/settings.json`).

| Field | Type | Description |
|-------|------|-------------|
| `library_directories` | `Vec<String>` | List of configured library paths |
| `audio_device` | `Option<String>` | Preferred audio output device name (None = default) |
| `volume` | `f64` (0.0–1.0) | Playback volume level |
| `view_mode` | `enum { Grid, Column }` | Current view mode preference |
| `active_tab` | `enum { Albums, Artists }` | Last active tab |
| `window_width` | `i32` | Stored window width |
| `window_height` | `i32` | Stored window height |
| `window_maximized` | `bool` | Whether window is maximized |

---

## Entity Relationships

```
Artist (1) ──< (N) Album (1) ──< (N) Track
                                           
LibraryDirectory (1) ──< (N) Track (scanned from directory)
                                           
PlaybackQueue (N) ──> (1) Track (ordered entries)
```

## SQL Schema Indexes

| Table | Index | Columns | Purpose |
|-------|-------|---------|---------|
| Track | `idx_track_album_id` | `album_id` | Album detail page queries |
| Track | `idx_track_artist_id` | `artist_id` | Artist detail page queries |
| Track | `idx_track_file_path` | `file_path` | Duplicate detection by path |
| Track | `idx_track_content_hash` | `content_hash` | Duplicate detection by hash |
| Album | `idx_album_artist_id` | `artist_id` | Artist → albums queries |
| PlaybackQueue | `idx_queue_position` | `position` | Ordered queue retrieval |
