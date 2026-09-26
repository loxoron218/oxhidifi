# Feature Specification: Audio Signal Path Inspector

**Feature Branch**: `002-signal-path`

**Created**: 2026-09-26

**Status**: In Progress (Setup + Foundational + US1 + US2 implementation complete; US3–US4 pending)

**Input**: User description: "Recreate Roon's Signal Path feature seen on /home/arch/Projekte/oxhidifi/signal-path. Analyze the images thoroughly to describe the visible functionality, look and feel in detail."

## Clarifications

### Session 2026-09-26

- Q: When playback includes both an intentional enhancement and an output limitation at the same time, which overall verdict should the header show? → A: Limited/degraded wins — any constrained stage forces the overall verdict to limited, even if enhancements are also present.
- Q: What exact wording should the three overall quality verdicts use in the header and player badge? → A: Bit-Perfect / Processed / Limited — plain technical terms centered on whether audio data was altered.
- Q: Which transport and output wording should the path use on Linux instead of Windows-only terms like WASAPI Exclusive Mode? → A: Linux terms — ALSA direct/exclusive, ALSA shared/system mixer, USB output, plus network/streaming transport where applicable.
- Q: What secondary actions should the overflow three-dot menu in the path header offer? → A: Copy path summary, open output/device settings elsewhere, and show about info — all read-only, no DSP editing inside the view.
- Q: Should a volume-only change show the processing-speed readout, or only sample-rate, bit-depth, and effect conversions? → A: Show for any in-app change including volume — leveling, headroom, or DSP volume alone triggers the readout.
- Q: Should the signal path be shown exclusively as a third `Signal` tab next to `Albums` and `Artists`, with no overlay dialog or popover at any window size? → A: Tab-only everywhere — third ViewStack page named `signal`, no dialog/popover at any breakpoint.
- Q: How should selecting a stage show its plain-language explanation without using any popover or dialog? → A: Inline expandable row — tapping a stage expands 1–2 sentence explanation directly under that row inside the scrolling chain.
- Q: How should the player-area quality badge open the full path when no popover or dialog is allowed? → A: Badge button navigates — tapping verdict badge switches library to `Signal` tab, empty state when idle.
- Q: Does the no-popover rule still allow a standard overflow menu for the header kebab, or must those actions be inline buttons? → A: Allow `MenuButton` popover for kebab only — Copy summary, open output settings, About, no other popovers/dialogs.
- Q: Where should the overflow About action show its info when no dialog is allowed? → A: Navigate to existing app About/preferences view, no new dialog in Signal tab.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - View live signal path during playback (Priority: P1)

A listener playing a track opens the signal path tab from the player badge button or the library tab switcher to see exactly what happens to the audio from the source file to the speakers or headphones. The view shows a vertical chain of stages: source format, any conversions or effects, the transport to the playback device, and the final output, topped by an overall quality verdict.

**Why this priority**: This is the core value of the feature — trust and transparency about whether playback is bit-perfect or altered. Every other capability builds on this chain being visible and correct.

**Independent Test**: Can be fully tested by playing any track, opening the signal path view, and confirming the chain from source to output is shown with correct formats. Delivers standalone value as a playback transparency display.

**Acceptance Scenarios**:

1. **Given** a track is playing, **When** the user opens the signal path view, **Then** a vertical chain is shown starting with a Source stage (codec, sample rate, bit depth, channel count) and ending with an Output stage (destination name and output kind).
2. **Given** a track is playing bit-perfect with no processing, **When** the user views the signal path, **Then** the header verdict reads Bit-Perfect and every stage carries the Bit-Perfect quality indicator.
3. **Given** nothing is playing, **When** the user opens the signal path view, **Then** an empty state is shown explaining that no active path exists and how to start playback.

---

### User Story 2 - Understand at a glance whether sound is untouched or processed (Priority: P1)

A listener glances at a single quality badge in the player and in the signal path header to know whether the audio is untouched, intentionally enhanced by processing, or degraded by a limitation. The badge uses both text and a distinctive indicator light so it is recognizable without reading the full chain.

**Why this priority**: Equally critical to Story 1 — users must not have to parse technical details to answer "is my music being altered?". The verdict is the primary decision-making signal.

