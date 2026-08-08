//! Calendar arithmetic for gantt charts.
//!
//! Gantt tasks are resolved to absolute instants at parse time (see
//! [`gantt`](super::gantt)), so this module turns `dateFormat` strings and duration
//! literals into seconds since the Unix epoch. Everything is UTC: Mermaid has no
//! timezone concept and a pager has no business inventing one.

use crate::error::MermaidError;

use super::lex;

/// Seconds in one day.
const DAY: i64 = 86_400;

/// Parses `text` according to a Mermaid `dateFormat` string.
///
/// Recognised tokens are `YYYY`, `YY`, `MM`, `DD`, `HH`, `mm`, `ss` and `X` (a Unix
/// timestamp in seconds); every other character must match literally. Numeric fields
/// accept fewer digits than the token suggests, because real diagrams write `2014-1-6`.
///
/// Returns `None` when `text` does not match the format, which the task parser uses to
/// tell a date apart from an id.
pub fn parse_date(text: &str, format: &str) -> Option<i64> {
    let text = text.trim();
    if format.trim() == "X" {
        return text.parse::<i64>().ok();
    }
    let mut fields = Fields::default();
    let mut input = text;
    let mut rest = format;
    while !rest.is_empty() {
        let (token, after) = next_token(rest);
        rest = after;
        let field = match token {
            "YYYY" => Some((&mut fields.year, 4)),
            "YY" => Some((&mut fields.short_year, 2)),
            "MM" | "M" => Some((&mut fields.month, 2)),
            "DD" | "D" => Some((&mut fields.day, 2)),
            "HH" | "H" => Some((&mut fields.hour, 2)),
            "mm" => Some((&mut fields.minute, 2)),
            "ss" => Some((&mut fields.second, 2)),
            _ => None,
        };
        match field {
            Some((slot, max_digits)) => {
                let digits = input
                    .char_indices()
                    .take_while(|(index, ch)| *index < max_digits && ch.is_ascii_digit())
                    .count();
                if digits == 0 {
                    return None;
                }
                *slot = input[..digits].parse().ok()?;
                input = &input[digits..];
            }
            None => {
                // A literal run: it must appear verbatim in the input.
                input = input.strip_prefix(token)?;
            }
        }
    }
    if !input.is_empty() {
        return None;
    }
    let year = if fields.year != 0 {
        fields.year
    } else {
        2000 + fields.short_year
    };
    to_epoch(year, fields.month.max(1), fields.day.max(1), fields)
}

/// The fields a `dateFormat` may fill in.
#[derive(Debug, Default, Clone, Copy)]
struct Fields {
    year: i64,
    short_year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
}

/// Splits the next format token (or literal run) off `format`.
fn next_token(format: &str) -> (&str, &str) {
    const TOKENS: [&str; 9] = ["YYYY", "YY", "MM", "DD", "HH", "mm", "ss", "M", "D"];
    for token in TOKENS {
        if let Some(rest) = format.strip_prefix(token) {
            return (token, rest);
        }
    }
    let end = format
        .char_indices()
        .find(|(index, _)| *index > 0 && TOKENS.iter().any(|t| format[*index..].starts_with(t)))
        .map_or(format.len(), |(index, _)| index);
    format.split_at(end)
}

/// Converts a validated calendar date and time to seconds since the Unix epoch.
fn to_epoch(year: i64, month: i64, day: i64, fields: Fields) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if fields.hour > 23 || fields.minute > 59 || fields.second > 59 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(days * DAY + fields.hour * 3600 + fields.minute * 60 + fields.second)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Parses a duration literal such as `3d`, `2w`, `12h`, `90m` or `1.5d`.
///
/// Returns `None` when `text` is not a duration at all, and an error when it is a
/// duration in a unit outside the supported set.
pub fn parse_duration(text: &str, line: usize) -> Result<Option<i64>, MermaidError> {
    let text = text.trim();
    let split = text
        .char_indices()
        .find(|(_, ch)| !(ch.is_ascii_digit() || *ch == '.'))
        .map_or(text.len(), |(index, _)| index);
    let (amount, unit) = text.split_at(split);
    if amount.is_empty() {
        return Ok(None);
    }
    let Ok(amount) = amount.parse::<f64>() else {
        return Ok(None);
    };
    if !amount.is_finite() || amount < 0.0 {
        return Ok(None);
    }
    let seconds = match unit.trim() {
        "ms" => 0.001,
        "s" => 1.0,
        "m" | "min" | "minute" | "minutes" => 60.0,
        "h" | "hour" | "hours" => 3600.0,
        "d" | "day" | "days" => DAY as f64,
        "w" | "week" | "weeks" => 7.0 * DAY as f64,
        "" => return Ok(None),
        other => {
            return Err(lex::unsupported(
                line,
                format!("duration unit `{other}`; use ms, s, m, h, d or w"),
            ));
        }
    };
    Ok(Some((amount * seconds).round() as i64))
}
