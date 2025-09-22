use std::{
    error::Error,
    fs::{File, read_dir},
    io::{BufRead, BufReader},
    path::Path,
};

/// Scans `.txt` and `.log` files within a specified folder for individual song Dynamic Range (DR) values.
/// It parses the table-like structure in DR log files and extracts the DR value for each song in order.
///
/// # Arguments
/// * `folder_path` - The path to the folder to scan for DR values.
///
/// # Returns
/// A `Result` containing `Vec<Option<u8>>`:
/// - `Vec<Option<u8>>` with DR values for each song (None if not available)
/// - `Box<dyn Error>` if a critical I/O error occurs during directory reading.
pub fn scan_individual_dr_values(
    folder_path: &Path,
) -> Result<Vec<Option<u8>>, Box<dyn Error + Send + Sync>> {
    // Attempt to read the directory entries at the given path
    let entries = read_dir(folder_path).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
    let mut all_dr_values = Vec::new();

    // Iterate through each entry in the directory
    for entry in entries {
        let entry = entry.map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        let path = entry.path();

        // Check if the entry is a file and has a valid extension
        if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
        {
            // Convert extension to lowercase for case-insensitive comparison
            let ext = ext.to_lowercase();

            // Process only .txt and .log files which may contain DR data
            if ext == "txt" || ext == "log" {
                // Parse the DR values from this file
                let file_dr_values = parse_dr_file(&path)?;

                // Only extend if we found DR values in this file
                // This prevents adding empty vectors that would not contribute data
                if !file_dr_values.is_empty() {
                    all_dr_values.extend(file_dr_values);
                }
            }
        }
    }
    Ok(all_dr_values)
}

/// Parses a single DR log file and extracts individual song DR values.
///
/// # Arguments
/// * `file_path` - The path to the DR log file to parse.
///
/// # Returns
/// A `Result` containing `Vec<Option<u8>>`:
/// - `Vec<Option<u8>>` with DR values for each song (None if not available)
/// - `Box<dyn Error>` if a critical I/O error occurs during file reading.
fn parse_dr_file(file_path: &Path) -> Result<Vec<Option<u8>>, Box<dyn Error + Send + Sync>> {
    // Open the file for reading
    let file = File::open(file_path).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

    // Create a buffered reader for efficient reading
    let reader = BufReader::new(file);

    // Read all lines from the file, skipping any that cannot be read as UTF-8
    let lines: Vec<String> = reader.lines().filter_map(|line| line.ok()).collect();

    let mut dr_values = Vec::new();

    // Flag to track if we're currently parsing within a table structure
    let mut in_table = false;

    // Store the column index where DR values are located
    let mut dr_column_index = None;

    // Process each line in the file
    for line in lines {
        // Look for the table header which contains "File Name" and "DR"
        // This identifies the beginning of the DR data table
        if line.contains("|") && line.contains("File Name") && line.contains("DR") {
            in_table = true;

            // Find the DR column index by splitting the header line
            let headers: Vec<&str> = line.split('|').collect();
            for (i, header) in headers.iter().enumerate() {
                // Look for a header that starts with "DR" (e.g., "DR", "DR Peak")
                if header.trim().starts_with("DR") {
                    dr_column_index = Some(i);
                    break;
                }
            }

            // Continue to the next line after identifying the header
            continue;
        }

        // If we're in the table and have identified the DR column
        if in_table && dr_column_index.is_some() {
            // Split the line into columns using the pipe delimiter
            let columns: Vec<&str> = line.split('|').collect();

            // Check if this looks like a data row (has enough columns to include the DR column)
            if columns.len() > dr_column_index.unwrap() {
                // Extract and trim the content of the DR column
                let dr_column = columns[dr_column_index.unwrap()].trim();

                // Try to parse the DR value as an unsigned 8-bit integer
                if let Ok(dr) = dr_column.parse::<u8>() {
                    // Validate DR value is within the typical range [1, 20]
                    // DR values outside this range are considered invalid
                    if (1..=20).contains(&dr) {
                        dr_values.push(Some(dr));
                    } else {
                        // If DR value is out of range, push None to indicate invalid data
                        dr_values.push(None);
                    }
                } else {
                    // If parsing fails (e.g., non-numeric content), push None
                    dr_values.push(None);
                }
            }
        }

        // Stop parsing if we reach the end of the table
        // "Number of" typically indicates a summary line after the data table
        if line.starts_with("Number of") {
            break;
        }
    }
    Ok(dr_values)
}
