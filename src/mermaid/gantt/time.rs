//! Proleptic Gregorian calendar arithmetic and `axisFormat` rendering.
//!
//! Gantt tasks arrive with their dates already resolved to seconds since the Unix
//! epoch (see [`GanttTask`](crate::mermaid::ast::GanttTask)), so the renderer only
//! needs two things the standard library does not offer: turning an instant back into
//! a civil date so a tick can be labelled, and stepping forward by whole calendar
//! months so ticks land on month and year boundaries.
//!
//! The conversions are Howard Hinnant's `days_from_civil` / `civil_from_days`
//! algorithms, which are exact for the whole range of `i64` days and need no
//! lookup tables. Everything here is UTC; Mermaid gantt charts carry no timezone.

/// Seconds in one minute.
pub const MINUTE: i64 = 60;
/// Seconds in one hour.
pub const HOUR: i64 = 60 * MINUTE;
/// Seconds in one day.
pub const DAY: i64 = 24 * HOUR;
/// Seconds in one week.
pub const WEEK: i64 = 7 * DAY;

/// The earliest instant a chart may refer to: `0000-01-01T00:00:00Z`.
pub const MIN_INSTANT: i64 = -62_167_219_200;
/// The latest instant a chart may refer to: `9999-12-31T23:59:59Z`.
pub const MAX_INSTANT: i64 = 253_402_300_799;
/// The longest span any chart can cover, and therefore the longest usable duration.
pub const MAX_SPAN: i64 = MAX_INSTANT - MIN_INSTANT;

/// Brings an instant inside the range charts are drawn in.
///
/// `dateFormat X` hands the parser an arbitrary `i64`, and a duration literal can be
/// astronomically large, so every instant entering the timeline passes through here.
/// Downstream arithmetic — spans, tick steps, column mapping — is then guaranteed to
/// stay far inside `i64`, which is what stops a diagram from panicking the pager
/// (design spec §12).
pub fn clamp_instant(seconds: i64) -> i64 {
    seconds.clamp(MIN_INSTANT, MAX_INSTANT)
}

/// Brings a duration inside the range charts are drawn in.
pub fn clamp_span(seconds: i64) -> i64 {
    seconds.clamp(0, MAX_SPAN)
}

/// A civil date and time of day in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    /// Proleptic Gregorian year; may be negative.
    pub year: i64,
    /// Month of year, `1..=12`.
    pub month: u32,
    /// Day of month, `1..=31`.
    pub day: u32,
    /// Hour of day, `0..=23`.
    pub hour: u32,
    /// Minute of hour, `0..=59`.
    pub minute: u32,
    /// Second of minute, `0..=59`.
    pub second: u32,
    /// Day of year, `1..=366`.
    pub year_day: u32,
}

/// Abbreviated English month names, indexed by `month - 1`.
const MONTHS_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Full English month names, indexed by `month - 1`.
const MONTHS_LONG: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

impl DateTime {
    /// Decomposes an instant given in seconds since the Unix epoch.
    pub fn from_epoch(seconds: i64) -> Self {
        let days = seconds.div_euclid(DAY);
        let rest = seconds.rem_euclid(DAY);
        let (year, month, day) = civil_from_days(days);
        let year_start = days_from_civil(year, 1, 1);
        Self {
            year,
            month,
            day,
            hour: (rest / HOUR) as u32,
            minute: ((rest % HOUR) / MINUTE) as u32,
            second: (rest % MINUTE) as u32,
            // `days` is never before the start of its own year, so this fits `u32`.
            year_day: (days - year_start + 1).clamp(1, 366) as u32,
        }
    }

    /// Renders this instant with a `strftime`-style format string.
    ///
    /// Mermaid's `axisFormat` uses the d3 subset of `strftime`. The specifiers below
    /// are honoured; any other `%x` sequence is emitted verbatim, which is the least
    /// surprising fallback for a format string we do not understand.
    ///
    /// | Specifier | Meaning |
    /// |---|---|
    /// | `%Y` / `%y` | four-digit / two-digit year |
    /// | `%m` / `%-m` | zero-padded / unpadded month number |
    /// | `%d` / `%-d` / `%e` | zero-padded / unpadded / space-padded day |
    /// | `%b` / `%B` | abbreviated / full month name |
    /// | `%H` / `%M` / `%S` | zero-padded hour, minute, second |
    /// | `%j` | day of year |
    /// | `%%` | a literal percent sign |
    pub fn format(&self, pattern: &str) -> String {
        let month = usize::from(self.month.clamp(1, 12) as u8) - 1;
        let mut out = String::with_capacity(pattern.len() + 8);
        let mut rest = pattern;
        while let Some(at) = rest.find('%') {
            out.push_str(&rest[..at]);
            rest = &rest[at..];
            let (spec, len) = specifier(rest);
            match spec {
                "%Y" => out.push_str(&self.year.to_string()),
                "%y" => out.push_str(&format!("{:02}", self.year.rem_euclid(100))),
                "%m" => out.push_str(&format!("{:02}", self.month)),
                "%-m" => out.push_str(&self.month.to_string()),
                "%d" => out.push_str(&format!("{:02}", self.day)),
                "%-d" => out.push_str(&self.day.to_string()),
                "%e" => out.push_str(&format!("{:2}", self.day)),
                "%b" => out.push_str(MONTHS_SHORT[month]),
                "%B" => out.push_str(MONTHS_LONG[month]),
                "%H" => out.push_str(&format!("{:02}", self.hour)),
                "%M" => out.push_str(&format!("{:02}", self.minute)),
                "%S" => out.push_str(&format!("{:02}", self.second)),
                "%j" => out.push_str(&format!("{:03}", self.year_day)),
                "%%" => out.push('%'),
                other => out.push_str(other),
            }
            rest = &rest[len..];
        }
        out.push_str(rest);
        out
    }
}

