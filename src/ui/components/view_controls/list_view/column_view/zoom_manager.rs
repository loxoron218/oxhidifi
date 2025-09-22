use std::{
    cell::{Cell, RefCell},
    fmt::{Debug, Formatter, Result},
    rc::Rc,
};

use super::zoom::ColumnViewZoomLevel;

/// Manages the zoom level state for ColumnView and provides methods to change zoom levels.
///
/// The `ColumnViewZoomManager` is responsible for maintaining the current zoom level
/// and notifying registered callbacks when the zoom level changes. It provides
/// methods for adjusting the zoom level (zooming in/out, resetting) and
/// retrieving size information associated with the current zoom level.
///
/// This manager operates independently from the grid view zoom manager to ensure
/// zoom changes in one view don't affect the other.
///
/// # Examples
///
/// ```
/// use your_crate::ui::components::view_controls::list_view::column_view::{ColumnViewZoomLevel, ColumnViewZoomManager};
///
/// let zoom_manager = ColumnViewZoomManager::new(ColumnViewZoomLevel::Normal);
/// zoom_manager.zoom_in();
/// assert_eq!(zoom_manager.current_zoom_level(), ColumnViewZoomLevel::Expanded);
/// ```
#[derive(Clone)]
pub struct ColumnViewZoomManager {
    /// The current zoom level, wrapped in `Rc<Cell<T>>` for shared mutable access.
    ///
    /// Using `Rc<Cell<T>>` allows multiple parts of the application to share
    /// and modify the zoom level without requiring mutable references.
    current_zoom_level: Rc<Cell<ColumnViewZoomLevel>>,

    /// Callback function to notify when zoom level changes, wrapped in `Rc<RefCell<T>>`.
    ///
    /// This allows registering a callback that will be executed whenever the
    /// zoom level is changed. The `RefCell` provides interior mutability for
    /// updating the callback, while `Rc` allows sharing the callback across
    /// multiple instances of `ColumnViewZoomManager` (after cloning).
    on_zoom_changed: Rc<RefCell<Option<Box<dyn Fn(ColumnViewZoomLevel)>>>>,
}

impl Debug for ColumnViewZoomManager {
    /// Formats the `ColumnViewZoomManager` for debugging purposes.
    ///
    /// This implementation avoids printing the actual callback function,
    /// instead showing a placeholder string to prevent potential issues
    /// with printing function pointers or closures.
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.debug_struct("ColumnViewZoomManager")
            .field("current_zoom_level", &self.current_zoom_level)
            .field("on_zoom_changed", &"CallbackFunction")
            .finish()
    }
}

impl ColumnViewZoomManager {
    /// Creates a new `ColumnViewZoomManager` with the specified initial zoom level.
    ///
    /// Initializes the zoom manager with the provided zoom level and sets
    /// up the callback storage with an empty value.
    ///
    /// # Arguments
    ///
    /// * `initial_zoom_level` - The initial zoom level to set for the manager.
    ///
    /// # Returns
    ///
    /// A new `ColumnViewZoomManager` instance with the specified initial zoom level.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::ui::components::view_controls::list_view::column_view::{ColumnViewZoomLevel, ColumnViewZoomManager};
    ///
    /// let zoom_manager = ColumnViewZoomManager::new(ColumnViewZoomLevel::Compact);
    /// assert_eq!(zoom_manager.current_zoom_level(), ColumnViewZoomLevel::Compact);
    /// ```
    pub fn new(initial_zoom_level: ColumnViewZoomLevel) -> Self {
        Self {
            current_zoom_level: Rc::new(Cell::new(initial_zoom_level)),
            on_zoom_changed: Rc::new(RefCell::new(None)),
        }
    }

    /// Sets the current zoom level and notifies the registered callback.
    ///
    /// Updates the internal zoom level and executes the registered callback
    /// function (if any) with the new zoom level as an argument.
    ///
    /// # Arguments
    ///
    /// * `zoom_level` - The new zoom level to set.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::ui::components::view_controls::list_view::column_view::{ColumnViewZoomLevel, ColumnViewZoomManager};
    ///
    /// let zoom_manager = ColumnViewZoomManager::new(ColumnViewZoomLevel::Normal);
    /// zoom_manager.set_zoom_level(ColumnViewZoomLevel::Expanded);
    /// assert_eq!(zoom_manager.current_zoom_level(), ColumnViewZoomLevel::Expanded);
    /// ```
    pub fn set_zoom_level(&self, zoom_level: ColumnViewZoomLevel) {
        self.current_zoom_level.set(zoom_level);
        if let Some(ref callback) = *self.on_zoom_changed.borrow() {
            callback(zoom_level);
        }
    }

