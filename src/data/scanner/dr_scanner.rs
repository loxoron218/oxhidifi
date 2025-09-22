use std::{error::Error, path::Path, sync::OnceLock};

use regex::Regex;
use tokio::{
    fs::{File, read_dir},
    io::{AsyncBufReadExt, BufReader},
};

/// Returns a reference to the lazily-initialized, compiled regex.
///
/// This function ensures the regex is compiled only once, in a thread-safe manner,
/// using `OnceLock` for safe, concurrent initialization.
fn get_dr_regex() -> &'static Regex {
    // This provides safe, concurrent, one-time initialization.
    static DR_REGEX: OnceLock<Regex> = OnceLock::new();
    DR_REGEX.get_or_init(|| {

        // The regex is compiled here only on the first call.
        // `unwrap` is safe as the pattern is hardcoded and valid.
        Regex::new(
            r"(?i)DR(\d+|ERR)|Official DR value:\s*(?:DR)?\s*(\d+|ERR)|Реальные значения DR:\s*(\d+|ERR)|Official EP/Album DR:\s*(\d+|ERR)",
        ).unwrap()
    })
}

/// Scans `.txt` and `.log` files within a specified folder for Dynamic Range (DR) values.
/// It parses various common DR value formats and returns the official album DR value when present,
/// or the highest individual track DR value otherwise.
///
/// The function uses a regular expression to find DR values in lines of text files.
/// It supports multiple formats including:
/// - Simple "DR" followed by digits or "ERR" (treated as individual track DR values)
/// - "Official DR value:" followed by optional "DR" and digits or "ERR" (treated as official album DR)
/// - Russian "Реальные значения DR:" followed by digits or "ERR" (treated as official album DR)
/// - "Official EP/Album DR:" followed by digits or "ERR" (treated as official album DR)
///
/// When both official album DR values and individual track DR values are present in a file,
/// the official album DR value is prioritized.
///
/// It iterates through entries in the folder, and for each `.txt` or `.log` file,
/// it reads line by line, attempting to capture DR values.
///
/// # Arguments
/// * `folder_path` - The path to the folder to scan for DR values.
///
/// # Returns
/// A `Result` containing `Option<u8>`:
/// - `Some(dr_value)` if at least one valid DR value (between 1 and 20) is found.
/// - `None` if no valid DR value is found in any scanned file.
/// - `Box<dyn Error>` if a critical I/O error occurs during directory reading.
///   Errors during file opening or line reading are caught internally to allow
///   the scan to continue for other files.
pub async fn scan_dr_value(folder_path: &Path) -> Result<Option<u8>, Box<dyn Error + Send + Sync>> {
    let mut entries = read_dir(folder_path)
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

    // Lazily initialize the regex to capture DR values.
    let dr_regex = get_dr_regex();
    let mut official_dr: Option<u8> = None;
    let mut highest_individual_dr: Option<u8> = None;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?
    {
        let path = entry.path();
        if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
        {
            let ext = ext.to_lowercase();
            if ext == "txt" || ext == "log" {
                // Attempt to open and read the file. If it fails, print error and continue.
                let file = match File::open(&path).await {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("Error opening DR log file {}: {}", path.display(), e);

                        // Skip to the next entry
                        continue;
                    }
                };
                let mut reader = BufReader::new(file);
                let mut buffer = Vec::with_capacity(256);
                loop {
                    // Clear buffer for each new line
                    buffer.clear();
                    let bytes_read = reader
                        .read_until(b'\n', &mut buffer)
                        .await
                        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                    if bytes_read == 0 {
                        // EOF
                        break;
                    }
                    let line = String::from_utf8_lossy(&buffer).into_owned();
                    if let Some(caps) = dr_regex.captures(&line) {
                        // Check which capture group matched to determine if it's an official DR value
                        // Group 1: Simple DR(\d+|ERR)
                        // Group 2: Official DR value: (?:DR)?\s*(\d+|ERR)
                        // Group 3: Реальные значения DR: (\d+|ERR)
                        // Group 4: Official EP/Album DR: (\d+|ERR)
                        // If we found an official DR value (groups 2, 3, or 4), use it
                        if caps.get(2).is_some() || caps.get(3).is_some() || caps.get(4).is_some() {
                            // Extract the DR value from the official DR capture groups
                            let dr_str_match =
                                caps.get(2).or_else(|| caps.get(3)).or_else(|| caps.get(4));
                            if let Some(dr_str_match) = dr_str_match {
                                let dr_str = dr_str_match.as_str();

                                // Only parse if the captured string is not "ERR".
                                if dr_str.to_uppercase() != "ERR"
                                    && let Ok(dr) = dr_str.parse::<u8>()
                                {
                                    // Validate DR value is within the typical range [1, 20].
                                    if (1..=20).contains(&dr) {
                                        // For official DR values, we take the first one we find
                                        // since there should only be one per file
                                        official_dr = Some(dr);
                                    }
                                }
                            }
                        } else if let Some(dr_str_match) = caps.get(1) {
                            // For simple DR patterns, treat as individual track DR values
                            let dr_str = dr_str_match.as_str();

                            // Only parse if the captured string is not "ERR".
                            if dr_str.to_uppercase() != "ERR"
                                && let Ok(dr) = dr_str.parse::<u8>()
                            {
                                // Validate DR value is within the typical range [1, 20].
                                if (1..=20).contains(&dr) {
                                    // Update highest_individual_dr if the current dr is higher
                                    // or if highest_individual_dr is currently None.
                                    highest_individual_dr = match highest_individual_dr {
                                        Some(current_max) => Some(current_max.max(dr)),
                                        None => Some(dr),
                                    };
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Prioritize official DR values over individual track DR values
    Ok(official_dr.or(highest_individual_dr))
}
