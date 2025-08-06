use std::{cell::RefCell, rc::Rc};

use gtk4::{Button, FlowBox, Label, Stack};
use libadwaita::ViewStack;

use crate::ui::grids::artist_grid_builder::build_artist_grid;

/// Rebuilds the artists grid in the main window.
///
/// This function is responsible for removing the existing "artists" child from the
/// main `ViewStack`, resetting the `artist_grid_cell` and `artists_stack_cell`,
/// building a new artists grid and its containing stack, and then adding it back
/// to the `ViewStack`. This ensures the artists view is always up-to-date and
/// correctly displayed.
///
/// # Arguments
///
/// * `stack` - The main `ViewStack` of the application where the artists grid will be displayed.
/// * `scanning_label_artists` - The `gtk4::Label` used to indicate scanning progress for artists.
/// * `artist_grid_cell` - A `Rc<RefCell<Option<FlowBox>>>` holding a reference to the `FlowBox`
///   that displays artist tiles. This is updated with the newly built grid.
/// * `artists_stack_cell` - A `Rc<RefCell<Option<Stack>>>` holding a reference to the `Stack`
///   that contains the artists grid and its various states (loading, empty, populated).
/// * `add_music_button` - A `gtk4::Button` (currently unused) that could be used for
///   adding music functionality directly from the artists view.
pub fn rebuild_artist_grid_for_window(
    stack: &ViewStack,
    scanning_label_artists: &Label,
    artist_grid_cell: &Rc<RefCell<Option<FlowBox>>>,
    artists_stack_cell: &Rc<RefCell<Option<Stack>>>,
    add_music_button: &Button,
) {
    // Always remove existing "artists" child before adding a new one to prevent duplicates
    // and ensure a fresh build.
    if let Some(child) = stack.child_by_name("artists") {
        stack.remove(&child);
    }
    // Clear the existing references to the artists grid and stack.
    *artist_grid_cell.borrow_mut() = None;
    *artists_stack_cell.borrow_mut() = None;

    // Build the new artists grid and its containing stack.
    let (artists_stack, artist_grid) =
        build_artist_grid(scanning_label_artists, add_music_button);

    // Add the newly built artists stack to the main ViewStack.
    stack.add_titled(&artists_stack, Some("artists"), "Artists");

    // Update the references to the new artists grid and stack in the `Rc<RefCell>`s.
    *artist_grid_cell.borrow_mut() = Some(artist_grid.clone());
    *artists_stack_cell.borrow_mut() = Some(artists_stack.clone());
}
