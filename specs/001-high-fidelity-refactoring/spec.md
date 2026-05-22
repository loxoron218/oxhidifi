# Feature Specification: High-Fidelity Music Player Refactoring

**Feature Branch**: `001-high-fidelity-refactoring`

**Created**: 2026-05-22

**Status**: Draft

**Input**: User description: "Build a high-fidelity refactoring of `/home/arch/Downloads/github/oxhidifi` that has better performance and improves on maintainability. An empty state page appears when nothing has been added to the library. Album/artist tabs, each with grid/column views can be toggled via buttons on the header bar. All albums/artists have detail pages and a side panel with the player slides from the left side when playback starts. Adaptive design should adapt to GTK4/Libadwaita modern and idiomatic standards. Audio pipeline should be gapless and bit-perfect, while resampling when needed"

## Clarifications

### Session 2026-05-22

- Q: How should duplicate audio files be detected? → A: Layered strategy — primary dedup by file path, content hash (SHA-256) on path collision, metadata fingerprint (artist+album+title+track) as final fallback.
- Q: How should the playback queue be populated and managed? → A: Auto-queue from current context (playing an album queues its tracks in order) with full manual reorder/add/remove support; queue state persisted across application restarts.
- Q: How should the UI behave during library scanning? → A: Incremental non-blocking — UI stays responsive, items appear as discovered, scanning indicator in status bar.
- Q: What level of observability should be built in? → A: Structured logging (`tracing` crate) plus performance metrics covering playback latency, scan throughput, memory usage, and UI response times.
- Q: What level of accessibility support is targeted? → A: GNOME HIG baseline — keyboard navigation (Tab/arrows/Enter/Escape), accessible labels on all interactive widgets, focus indicators.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Browse and Play Music from Library (Priority: P1)

A user opens the application after adding music files to their library. They see their albums displayed in a grid layout. They can switch to an artist view. They click an album to see its detail page with track listing, then click play to start gapless playback.

**Why this priority**: Core value proposition — without playback and browsing, the application serves no purpose.

**Independent Test**: Can be tested by pointing the application to a directory with audio files, verifying the library populates, selecting a track, and confirming audible playback starts.

**Acceptance Scenarios**:

1. **Given** a user has audio files in a configured library directory, **When** the application launches, **Then** all albums and artists are displayed in the library view
2. **Given** the library view is displayed, **When** the user clicks an album or artist, **Then** a detail page opens showing tracks and metadata
3. **Given** a detail page is displayed with tracks, **When** the user clicks play on a track, **Then** gapless audio playback begins from the first track
4. **Given** playback is in progress, **When** a track ends, **Then** the next track begins seamlessly without audible gap or interruption
5. **Given** playback is active, **When** the user clicks pause, **Then** playback stops and can be resumed from the same position

---

### User Story 2 - Empty State and Library Navigation (Priority: P1)

A user opens the application for the first time with no music library configured. They see a helpful empty state inviting them to add music. Once music is added, they can navigate between album and artist views, toggle between grid and column layouts.

**Why this priority**: First-run experience is critical for user onboarding; navigation controls must be immediately discoverable.

**Independent Test**: Can be tested by launching with no library directories configured — the empty state should appear. Then configuring a directory should immediately populate the library.

**Acceptance Scenarios**:

1. **Given** the application launches with no music library, **When** the main view is displayed, **Then** an empty state page is shown with guidance on adding music
2. **Given** the library is populated, **When** the user clicks the Albums tab, **Then** albums are displayed in the current view mode
3. **Given** the library is populated, **When** the user clicks the Artists tab, **Then** artists are displayed in the current view mode
4. **Given** any tab is active, **When** the user clicks the grid/column toggle button in the header bar, **Then** the view mode switches accordingly
5. **Given** music is added to the library after an empty state, **When** scanning completes, **Then** the empty state is replaced by the populated library view

---

### User Story 3 - Bit-Perfect Gapless Playback with Resampling (Priority: P2)

A user plays a high-resolution audio file (e.g., 96 kHz / 24-bit FLAC). The player detects the file format and sample rate, configures the audio output for bit-perfect playback if the device supports it, or resamples to the device's native rate otherwise, all without interrupting the listening experience.

