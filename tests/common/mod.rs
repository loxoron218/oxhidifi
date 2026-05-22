//! Shared test infrastructure: fixture helpers and mock utilities.

use std::{
    fs::write,
    io::{Error, Result},
    path::{Path, PathBuf},
};

use tempfile::{TempDir, tempdir};

/// Creates a temporary directory for test fixtures.
///
/// # Errors
///
/// Returns an error if the OS cannot create the directory.
pub fn create_fixture_dir() -> Result<TempDir> {
    tempdir().map_err(Error::other)
}

/// Creates a dummy audio file at the given path for scanner testing.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub fn create_dummy_audio_file(dir: &Path, name: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    write(&path, [])?;
    Ok(path)
}

/// Creates a minimal FLAC file for testing.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub fn create_minimal_flac(dir: &Path, name: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    let mut data = Vec::new();
    data.extend_from_slice(b"fLaC");

    // STREAMINFO metadata block (minimum required)
    data.extend_from_slice(&[0x00, 0x22, 0x00, 0x00]);
    data.extend_from_slice(&[0; 34]);
    write(&path, &data)?;
    Ok(path)
}
