//! Shared catalog browsability assertion for library verification tests.
//!
//! Provides [`assert_browsable`] used by `SC-004` (`load_verification`) and
//! `SC-006` (`sc006`) to ensure the scanned library can be navigated via the
//! `Album` → `Track` relation (`get_tracks_by_album`). Extracted to eliminate
//! the 7-line clone between `load_verification.rs` and `sc006.rs`.

use anyhow::{Context, Result, ensure};

use oxhidifi::storage::{Storage, catalog::Album};

/// Assert that the library is browsable by fetching tracks for a sample album.
///
/// Verifies that `get_tracks_by_album` returns a non-empty result for the
/// first album in `albums`. The `context` string is prefixed to the error
/// message to identify the spec case (e.g. `"SC-004"` or `"SC-006"`).
///
/// # Arguments
///
/// * `storage` - Storage implementing [`Storage`].
/// * `albums` - Slice of albums; the first entry is probed.
/// * `context` - Spec label for error messages (e.g. `"SC-004"`).
///
/// # Returns
///
/// `Ok(())` if the library is browsable, otherwise an error.
///
/// # Errors
///
/// Returns an error if `get_tracks_by_album` fails or returns no tracks.
pub async fn assert_browsable<S>(storage: &S, albums: &[Album], context: &str) -> Result<()>
where
    S: Storage,
{
    let sample_album_id = albums.first().map_or(0, |a| a.id);
    let album_tracks = storage
        .get_tracks_by_album(sample_album_id)
        .await
        .context("get_tracks_by_album failed")?;
    ensure!(
        !album_tracks.is_empty(),
        "{context}: browsable check failed — get_tracks_by_album returned empty for album \
         {sample_album_id}"
    );
    Ok(())
}