**Why this priority**: High-fidelity playback is the defining feature, but basic playback must work first.

**Independent Test**: Can be tested by playing files of varying sample rates (44.1 kHz, 48 kHz, 96 kHz, 192 kHz) and verifying playback is correct and gapless across transitions between different sample rates. Bit-perfect output can be verified by comparing input/output bit patterns.

**Acceptance Scenarios**:

1. **Given** a high-resolution audio file is loaded, **When** playback starts, **Then** the audio is output at the file's native sample rate when the device supports it
2. **Given** a file's sample rate differs from the device's capabilities, **When** playback starts, **Then** the audio is transparently resampled with imperceptible quality loss
3. **Given** files with different sample rates play consecutively, **When** transitioning between tracks, **Then** playback remains gapless with seamless sample rate reconfiguration
4. **Given** a lossless file is played, **When** the audio reaches the output device, **Then** no data is altered or re-encoded, preserving the original bit-perfect fidelity

---

### User Story 4 - Side Panel Player (Priority: P2)

A user starts playback from the library view. A side panel slides in from the left side of the window showing album artwork, current track information, and playback controls. The user can continue browsing while the panel remains visible.

**Why this priority**: Enhances the browsing-while-listening experience but depends on basic playback.

**Independent Test**: Can be tested by starting playback and verifying the side panel appears, shows correct track info, allows navigation of the library simultaneously, and can be closed or auto-hides when playback stops.

**Acceptance Scenarios**:

1. **Given** no playback is active, **When** the user starts playing a track, **Then** a side panel slides in from the left with album artwork, track title, artist name, and playback controls
2. **Given** the side panel is visible, **When** the user interacts with the main library view, **Then** both the panel and library remain accessible
3. **Given** the side panel is visible, **When** the user stops playback or the queue empties, **Then** the panel slides back out
4. **Given** the side panel is visible on a narrow window, **When** the user clicks a back button in the panel, **Then** the panel hides to maximize content space

---

### User Story 5 - Detail Pages for Albums and Artists (Priority: P3)

A user on an album detail page sees the full track listing, album metadata (year, genre, format, sample rate, bit depth), and album artwork. On an artist detail page, they see the artist's albums grouped together.

**Why this priority**: Rich detail pages enhance exploration but are not required for core playback.

**Independent Test**: Can be tested by navigating from any album or artist in the library to its detail page and verifying all expected information is displayed.

**Acceptance Scenarios**:

1. **Given** the user clicks an album from the library view, **When** the detail page loads, **Then** it displays the album artwork, title, artist, year, genre, format details, and a complete track listing
2. **Given** the user clicks an artist from the library view, **When** the detail page loads, **Then** it displays the artist name and all albums by that artist
3. **Given** a detail page is displayed, **When** the user clicks a track, **Then** playback starts or is queued

---

### Edge Cases

- What happens when the user has thousands of albums/artists in the library? Library browsing should remain smooth without UI freezes.
- How does the system handle corrupted or unreadable audio files? They should be skipped during scanning and gracefully excluded from playback.
- What happens when the audio output device is disconnected during playback? The player should pause gracefully and indicate the device was lost.
- How does the player handle an empty playlist or queue after the last track finishes? It should stop and show the idle state.
- What happens when sample rate changes mid-playlist between tracks with no common divisor rate? The resampler should transparently reconfigure.
- How does the system behave when the same audio file is added to the library twice? Duplicate detection should prevent double entries.
- What if no audio output device is available at launch? The application should start gracefully and show a message about missing audio hardware.
- How does the system handle files that contain no embedded metadata? Use filename-derived display names as fallback.
- What happens during library scanning when new files appear mid-scan? Incremental handling should pick them up without restarting.

## Requirements *(mandatory)*

### Functional Requirements

**Library Management**

