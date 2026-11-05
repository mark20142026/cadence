//! Local dates and times for the servers' Unix time stamps (the thread view's "Nov 15, 2023"
//! separators and "1:13 AM" message times, in the system's time zone as the game shows them).

/// This system's offset from UTC at `ts`, in seconds (east positive).
#[cfg(unix)]
pub fn local_offset(ts: i64) -> i64 {
    let t = ts as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return 0;
    }
    tm.tm_gmtoff as i64
}

#[cfg(windows)]
pub fn local_offset(_ts: i64) -> i64 {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    let mut tz: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    // 2: daylight time now; the bias is minutes west of UTC
    let mode = unsafe { GetTimeZoneInformation(&mut tz) };
    let bias = tz.Bias + if mode == 2 { tz.DaylightBias } else { tz.StandardBias };
    -(bias as i64) * 60
}

#[cfg(target_arch = "wasm32")]
pub fn local_offset(ts: i64) -> i64 {
    // minutes west of UTC
    -(js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ts as f64 * 1000.0)).get_timezone_offset() as i64) * 60
}

#[cfg(not(any(unix, windows, target_arch = "wasm32")))]
pub fn local_offset(_ts: i64) -> i64 {
    0
}

/// (year, month 1–12, day, hour, minute) of `ts` shifted by `offset` seconds.
pub fn civil(ts: i64, offset: i64) -> (i64, u32, u32, u32, u32) {
    let t = ts + offset;
    let (days, secs) = (t.div_euclid(86400), t.rem_euclid(86400));
    // days since 1970-01-01 → date (Howard Hinnant's civil_from_days)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d, (secs / 3600) as u32, (secs % 3600 / 60) as u32)
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "Nov 15, 2023"
pub fn date_label(ts: i64, offset: i64) -> String {
    let (y, m, d, _, _) = civil(ts, offset);
    format!("{} {d}, {y}", MONTHS[(m - 1) as usize])
}

/// "1:13 AM"
pub fn time_label(ts: i64, offset: i64) -> String {
    let (_, _, _, h, min) = civil(ts, offset);
    let (h12, half) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{h12}:{min:02} {half}")
}

/// "2023-11-15 01:13:20" (the thread options' "Created:")
pub fn stamp_label(ts: i64, offset: i64) -> String {
    let (y, m, d, h, min) = civil(ts, offset);
    format!("{y}-{m:02}-{d:02} {h:02}:{min:02}:{:02}", (ts + offset).rem_euclid(60))
}

/// The local day of `ts` (for date separators).
pub fn day(ts: i64, offset: i64) -> i64 {
    (ts + offset).div_euclid(86400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_like_the_game() {
        // the capture's first message: 1700000000 shown as Nov 15, 2023 1:13 AM at UTC+3
        assert_eq!(date_label(1700000000, 3 * 3600), "Nov 15, 2023");
        assert_eq!(time_label(1700000000, 3 * 3600), "1:13 AM");
        assert_eq!(time_label(1700000000, 0), "10:13 PM");
        assert_eq!(civil(0, 0), (1970, 1, 1, 0, 0));
        assert_eq!(stamp_label(1700000000, 3 * 3600), "2023-11-15 01:13:20");
        assert_eq!(civil(951782400, 0), (2000, 2, 29, 0, 0));
    }
}
