use std::path::PathBuf;

use glib::{Object, wrapper};
use gtk4::subclass::prelude::ObjectSubclassIsExt;

/// Represents an album item in the column view.
/// This struct contains all the necessary information about an album
/// to be displayed in the list view.
#[derive(Debug, Clone)]
pub struct AlbumListItem {
    /// The unique identifier for the album
    pub id: i64,
    /// The title of the album
    pub title: String,
    /// The artist who created the album
    pub artist: String,
    /// Optional path to the album's cover art image
    pub cover_art: Option<String>,
    /// The release year of the album, if available
    pub year: Option<i32>,
    /// The original release date of the album as a string, if available
    pub original_release_date: Option<String>,
    /// The DR (Dynamic Range) value of the album, if available
    pub dr_value: Option<i32>,
    /// Indicates whether the DR (Dynamic Range) analysis for this album is completed
    pub dr_completed: bool,
    /// The audio format of the album files (e.g., "FLAC", "MP3"), if available
    pub format: Option<String>,
    /// The bit depth of the audio files, if available
    pub bit_depth: Option<i32>,
    /// The sample frequency of the audio files in Hz, if available
    pub frequency: Option<i32>,
    /// The file system path to the album's folder
    pub folder_path: PathBuf,
}

impl AlbumListItem {
    /// Creates a new AlbumListItem with the provided album information
    ///
    /// # Parameters
    /// - `id`: The unique identifier for the album
    /// - `title`: The title of the album
    /// - `artist`: The artist who created the album
    /// - `cover_art`: Optional path to the album's cover art image
    /// - `year`: The release year of the album, if available
    /// - `original_release_date`: The original release date of the album as a string, if available
    /// - `dr_value`: The DR (Dynamic Range) value of the album, if available
    /// - `dr_completed`: Indicates whether the DR analysis for this album is completed
    /// - `format`: The audio format of the album files (e.g., "FLAC", "MP3"), if available
    /// - `bit_depth`: The bit depth of the audio files, if available
    /// - `frequency`: The sample frequency of the audio files in Hz, if available
    /// - `folder_path`: The file system path to the album's folder
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: i64,
        title: String,
        artist: String,
        cover_art: Option<String>,
        year: Option<i32>,
        original_release_date: Option<String>,
        dr_value: Option<i32>,
        dr_completed: bool,
        format: Option<String>,
        bit_depth: Option<i32>,
        frequency: Option<i32>,
        folder_path: PathBuf,
    ) -> Self {
        Self {
            id,
            title,
            artist,
            cover_art,
            year,
            original_release_date,
            dr_value,
            dr_completed,
            format,
            bit_depth,
            frequency,
            folder_path,
        }
    }
}

/// GObject wrapper for AlbumListItem to be used in ColumnView
/// This module implements the GObject subclass for AlbumListItemObject
/// which allows AlbumListItem to be used with GTK's object system
mod imp {
    use super::AlbumListItem;
    use glib::object_subclass;
    use gtk4::subclass::prelude::{ObjectImpl, ObjectSubclass};
    use std::cell::RefCell;

    /// The internal structure for AlbumListItemObject
    /// This holds the actual AlbumListItem data in a RefCell for interior mutability
    #[derive(Debug, Default)]
    pub struct AlbumListItemObject {
        /// The wrapped AlbumListItem, stored in a RefCell for interior mutability
        /// This allows for modification of the item even when the object is shared
        pub item: RefCell<Option<AlbumListItem>>,
    }

    /// Implementation of ObjectSubclass for AlbumListItemObject
    /// This defines the metadata and type information for the GObject
    #[object_subclass]
    impl ObjectSubclass for AlbumListItemObject {
        /// The name of the GObject type
        const NAME: &'static str = "AlbumListItemObject";
        /// The parent type (Object) that this subclass extends
        type Type = super::AlbumListItemObject;
    }

    /// Implementation of ObjectImpl for AlbumListItemObject
    /// This provides the implementation for the GObject interface
    /// Currently empty as we're just using the default implementation
    impl ObjectImpl for AlbumListItemObject {}
}

wrapper! {
    /// GObject wrapper for AlbumListItem
    /// This allows AlbumListItem to be used in GTK's ColumnView and other
    /// GObject-based components
    pub struct AlbumListItemObject(ObjectSubclass<imp::AlbumListItemObject>);
}

impl AlbumListItemObject {
    /// Creates a new AlbumListItemObject from an AlbumListItem
    ///
    /// # Parameters
    /// - `item`: The AlbumListItem to wrap in a GObject
    pub fn new(item: AlbumListItem) -> Self {
        let obj: Self = Object::builder().build();
        obj.imp().item.replace(Some(item));
        obj
    }

    /// Gets a reference to the AlbumListItem
    ///
    /// # Returns
    /// A reference to the wrapped AlbumListItem, or None if not set
    pub fn item(&self) -> std::cell::Ref<'_, Option<AlbumListItem>> {
        self.imp().item.borrow()
    }

    /// Gets a reference to the AlbumListItem's title
    ///
    /// # Returns
    /// The album title, or an empty string if the item is not set
    pub fn title(&self) -> String {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.title.clone()
        } else {
            String::new()
        }
    }

    /// Gets a reference to the AlbumListItem's cover art
    ///
    /// # Returns
    /// The path to the cover art image, or None if not available or item not set
    pub fn cover_art(&self) -> Option<String> {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.cover_art.clone()
        } else {
            None
        }
    }

    /// Gets a reference to the AlbumListItem's artist
    ///
    /// # Returns
    /// The album artist, or an empty string if the item is not set
    pub fn artist(&self) -> String {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.artist.clone()
        } else {
            String::new()
        }
    }

    /// Gets a reference to the AlbumListItem's format
    ///
    /// # Returns
    /// The audio format of the album files, or None if not available or item not set
    pub fn format(&self) -> Option<String> {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.format.clone()
        } else {
            None
        }
    }

    /// Gets the AlbumListItem's bit depth
    ///
    /// # Returns
    /// The bit depth of the audio files, or None if not available or item not set
    pub fn bit_depth(&self) -> Option<i32> {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.bit_depth
        } else {
            None
        }
    }

    /// Gets the AlbumListItem's frequency
    ///
    /// # Returns
    /// The sample frequency of the audio files in Hz, or None if not available or item not set
    pub fn frequency(&self) -> Option<i32> {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.frequency
        } else {
            None
        }
    }

    /// Gets the AlbumListItem's year
    ///
    /// # Returns
    /// The release year of the album, or None if not available or item not set
    pub fn year(&self) -> Option<i32> {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.year
        } else {
            None
        }
    }

    /// Gets the AlbumListItem's DR value
    ///
    /// # Returns
    /// The DR (Dynamic Range) value of the album, or None if not available or item not set
    pub fn dr_value(&self) -> Option<i32> {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.dr_value
        } else {
            None
        }
    }

    /// Gets the AlbumListItem's DR completion status
    ///
    /// # Returns
    /// Whether the DR (Dynamic Range) analysis for this album is completed
    pub fn dr_completed(&self) -> bool {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.dr_completed
        } else {
            false
        }
    }
}