- **FR-001**: The system MUST detect and catalog all supported audio files from one or more user-configured directories.
- **FR-002**: The system MUST extract and store metadata (title, artist, album, year, genre, track number, disc number, duration) from each audio file.
- **FR-003**: The system MUST extract and store technical metadata (sample rate, bit depth, number of channels, codec, lossless status) from each audio file.
- **FR-004**: The system MUST extract and display embedded album artwork from audio files.
- **FR-005**: The system MUST detect and exclude duplicate files using a layered strategy: file path uniqueness as primary dedup, content hash (SHA-256) on path collision, and metadata fingerprint (artist+album+title+track) as final fallback.
- **FR-006**: The system MUST gracefully handle files with missing or corrupt metadata using the following fallback chain: filename stem as title, "Unknown Artist" as artist, "Unknown Album" as album, 0 as year, "Unknown Genre" as genre, null as track/disc number, 0 as duration (files with 0 duration MUST be skipped as corrupt).
- **FR-007**: The system MUST automatically scan library directories for changes (additions, removals, updates) and reflect them without manual intervention.
- **FR-007b**: Library scanning MUST operate incrementally and non-blocking — the UI remains responsive during scan, discovered items appear as they are indexed, and a scanning indicator is shown in the status bar.
- **FR-008**: The system MUST present an informative empty state when no music library is configured or when the library contains no files.

**Navigation and Views**

- **FR-009**: The system MUST provide separate browsable views for Albums and Artists, switchable via tab buttons in the header bar.
- **FR-010**: The system MUST support at least two view modes per tab: a grid layout and a column/list layout.
- **FR-011**: The system MUST provide a toggle control in the header bar to switch between grid and column views.
- **FR-012**: Each album and artist MUST have a dedicated detail page showing full metadata and associated content (tracks for albums, albums for artists).
- **FR-013**: The system MUST support adaptive/responsive layouts using `AdwBreakpoint` with the modern widget stack (`AdwNavigationSplitView` for sidebar/content, `AdwNavigationView` for page stacks, `AdwOverlaySplitView` for player panel, `AdwViewSwitcher` + `AdwViewSwitcherBar` for tab navigation) that adjust to different window sizes, with at minimum: a wide mode (>800px) showing side panel and library side-by-side, and a narrow mode (≤800px) stacking them with back-navigation.
- **FR-013b**: The system MUST support keyboard navigation (Tab, arrows, Enter, Escape), provide accessible labels via `AccessibleProperty::Label` on all interactive widgets, and provide tooltip text via `set_tooltip_text()` on all actionable controls.

**Playback**

- **FR-014**: The system MUST support gapless playback — consecutive tracks play without any audible silence or interruption between them.
- **FR-015**: The system MUST output audio at the file's native sample rate and bit depth when the output device supports it, preserving the original bit-perfect stream.
- **FR-016**: When the output device does not support the file's native sample rate, the system MUST transparently resample to a supported rate. Resampled output MUST maintain SNR > 120 dB relative to the original and MUST pass a blind ABX test with p < 0.05 against the original at the matched sample rate.
- **FR-017**: The system MUST support common audio formats including FLAC, MP3, AAC, Ogg Vorbis, Opus, WAV, and AIFF.
- **FR-018**: The system MUST support high-resolution audio (sample rates up to at least 192 kHz, bit depths up to 24-bit).
- **FR-019**: The system MUST provide standard playback controls: play, pause, stop, next track, previous track.
- **FR-020**: The system MUST provide a seek control to navigate within the currently playing track.
- **FR-021**: The system MUST provide a volume slider in the player panel (range 0.0–1.0 mapped to dB attenuation) with a mute toggle button. Volume level MUST be persisted across application restarts via `UserSettings.volume`.

**Queue Management**

