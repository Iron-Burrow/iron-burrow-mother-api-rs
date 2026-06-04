use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

pub const ALPHA_MAX_RANGE_DAYS: i64 = 31;

const SECONDS_PER_DAY: i64 = 86_400;
const SECONDS_PER_HOUR: i64 = 3_600;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignalRange {
    Window {
        window: PriceWindow,
        from: String,
        to: String,
    },
    Explicit {
        from_date: String,
        to_date: String,
        from: String,
        to: String,
    },
}

impl SignalRange {
    pub fn parse(raw_query: Option<&str>) -> Result<Self, RangeValidationError> {
        Self::parse_with_now(raw_query, now_unix_seconds())
    }

    pub(crate) fn parse_with_now(
        raw_query: Option<&str>,
        now_seconds: i64,
    ) -> Result<Self, RangeValidationError> {
        let params = parse_query_params(raw_query)?;
        let window = params
            .iter()
            .find(|(key, _)| key == "window")
            .map(|(_, value)| value.as_str());
        let from_date = params
            .iter()
            .find(|(key, _)| key == "fromDate")
            .map(|(_, value)| value.as_str());
        let to_date = params
            .iter()
            .find(|(key, _)| key == "toDate")
            .map(|(_, value)| value.as_str());

        match (window, from_date, to_date) {
            (Some(raw_window), None, None) => {
                let window = PriceWindow::parse(raw_window)?;
                let to_seconds = floor_to_hour(now_seconds);
                let from_seconds = to_seconds - window.days() * SECONDS_PER_DAY;

                Ok(Self::Window {
                    window,
                    from: unix_seconds_to_rfc3339(from_seconds),
                    to: unix_seconds_to_rfc3339(to_seconds),
                })
            }
            (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
                Err(RangeValidationError::MutuallyExclusive)
            }
            (None, Some(from_date), Some(to_date)) => {
                let now_day = floor_div(now_seconds, SECONDS_PER_DAY);
                let from_day = parse_strict_date_to_epoch_day(from_date)?;
                let to_day = parse_strict_date_to_epoch_day(to_date)?;

                if from_day > now_day || to_day > now_day {
                    return Err(RangeValidationError::FutureDate);
                }

                if from_day > to_day {
                    return Err(RangeValidationError::FromAfterTo);
                }

                if to_day - from_day > ALPHA_MAX_RANGE_DAYS {
                    return Err(RangeValidationError::RangeTooLong);
                }

                Ok(Self::Explicit {
                    from_date: from_date.to_string(),
                    to_date: to_date.to_string(),
                    from: format!("{from_date}T00:00:00Z"),
                    to: format!("{to_date}T00:00:00Z"),
                })
            }
            (None, Some(_), None) => Err(RangeValidationError::MissingToDate),
            (None, None, Some(_)) => Err(RangeValidationError::MissingFromDate),
            (None, None, None) => Err(RangeValidationError::Missing),
        }
    }

