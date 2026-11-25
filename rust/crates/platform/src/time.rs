//! The machine's current UTC offset, for the timestamps of the game's `Client.txt` lines.
//!
//! The reference parses `YYYY/MM/DD HH:mm:ss` with `Date.parse`, which reads a slash-separated
//! date in the machine's zone (`poe2::client_log`'s documentation). The port makes the offset
//! explicit there, so a live run asks this module for it once.
//!
//! This is the offset *now*, not the offset that was in force when a given line was written:
//! a log that spans a daylight-saving change is an hour off for the other half. The XP tracker
//! (the first reader) never looks at `ts`, so the approximation only matters for the main
//! process events the port does not forward yet; `poe2::client_log::ClientLog::new` documents
//! the same caveat.

/// The local zone's offset east of UTC, in seconds (UTC+2 is `7200`).
///
/// Windows reads the current `TIME_ZONE_INFORMATION`, Unix `localtime_r`'s `tm_gmtoff`; a
/// platform without either reports UTC.
pub fn local_utc_offset_seconds() -> i32 {
    imp::local_utc_offset_seconds()
}

#[cfg(unix)]
mod imp {
    /// `localtime_r` of the current time, then its `tm_gmtoff` (seconds east of UTC).
    pub fn local_utc_offset_seconds() -> i32 {
        // SAFETY: `time` and `localtime_r` take plain C values; `now` and `tm` are owned by
        // this function and don't outlive the call.
        unsafe {
            let now = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&now, &mut tm).is_null() {
                return 0;
            }
            tm.tm_gmtoff as i32
        }
    }
}

#[cfg(windows)]
mod imp {
    use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};

    /// `TIME_ZONE_ID_DAYLIGHT` (2), which `windows` exports from `SystemServices` rather than
    /// `Time`.
    const TIME_ZONE_ID_DAYLIGHT: u32 = 2;

    /// `Bias` is minutes *west* of UTC; the daylight bias applies while daylight time is in
    /// force.
    pub fn local_utc_offset_seconds() -> i32 {
        let mut zone = TIME_ZONE_INFORMATION::default();
        // SAFETY: the call fills the structure this function owns.
        let state = unsafe { GetTimeZoneInformation(&mut zone) };
        let bias =
            if state == TIME_ZONE_ID_DAYLIGHT { zone.Bias + zone.DaylightBias } else { zone.Bias + zone.StandardBias };
        -(bias * 60)
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    pub fn local_utc_offset_seconds() -> i32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever the machine's zone, the offset is a whole number of quarter-hours: real zones
    /// are within ±14 h and no longer offset by seconds.
    #[test]
    fn the_offset_is_a_sane_quarter_hour() {
        let offset = local_utc_offset_seconds();
        assert!((-14 * 3600..=14 * 3600).contains(&offset), "{offset}");
        assert_eq!(offset % 900, 0, "{offset}");
    }
}
