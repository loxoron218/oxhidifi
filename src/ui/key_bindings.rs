//! Application-wide keyboard shortcuts for the main window.
//!
//! Escape hides the sidebar when shown; `Ctrl+B` toggles it; `Ctrl+`/`Ctrl-`
//! (including the numpad) zoom the active view; plain `Space` toggles
//! play/pause while the player panel is shown; `Ctrl+Tab` cycles library tabs
//! forward and `Ctrl+Shift+Tab` cycles back. Each submodule owns one
//! handler family plus its tests; the window wires them through the sibling
//! [`key_controllers`](crate::ui::key_controllers) module.

pub mod play_pause_key;
pub mod sidebar_keys;
pub mod tab_switch_key;
pub mod text_entry;
pub mod zoom_key;
