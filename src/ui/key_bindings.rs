//! Application-wide keyboard shortcuts for the main window.
//!
//! Escape hides the sidebar when shown; `Ctrl+B` toggles the sidebar; `Ctrl+`/`Ctrl-` (including
//! the numpad) zoom the active view; `Ctrl+G` toggles grid/column; plain `Space`
//! toggles the open panel when it has an active track, else starts the
//! visible album/artist detail page; `Ctrl+Tab` cycles
//! library tabs forward and `Ctrl+Shift+Tab` cycles back. `Ctrl+?` opens the
//! keyboard shortcuts dialog and `Ctrl+,` opens the preferences dialog (see
//! the siblings [`shortcuts`](crate::ui::shortcuts) and
//! [`preferences`](crate::ui::preferences) modules). Zoom and view-toggle are
//! ignored on the `Signal` tab and while a
//! detail page is pushed. Each submodule owns one handler family plus its
//! tests; the window wires them through the sibling
//! [`key_controllers`](crate::ui::key_controllers) module.

pub mod play_pause_key;
pub mod preferences_key;
pub mod sidebar_keys;
pub mod tab_switch_key;
pub mod text_entry;
pub mod view_toggle_key;
pub mod zoom_key;
