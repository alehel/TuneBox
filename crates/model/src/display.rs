//! Human-readable durations and sizes, in the style of the status bar and the Time column.

use std::time::Duration;

/// "3:07" or "1:02:03": the Time column and the LCD.
pub fn format_clock(d: Duration) -> String {
    let total = d.as_secs();
    let (hours, minutes, seconds) = (total / 3600, (total / 60) % 60, total % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes}:{seconds:02}") }
}

/// The status bar's total: "42 minutes", "3.4 hours", "2.1 days".
pub fn format_total_time(d: Duration) -> String {
    let secs = d.as_secs_f64();
    const MINUTE: f64 = 60.0;
    const HOUR: f64 = 3600.0;
    const DAY: f64 = 86_400.0;
    if secs < HOUR {
        let minutes = (secs / MINUTE).round();
        if minutes == 1.0 { "1 minute".to_owned() } else { format!("{minutes} minutes") }
    } else if secs < DAY {
        format!("{:.1} hours", secs / HOUR)
    } else {
        format!("{:.1} days", secs / DAY)
    }
}

/// "812 KB", "4.2 MB", "31.7 GB" (decimal units, as storage is sold).
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    let name = UNITS.get(unit).copied().unwrap_or("TB");
    match unit {
        0 => format!("{bytes} {name}"),
        1 => format!("{value:.0} {name}"),
        _ => format!("{value:.1} {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock() {
        assert_eq!(format_clock(Duration::from_secs(187)), "3:07");
        assert_eq!(format_clock(Duration::from_secs(3723)), "1:02:03");
        assert_eq!(format_clock(Duration::ZERO), "0:00");
    }

    #[test]
    fn totals() {
        assert_eq!(format_total_time(Duration::from_secs(60)), "1 minute");
        assert_eq!(format_total_time(Duration::from_secs(42 * 60)), "42 minutes");
        assert_eq!(format_total_time(Duration::from_secs(5 * 3600)), "5.0 hours");
        assert_eq!(format_total_time(Duration::from_secs(3 * 86_400)), "3.0 days");
    }

    #[test]
    fn bytes() {
        assert_eq!(format_bytes(999), "999 bytes");
        assert_eq!(format_bytes(812_000), "812 KB");
        assert_eq!(format_bytes(4_200_000), "4.2 MB");
        assert_eq!(format_bytes(31_700_000_000), "31.7 GB");
    }
}
