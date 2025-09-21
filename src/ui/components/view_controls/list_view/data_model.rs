use std::path::PathBuf;

use gtk4::{
    glib::{Object, wrapper},
    subclass::prelude::ObjectSubclassIsExt,
};

/// Basic album information
#[derive(Debug, Clone)]
pub struct AlbumBasicInfo {
    /// The unique identifier for the album
    pub id: i64,
    /// The title of the album
    pub title: String,
    /// The artist who created the album
    pub artist: String,
    /// The file system path to the album's folder
    pub folder_path: PathBuf,
}

/// Album metadata information
#[derive(Debug, Clone)]
pub struct AlbumMetadata {
    /// The release year of the album, if available
    pub year: Option<i32>,
    /// The original release date of the album as a string, if available
    pub original_release_date: Option<String>,
}

/// Audio quality information
#[derive(Debug, Clone)]
pub struct AudioQualityInfo {
    /// The DR (Dynamic Range) value of the album, if available
    pub dr_value: Option<i32>,
    /// Indicates whether the DR (Dynamic Range) analysis for this album is the best
    pub dr_is_best: bool,
    /// The audio format of the album files (e.g., "FLAC", "MP3"), if available
    pub format: Option<String>,
    /// The bit depth of the audio files, if available
    pub bit_depth: Option<i32>,
    /// The sample rate of the audio files in Hz, if available
    pub sample_rate: Option<i32>,
}

/// Represents an album item in the column view.
/// This struct contains all the necessary information about an album
/// to be displayed in the list view.
#[derive(Debug, Clone)]
pub struct AlbumListItem {
    /// Basic album information
    pub basic_info: AlbumBasicInfo,
    /// Album metadata
    pub metadata: AlbumMetadata,
    /// Audio quality information
    pub audio_quality: AudioQualityInfo,
    /// Optional path to the album's cover art image
    pub cover_art: Option<String>,
}

impl AlbumListItem {
    /// Creates a new AlbumListItem with the provided album information
    ///
    /// # Parameters
    /// - `basic_info`: Basic album information (id, title, artist, folder_path)
    /// - `metadata`: Album metadata (year, original_release_date)
    /// - `audio_quality`: Audio quality information (dr_value, dr_is_best, format, bit_depth, sample_rate)
    /// - `cover_art`: Optional path to the album's cover art image
    pub fn new(
        basic_info: AlbumBasicInfo,
        metadata: AlbumMetadata,
        audio_quality: AudioQualityInfo,
        cover_art: Option<String>,
    ) -> Self {
        Self {
            basic_info,
            metadata,
            audio_quality,
            cover_art,
        }
    }
}

/// GObject wrapper for AlbumListItem to be used in ColumnView
/// This module implements the GObject subclass for AlbumListItemObject
/// which allows AlbumListItem to be used with GTK's object system
mod imp {
    use super::AlbumListItem;
    use gtk4::{
        glib::{self, object_subclass},
        subclass::prelude::{ObjectImpl, ObjectSubclass},
    };
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
            item.basic_info.title.clone()
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
            item.basic_info.artist.clone()
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
            item.audio_quality.format.clone()
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
            item.audio_quality.bit_depth
        } else {
            None
        }
    }

    /// Gets the AlbumListItem's sample rate
    ///
    /// # Returns
    /// The sample rate of the audio files in Hz, or None if not available or item not set
    pub fn sample_rate(&self) -> Option<i32> {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.audio_quality.sample_rate
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
            item.metadata.year
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
            item.audio_quality.dr_value
        } else {
            None
        }
    }

    /// Gets the AlbumListItem's DR best status
    ///
    /// # Returns
    /// Whether the DR (Dynamic Range) analysis for this album is the best
    pub fn dr_is_best(&self) -> bool {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.audio_quality.dr_is_best
        } else {
            false
        }
    }

    /// Gets the AlbumListItem's original release date
    ///
    /// # Returns
    /// The original release date of the album, or None if not available or item not set
    pub fn original_release_date(&self) -> Option<String> {
        if let Some(ref item) = *self.imp().item.borrow() {
            item.metadata.original_release_date.clone()
        } else {
            None
        }
    }
}
