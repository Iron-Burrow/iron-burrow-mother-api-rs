/// Parsed time-of-day components extracted from an ASCII time string.
///
/// The `fraction` field contains the fractional second digits without the
/// leading `.`. For example, parsing `12:30:45.123` stores `"123"`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParsedTime {
    /// Hour of day, in the range `0..=23`.
    pub(super) hour: u32,
    /// Minute within the hour, in the range `0..=59`.
    pub(super) minute: u32,
    /// Second within the minute, in the range `0..=59`.
    pub(super) second: u32,
    /// Fractional second digits, without the leading `.`.
    pub(super) fraction: String,
}

/// Parses an ASCII time-of-day string into its components.
///
/// Accepts values shaped like `HH:MM:SS` with an optional fractional second
/// suffix, such as `HH:MM:SS.123`. The seconds field is fixed-width.
///
/// Returns `None` when:
///
/// - the required `:` separators are missing;
/// - any numeric component contains non-ASCII digits;
/// - the fractional part is empty after `.`;
/// - the hour, minute, or second is outside its valid range.
///
/// This parser does not allocate except when preserving the fractional
/// second digits.
pub(super) fn parse_time(value: &str) -> Option<ParsedTime> {
    let bytes = value.as_bytes();
    if bytes.len() < 8 || bytes[2] != b':' || bytes[5] != b':' {
        return None;
    }

    let hour = parse_ascii_u32(&bytes[0..2])?;
    let minute = parse_ascii_u32(&bytes[3..5])?;
    let seconds_and_fraction = &bytes[6..];
    let (second, fraction) = if let Some(dot_index) = seconds_and_fraction
        .iter()
        .position(|character| *character == b'.')
    {
        if dot_index != 2 {
            return None;
        }

        let second = &seconds_and_fraction[..dot_index];
        let fraction = &seconds_and_fraction[dot_index + 1..];
        if fraction.is_empty() || !fraction.iter().all(|character| character.is_ascii_digit()) {
            return None;
        }

        (
            parse_ascii_u32(second)?,
            String::from_utf8(fraction.to_vec()).ok()?,
        )
    } else {
        if seconds_and_fraction.len() != 2 {
            return None;
        }

        (parse_ascii_u32(seconds_and_fraction)?, String::new())
    };

    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    Some(ParsedTime {
        hour,
        minute,
        second,
        fraction,
    })
}

/// Parses a non-empty ASCII digit slice into a `u32`.
///
/// Returns `None` if the slice is empty, contains non-ASCII digits, is not
/// valid UTF-8, or cannot fit into a `u32`.
pub(super) fn parse_ascii_u32(value: &[u8]) -> Option<u32> {
    if value.is_empty() || !value.iter().all(|character| character.is_ascii_digit()) {
        return None;
    }

    std::str::from_utf8(value).ok()?.parse::<u32>().ok()
}

/// Parses a non-empty ASCII digit slice into an `i32`.
///
/// Returns `None` if the slice is empty, contains non-ASCII digits, is not
/// valid UTF-8, or cannot fit into an `i32`.
///
/// This function only accepts unsigned decimal digits. It does not accept a
/// leading `+` or `-` sign.
pub(super) fn parse_ascii_i32(value: &[u8]) -> Option<i32> {
    if value.is_empty() || !value.iter().all(|character| character.is_ascii_digit()) {
        return None;
    }

    std::str::from_utf8(value).ok()?.parse::<i32>().ok()
}

/// Returns the number of days in a Gregorian calendar month.
///
/// Returns `0` when `month` is not in the range `1..=12`.
///
/// February is resolved using Gregorian leap-year rules.
pub(super) fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Returns whether a year is a Gregorian leap year.
///
/// A year is a leap year when it is divisible by 4, except years divisible
/// by 100 are not leap years unless they are also divisible by 400.
pub(super) fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Converts a Gregorian calendar date to a day offset from the Unix epoch.
///
/// The returned value is the number of days relative to `1970-01-01`.
/// Therefore:
///
/// - `1970-01-01` returns `0`;
/// - dates before the Unix epoch return negative values;
/// - dates after the Unix epoch return positive values.
///
/// The caller is expected to pass a valid Gregorian date. This function does
/// not validate whether `month` and `day` form a real date.
pub(super) fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let adjusted_year = year - i32::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year / 400
    } else {
        (adjusted_year - 399) / 400
    };
    let year_of_era = adjusted_year - era * 400;
    let month_prime = i32::try_from(month).unwrap() + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + i32::try_from(day).unwrap() - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    i64::from(era) * 146_097 + i64::from(day_of_era) - 719_468
}

#[cfg(test)]
mod tests;
