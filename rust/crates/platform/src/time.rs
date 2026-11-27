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
    #[expect(unsafe_code, reason = "`libc`'s C time calls have no safe wrapper in this crate")]
    pub fn local_utc_offset_seconds() -> i32 {
        // SAFETY: `time` takes a plain `time_t` pointer; null asks for the current time, which
        // is always a valid request.
        let now = unsafe { libc::time(std::ptr::null_mut()) };
        // SAFETY: `libc::tm` is plain old data whose all-zero bit pattern is valid.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: `now` and `tm` are live locals of this call, and `localtime_r` writes only
        // into the structure it is given.
        if unsafe { libc::localtime_r(&raw const now, &raw mut tm) }.is_null() {
            return 0;
        }
        // A real zone is within ±14 h of UTC, so its offset cannot truncate.
        i32::try_from(tm.tm_gmtoff).unwrap_or(0)
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
    #[expect(
        unsafe_code,
        reason = "`GetTimeZoneInformation` fills a Win32 structure and has no safe wrapper in this crate"
    )]
    pub fn local_utc_offset_seconds() -> i32 {
        let mut zone = TIME_ZONE_INFORMATION::default();
        // SAFETY: the call fills the structure this function owns.
        let state = unsafe { GetTimeZoneInformation(&raw mut zone) };
        let bias = if state == TIME_ZONE_ID_DAYLIGHT {
            zone.Bias.saturating_add(zone.DaylightBias)
        } else {
            zone.Bias.saturating_add(zone.StandardBias)
        };
        bias.saturating_mul(60).saturating_neg()
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
