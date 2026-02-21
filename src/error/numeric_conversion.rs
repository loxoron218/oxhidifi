//! Safe numeric conversion utilities with consistent error handling.
//!
//! This module provides helper functions for converting between numeric types
//! with consistent clamping and error logging behavior across the codebase.

use std::convert::TryFrom;

use tracing::warn;

/// Converts a `u64` to `i64` after clamping to a maximum value.
///
/// This function first clamps the input value to the specified maximum,
/// then attempts the conversion. If conversion still fails (which should
/// only happen if `max` exceeds `i64::MAX`), it logs a warning and returns
/// the maximum value as i64 (which is guaranteed to be a reasonable fallback).
///
/// # Arguments
///
/// * `value` - The value to convert
/// * `max` - The maximum value to clamp to before conversion (must be <= `i64::MAX`)
/// * `field_name` - Human-readable name for logging purposes
///
/// # Returns
///
/// The converted value, clamped and logged appropriately.
///
/// # Example
///
/// ```
/// use oxhidifi::error::numeric_conversion::safe_u64_to_i64;
///
/// let result = safe_u64_to_i64(5000, 3000, "duration_ms");
/// assert_eq!(result, 3000);
/// ```
#[must_use]
pub fn safe_u64_to_i64(value: u64, max: u64, field_name: &str) -> i64 {
    let clamped = value.min(max);

    i64::try_from(clamped).unwrap_or_else(|_| {
        let fallback = i64::try_from(max).unwrap_or(i64::MAX);

        warn!(
            value = %value,
            max = %max,
            field = field_name,
            fallback = fallback,
            "Value exceeds i64 range after clamping, using max as fallback"
        );
        fallback
    })
}

/// Converts a `u32` to `i32` after clamping to a maximum value.
///
/// This function first clamps the input value to the specified maximum,
/// then attempts the conversion. If conversion fails, it logs a warning
/// and returns the specified default value.
///
/// # Arguments
///
/// * `value` - The value to convert
/// * `max` - The maximum value to clamp to before conversion
/// * `default` - The default value to return if conversion fails
/// * `field_name` - Human-readable name for logging purposes
///
/// # Returns
///
/// The converted value, clamped and logged appropriately.
///
/// # Example
///
/// ```
/// use oxhidifi::error::numeric_conversion::safe_u32_to_i32;
///
/// let result = safe_u32_to_i32(5000, 3000, 180, "cover_size");
/// assert_eq!(result, 3000);
/// ```
#[must_use]
pub fn safe_u32_to_i32(value: u32, max: u32, default: i32, field_name: &str) -> i32 {
    let clamped = value.min(max);
    i32::try_from(clamped).unwrap_or_else(|_| {
        warn!(
            value = %value,
            max = %max,
            field = field_name,
            "Value exceeds i32 range after clamping, using default"
        );
        default
    })
}

/// Converts an `i32` to `u32` with a fallback default.
///
/// This function attempts the conversion and returns the default value
/// if conversion fails (negative input).
///
/// # Arguments
///
/// * `value` - The value to convert
/// * `default` - The default value to return if conversion fails
/// * `field_name` - Human-readable name for logging purposes
///
/// # Returns
///
/// The converted value or the default.
///
/// # Example
///
/// ```
/// use oxhidifi::error::numeric_conversion::safe_i32_to_u32;
///
/// let result = safe_i32_to_u32(-1, 180, "cover_size");
/// assert_eq!(result, 180);
/// ```
#[must_use]
pub fn safe_i32_to_u32(value: i32, default: u32, field_name: &str) -> u32 {
    u32::try_from(value).unwrap_or_else(|_| {
        warn!(
            value = %value,
            field = field_name,
            "Invalid value, using default"
        );
        default
    })
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail};

    use crate::error::numeric_conversion::{safe_i32_to_u32, safe_u32_to_i32, safe_u64_to_i64};

    #[test]
    fn test_safe_u64_to_i64_within_range() -> Result<()> {
        let result = safe_u64_to_i64(1000, 5000, "test");
        if result != 1000 {
            bail!("Expected 1000, got {result}");
        }
        Ok(())
    }

    #[test]
    fn test_safe_u64_to_i64_clamped() -> Result<()> {
        let result = safe_u64_to_i64(8000, 5000, "test");
        if result != 5000 {
            bail!("Expected 5000, got {result}");
        }
        Ok(())
    }

    #[test]
    fn test_safe_u64_to_i64_max_exceeds_i64() -> Result<()> {
        // When max exceeds i64::MAX, fallback should be max converted to i64
        let max_exceeding_i64 = u64::MAX;
        let result = safe_u64_to_i64(u64::MAX, max_exceeding_i64, "test");

        if result != i64::MAX {
            bail!("Expected {}, got {result}", i64::MAX);
        }
        Ok(())
    }

    #[test]
    fn test_safe_u64_to_i64_reasonable_max_preserved() -> Result<()> {
        // With a reasonable max, values should be clamped to that max
        let reasonable_max: u64 = 10 * 1024 * 1024 * 1024; // 10 GB
        let result = safe_u64_to_i64(u64::MAX, reasonable_max, "test");

        let expected = i64::try_from(reasonable_max)?;
        if result != expected {
            bail!("Expected {expected}, got {result}");
        }
        Ok(())
    }

    #[test]
    fn test_safe_u32_to_i32_within_range() -> Result<()> {
        let result = safe_u32_to_i32(100, 500, 50, "test");
        if result != 100 {
            bail!("Expected 100, got {result}");
        }
        Ok(())
    }

    #[test]
    fn test_safe_u32_to_i32_clamped() -> Result<()> {
        let result = safe_u32_to_i32(800, 500, 50, "test");
        if result != 500 {
            bail!("Expected 500, got {result}");
        }
        Ok(())
    }

    #[test]
    fn test_safe_i32_to_u32_positive() -> Result<()> {
        let result = safe_i32_to_u32(100, 50, "test");
        if result != 100 {
            bail!("Expected 100, got {result}");
        }
        Ok(())
    }

    #[test]
    fn test_safe_i32_to_u32_negative_uses_default() -> Result<()> {
        let result = safe_i32_to_u32(-1, 180, "test");
        if result != 180 {
            bail!("Expected 180, got {result}");
        }
        Ok(())
    }
}