**Independent Test**: Can be fully tested by playing three tracks: one with no processing, one with volume adjustment or sample-rate conversion enabled, and one routed through a limited shared output. Each shows a distinct verdict. Delivers standalone value as a trust indicator.

**Acceptance Scenarios**:

1. **Given** playback with no altering stages, **When** the user looks at the player badge or path header, **Then** the verdict reads Bit-Perfect with its dedicated indicator.
2. **Given** playback with at least one intentional alteration (e.g. DSP-volume adjustment, sample-rate or bit-depth conversion), **When** the user looks at the verdict, **Then** the verdict reads Processed with its dedicated indicator.
3. **Given** playback constrained by the output (e.g. shared system mixer, forced downsampling), **When** the user looks at the verdict, **Then** the verdict reads Limited with its dedicated indicator.

---

### User Story 3 - Inspect and learn from any single stage (Priority: P2)

A curious listener taps any stage in the chain to learn what it means — for example what "Bit Depth Conversion 24bit to 64bit Float" or "ALSA Direct Output" actually does and why it affects (or preserves) quality. Each stage shows a concise title plus a detail line, and selecting it reveals a plain-language explanation.

**Why this priority**: Transforms the display from a static diagram into an educational tool, matching the observed "Click on any stage of the path to learn more" behavior. Important but secondary to showing the chain and verdict.

**Independent Test**: Can be fully tested by opening the path, selecting the Source stage and one processing stage, and confirming an explanation is shown for each. Delivers standalone value as in-app audio education.

**Acceptance Scenarios**:

1. **Given** the signal path is open, **When** the user selects any stage, **Then** that row expands inline to show a plain-language explanation of that stage type and its effect on sound quality without opening a popover or dialog.
2. **Given** a processing stage such as sample-rate conversion, **When** the user selects it, **Then** the explanation includes the input-to-output transformation (e.g. "96kHz to 192kHz") in understandable terms.
3. **Given** the signal path is open on a narrow screen, **When** the user selects a stage, **Then** the inline explanation remains fully readable without losing the surrounding chain context.

---

### User Story 4 - Identify the playback device and check processing headroom (Priority: P3)

A listener with an external DAC or network streamer confirms at the bottom of the path which device is actually rendering audio, with a recognizable brand/device card and a link to its manual, and — when processing is active — sees a processing-speed indicator confirming the device has enough headroom.

**Why this priority**: High value for owners of external hardware (the reference images show Mytek, Bluesound, dCS, exaSound, OPPO, Devialet, HQPlayer, Sonore, Meridian cards), but not required for the basic path to be useful.

**Independent Test**: Can be fully tested by playing to a named output device and confirming the footer card shows the device name (MVP: manual link hidden — `manual_url` always `None`), and that a processing-speed readout appears whenever any in-app conversion or DSP-volume adjustment is active. Delivers standalone value as device confirmation.

**Acceptance Scenarios**:

1. **Given** playback to a known device, **When** the user scrolls to the bottom of the path, **Then** a device card shows the device display name (MVP: the `View Product Manual` link is hidden — `manual_url` is always `None` until the optional `device_manuals` user-config map exists).
2. **Given** playback with any active in-app conversion, effect, or volume adjustment (including volume-only), **When** the user views the path header area, **Then** a processing-speed readout (multiple of real time, e.g. "Processing speed: 35.2x") is shown.
3. **Given** playback with no processing, **When** the user views the path, **Then** no processing-speed readout is shown (it is hidden rather than showing a meaningless value).

---

### Edge Cases

