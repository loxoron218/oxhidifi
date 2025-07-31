use std::error::Error;

use regex::Regex;
use tokio::{
    fs::{File, read_dir},
    io::{AsyncBufReadExt, BufReader}};

/// Scans `.txt` and `.log` files within a specified folder for Dynamic Range (DR) values.
/// It parses various common DR value formats and returns the highest valid DR value found.
///
/// The function uses a regular expression to find DR values in lines of text files.
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
pub async fn scan_dr_value(folder_path: &str) -> Result<Option<u8>, Box<dyn Error>> {
    let mut entries = read_dir(folder_path).await?;

    // Regex to capture DR values. Handles "DRXX", "DRXX|ERR", and "XX|ERR".
    let dr_regex = Regex::new(
        r"(?i)DR(\d+|ERR)|Official DR value:\s*(\d+|ERR)|Реальные значения DR:\s*(\d+|ERR)|Official EP/Album DR:\s*(\d+|ERR)",
    ).unwrap(); // Unwrap is safe here as the regex is hardcoded and valid.
    let mut highest_dr: Option<u8> = None;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext = ext.to_lowercase();
                if ext == "txt" || ext == "log" {

                    // Attempt to open and read the file. If it fails, print error and continue.
                    let file = match File::open(&path).await {
                        Ok(f) => f,
                        Err(e) => {
                            eprintln!("Error opening DR log file {}: {}", path.display(), e);
                            continue; // Skip to the next entry
                        }
                    };
                    let reader = BufReader::new(file);
                    let mut lines = reader.lines();

                    // Read lines until EOF or an error occurs.
                    while let Some(line) = lines.next_line().await? {
                        if let Some(caps) = dr_regex.captures(&line) {

                            // Iterate through all possible capture groups (1 to 4 for this regex).
                            // The first successful capture will be used.
                            for i in 1..=4 {
                                if let Some(dr_str_match) = caps.get(i) {
                                    let dr_str = dr_str_match.as_str();

                                    // Only parse if the captured string is not "ERR".
                                    if dr_str.to_uppercase() != "ERR" {
                                        if let Ok(dr) = dr_str.parse::<u8>() {

                                            // Validate DR value is within the typical range [1, 20].
                                            if (1..=20).contains(&dr) {

                                                // Update `highest_dr` if the current `dr` is higher
                                                // or if `highest_dr` is currently `None`.
                                                highest_dr = match highest_dr {
                                                    Some(current_max) if dr > current_max => Some(dr),
                                                    None => Some(dr),
                                                    _ => highest_dr, // Keep current max
                                                };
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(highest_dr)
}