    pub fn mode(&self) -> &'static str {
        match self {
            Self::Window { .. } => "window",
            Self::Explicit { .. } => "explicit",
        }
    }

    pub fn window_value(&self) -> Option<&'static str> {
        match self {
            Self::Window { window, .. } => Some(window.as_str()),
            Self::Explicit { .. } => None,
        }
    }

    pub fn from(&self) -> &str {
        match self {
            Self::Window { from, .. } | Self::Explicit { from, .. } => from,
        }
    }

    pub fn to(&self) -> &str {
        match self {
            Self::Window { to, .. } | Self::Explicit { to, .. } => to,
        }
    }

    pub fn range_days(&self) -> i64 {
        match self {
            Self::Window { window, .. } => window.days(),
            Self::Explicit {
                from_date, to_date, ..
            } => {
                let from_day = parse_strict_date_to_epoch_day(from_date).unwrap_or(0);
                let to_day = parse_strict_date_to_epoch_day(to_date).unwrap_or(from_day);

                to_day - from_day
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PriceWindow {
    SevenDays,
    OneWeek,
    OneMonth,
}

impl PriceWindow {
    fn parse(raw: &str) -> Result<Self, RangeValidationError> {
        match raw {
            "7d" => Ok(Self::SevenDays),
            "1w" => Ok(Self::OneWeek),
            "1m" => Ok(Self::OneMonth),
            _ => Err(RangeValidationError::UnsupportedWindow),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::SevenDays => "7d",
            Self::OneWeek => "1w",
            Self::OneMonth => "1m",
        }
    }

    pub fn days(self) -> i64 {
        match self {
            Self::SevenDays | Self::OneWeek => 7,
            Self::OneMonth => 31,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeValidationError {
    Missing,
    UnknownQuery,
    DuplicateQuery,
    EmptyQueryValue,
    MutuallyExclusive,
    MissingFromDate,
    MissingToDate,
    InvalidDateFormat,
    FutureDate,
    FromAfterTo,
    RangeTooLong,
    UnsupportedWindow,
}

impl RangeValidationError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Missing => "Query requires `window` or both `fromDate` and `toDate`.",
            Self::UnknownQuery => {
                "Only `window`, `fromDate`, and `toDate` query parameters are supported."
            }
            Self::DuplicateQuery => "Duplicate query parameters are not supported.",
            Self::EmptyQueryValue => "Signal query parameters must not be empty.",
            Self::MutuallyExclusive => "`window` and `fromDate`/`toDate` are mutually exclusive.",
            Self::MissingFromDate => "`fromDate` is required when `toDate` is present.",
            Self::MissingToDate => "`toDate` is required when `fromDate` is present.",
            Self::InvalidDateFormat => "Dates must use strict `YYYY-MM-DD` format.",
            Self::FutureDate => "Date ranges must not include future dates.",
            Self::FromAfterTo => "`fromDate` must be before or equal to `toDate`.",
            Self::RangeTooLong => "Alpha price signal ranges must be 31 days or shorter.",
            Self::UnsupportedWindow => "Supported `window` values are `7d`, `1w`, and `1m`.",
        }
    }
}

pub fn parse_utc_rfc3339_seconds(raw: &str) -> Result<i64, RangeValidationError> {
    let trimmed = raw.trim();
    let without_z = trimmed
        .strip_suffix('Z')
        .ok_or(RangeValidationError::InvalidDateFormat)?;
    let (date, time) = without_z
        .split_once('T')
        .ok_or(RangeValidationError::InvalidDateFormat)?;
    let day = parse_strict_date_to_epoch_day(date)?;
    let time = time.split_once('.').map(|(time, _)| time).unwrap_or(time);
    let parts = time.split(':').collect::<Vec<_>>();

    if parts.len() != 3 {
        return Err(RangeValidationError::InvalidDateFormat);
    }

    let hour = parse_two_digits(parts[0])?;
    let minute = parse_two_digits(parts[1])?;
    let second = parse_two_digits(parts[2])?;

    if hour > 23 || minute > 59 || second > 59 {
        return Err(RangeValidationError::InvalidDateFormat);
    }

    Ok(day * SECONDS_PER_DAY
        + i64::from(hour) * SECONDS_PER_HOUR
        + i64::from(minute) * 60
        + i64::from(second))
}

fn parse_query_params(
    raw_query: Option<&str>,
) -> Result<Vec<(String, String)>, RangeValidationError> {
    let Some(raw_query) = raw_query else {
        return Err(RangeValidationError::Missing);
    };
    let trimmed = raw_query.trim();

    if trimmed.is_empty() {
        return Err(RangeValidationError::Missing);
    }

    let mut seen = HashSet::new();
    let mut params = Vec::new();

    for pair in trimmed.split('&') {
        let (key, value) = pair
            .split_once('=')
            .ok_or(RangeValidationError::UnknownQuery)?;

        if !matches!(key, "window" | "fromDate" | "toDate") {
            return Err(RangeValidationError::UnknownQuery);
        }

        if !seen.insert(key.to_string()) {
            return Err(RangeValidationError::DuplicateQuery);
        }

        if value.is_empty() {
            return Err(RangeValidationError::EmptyQueryValue);
        }

        params.push((key.to_string(), value.to_string()));
    }

    Ok(params)
}

fn parse_strict_date_to_epoch_day(raw: &str) -> Result<i64, RangeValidationError> {
    if raw.len() != 10 {
        return Err(RangeValidationError::InvalidDateFormat);
    }

    let bytes = raw.as_bytes();

    if bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(RangeValidationError::InvalidDateFormat);
    }

    let year = parse_digits(&raw[0..4])? as i32;
    let month = parse_digits(&raw[5..7])?;
    let day = parse_digits(&raw[8..10])?;

    if month == 0 || month > 12 {
        return Err(RangeValidationError::InvalidDateFormat);
    }

    let max_day = days_in_month(year, month);

    if day == 0 || day > max_day {
        return Err(RangeValidationError::InvalidDateFormat);
    }

    Ok(days_from_civil(year, month, day))
}

fn parse_two_digits(raw: &str) -> Result<u32, RangeValidationError> {
    if raw.len() != 2 {
        return Err(RangeValidationError::InvalidDateFormat);
    }

    parse_digits(raw)
}

fn parse_digits(raw: &str) -> Result<u32, RangeValidationError> {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(RangeValidationError::InvalidDateFormat);
    }

    raw.parse::<u32>()
        .map_err(|_| RangeValidationError::InvalidDateFormat)
}

fn days_in_month(year: i32, month: u32) -> u32 {
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

fn floor_to_hour(seconds: i64) -> i64 {
    floor_div(seconds, SECONDS_PER_HOUR) * SECONDS_PER_HOUR
}

fn floor_div(value: i64, divisor: i64) -> i64 {
    let quotient = value / divisor;
    let remainder = value % divisor;

    if remainder != 0 && ((remainder > 0) != (divisor > 0)) {
        quotient - 1
    } else {
        quotient
    }
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn unix_seconds_to_rfc3339(seconds: i64) -> String {
    let days = floor_div(seconds, SECONDS_PER_DAY);
    let seconds_of_day = seconds - days * SECONDS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / SECONDS_PER_HOUR;
    let minute = (seconds_of_day % SECONDS_PER_HOUR) / 60;
    let second = seconds_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day = i64::from(day);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };

    year += if month <= 2 { 1 } else { 0 };

    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_780_066_800; // 2026-05-29T15:00:00Z

    #[test]
    fn parses_supported_windows() {
        let range = SignalRange::parse_with_now(Some("window=7d"), NOW).unwrap();

        assert_eq!(range.mode(), "window");
        assert_eq!(range.window_value(), Some("7d"));
        assert_eq!(range.from(), "2026-05-22T15:00:00Z");
        assert_eq!(range.to(), "2026-05-29T15:00:00Z");
        assert_eq!(range.range_days(), 7);

        assert_eq!(
            SignalRange::parse_with_now(Some("window=1w"), NOW)
                .unwrap()
                .range_days(),
            7
        );
        assert_eq!(
            SignalRange::parse_with_now(Some("window=1m"), NOW)
                .unwrap()
                .range_days(),
            31
        );
    }

    #[test]
    fn parses_explicit_date_ranges() {
        let range = SignalRange::parse_with_now(Some("fromDate=2020-05-21&toDate=2020-05-29"), NOW)
            .unwrap();

        assert_eq!(range.mode(), "explicit");
        assert_eq!(range.window_value(), None);
        assert_eq!(range.from(), "2020-05-21T00:00:00Z");
        assert_eq!(range.to(), "2020-05-29T00:00:00Z");
        assert_eq!(range.range_days(), 8);
    }

    #[test]
    fn rejects_invalid_query_shapes() {
        for (query, expected) in [
            (None, RangeValidationError::Missing),
            (Some(""), RangeValidationError::Missing),
            (
                Some("window=7d&fromDate=2020-05-21"),
                RangeValidationError::MutuallyExclusive,
            ),
            (
                Some("fromDate=2020-05-21"),
                RangeValidationError::MissingToDate,
            ),
            (
                Some("toDate=2020-05-29"),
                RangeValidationError::MissingFromDate,
            ),
            (Some("window=2d"), RangeValidationError::UnsupportedWindow),
            (
                Some("window=7d&window=1w"),
                RangeValidationError::DuplicateQuery,
            ),
            (Some("currency=USD"), RangeValidationError::UnknownQuery),
            (Some("window="), RangeValidationError::EmptyQueryValue),
        ] {
            assert_eq!(SignalRange::parse_with_now(query, NOW), Err(expected));
        }
    }

    #[test]
    fn rejects_invalid_dates() {
        for query in [
            "fromDate=2020-5-21&toDate=2020-05-29",
            "fromDate=2020-02-30&toDate=2020-05-29",
            "fromDate=2026-05-30&toDate=2026-05-30",
        ] {
            assert!(matches!(
                SignalRange::parse_with_now(Some(query), NOW),
                Err(RangeValidationError::InvalidDateFormat | RangeValidationError::FutureDate)
            ));
        }
    }

    #[test]
    fn rejects_reversed_and_too_long_ranges() {
        assert_eq!(
            SignalRange::parse_with_now(Some("fromDate=2020-05-30&toDate=2020-05-29"), NOW),
            Err(RangeValidationError::FromAfterTo)
        );
        assert_eq!(
            SignalRange::parse_with_now(Some("fromDate=2020-05-01&toDate=2020-06-02"), NOW),
            Err(RangeValidationError::RangeTooLong)
        );
    }

    #[test]
    fn parses_rfc3339_utc_seconds() {
        assert_eq!(
            parse_utc_rfc3339_seconds("2026-05-29T15:00:00Z").unwrap(),
            NOW
        );
        assert_eq!(
            parse_utc_rfc3339_seconds("2026-05-29T15:00:00.000Z").unwrap(),
            NOW
        );
    }
}