/// Splits the specifier at the start of `rest`, returning it and its byte length.
///
/// `rest` always begins with `%`. A trailing lone `%` is returned as itself.
fn specifier(rest: &str) -> (&str, usize) {
    for len in [3usize, 2] {
        if rest.is_char_boundary(len) && rest.len() >= len {
            let candidate = &rest[..len];
            if len == 3 && candidate.starts_with("%-") {
                return (candidate, len);
            }
            if len == 2 && !candidate.ends_with('-') {
                return (candidate, len);
            }
        }
    }
    (rest, rest.len())
}

/// Days since the Unix epoch for a proleptic Gregorian civil date.
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let month = i64::from(month.clamp(1, 12));
    let day = i64::from(day.clamp(1, 31));
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The proleptic Gregorian civil date of a day count since the Unix epoch.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    // Every intermediate is bounded by the calendar, so the casts cannot truncate.
    (year + i64::from(month <= 2), month as u32, day as u32)
}

/// The instant `months` whole calendar months after the start of `seconds`' month.
///
/// Used to place month, quarter and year ticks on calendar boundaries rather than on
/// multiples of an average month length.
pub fn add_months(seconds: i64, months: i64) -> i64 {
    let at = DateTime::from_epoch(seconds);
    let total = at.year * 12 + i64::from(at.month) - 1 + months;
    let year = total.div_euclid(12);
    // `rem_euclid(12)` is in `0..=11`, so the `+ 1` keeps this a valid month number.
    let month = (total.rem_euclid(12) + 1) as u32;
    days_from_civil(year, month, 1) * DAY
}

/// The start of the calendar month containing `seconds`.
pub fn month_start(seconds: i64) -> i64 {
    let at = DateTime::from_epoch(seconds);
    days_from_civil(at.year, at.month, 1) * DAY
}

/// The first multiple of `step` at or after `seconds`, measured from the epoch.
pub fn align_up(seconds: i64, step: i64) -> i64 {
    if step <= 0 {
        return seconds;
    }
    let floor = seconds.div_euclid(step) * step;
    if floor == seconds {
        floor
    } else {
        floor + step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_conversions_round_trip() {
        for days in [-100_000i64, -1, 0, 1, 19_000, 100_000] {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days, "day {days}");
        }
    }

    #[test]
    fn epoch_decomposes_correctly() {
        let at = DateTime::from_epoch(0);
        assert_eq!((at.year, at.month, at.day), (1970, 1, 1));
        assert_eq!((at.hour, at.minute, at.second), (0, 0, 0));
        assert_eq!(at.year_day, 1);
    }

    #[test]
    fn leap_day_is_handled() {
        let at = DateTime::from_epoch(days_from_civil(2024, 2, 29) * DAY + 13 * HOUR + 45 * MINUTE);
        assert_eq!((at.year, at.month, at.day), (2024, 2, 29));
        assert_eq!(at.format("%Y-%m-%d %H:%M"), "2024-02-29 13:45");
        assert_eq!(at.format("%e %b %y"), "29 Feb 24");
        assert_eq!(at.year_day, 60);
    }

    #[test]
    fn unknown_specifiers_survive_verbatim() {
        let at = DateTime::from_epoch(0);
        assert_eq!(at.format("%Q/%%/%B"), "%Q/%/January");
        assert_eq!(at.format("no specifiers"), "no specifiers");
        assert_eq!(at.format("trailing %"), "trailing %");
    }

    #[test]
    fn month_arithmetic_lands_on_boundaries() {
        let march = days_from_civil(2024, 3, 17) * DAY;
        assert_eq!(month_start(march), days_from_civil(2024, 3, 1) * DAY);
        assert_eq!(add_months(march, 1), days_from_civil(2024, 4, 1) * DAY);
        assert_eq!(add_months(march, 10), days_from_civil(2025, 1, 1) * DAY);
        assert_eq!(add_months(march, -3), days_from_civil(2023, 12, 1) * DAY);
    }

    #[test]
    fn align_up_snaps_forward() {
        assert_eq!(align_up(0, DAY), 0);
        assert_eq!(align_up(1, DAY), DAY);
        assert_eq!(align_up(DAY, DAY), DAY);
        assert_eq!(align_up(5, 0), 5);
    }
}
