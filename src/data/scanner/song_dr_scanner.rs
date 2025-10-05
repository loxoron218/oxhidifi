use std::{
    error::Error,
    fs::{File, read_dir},
    io::{BufRead, BufReader},
    path::Path,
};

/// Enum representing the different DR log file formats
#[derive(Debug, PartialEq)]
pub enum DrFormat {
    Maat,
    Foobar2000,
    Ttdr,
    Unknown,
}

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
pub fn scan_song_dr_values(
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
    let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

    // Detect format
    let format = detect_format(&lines);

    // Parse based on detected format
    match format {
        DrFormat::Maat => parse_maat_format(&lines),
        DrFormat::Foobar2000 => parse_foobar2000_format(&lines),
        DrFormat::Ttdr => parse_ttdr_format(&lines),
        DrFormat::Unknown => Ok(vec![]),
    }
}

/// Detects the format of a DR log file based on header patterns
///
/// # Arguments
/// * `lines` - The lines of the DR log file
///
/// # Returns
/// The detected `DrFormat`
pub fn detect_format(lines: &[String]) -> DrFormat {
    for line in lines {
        // MAAT DROffline Format: Look for lines with pipe delimiters and "File Name" + "DR" headers
        if line.contains("|") && line.contains("File Name") && line.contains("DR") {
            return DrFormat::Maat;
        }

        // Foobar2000 Format: Look for "Analyzed:" line and then a line with table headers containing "DR", "Peak", "RMS"
        if line.contains("Analyzed:") {
            // Look for the next line that contains the table headers
            return DrFormat::Foobar2000;
        }

        // TT DR Offline Meter Format: Look for "Analyzed folder:" or "Analyzed Folder:" and then a line with table headers containing "DR", "Peak", "RMS"
        if line.contains("Analyzed folder:") || line.contains("Analyzed Folder:") {
            // Look for the next line that contains the table headers
            return DrFormat::Ttdr;
        }
    }
    DrFormat::Unknown
}

/// Parses the MAAT DROffline format (pipe-delimited)
///
/// # Arguments
/// * `lines` - The lines of the DR log file
///
/// # Returns
/// A `Result` containing `Vec<Option<u8>>` with DR values for each song
fn parse_maat_format(lines: &[String]) -> Result<Vec<Option<u8>>, Box<dyn Error + Send + Sync>> {
    let mut dr_values = Vec::new();

    // Flag to song if we're currently parsing within a table structure
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
        if in_table && let Some(index) = dr_column_index {
            // Split the line into columns using the pipe delimiter
            let columns: Vec<&str> = line.split('|').collect();

            // Check if this looks like a data row (has enough columns to include the DR column)
            if columns.len() > index {
                // Extract and trim the content of the DR column
                let dr_column = columns[index].trim();

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

/// Parses the Foobar2000 format
///
/// # Arguments
/// * `lines` - The lines of the DR log file
///
/// # Returns
/// A `Result` containing `Vec<Option<u8>>` with DR values for each song
fn parse_foobar2000_format(
    lines: &[String],
) -> Result<Vec<Option<u8>>, Box<dyn Error + Send + Sync>> {
    let mut dr_values = Vec::new();
    let mut in_table = false;
    let mut dr_column_index = None;

    // Process each line in the file
    for line in lines {
        // Look for the start of the table with dashed line separator
        if is_separator_line(line) && line.len() > 10 {
            in_table = true;
            continue;
        }

        // If we're in the table, look for the header line
        if in_table
            && !line.is_empty()
            && line.contains("DR")
            && line.contains("Peak")
            && line.contains("RMS")
        {
            // Find the DR column index by splitting the header line
            let headers: Vec<&str> = line.split_whitespace().collect();
            for (i, header) in headers.iter().enumerate() {
                if header.trim() == "DR" {
                    dr_column_index = Some(i);
                    break;
                }
            }
            continue;
        }

        // If we're in the table and have identified the DR column
        if in_table
            && !line.is_empty()
            && !is_separator_line(line)
            && let Some(index) = dr_column_index
        {
            // Split the line into columns using whitespace
            let columns: Vec<&str> = line.split_whitespace().collect();

            // Check if this looks like a data row (has enough columns to include the DR column)
            if columns.len() > index {
                // Extract the DR column content
                let dr_column = columns[index];

                // Try to parse the DR value from the DR column (e.g., "DR13" -> 13)
                if let Some(dr_str) = dr_column.strip_prefix("DR") {
                    if let Ok(dr) = dr_str.parse::<u8>() {
                        // Validate DR value is within the typical range [1, 20]
                        if (1..=20).contains(&dr) {
                            dr_values.push(Some(dr));
                        } else {
                            dr_values.push(None);
                        }
                    } else {
                        dr_values.push(None);
                    }
                } else {
                    dr_values.push(None);
                }
            }
        }

        // Stop parsing if we reach the end of the table
        // Look for lines with "Number of songs:" or another dashed line
        if line.starts_with("Number of")
            || (is_separator_line(line) && in_table && dr_column_index.is_some())
        {
            break;
        }
    }
    Ok(dr_values)
}

/// Parses the TT DR Offline Meter format
///
/// # Arguments
/// * `lines` - The lines of the DR log file
///
/// # Returns
/// A `Result` containing `Vec<Option<u8>>` with DR values for each song
pub fn parse_ttdr_format(
    lines: &[String],
) -> Result<Vec<Option<u8>>, Box<dyn Error + Send + Sync>> {
    let mut dr_values = Vec::new();
    let mut in_table = false;
    let mut dr_column_index = None;

    // Process each line in the file
    for line in lines {
        // Look for the start of the table with dashed line separator
        if is_separator_line(line) && line.len() > 10 {
            in_table = true;
            continue;
        }

        // If we're in the table, look for the header line
        if in_table
            && !line.is_empty()
            && line.contains("DR")
            && line.contains("Peak")
            && line.contains("RMS")
        {
            // Find the DR column index by splitting the header line
            let headers: Vec<&str> = line.split_whitespace().collect();
            for (i, header) in headers.iter().enumerate() {
                if header.trim() == "DR" {
                    dr_column_index = Some(i);
                    break;
                }
            }
            continue;
        }

        // If we're in the table and have identified the DR column
        if in_table
            && !line.is_empty()
            && !is_separator_line(line)
            && let Some(index) = dr_column_index
        {
            // Split the line into columns using whitespace
            let columns: Vec<&str> = line.split_whitespace().collect();

            // Check if this looks like a data row (has enough columns to include the DR column)
            if columns.len() > index {
                // Extract the DR column content
                let dr_column = columns[index];

                // Try to parse the DR value from the DR column (e.g., "DR13" -> 13)
                if let Some(dr_str) = dr_column.strip_prefix("DR") {
                    if let Ok(dr) = dr_str.parse::<u8>() {
                        // Validate DR value is within the typical range [1, 20]
                        if (1..=20).contains(&dr) {
                            dr_values.push(Some(dr));
                        } else {
                            dr_values.push(None);
                        }
                    } else {
                        dr_values.push(None);
                    }
                } else {
                    dr_values.push(None);
                }
            }
        }

        // Stop parsing if we reach the end of the table
        // Look for lines with "Number of files:" or another dashed line
        if line.starts_with("Number of")
            || (is_separator_line(line) && in_table && dr_column_index.is_some())
        {
            break;
        }
    }
    Ok(dr_values)
}

/// Helper function to check if a line is a separator line (dashed line)
fn is_separator_line(line: &str) -> bool {
    line.chars().all(|c| c == '-' || c == ' ') && line.len() > 10
}