- What happens when playback transitions gaplessly from one track to the next with a different format (e.g. 44.1kHz/16bit to 96kHz/24bit)? The path MUST refresh atomically to the new track's chain without showing a mixed or stale chain.
- How does the view handle a very long chain (8+ stages, e.g. Source + Decoder + DSD-to-PCM + BitDepth + SampleRate + Volume + Transport + Output + ExternalRenderer)? The chain MUST remain scrollable with the header verdict and device card reachable, without truncating middle stages.
- What happens when source metadata is incomplete (unknown codec, sample rate, or channel count)? The Source stage MUST show available info and label unknown fields as unknown rather than guessing.
- What happens when the output device disappears mid-playback (unplugged/disconnected)? The Output stage MUST reflect the lost device and the verdict MUST NOT claim Bit-Perfect playback to a missing destination.
- What happens when playback is paused or stopped while the view is open? The last-known path stays visible with a clear "paused/stopped" indication rather than vanishing, and resumes live updates on play.
- How does the view handle a direct-stream format that cannot be mixed (e.g. DSD) combined with volume processing? The path MUST show the required conversion stage (e.g. DSD-to-PCM) explicitly instead of silently hiding it.
- What happens when the same device appears twice in the chain (e.g. streamer plus separate DAC, as in the exaSound PlayPoint + e12 example)? Deferred past MVP: the MVP shows a single footer card for the active output device (no second-device input facts exist); the forward rule is that each rendering device MUST appear as a separate footer card in chain order once multi-device inputs land (contract tests cover the ordering with injected `RenderingDevice` fixtures only).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST display an always-available signal-path entry as a verdict badge button in the player area that switches the library to the `Signal` tab when activated (no popover or dialog). During playback it also conveys the current overall quality verdict without leaving the player; when nothing is playing the badge remains available and navigates to the empty state with start-playback guidance.
- **FR-002**: System MUST display the full signal path as a vertical stage chain ordered from audio source at the top to final output at the bottom, connected by a continuous rail.
- **FR-003**: System MUST show a header with the overall quality verdict using canonical labels Bit-Perfect, Processed, or Limited, the playing output/zone name, and the hint text with the exact literal `Click on any stage of the path to learn more`. When stages of mixed quality exist, the overall verdict MUST resolve by precedence Limited > Processed > Bit-Perfect (any limited stage forces Limited).
- **FR-004**: System MUST render a Source stage showing the origin (local file, network, or streaming provider equivalent to TIDAL/Qobuz labels in references) plus codec/container (e.g. FLAC, ALAC, DSF, MQA), sample rate, bit depth, and channel count.
- **FR-005**: System MUST render one stage per active audio transformation in execution order, including at minimum: authentication/verification only when provider auth facts are present for the playing source (otherwise omitted; MVP: `auth` is always `None`, so the stage is omitted), decoding (Decoder stage emitted iff live decoder facts are present — `decoder_params` is `Some`, otherwise omitted), bit-depth conversion (with input-to-output wording), sample-rate conversion (with input-to-output wording), volume adjustment for the single DSP volume control when scaling is active (`volume < 1.0` or muted, with decibel value via `format_volume_db`), format conversions (e.g. DSD-to-PCM with target rate), and transport/delivery steps (device name plus Linux mode such as ALSA direct/exclusive, ALSA shared/system mixer, or USB output). Channel boundary: resampler channel-count conversion (source channels vs device channels, emitted as a `FormatConverter` "Channel Conversion" stage) is in-scope builder emission; channel-map `Effect` vocabulary is `describe()`-only in MVP (never emitted by the builder). MVP scope: leveling/headroom volume variants and equalizer/crossover/crossfeed/channel-mapping effect stages are omitted (no engine source exists; the `StageFacts` effect vocabulary is reserved and covered at the pure-`describe()` level only); network/streaming transport wording is omitted in MVP (no streaming-provider input).
- **FR-006**: System MUST render an Output stage identifying the final destination kind (e.g. analog output, speakers, headphones, USB output, device outputs) and a terminal device step when audio hands off beyond the application (equivalent to "Signal Leaves Roon" / external renderer step). MVP: filter/modulator details are always `None` (no output/device source fields exist), so the external-renderer step is title-only.
- **FR-007**: System MUST assign each stage a per-stage quality indicator (Bit-Perfect / Processed / Limited) using both color/shape and a non-color cue so verdicts remain distinguishable without color vision.
- **FR-008**: System MUST make every stage selectable to reveal a plain-language explanation inline by expanding the selected row directly under its title/detail lines (no popover, dialog, or navigation away from the chain) describing what that stage does and how it affects sound quality, including input-to-output values where applicable. Each explanation is 1–2 plain sentences plus applicable values, never empty; expanding one row MUST NOT collapse or hide surrounding stages.
- **FR-009**: System MUST show a device footer card for the active rendering device with the device display name, a generic symbolic brand/device visual with fallback icon when the brand is unknown (never per-brand artwork), and a manual link ("View Product Manual") only when `manual_url` is `Some` from explicit optional user config/catalog (never hardcoded or bundled); no card may name a non-rendering device. MVP: exactly one card for the active output device (`device_id`/`device_name`); `manual_url` is always `None` (no device-manual config key exists yet), so the link is hidden; streamer+DAC multi-card chains are deferred (no second-device input facts exist in MVP).
- **FR-010**: System MUST show a processing-speed readout formatted as `Processing speed: {x.x}x` (one decimal, header area below the verdict/zone line) whenever any in-app conversion or DSP-volume adjustment is active (MVP: the single DSP volume control, including volume-only paths), and MUST hide it when playback is bit-perfect and when the path is Limited-only with no in-app alteration. The value is an informational snapshot-sampled estimate (not a measured device guarantee); contracts assert presence (`Some`/`None`) only, never the number.
- **FR-011**: System MUST update the chain live and atomically on track change, format change, setting change (e.g. adjusting DSP volume or toggling output mode), output-device change, and playback-status change (play/pause/stop) during playback, without requiring the view to be reopened. Trigger mapping (no dedicated device/format events exist): track/format changes arrive as `TrackStarted` with new resampler/device facts re-read on every rebuild; setting changes as `VolumeChanged`/`OutputModeChanged`; device changes as `DeviceLost`/`OutputModeChanged`/`TrackStarted` (a manual device reselection that emits none of these, and mute toggles — `apply_muted` emits no event — are out-of-scope follow-ups, not covered here); status changes as `Paused`/`Resumed`/`Stopped`.
- **FR-012**: System MUST present the path as a persistent third library tab named `Signal` next to `Albums` and `Artists` (`ViewStack` page `signal`) at all window sizes with no overlay dialog, popover, or modal sheet; narrow layout adapts via the existing `ViewSwitcher` / `ViewSwitcherBar` pattern with the chain scrolling inside the tab content.
- **FR-013**: System MUST support both dark and light appearances: dark tab page uses near-black background with light primary text and theme-link-colored detail links (theme accent role, never a hardcoded hex); light tab page uses light background with dark primary text and the same theme link role; the vertical rail, circular badge icons (generic symbolic icons only), and per-stage indicators remain legible in both via theme-aware styling (no hardcoded colors or radii). Acceptance: side-by-side dark/light review with all indicators distinguishable by icon/shape plus text label without color vision (never color alone).
- **FR-014**: System MUST provide an overflow menu in the path header implemented as a standard `MenuButton` popover (sole allowed popover exception to the tab-only rule) with exactly three read-only secondary actions — Copy path summary (confirms via toast, no dialog), open output/device settings elsewhere (presents the existing `show_preferences_dialog` PreferencesDialog with the audio page (`src/ui/preferences/audio.rs:build_audio_page`) selected via page-selection support — no inline DSP editing), and show app info by presenting the existing PreferencesDialog default view (`src/ui/preferences.rs` — default landing page with no page selection, which serves as the About-equivalent info landing (MVP stand-in; tracked follow-up: dedicated About page); no dedicated About view exists, no new dialog is built in the `Signal` tab; a dedicated About page is out-of-scope follow-up) — without cluttering the chain itself and without DSP editing inside the view. Acceptance per action is dialog presentation on the correct landing page (settings lands on the audio page, About-equivalent lands on the default view, which counts as the info landing in MVP). No other popovers, dialogs, or sheets are allowed in this feature.
- **FR-015**: System MUST expose the current verdict, stage list, and processing-speed value in a form assistive technologies can read (each stage announced as title plus detail; verdict and processing-speed readout announced as text), and MUST keep all stages operable by keyboard.

