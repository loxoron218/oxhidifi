use std::{cell::RefCell, rc::Rc};

use gtk4::{Button, FlowBox, Label, Stack};
use libadwaita::ViewStack;

use crate::{ui::grids::album_grid_builder::build_albums_grid, utils::screen::ScreenInfo};

/// Rebuilds the albums grid in the main window.
///
/// This function is responsible for clearing the existing albums grid,
/// re-initializing the `FlowBox` and `Stack` for albums, and adding them
/// back to the main `ViewStack`. It's called when the grid needs to be
/// completely refreshed, for instance, after a major library update.
///
/// # Arguments
/// * `stack` - The main `libadwaita::ViewStack` where the albums grid is displayed.
/// * `scanning_label_albums` - A `gtk4::Label` used to show scanning feedback.
/// * `screen_info` - A `Rc<RefCell<ScreenInfo>>` providing screen dimension details.
/// * `albums_grid_cell` - A `Rc<RefCell<Option<FlowBox>>>` holding a reference to the albums `FlowBox`.
/// * `albums_stack_cell` - A `Rc<RefCell<Option<Stack>>>` holding a reference to the albums `Stack`.
/// * `add_music_button` - A `gtk4::Button` used in the empty state to add music.
pub fn rebuild_albums_grid_for_window(
    stack: &ViewStack,
    scanning_label_albums: &Label,
    screen_info: &Rc<RefCell<ScreenInfo>>,
    albums_grid_cell: &Rc<RefCell<Option<FlowBox>>>,
    albums_stack_cell: &Rc<RefCell<Option<Stack>>>,
    add_music_button: &Button,
) {
    // Remove old grid widget from the stack if it exists to prevent duplicates.
    if let Some(child) = stack.child_by_name("albums") {
        stack.remove(&child);
    }

    // Clear the Rc<RefCell>s to drop previous instances of FlowBox and Stack,
    // ensuring a clean rebuild and releasing associated resources.
    *albums_grid_cell.borrow_mut() = None;
    *albums_stack_cell.borrow_mut() = None;

    // Build a new albums grid and its containing stack.
    let (albums_stack, albums_grid) = build_albums_grid(
        scanning_label_albums,
        screen_info.borrow().cover_size, // Pass cover_size from screen_info
        screen_info.borrow().tile_size,  // Pass tile_size from screen_info
        add_music_button,
    );

    // Add the newly created albums stack to the main ViewStack.
    stack.add_titled(&albums_stack, Some("albums"), "Albums");

    // Store references to the new FlowBox and Stack in the cells for later access.
    *albums_grid_cell.borrow_mut() = Some(albums_grid.clone());
    *albums_stack_cell.borrow_mut() = Some(albums_stack.clone());
}
