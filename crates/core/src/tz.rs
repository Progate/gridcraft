//! The local time zone's offset from UTC, for "now" (TODAY, NOW, dates typed without a year,
//! dynamic date filters): Excel reads the computer's local clock, so before 09:00 in Japan it
//! must already be today there, not yesterday in UTC.
//!
//! A `TZ` environment variable wins everywhere it can be read: a POSIX string (`JST-9`, `UTC0`,
//! `EST5EDT,M3.2.0,M11.1.0`) or a zone name with a readable TZif file under `TZDIR` or
//! `/usr/share/zoneinfo` (`Asia/Tokyo`). BrowserOS (WASI, which has no time zone API) passes the
//! browser's zone that way. Without `TZ`, native builds ask the OS through chrono, the web build
//! asks the browser, and WASI stays on UTC.

/// Seconds east of UTC that local wall-clock time is at the instant `utc_ms` (Unix milliseconds).
pub fn local_offset_seconds(utc_ms: f64) -> i32 {
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    if let Some(tz) = std::env::var("TZ").ok().filter(|tz| !tz.trim().is_empty())
        && let Some(offset) = offset_for_tz(&tz, utc_ms)
    {
        return offset;
    }
    system_offset_seconds(utc_ms)
}

/// The OS's local offset (chrono reads `/etc/localtime` on Unix and the system zone on Windows
/// and macOS).
#[cfg(not(target_arch = "wasm32"))]
fn system_offset_seconds(utc_ms: f64) -> i32 {
    use chrono::{Offset, TimeZone};
    chrono::DateTime::from_timestamp_millis(utc_ms as i64)
        .map_or(0, |utc| chrono::Local.offset_from_utc_datetime(&utc.naive_utc()).fix().local_minus_utc())
}

/// The browser's offset at that instant (`getTimezoneOffset` is UTC minus local, in minutes).
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn system_offset_seconds(utc_ms: f64) -> i32 {
    let minutes = js_sys::Date::new(&js_sys::Number::from(utc_ms).into()).get_timezone_offset();
    -(minutes * 60.0) as i32
}

/// WASI has no zone API: UTC unless `TZ` says otherwise.
#[cfg(all(target_arch = "wasm32", not(target_os = "unknown")))]
fn system_offset_seconds(_utc_ms: f64) -> i32 {
    0
}

/// The offset a `TZ` value gives at `utc_ms`: a zone name with a TZif file, else a POSIX string;
/// `None` when it is neither.
pub fn offset_for_tz(tz: &str, utc_ms: f64) -> Option<i32> {
    let tz = tz.trim();
    let tz = tz.strip_prefix(':').unwrap_or(tz);
    if let Some(data) = zone_file(tz) {
        return tzif_offset(&data, utc_ms);
    }
    PosixTz::parse(tz)?.offset_at(utc_ms)
}

/// The TZif file of the zone `name` (`Asia/Tokyo`), from `TZDIR` or `/usr/share/zoneinfo`.
fn zone_file(name: &str) -> Option<Vec<u8>> {
    // A name only: no absolute path or `..` out of the zone directory.
    if name.is_empty() || name.contains("..") || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '+')) {
        return None;
    }
    let dirs = std::env::var("TZDIR").ok().into_iter().chain(["/usr/share/zoneinfo".to_string()]);
    for dir in dirs {
        let path = std::path::Path::new(&dir).join(name.trim_start_matches('/'));
        if let Ok(data) = std::fs::read(&path)
            && data.starts_with(b"TZif")
        {
            return Some(data);
        }
    }
    None
}

