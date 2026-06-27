#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParsedTime {
    pub(super) hour: u32,
    pub(super) minute: u32,
    pub(super) second: u32,
    pub(super) fraction: String,
}

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

pub(super) fn parse_ascii_u32(value: &[u8]) -> Option<u32> {
    if value.is_empty() || !value.iter().all(|character| character.is_ascii_digit()) {
        return None;
    }

    std::str::from_utf8(value).ok()?.parse::<u32>().ok()
}

pub(super) fn parse_ascii_i32(value: &[u8]) -> Option<i32> {
    if value.is_empty() || !value.iter().all(|character| character.is_ascii_digit()) {
        return None;
    }

    std::str::from_utf8(value).ok()?.parse::<i32>().ok()
}

pub(super) fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

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
