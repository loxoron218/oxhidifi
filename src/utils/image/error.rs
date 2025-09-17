use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
    io,
};

use gtk4::glib;
use image::ImageError;

use crate::utils::image::ImageLoaderError::{Glib, Image, InvalidPath, Io};

/// Error types for the image loader
///
/// This enum represents all possible errors that can occur during image loading operations.
/// It provides a unified error type that can be used throughout the image loading pipeline.
#[derive(Debug)]
pub enum ImageLoaderError {
    /// An I/O error occurred (e.g., file not found, permission denied)
    Io(io::Error),
    /// An image processing error occurred (e.g., unsupported format, corrupted data)
    Image(ImageError),
    /// A GLib error occurred (e.g., during pixbuf operations)
    Glib(glib::Error),
    /// The image path was invalid or the pixbuf could not be created
    InvalidPath,
}

/// Implementation of Display trait for ImageLoaderError
///
/// This implementation provides user-friendly error messages for all error variants.
impl Display for ImageLoaderError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Io(e) => write!(f, "IO error: {}", e),
            Image(e) => write!(f, "Image error: {}", e),
            Glib(e) => write!(f, "GLib error: {}", e),
            InvalidPath => write!(f, "Invalid path"),
        }
    }
}

/// Implementation of Error trait for ImageLoaderError
///
/// This implementation allows ImageLoaderError to be used as a standard error type
/// and provides access to the underlying error source when available.
impl Error for ImageLoaderError {}

/// Implementation of From trait to convert io::Error to ImageLoaderError
///
/// This implementation allows io::Error to be automatically converted to
/// ImageLoaderError::Io variant when using the ? operator.
impl From<io::Error> for ImageLoaderError {
    fn from(err: io::Error) -> Self {
        Io(err)
    }
}

/// Implementation of From trait to convert ImageError to ImageLoaderError
///
/// This implementation allows ImageError to be automatically converted to
/// ImageLoaderError::Image variant when using the ? operator.
impl From<ImageError> for ImageLoaderError {
    fn from(err: ImageError) -> Self {
        Image(err)
    }
}

/// Implementation of From trait to convert glib::Error to ImageLoaderError
///
/// This implementation allows glib::Error to be automatically converted to
/// ImageLoaderError::Glib variant when using the ? operator.
impl From<glib::Error> for ImageLoaderError {
    fn from(err: glib::Error) -> Self {
        Glib(err)
    }
}
