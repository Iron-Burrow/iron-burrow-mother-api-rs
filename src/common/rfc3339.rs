use super::time::{
    days_from_civil, days_in_month, parse_ascii_i32, parse_ascii_u32, parse_time, ParsedTime,
};
use std::cmp::Ordering;

/// Parsed RFC 3339 timestamp normalized for comparison.
///
/// `epoch_seconds` stores the timestamp normalized to seconds relative to the
/// Unix epoch, after applying the UTC offset.
///
/// `fraction` stores the fractional second digits without the leading `.`.
/// It is kept separately so timestamps with the same epoch second can still be
/// ordered by subsecond precision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedRfc3339 {
    epoch_seconds: i64,
    fraction: String,
}

/// Parses an RFC 3339 timestamp into a normalized comparable representation.
///
/// Accepts timestamps shaped like:
///
/// - `YYYY-MM-DDTHH:MM:SSZ`
/// - `YYYY-MM-DDTHH:MM:SS.sssZ`
/// - `YYYY-MM-DDTHH:MM:SS+HH:MM`
/// - `YYYY-MM-DDTHH:MM:SS-HH:MM`
///
/// The returned `epoch_seconds` value is normalized to UTC by applying the
/// timestamp offset. Fractional second digits are preserved separately.
///
/// Returns `None` when the timestamp has an invalid date, time, offset,
/// separator layout, or when epoch-second arithmetic overflows.
///
/// Leap seconds are not accepted because `parse_time` only accepts seconds in
/// the range `0..=59`.
pub(crate) fn parse_rfc3339(value: &str) -> Option<ParsedRfc3339> {
    let (date, time_and_offset) = value.split_once('T')?;
    if time_and_offset.contains('T') {
        return None;
    }

    let (year, month, day) = parse_date(date)?;
    let (time, offset_seconds) = parse_time_and_offset(time_and_offset)?;
    let days = days_from_civil(year, month, day);
    let epoch_seconds = days
        .checked_mul(86_400)?
        .checked_add(i64::from(time.hour) * 3_600)?
        .checked_add(i64::from(time.minute) * 60)?
        .checked_add(i64::from(time.second))?
        .checked_sub(offset_seconds)?;

    Some(ParsedRfc3339 {
        epoch_seconds,
        fraction: time.fraction,
    })
}

/// Compares two parsed RFC 3339 timestamps chronologically.
///
/// Comparison first uses normalized epoch seconds. If both timestamps fall
/// within the same second, fractional seconds are compared numerically with
/// implicit trailing zeroes.
///
/// For example, `.1`, `.10`, and `.100` compare as equal.
pub(crate) fn compare_rfc3339(left: &ParsedRfc3339, right: &ParsedRfc3339) -> Ordering {
    left.epoch_seconds
        .cmp(&right.epoch_seconds)
        .then_with(|| compare_fractional_seconds(&left.fraction, &right.fraction))
}

/// Compares fractional second strings as decimal subsecond values.
///
/// Missing digits are treated as trailing zeroes, so `"1"` and `"100"` are
/// considered equal. The inputs are expected to contain ASCII digits only.
fn compare_fractional_seconds(left: &str, right: &str) -> Ordering {
    let len = left.len().max(right.len());

    for index in 0..len {
        let left_digit = left.as_bytes().get(index).copied().unwrap_or(b'0');
        let right_digit = right.as_bytes().get(index).copied().unwrap_or(b'0');
        match left_digit.cmp(&right_digit) {
            Ordering::Equal => continue,
            ordering => return ordering,
        }
    }

    Ordering::Equal
}

/// Parses a strict RFC 3339 calendar date.
///
/// Accepts only the `YYYY-MM-DD` shape.
///
/// Returns `None` when the layout is invalid, the month is outside `1..=12`,
/// or the day does not exist in that month and year.
fn parse_date(value: &str) -> Option<(i32, u32, u32)> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }

    let year = parse_ascii_i32(&bytes[0..4])?;
    let month = parse_ascii_u32(&bytes[5..7])?;
    let day = parse_ascii_u32(&bytes[8..10])?;
    if !(1..=12).contains(&month) {
        return None;
    }

    let max_day = days_in_month(year, month);
    if day == 0 || day > max_day {
        return None;
    }

    Some((year, month, day))
}

/// Splits and parses the time-of-day and UTC offset portion of an RFC 3339 timestamp.
///
/// Accepts either a `Z` suffix for UTC or a signed numeric offset like
/// `+06:00` or `-06:00`.
///
/// Returns the parsed local time and the offset in seconds.
fn parse_time_and_offset(value: &str) -> Option<(ParsedTime, i64)> {
    let (time, offset_seconds) = if let Some(time) = value.strip_suffix('Z') {
        (time, 0)
    } else {
        let offset_start = value.rfind(['+', '-'])?;
        let time = &value[..offset_start];
        let offset = &value[offset_start..];
        let offset_seconds = parse_offset(offset)?;
        (time, offset_seconds)
    };

    let parsed_time = parse_time(time)?;

    Some((parsed_time, offset_seconds))
}

/// Parses an RFC 3339 numeric UTC offset.
///
/// Accepts offsets shaped like `+HH:MM` or `-HH:MM`.
///
/// Returns the offset in seconds. Positive offsets are east of UTC and
/// negative offsets are west of UTC.
///
/// Returns `None` when the layout is invalid or when the hour or minute
/// component is outside its valid range.
fn parse_offset(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 6 || bytes[3] != b':' {
        return None;
    }

    let sign = match bytes[0] {
        b'+' => 1_i64,
        b'-' => -1_i64,
        _ => return None,
    };
    let hours = parse_ascii_u32(&bytes[1..3])?;
    let minutes = parse_ascii_u32(&bytes[4..6])?;
    if hours > 23 || minutes > 59 {
        return None;
    }

    Some(sign * (i64::from(hours) * 3_600 + i64::from(minutes) * 60))
}

#[cfg(test)]
mod tests;
