use std::{path::Path, sync::Arc};

use glib::{MainContext, Priority};
use gtk4::Picture;
use libadwaita::prelude::ObjectExt;

use crate::{
    ui::grids::album_grid_utils::create_colored_placeholder,
    utils::image::{ImageLoader, ImageLoaderError},
};

/// Async image loader for album covers
///
/// This struct provides asynchronous image loading functionality for GTK Picture widgets.
/// It integrates with the core ImageLoader to provide cached image loading while
/// ensuring that UI operations happen on the main thread. The loader also displays
/// placeholder images immediately while loading occurs in the background.
pub struct AsyncImageLoader {
    /// Thread-safe reference to the core image loader
    image_loader: Arc<ImageLoader>,
}

impl AsyncImageLoader {
    /// Creates a new async image loader
    ///
    /// This method initializes a new async image loader by creating a core ImageLoader
    /// instance and wrapping it in a thread-safe reference counter.
    ///
    /// # Returns
    /// A `Result` containing the new `AsyncImageLoader` instance or an `ImageLoaderError`
    pub fn new() -> Result<Self, ImageLoaderError> {
        let image_loader = ImageLoader::new()?;
        Ok(Self {
            image_loader: Arc::new(image_loader),
        })
    }

    /// Load an image asynchronously and update the Picture widget when done
    ///
    /// This method immediately displays a placeholder image and then loads the
    /// actual image in the background. When loading is complete, the Picture
    /// widget is updated with the loaded image on the main thread.
    ///
    /// # Arguments
    /// * `picture` - The GTK Picture widget to update with the loaded image
    /// * `cover_art_path` - An optional path to the image file to load
    /// * `cover_size` - The size (width and height) to scale the image to
    pub fn load_image_async(
        &self,
        picture: Picture,
        cover_art_path: Option<&Path>,
        cover_size: i32,
    ) {
        // Show placeholder immediately
        let placeholder_path = cover_art_path
            .map(|p| p.to_string_lossy())
            .unwrap_or_default();
        let placeholder = create_colored_placeholder(&placeholder_path, cover_size);
        picture.set_pixbuf(Some(&placeholder));

        // If we have a path, load the image asynchronously
        if let Some(path) = cover_art_path {
            let image_loader = Arc::clone(&self.image_loader);
            let path = path.to_path_buf();
            let weak_picture = picture.downgrade();

            // Spawn async task to load the image
            let context = MainContext::default();
            context.spawn_local_with_priority(Priority::HIGH_IDLE, async move {
                match image_loader.load_image_adaptive(&path, cover_size) {
                    Ok(pixbuf) => {
                        // Update the UI with the loaded image
                        if let Some(pic) = weak_picture.upgrade() {
                            pic.set_pixbuf(Some(&pixbuf));
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to load image {}: {:?}", path.display(), e);

                        // Keep the placeholder if loading fails
                    }
                }
            });
        }
    }
}