    /// Zooms in to the next zoom level.
    ///
    /// Increases the zoom level to the next available level according to the
    /// `ColumnViewZoomLevel::next()` method. If already at the maximum zoom level,
    /// the zoom level remains unchanged.
    ///
    /// This method internally calls `set_zoom_level` to update the zoom level
    /// and notify any registered callbacks.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::ui::components::view_controls::list_view::column_view::{ColumnViewZoomLevel, ColumnViewZoomManager};
    ///
    /// let zoom_manager = ColumnViewZoomManager::new(ColumnViewZoomLevel::Normal);
    /// zoom_manager.zoom_in();
    /// assert_eq!(zoom_manager.current_zoom_level(), ColumnViewZoomLevel::Expanded);
    /// ```
    pub fn zoom_in(&self) {
        let current = self.current_zoom_level.get();
        let next = current.next();

        // Only update zoom level if it will actually change
        if next != current {
            self.set_zoom_level(next);
        }
    }

    /// Zooms out to the previous zoom level.
    ///
    /// Decreases the zoom level to the previous available level according to the
    /// `ColumnViewZoomLevel::previous()` method. If already at the minimum zoom level,
    /// the zoom level remains unchanged.
    ///
    /// This method internally calls `set_zoom_level` to update the zoom level
    /// and notify any registered callbacks.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::ui::components::view_controls::list_view::column_view::{ColumnViewZoomLevel, ColumnViewZoomManager};
    ///
    /// let zoom_manager = ColumnViewZoomManager::new(ColumnViewZoomLevel::Expanded);
    /// zoom_manager.zoom_out();
    /// assert_eq!(zoom_manager.current_zoom_level(), ColumnViewZoomLevel::Normal);
    /// ```
    pub fn zoom_out(&self) {
        let current = self.current_zoom_level.get();
        let previous = current.previous();

        // Only update zoom level if it will actually change
        if previous != current {
            self.set_zoom_level(previous);
        }
    }

    /// Resets zoom to the default level.
    ///
    /// Sets the zoom level back to the default value (`ColumnViewZoomLevel::Normal`).
    /// This method internally calls `set_zoom_level` to update the zoom level
    /// and notify any registered callbacks.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::ui::components::view_controls::list_view::column_view::{ColumnViewZoomLevel, ColumnViewZoomManager};
    ///
    /// let zoom_manager = ColumnViewZoomManager::new(ColumnViewZoomLevel::Expanded);
    /// zoom_manager.reset_zoom();
    /// assert_eq!(zoom_manager.current_zoom_level(), ColumnViewZoomLevel::Normal);
    /// ```
    pub fn reset_zoom(&self) {
        self.set_zoom_level(ColumnViewZoomLevel::default());
    }

    /// Returns the current zoom level.
    ///
    /// # Returns
    ///
    /// The current `ColumnViewZoomLevel`.
    pub fn current_zoom_level(&self) -> ColumnViewZoomLevel {
        self.current_zoom_level.get()
    }

    /// Connects a callback function that will be called when the zoom level changes.
    ///
    /// Registers a callback function that will be executed whenever the zoom level
    /// is changed through any of the zoom adjustment methods. Only one callback
    /// can be registered at a time; registering a new callback replaces the previous one.
    ///
    /// # Arguments
    ///
    /// * `callback` - A function that takes a `ColumnViewZoomLevel` parameter and will be
    ///   called whenever the zoom level changes.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::ui::components::view_controls::list_view::column_view::{ColumnViewZoomLevel, ColumnViewZoomManager};
    ///
    /// let zoom_manager = ColumnViewZoomManager::new(ColumnViewZoomLevel::Normal);
    /// zoom_manager.connect_zoom_changed(|zoom_level| {
    ///     println!("ColumnView zoom level changed to {:?}", zoom_level);
    /// });
    /// zoom_manager.zoom_in(); // This will trigger the callback
    /// ```
    pub fn connect_zoom_changed<F>(&self, callback: F)
    where
        F: Fn(ColumnViewZoomLevel) + 'static,
    {
        *self.on_zoom_changed.borrow_mut() = Some(Box::new(callback));
    }
}
