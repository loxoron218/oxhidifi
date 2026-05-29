//! `HeaderBar` with Albums/Artists tab buttons and view toggle controls.
//!
//! Uses `AdwViewSwitcher` for tab navigation per GNOME HIG. The switcher
//! is placed in the title widget slot of `AdwHeaderBar`.
//!
//! This module is intentionally minimal — the header bar construction and
//! switcher wiring live in `window.rs` where the `gtk::Stack` is available.