/// The offset a TZif file (RFC 8536) gives at `utc_ms`: the local time type of the last
/// transition at or before it; past the last transition, the footer's POSIX rule when there is
/// one (files that list transitions only up to now rely on it for daylight saving time).
pub fn tzif_offset(data: &[u8], utc_ms: f64) -> Option<i32> {
    let header = |at: usize| -> Option<[usize; 6]> {
        let counts = data.get(at + 20..at + 44)?;
        let n = |i: usize| counts.get(i * 4..i * 4 + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
        Some([n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, n(5)?])
    };
    let version = *data.get(4)?;
    let [isut, isstd, leap, time, types, chars] = header(0)?;
    // Version 2+ repeats the data with 64-bit times after the version 1 block, then a footer.
    let (start, time_size, counts) = if version >= b'2' {
        let v1_len = 44 + time * 4 + time + types * 6 + chars + leap * 8 + isstd + isut;
        (v1_len, 8, header(v1_len)?)
    } else {
        (0, 4, [isut, isstd, leap, time, types, chars])
    };
    let [isut, isstd, leap, time, types, chars] = counts;
    let times_at = start + 44;
    let indices_at = times_at + time * time_size;
    let types_at = indices_at + time;
    let utc = (utc_ms / 1000.0).floor() as i64;
    let transition = |i: usize| -> Option<i64> {
        let b = data.get(times_at + i * time_size..times_at + (i + 1) * time_size)?;
        Some(if time_size == 8 {
            i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        } else {
            i64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        })
    };
    let type_offset = |t: usize| -> Option<i32> {
        let b = data.get(types_at + t * 6..types_at + t * 6 + 4)?;
        Some(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    // The last transition at or before `utc` (binary search: transitions are sorted).
    let (mut lo, mut hi) = (0, time);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if transition(mid)? <= utc { lo = mid + 1 } else { hi = mid }
    }
    if lo == time && version >= b'2' {
        let footer_at = types_at + types * 6 + chars + leap * 12 + isstd + isut;
        let footer = data.get(footer_at..).and_then(|f| std::str::from_utf8(f).ok()).unwrap_or("");
        if let Some(rule) = footer.trim_matches('\n').lines().next().and_then(PosixTz::parse) {
            return rule.offset_at(utc_ms);
        }
    }
    let index = if lo == 0 { 0 } else { usize::from(*data.get(indices_at + lo - 1)?) };
    type_offset(index)
}

/// A POSIX `TZ` string: standard offset, and the daylight saving offset and rules if any
/// (`EST5EDT,M3.2.0,M11.1.0`). Offsets are stored east of UTC, the opposite of POSIX's sign.
#[derive(Debug, PartialEq)]
struct PosixTz {
    std: i32,
    dst: Option<(i32, Rule, Rule)>,
}

/// When daylight saving time starts or ends: `Mm.w.d/time` (month, week 1–5 with 5 the last,
/// weekday 0 = Sunday) at local seconds.
#[derive(Debug, PartialEq)]
struct Rule {
    month: u32,
    week: u32,
    weekday: u32,
    time: i32,
}

impl PosixTz {
    fn parse(s: &str) -> Option<PosixTz> {
        let mut rest = s;
        name(&mut rest)?;
        let std = -offset(&mut rest)?;
        if rest.is_empty() {
            return Some(PosixTz { std, dst: None });
        }
        name(&mut rest)?;
        // Daylight time is an hour ahead unless it says otherwise.
        let dst = if rest.starts_with(',') { std + 3600 } else { -offset(&mut rest)? };
        // Without rules POSIX leaves the dates to the implementation; the US ones are the usual default.
        let (start, end) = match rest.strip_prefix(',') {
            Some(rules) => {
                let (a, b) = rules.split_once(',')?;
                (Rule::parse(a)?, Rule::parse(b)?)
            }
            None if rest.is_empty() => (Rule::parse("M3.2.0")?, Rule::parse("M11.1.0")?),
            None => return None,
        };
        Some(PosixTz { std, dst: Some((dst, start, end)) })
    }

    fn offset_at(&self, utc_ms: f64) -> Option<i32> {
        let Some((dst, start, end)) = &self.dst else { return Some(self.std) };
        let utc = (utc_ms / 1000.0).floor() as i64;
        let year = chrono::DateTime::from_timestamp(utc + i64::from(self.std), 0).map(|t| chrono::Datelike::year(&t))?;
        // The switch to daylight time happens at standard local time, the switch back at daylight local time.
        let starts = start.utc_seconds(year, self.std)?;
        let ends = end.utc_seconds(year, *dst)?;
        let in_dst = if starts < ends { (starts..ends).contains(&utc) } else { utc >= starts || utc < ends };
        Some(if in_dst { *dst } else { self.std })
    }
}

impl Rule {
    fn parse(s: &str) -> Option<Rule> {
        let (date, time) = s.split_once('/').map_or((s, None), |(d, t)| (d, Some(t)));
        let mut parts = date.strip_prefix('M')?.split('.').map(|p| p.parse::<u32>().ok());
        let (month, week, weekday) = (parts.next()??, parts.next()??, parts.next()??);
        if !(1..=12).contains(&month) || !(1..=5).contains(&week) || weekday > 6 || parts.next().is_some() {
            return None;
        }
        let time = match time {
            Some(t) => {
                let mut t = t;
                hms(&mut t)?
            }
            None => 7200,
        };
        Some(Rule { month, week, weekday, time })
    }

    /// The instant (Unix seconds) the rule fires in `year`, its time read at UTC offset `offset`.
    fn utc_seconds(&self, year: i32, offset: i32) -> Option<i64> {
        let first = chrono::NaiveDate::from_ymd_opt(year, self.month, 1)?;
        let first_weekday = chrono::Datelike::weekday(&first).num_days_from_sunday();
        let mut day = 1 + (self.weekday + 7 - first_weekday) % 7 + (self.week - 1) * 7;
        let days_in_month = chrono::NaiveDate::from_ymd_opt(year, self.month, 1)?.checked_add_months(chrono::Months::new(1))?.pred_opt()?;
        while day > chrono::Datelike::day(&days_in_month) {
            day -= 7;
        }
        let date = chrono::NaiveDate::from_ymd_opt(year, self.month, day)?;
        let midnight = date.and_hms_opt(0, 0, 0)?.and_utc().timestamp();
        Some(midnight + i64::from(self.time) - i64::from(offset))
    }
}

/// Skips a zone abbreviation: letters (`JST`), or anything in angle brackets (`<+09>`).
fn name(s: &mut &str) -> Option<()> {
    let len =
        if let Some(inner) = s.strip_prefix('<') { inner.find('>')? + 2 } else { s.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(s.len()) };
    if len < 3 {
        return None;
    }
    *s = s.get(len..)?;
    Some(())
}

/// A POSIX offset (`-9`, `5`, `+05:30`) in seconds, as written (west of UTC positive).
fn offset(s: &mut &str) -> Option<i32> {
    let sign = if let Some(rest) = s.strip_prefix('-') {
        *s = rest;
        -1
    } else {
        *s = s.strip_prefix('+').unwrap_or(s);
        1
    };
    Some(sign * hms(s)?)
}

/// `h[:mm[:ss]]` in seconds, consumed from the front of `s`.
fn hms(s: &mut &str) -> Option<i32> {
    let mut total = 0;
    for (i, unit) in [3600, 60, 1].into_iter().enumerate() {
        if i > 0 {
            let Some(rest) = s.strip_prefix(':') else { break };
            *s = rest;
        }
        let digits = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        if digits == 0 {
            return None;
        }
        total += s.get(..digits)?.parse::<i32>().ok()? * unit;
        *s = s.get(digits..)?;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-12 23:30 UTC, already the 13th in Japan.
    const LATE_UTC: f64 = 1_794_526_200_000.0;

    #[test]
    fn posix_strings_give_their_standard_offset() {
        assert_eq!(offset_for_tz("JST-9", LATE_UTC), Some(9 * 3600));
        assert_eq!(offset_for_tz("UTC0", LATE_UTC), Some(0));
        assert_eq!(offset_for_tz("<+0530>-5:30", LATE_UTC), Some(5 * 3600 + 1800));
        assert_eq!(offset_for_tz(":EST+5", LATE_UTC), Some(-5 * 3600));
        assert_eq!(offset_for_tz("nonsense", LATE_UTC), None);
        assert_eq!(offset_for_tz("", LATE_UTC), None);
    }

    #[test]
    fn posix_daylight_rules_switch_on_their_dates() {
        let us = "EST5EDT,M3.2.0,M11.1.0";
        // 2026: DST runs from 8 March 07:00 UTC to 1 November 06:00 UTC.
        let utc = |y, m, d, h| chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, 0, 0).unwrap().and_utc().timestamp_millis() as f64;
        assert_eq!(offset_for_tz(us, utc(2026, 1, 15, 12)), Some(-5 * 3600));
        assert_eq!(offset_for_tz(us, utc(2026, 3, 8, 6)), Some(-5 * 3600));
        assert_eq!(offset_for_tz(us, utc(2026, 3, 8, 7)), Some(-4 * 3600));
        assert_eq!(offset_for_tz(us, utc(2026, 7, 1, 0)), Some(-4 * 3600));
        assert_eq!(offset_for_tz(us, utc(2026, 11, 1, 5)), Some(-4 * 3600));
        assert_eq!(offset_for_tz(us, utc(2026, 11, 1, 6)), Some(-5 * 3600));
        // Southern hemisphere: daylight time spans the new year.
        let au = "AEST-10AEDT,M10.1.0,M4.1.0/3";
        assert_eq!(offset_for_tz(au, utc(2026, 1, 15, 0)), Some(11 * 3600));
        assert_eq!(offset_for_tz(au, utc(2026, 7, 15, 0)), Some(10 * 3600));
    }

    /// Zone names read the system's TZif files; skipped where there are none.
    #[test]
    fn zone_names_read_tzif_files() {
        if zone_file("Asia/Tokyo").is_none() {
            return;
        }
        assert_eq!(offset_for_tz("Asia/Tokyo", LATE_UTC), Some(9 * 3600));
        assert_eq!(offset_for_tz(":Asia/Tokyo", 0.0), Some(9 * 3600));
        if zone_file("America/New_York").is_some() {
            let utc = |y, m, d| chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp_millis() as f64;
            assert_eq!(offset_for_tz("America/New_York", utc(2026, 1, 15)), Some(-5 * 3600));
            assert_eq!(offset_for_tz("America/New_York", utc(2026, 7, 15)), Some(-4 * 3600));
            // Far past the file's transitions: the footer rule.
            assert_eq!(offset_for_tz("America/New_York", utc(2090, 7, 15)), Some(-4 * 3600));
        }
        assert_eq!(offset_for_tz("../../etc/passwd", 0.0), None);
    }
}