- **FR-022**: The system MUST provide a visible playback queue with the ability to view upcoming tracks, manually reorder (drag-and-drop), add tracks from any browse/detail view, and remove individual entries.
- **FR-023**: The queue MUST auto-populate from the current browsing context (playing an album queues all its tracks in order; playing an artist queues all albums' tracks). Manual additions and reordering MUST be preserved until the context changes.

**Player Panel**

- **FR-024**: When playback starts, a side panel MUST slide in from the left displaying album artwork, current track metadata, and playback controls.
- **FR-025**: The side panel MUST remain visible and functional while the user interacts with the main library view.
- **FR-026**: The side panel MUST slide out when playback stops or the queue is empty.
- **FR-027**: On narrow/compact window sizes, the side panel MUST support a back navigation to hide it and maximize content space.

**Performance and Reliability**

- **FR-028**: Library browsing operations (view switching, scrolling, detail page navigation) MUST remain responsive (UI response <100 ms per SC-005) regardless of library size.
- **FR-029**: The system MUST persist library, playback queue, and settings data across application restarts.
- **FR-030**: The system MUST recover gracefully from audio device disconnection or configuration changes.
- **FR-031**: The system MUST handle application startup even when no audio device is available, displaying appropriate messaging.

**Observability**

- **FR-032**: The system MUST emit structured logs using the `tracing` crate at minimum error, warn, and info levels across all subsystems (library scanning, playback engine, UI).
- **FR-033**: The system MUST collect and expose performance metrics for playback latency (play initiation to first audio output), library scan throughput (files/second), memory usage, and UI response times to validate success criteria.

### Key Entities *(include if feature involves data)*

- **Track**: Represents a single audio file. Attributes include title, track number, duration, file path, content hash (SHA-256 for dedup), format, sample rate, bit depth, channels, codec, lossless flag, and a foreign key to its album.
- **Album**: A collection of tracks grouped by release. Attributes include title, artist, year, genre, artwork path, track count, format summary, and a foreign key to the artist.
- **Artist**: A music artist or group. Attributes include name and a list of associated albums.
- **Library Directory**: A user-configured filesystem path containing audio files to be cataloged.
- **Playback Queue**: An ordered list of tracks awaiting or currently being played, supporting advance to next track and return to previous track. Auto-populated from current browsing context (album or artist). Supports manual reorder, add, and remove. Queue state is persisted across application restarts.
- **User Settings**: Persisted user preferences including library directories, audio output configuration, view preferences, and volume level.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can browse their full library (albums and artists), start playback of any track, and hear audio output within 3 seconds of initiating play.
- **SC-002**: Gapless playback is verified by playing a sequence of tracks — there is no audible silence or gap between any two consecutive tracks.
- **SC-003**: Bit-perfect playback is verified by comparing the digital audio output against the source file — the bit stream matches exactly when the device supports the file's native format.
- **SC-004**: A library of 10,000 tracks loads and becomes browsable within 30 seconds on reference hardware (Intel i5-1135G7, 16 GB RAM, NVMe SSD).
- **SC-005**: Users can navigate between Albums and Artists views, toggle between grid and column layouts, and access detail pages without perceivable UI lag (response under 100ms).
- **SC-006**: The empty state is shown on first launch when no library is configured; the library view populates within 10 seconds of configuring a directory with 1,000 audio files.
- **SC-007**: The side player panel appears within 500ms of playback starting and display correct track metadata and artwork.
- **SC-008**: Transitions between tracks with different sample rates (e.g., 44.1 kHz to 96 kHz) are seamless and gapless without requiring user intervention.
- **SC-009**: Resampled audio MUST pass a blind ABX test (p < 0.05 threshold) comparing resampled output against the original source at matched sample rate, with a minimum of 10 trials per test.

## Assumptions

- The application targets the GNOME desktop environment on Linux and follows GNOME Human Interface Guidelines.
- Users run the application on systems with a working audio server (e.g., PulseAudio or PipeWire) that provides a standard audio output device.
- All music files are stored locally on the user's filesystem — no streaming or network-based music sources are in scope.
- The application is a single-user, offline-first desktop application with no network connectivity requirements.
- Users are expected to have audio files they already own or have legally acquired — there is no music store, download, or purchase functionality.
- The primary audio output device is stereo (2-channel); surround sound configurations are not a priority for initial release.
- Audio files are assumed to be properly tagged with standard metadata (ID3v2, Vorbis comments, etc.) where available.
- Library directories are added through settings/preferences rather than command-line arguments.
- The application window size defaults to a standard desktop size (~1200x800 pixels) and adapts down to a minimum viable size.
- The user is responsible for managing their audio files outside the application — file organization, renaming, and deletion happen in the filesystem and the library reflects these changes.
- Playback is expected to run for extended periods (hours) without degradation of audio quality or accumulation of audio drift.