### Key Entities

- **Signal Path**: The complete live chain for the currently playing track; attributes include overall verdict (Bit-Perfect / Processed / Limited), output zone name, processing speed (optional), ordered stage list, and referenced rendering devices.
- **Path Stage**: One node in the chain; attributes include position in order, stage kind (source, authentication, decoder, converter, volume, effect, transport, output, external renderer), title, detail line (formats, rates, decibel values, modes), per-stage quality indicator, badge icon, and plain-language explanation.
- **Quality Verdict**: The classification of the whole path and of each stage; values are Bit-Perfect (bit-perfect, unaltered), Processed (intentionally altered/enhanced), or Limited (high quality but constrained); includes indicator styling plus text label. The whole-path verdict resolves by precedence Limited > Processed > Bit-Perfect.
- **Rendering Device**: The hardware or external software endpoint producing sound; attributes include display name, brand visual, device illustration, manual link, and role in chain (transport target, output, external renderer).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Listeners can open the signal path from the player and identify whether playback is untouched or altered in under 10 seconds on first use without guidance (manual validation via quickstart scenario 1; no automated timing gate).
- **SC-002**: 9 out of 10 test listeners correctly classify three playback situations (untouched, with DSP-volume/resample processing, through a limited shared output) using only the verdict badge and header (human classification validated manually via quickstart scenarios 1–3; CI asserts the verdict-precedence proxy in `tests/signal_inspector.rs`).
- **SC-003**: Every altering stage active during playback appears in the displayed chain with its input-to-output detail — zero silent alterations in acceptance testing across bit-depth, sample-rate, DSP-volume, and output-mode changes.
- **SC-004**: Selecting any visible stage reveals its plain-language explanation within 1 second, and 90% of first-time users can paraphrase what the selected stage does after reading it (rendering asserted by GTK interaction test; paraphrase comprehension validated manually — no wall-clock CI gate on comprehension).
- **SC-005**: The device footer correctly names the active rendering device and never names a device that is not rendering audio (manual validation via quickstart scenario 5). MVP: the manual link is hidden (`manual_url` always `None` — no device-manual config exists); the 95%-link target applies once the optional `device_manuals` user-config map lands (contract tests inject `RenderingDevice` fixtures directly).
- **SC-006**: During gapless transitions between tracks of differing formats, the displayed chain switches to the new track's correct chain with no mixed-format display observed in test transitions.

