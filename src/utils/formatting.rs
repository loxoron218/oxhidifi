/// Format a duration in seconds as H:MM:SS (e.g., 1:23:45)
pub(crate) fn format_duration_hms(total_seconds: u32) -> String {
    let h = total_seconds / 3600;
    let m = (total_seconds % 3600) / 60;
    let s = total_seconds % 60;
    format!("{:01}:{:02}:{:02}", h, m, s)
}

/// Format a duration in seconds as MM:SS (e.g., 03:45)
pub(crate) fn format_duration_mmss(secs: u32) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// Format bit depth and frequency as a string (e.g., "24-Bit/96 kHz")
pub(crate) fn format_bit_freq(bit: Option<u32>, freq: Option<u32>) -> String {
    let bit_str = bit.map(|b| format!("{}-Bit", b));
    let freq_str = freq.map(|f| {
        let khz = (f as f32) / 1000.0;
        if (khz - khz.floor()).abs() < 0.01 {
            format!("{:.0} kHz", khz)
        } else {
            format!("{:.1} kHz", khz)
        }
    });
    match (bit_str, freq_str) {
        (Some(b), Some(f)) => format!("{}/{}", b, f),
        (Some(b), None) => b,
        (None, Some(f)) => f,
        (None, None) => String::new(),
    }
}

/// Format frequency as kHz (e.g., "44.1")
pub(crate) fn format_freq_khz(freq: u32) -> String {
    let khz = (freq as f32) / 1000.0;
    if (khz - khz.floor()).abs() < 0.01 {
        format!("{:.0}", khz)
    } else {
        format!("{:.1}", khz)
    }
}
