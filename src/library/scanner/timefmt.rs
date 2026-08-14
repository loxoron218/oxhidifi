//! UTC timestamp and sample-rate formatting for scan metadata.

use std::time::{SystemTime, UNIX_EPOCH};

/// Format sample rate for display in Hz.
///
/// Converts to kHz-style value: 44100 → "44.1", 48000 → "48".
#[must_use]
pub fn format_sample_rate(hz: i32) -> String {
    if hz % 1000 == 0 {
        (hz / 1000).to_string()
    } else {
        format!("{:.1}", f64::from(hz) / 1000.0)
    }
}

/// Get the current UTC time as an RFC 3339 formatted string.
#[must_use]
pub fn utc_now_rfc3339() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();

    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since UNIX epoch to (year, month, day).
///
/// `days` is bounded by wall-clock time (`secs / 86_400`, ~2×10⁴ today), so no
/// intermediate in the Hinnant algorithm can overflow for any reachable input;
/// the `wrapping_*` arithmetic is deliberate to avoid debug/release divergence.
const fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days.wrapping_add(719_468);
    let era = z / 146_097;
    let doe = z.wrapping_sub(era.wrapping_mul(146_097));
    let yoe = (doe
        .wrapping_sub(doe / 1460)
        .wrapping_add(doe / 36_524)
        .wrapping_sub(doe / 146_096))
        / 365;
    let y = yoe.wrapping_add(era.wrapping_mul(400));
    let doy = doe.wrapping_sub(
        365_u64
            .wrapping_mul(yoe)
            .wrapping_add(yoe / 4)
            .wrapping_sub(yoe / 100),
    );
    let mp = (5_u64.wrapping_mul(doy).wrapping_add(2)) / 153;
    let d = doy
        .wrapping_sub((153_u64.wrapping_mul(mp).wrapping_add(2)) / 5)
        .wrapping_add(1);
    let m = if mp < 10 {
        mp.wrapping_add(3)
    } else {
        mp.wrapping_sub(9)
    };
    let y = if m <= 2 { y.wrapping_add(1) } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use crate::library::scanner::timefmt::utc_now_rfc3339;

    #[test]
    fn utc_now_formats_rfc3339() {
        let stamp = utc_now_rfc3339();
        assert!(stamp.len() == 20, "stamp was: {stamp}");
        assert!(stamp.ends_with('Z'), "stamp was: {stamp}");
    }
}
