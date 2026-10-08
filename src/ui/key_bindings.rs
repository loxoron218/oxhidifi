//! Application-wide keyboard shortcuts for the main window.
//!
//! Escape hides the sidebar when shown; `Ctrl+B` toggles the sidebar; `Ctrl+`/`Ctrl-` (including
//! the numpad) zoom the active view; `Ctrl+G` toggles grid/column; plain `Space`
//! toggles play/pause while the player panel is shown; `Ctrl+Tab` cycles
//! library tabs forward and `Ctrl+Shift+Tab` cycles back. `Ctrl+?` opens the
//! keyboard shortcuts dialog (see the sibling [`shortcuts`](crate::ui::shortcuts)
//! module). Zoom and view-toggle are ignored on the `Signal` tab and while a
//! detail page is pushed. Each submodule owns one handler family plus its
//! tests; the window wires them through the sibling
//! [`key_controllers`](crate::ui::key_controllers) module.

pub mod play_pause_key;
pub mod sidebar_keys;
pub mod tab_switch_key;
pub mod text_entry;
pub mod view_toggle_key;
pub mod zoom_key;
