mod dr_scanner;
mod file_processor;

pub mod library_ops;

use std::{error::Error, future::Future, path::Path, pin::Pin};

use sqlx::SqlitePool;
use tokio::fs::read_dir;

pub use self::{dr_scanner::scan_dr_value, file_processor::process_files_batch};

/// Recursively scans a folder for supported audio files and subfolders,
/// extracting metadata and inserting it into the database.
/// It also scans for Dynamic Range (DR) values in `.txt` or `.log` files within the folder.
///
/// This function uses a `Pin<Box<dyn Future>>` to allow for recursive asynchronous calls
/// without requiring the `folder_path` or `pool` to outlive the future, which is necessary
/// for handling arbitrarily deep directory structures.
///
/// # Arguments
/// * `pool` - A reference to the SQLite database connection pool.
/// * `folder_path` - The path of the folder to scan.
/// * `folder_id` - The database ID of the folder being scanned.
///
/// # Returns
/// A `Result` indicating success or an `Box<dyn Error>` on failure.
/// Errors during file processing or subdirectory scanning are currently logged
/// (implicitly, by returning `Ok(())` if an error occurs during `process_file` or `scan_folder` recursion)
/// to allow the scan to continue, but a top-level error will halt the current scan operation.
pub fn scan_folder<'a>(
    pool: &'a SqlitePool,
    folder_path: &'a Path,
    folder_id: i64,
) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>> + Send + 'a>> {
    Box::pin(async move {
        // Scan for DR value in .txt/.log files in this folder.
        // If an error occurs during DR value scanning, it's propagated.
        let dr_value = scan_dr_value(folder_path).await?;

        // Read directory entries. If the directory cannot be read, return Ok(()) to
        // allow the overall scan to continue without crashing.
        let mut entries = match read_dir(folder_path).await {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Error reading directory {}: {}", folder_path.display(), e);
                return Ok(());
            }
        };
        let mut audio_files = Vec::new();

        // Iterate through each entry in the directory.
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                // If the entry is a directory, recursively call `scan_folder`.
                // Log errors during recursive calls but don't halt the main scan.
                if let Err(e) = scan_folder(pool, &path, folder_id).await {
                    eprintln!("Error scanning subfolder {}: {}", path.display(), e);
                }
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                // If the entry is a file, check if its extension is supported.
                let supported_extensions = ["mp3", "flac", "ogg", "wav", "m4a", "opus", "aiff"];
                if supported_extensions.contains(&ext.to_lowercase().as_str()) {
                    audio_files.push(path);
                }
            }
        }

        // Process all collected audio files in a batch, associating them with the current folder
        // and any DR value found in the folder. Errors are logged but don't halt the scan.
        if !audio_files.is_empty() {
            if let Err(e) = process_files_batch(pool, &audio_files, folder_id, dr_value).await {
                eprintln!(
                    "Error processing batch for folder {}: {}",
                    folder_path.display(),
                    e
                );
            }
        }
        Ok(())
    })
}
