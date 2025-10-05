mod album_dr_scanner;
mod file_processor;
mod song_dr_scanner;

pub mod library_ops;

use std::{
    collections::VecDeque, error::Error, future::Future, path::Path, path::PathBuf, pin::Pin,
    sync::Arc,
};

use sqlx::SqlitePool;
use tokio::{
    fs::read_dir,
    sync::{Mutex, Semaphore},
    task::JoinSet,
};

pub use self::{album_dr_scanner::scan_dr_value, file_processor::process_files_batch};

/// Checks if a file path has a supported audio file extension.
///
/// This function determines whether a given file path corresponds to a supported
/// audio file format by examining its extension. The comparison is case-insensitive.
///
/// # Supported Formats
///
/// * MP3 - MPEG Audio Layer III
/// * FLAC - Free Lossless Audio Codec
/// * OGG - Ogg Vorbis
/// * WAV - Waveform Audio File Format
/// * M4A - MPEG-4 Audio
/// * OPUS - Opus Audio Codec
/// * AIFF - Audio Interchange File Format
///
/// # Arguments
///
/// * `path` - A reference to the file path to check
///
/// # Returns
///
/// Returns `true` if the file has a supported audio extension, `false` otherwise.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// assert_eq!(is_supported_audio_file(Path::new("song.mp3")), true);
/// assert_eq!(is_supported_audio_file(Path::new("document.txt")), false);
/// ```
fn is_supported_audio_file(path: &Path) -> bool {
    // List of supported audio file extensions
    const SUPPORTED_EXTENSIONS: [&str; 7] = ["mp3", "flac", "ogg", "wav", "m4a", "opus", "aiff"];

    // Extract extension, convert to lowercase, and check if it's in our supported list
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| ext.to_lowercase())
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.as_str()))
        .unwrap_or(false)
}

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
type ScanFolderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send + 'a>>;

pub fn scan_folder<'a>(
    pool: &'a SqlitePool,
    folder_path: &'a Path,
    folder_id: i64,
) -> ScanFolderFuture<'a> {
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

        // Vector to collect audio files found in this directory for batch processing
        let mut audio_files = Vec::new();

        // Iterate through each entry in the directory.
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            match path.is_dir() {
                true => {
                    // If the entry is a directory, recursively call `scan_folder`.
                    // Log errors during recursive calls but don't halt the main scan.
                    if let Err(e) = scan_folder(pool, &path, folder_id).await {
                        eprintln!("Error scanning subfolder {}: {}", path.display(), e);
                    }
                }
                false => {
                    // If the entry is a file with a supported extension, add it to the list.
                    if is_supported_audio_file(&path) {
                        audio_files.push(path);
                    }
                }
            }
        }

        // Process all collected audio files in a batch, associating them with the current folder
        // and any DR value found in the folder. Errors are logged but don't halt the scan.
        if !audio_files.is_empty()
            && let Err(e) = process_files_batch(pool, &audio_files, folder_id, dr_value).await
        {
            eprintln!(
                "Error processing batch for folder {}: {}",
                folder_path.display(),
                e
            );
        }
        Ok(())
    })
}

/// Parallel version of scan_folder that uses concurrent directory traversal
/// for improved performance on multi-core systems.
///
/// This function scans a folder and all its subfolders concurrently, processing
/// multiple directories in parallel up to the specified concurrency limit.
/// Each directory is processed independently, and subdirectories are added to
/// a work queue for parallel processing.
///
/// # Arguments
/// * `pool` - An `Arc` reference to the SQLite database connection pool.
/// * `folder_path` - The path of the folder to scan.
/// * `folder_id` - The database ID of the folder being scanned.
/// * `max_concurrent_scans` - The maximum number of directories to scan concurrently.
///
/// # Returns
/// A `Result` indicating success or an `Box<dyn Error>` on failure.
/// Errors during file processing or subdirectory scanning are currently logged
/// to allow the scan to continue, but a top-level error will halt the current scan operation.
pub async fn scan_folder_parallel(
    pool: Arc<SqlitePool>,
    folder_path: &Path,
    folder_id: i64,
    max_concurrent_scans: usize,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Create a queue for directories to scan
    let queue = Arc::new(Mutex::new(VecDeque::new()));
    queue.lock().await.push_back(folder_path.to_path_buf());

    // Create a semaphore to limit concurrent scans
    let semaphore = Arc::new(Semaphore::new(max_concurrent_scans));

    // Create a JoinSet to manage our tasks
    let mut join_set = JoinSet::new();

    // Process directories until queue is empty and all tasks are complete
    loop {
        // Start new tasks if we have available permits and directories in the queue
        while !queue.lock().await.is_empty() && semaphore.available_permits() > 0 {
            // Acquire a permit from the semaphore to limit concurrency
            // acquire_owned() gives us an owned permit that can be moved into the task
            let permit = semaphore.clone().acquire_owned().await?;

            // Get the next directory to scan from the front of the queue
            let path = queue.lock().await.pop_front().unwrap();

            // Clone the necessary values for the task
            let pool_clone = pool.clone();
            let queue_clone = queue.clone();
            let folder_id_clone = folder_id;

            // Spawn a new task to scan this directory
            join_set.spawn(async move {
                // Perform the actual directory scanning
                let result =
                    scan_single_directory(pool_clone, &path, folder_id_clone, queue_clone).await;

                // Release the permit
                drop(permit);
                result
            });
        }

        // If no tasks are running and queue is empty, we're done
        if join_set.is_empty() {
            break;
        }

        // Wait for one task to complete
        if let Some(Ok(Err(e))) = join_set.join_next().await {
            eprintln!("Error scanning directory: {}", e);
        }
    }
    Ok(())
}

/// Scans a single directory for audio files and subdirectories.
/// Adds subdirectories to the queue for parallel processing.
///
/// This function processes a single directory by:
/// 1. Scanning for DR values in .txt/.log files
/// 2. Reading directory entries
/// 3. Adding subdirectories to the work queue for parallel processing
/// 4. Collecting audio files for batch processing
/// 5. Processing collected audio files in batches
///
/// # Arguments
/// * `pool` - An `Arc` reference to the SQLite database connection pool.
/// * `folder_path` - The path of the folder to scan.
/// * `folder_id` - The database ID of the folder being scanned.
/// * `queue` - A shared queue for adding subdirectories to be processed.
///
/// # Returns
/// A `Result` indicating success or an `Box<dyn Error>` on failure.
async fn scan_single_directory(
    pool: Arc<SqlitePool>,
    folder_path: &Path,
    folder_id: i64,
    queue: Arc<Mutex<VecDeque<PathBuf>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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

    // Vector to collect audio files found in this directory for batch processing
    let mut audio_files = Vec::new();

    // Iterate through each entry in the directory.
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        match path.is_dir() {
            true => {
                // Add subdirectory to the queue for parallel processing
                queue.lock().await.push_back(path);
            }
            false => {
                // If the entry is a file with a supported extension, add it to the list.
                if is_supported_audio_file(&path) {
                    audio_files.push(path);
                }
            }
        }
    }

    // Process all collected audio files in a batch, associating them with the current folder
    // and any DR value found in the folder. Errors are logged but don't halt the scan.
    if !audio_files.is_empty()
        && let Err(e) = process_files_batch(pool.as_ref(), &audio_files, folder_id, dr_value).await
    {
        eprintln!(
            "Error processing batch for folder {}: {}",
            folder_path.display(),
            e
        );
    }
    Ok(())
}