## Assumptions

- Target is a desktop music player whose core promise is bit-perfect, gapless playback; listeners care deeply about whether any stage alters the audio.
- Source material spans local library files (FLAC, ALAC, DSF/DSD and similar) and may include streamed content labeled by provider; the Source stage labels origin accordingly.
- Playback pipeline concepts visible to users are: source file/stream, decoder, bit-depth and sample-rate conversion, volume handling, delivery transport with Linux modes (ALSA direct/exclusive or ALSA shared/system mixer, USB output), output device, and optional hand-off to an external renderer (post-MVP vocabulary — EQ/crossfeed/channel effects, leveling/headroom variants, and network/streaming transport are `describe()`-only or deferred per FR-005 MVP scope).
- Overall verdict uses three canonical states: Bit-Perfect (untouched/lossless), Processed (intentionally processed/enhanced), and Limited (high-quality-but-degraded, e.g. shared mixer or forced downsampling).
- Visual language follows references adapted to a full-width tab page (no overlay card): vertical icon rail with circular codec/device badges on the left and two-line text (bold title + blue detail) on the right, thin connecting rail with dots, glowing per-stage indicators, brand tile plus outline illustration plus manual link in the footer, overflow (three-dot) menu in the header, scrollable middle section for long chains.
- Out of scope for this feature: editing DSP settings from within the path view (it is inspect-and-learn only), room-correction wizards, historical path logs, and comparison of two paths side by side. MVP also excludes: leveling/headroom/EQ engine features (no engine source — display vocabulary with `describe()` coverage only), streaming-provider auth inputs (`auth` always `None`, authentication stage omitted), device-manual config (`manual_url` always `None`, link hidden), multi-device chains (single active output card), external-renderer filter/modulator details (title-only), network/streaming transport wording, and a dedicated About page.
- Accessibility baseline: all verdict information is conveyed by text plus icon shape, not color alone; full keyboard operation and screen-reader announcements are expected.
