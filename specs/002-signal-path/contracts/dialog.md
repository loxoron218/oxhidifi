# Contract: Signal Tab UI Behavior (`002-signal-path`)

> Note: this file is historically named `dialog.md`; it specifies the tab-only
> view — no dialog is built (sole exception: the standard `MenuButton` kebab
> popover per FR-014).

**Scope**: `src/ui/player/signal_badge.rs` + `src/ui/signal_view/signal_tab.rs` (+ `signal_footer.rs`, `signal_poll.rs`).
Read-only view — zero DSP editing. FR references are normative. Tab-only: no overlay dialog/popover/sheet except the standard `MenuButton` kebab popover.

## Entry badge (FR-001)

- Always available in the player area; during playback shows the current
  verdict text (`Bit-Perfect` / `Processed` / `Limited`) plus its dedicated
  indicator icon. Activating it switches the library `ViewStack` to the
  `Signal` tab (no dialog/popover); when nothing is playing it navigates to
  the tab's empty state with start-playback guidance.

## Tab chrome (FR-002, FR-003, FR-012, FR-013, FR-014)

- Persistent third library tab named `Signal` (`ViewStack` page `signal`)
  next to Albums/Artists at all window sizes; narrow layout adapts via the
  existing `ViewSwitcher` / `ViewSwitcherBar` pattern with the chain scrolling
  inside the tab content. No overlay dialog, popover, or modal sheet.
- Header: whole-path verdict label + indicator, output/zone name, hint text with the exact literal `Click on any stage of the path to learn more`, and an
   overflow (three-dot) `MenuButton` (sole allowed popover) with exactly three
     actions: **Copy path summary** (copies `summarize_text`, confirms with a
     `Toast`, no dialog), **open output/device settings** (presents the existing
     `show_preferences_dialog` PreferencesDialog with the audio page selected
     via page-selection support per `src/ui/preferences/audio.rs:build_audio_page` — no inline editing),
      **About** (presents the existing PreferencesDialog default view per
      `src/ui/preferences.rs` — default landing page with no page selection,
      which serves as the info landing in MVP; no dedicated About view exists
      and no new dialog is built in the tab; dedicated About page is out-of-scope
      follow-up). Acceptance per action is dialog presentation on the correct landing page
      (audio page for settings, default view for the About-equivalent info landing).
- Chain: vertical `ListBox` in a `ScrolledWindow` inside the tab, source at top → output at
  bottom, connected by a continuous rail; circular badge icons left,
   two-line title (bold) + detail (theme-accent link-styled, never a hardcoded color) right; scrollable for
  8+ stages with header and device footer reachable.
- Dark and light appearances: near-black tab page/light text vs. light tab page/dark
  text, theme-link-colored detail links (theme accent role, never a hardcoded hex),
  legible rail/badges/indicators in both (generic symbolic icons, theme-aware styling,
  no hardcoded colors or radii).

## Stages (FR-004, FR-005, FR-006, FR-007, FR-008)

- One row per snapshot stage in `position` order: Source (origin + codec +
  rate + depth + channels), each active transformation (authentication omitted in
  MVP — `auth` always `None`; decoding, bit-depth/sample-rate/format conversions with
  input→output wording, DSP-volume rows with dB, transport with MVP Linux mode wording:
  ALSA direct-exclusive, shared-mixer, or USB — network/streaming deferred), Output (destination kind) and terminal
  external-renderer step when audio hands off beyond the app (title-only in MVP).
- Per-stage verdict indicator = icon/shape + text label (never color alone).
- Every row is selectable and expands inline directly under its title/detail
  lines to reveal its plain-language explanation (1–2
  sentences + input→output values where applicable, never empty)
  within 1 s, without losing chain context
  on narrow screens. No popover, dialog, or navigation away from the chain;
  expanding one row MUST NOT hide surrounding stages.

## Footer (FR-009)

- MVP: exactly one device card for the active output device (`device_id` /
  `device_name` from `AudioOutput`; multi-card chain order is a deferred
  forward rule): display name (CPAL `DeviceInfo`, fallback to device id),
  generic symbolic brand/device visual with fallback icon when the brand is
  unknown (never per-brand artwork), `View Product Manual` link iff
  `manual_url.is_some()` (MVP: always `None` — no device-manual config key exists
  yet, so the link is hidden; tests inject `RenderingDevice` fixtures directly;
  future optional `device_manuals` user-config map may populate it; never
  hardcoded or bundled); never names a
  non-rendering device.

## Readout (FR-010)

- `Processing speed: {x.x}x` line (one decimal, header area below the
  verdict/zone line) iff `processing_speed.is_some()` (any in-app alteration
  incl. volume-only); fully hidden when `None` (bit-perfect and Limited-only
  without in-app alteration).

## Liveness (FR-011 + edge cases)

- Whole-snapshot swap on `generation` change (track/format/setting/device/
  status; the builder bumps `generation` on `PlaybackStatus` change).
  Trigger mapping (no dedicated device/format events exist): track/format
  changes arrive as `TrackStarted` with new resampler/device facts re-read on
  every rebuild; setting changes as `VolumeChanged`/`OutputModeChanged`;
  device changes as `DeviceLost`/`OutputModeChanged`/`TrackStarted`; status
  changes as `Paused`/`Resumed`/`Stopped` (`PositionTick`, `QueueChanged`,
  `Seeked`, `GaplessEnabledChanged`, and `Error` never rebuild);
  gapless transitions never show mixed chains; paused/stopped retains
  `track_id` plus the last path with a status ribbon; empty state iff
  `track_id.is_none()` explains how to start playback; lost device is
  reflected in the Output row and forces the verdict off Bit-Perfect.

## Accessibility (FR-015)

- Each row's accessible name is `title + detail`; all rows keyboard-activatable;
  overflow actions keyboard-reachable; verdict and processing-speed readout
  announced as text (`Processing speed {x.x} times real time`).
