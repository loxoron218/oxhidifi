//! Column view types and configuration.

use std::{
    cell::{Ref, RefCell, RefMut},
    collections::HashMap,
    rc::Rc,
};

use libadwaita::glib::JoinHandle;

/// Type of items to display in the column view.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ColumnListViewType {
    /// Display albums in column view
    #[default]
    Albums,
    /// Display artists in column view
    Artists,
}

/// Configuration options for the column view.
#[derive(Debug, Clone)]
pub struct ColumnListViewConfig {
    /// Type of items to display (albums or artists).
    pub view_type: ColumnListViewType,
    /// Whether to show DR badges on album covers.
    pub show_dr_badges: bool,
    /// Whether to use compact layout.
    pub compact: bool,
}

impl Default for ColumnListViewConfig {
    fn default() -> Self {
        Self {
            view_type: ColumnListViewType::default(),
            show_dr_badges: true,
            compact: false,
        }
    }
}

/// Handles for subscription tasks used for cleanup.
#[derive(Debug, Default)]
pub struct SubscriptionHandles {
    /// Handle for zoom change subscription.
    pub zoom_handle: Option<JoinHandle<()>>,
    /// Handle for settings change subscription.
    pub settings_handle: Option<JoinHandle<()>>,
    /// Handle for playback state subscription.
    pub playback_handle: Option<JoinHandle<()>>,
}

/// Cache for artist names keyed by artist ID.
#[derive(Debug, Default, Clone)]
pub struct ArtistNameCache {
    /// Inner cache storage.
    inner: Rc<RefCell<HashMap<i64, String>>>,
}

impl ArtistNameCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    #[must_use]
    pub fn borrow(&self) -> Ref<'_, HashMap<i64, String>> {
        self.inner.borrow()
    }

    #[must_use]
    pub fn borrow_mut(&self) -> RefMut<'_, HashMap<i64, String>> {
        self.inner.borrow_mut()
    }
}
