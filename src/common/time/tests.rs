use crate::common::time::{
    days_from_civil, days_in_month, is_leap_year, parse_ascii_i32, parse_ascii_u32, parse_time,
    ParsedTime,
};

#[test]
fn parse_time_accepts_basic_time() {
    assert_eq!(
        parse_time("12:34:56"),
        Some(ParsedTime {
            hour: 12,
            minute: 34,
            second: 56,
            fraction: String::new(),
        })
    );
}

#[test]
fn parse_time_accepts_fractional_seconds() {
    assert_eq!(
        parse_time("12:34:56.789"),
        Some(ParsedTime {
            hour: 12,
            minute: 34,
            second: 56,
            fraction: "789".to_string(),
        })
    );
}

#[test]
fn parse_time_rejects_invalid_separators() {
    assert_eq!(parse_time("12-34-56"), None);
    assert_eq!(parse_time("123456"), None);
}

#[test]
fn parse_time_rejects_out_of_range_values() {
    assert_eq!(parse_time("24:00:00"), None);
    assert_eq!(parse_time("23:60:00"), None);
    assert_eq!(parse_time("23:59:60"), None);
}

#[test]
fn parse_time_rejects_invalid_fraction() {
    assert_eq!(parse_time("12:34:56."), None);
    assert_eq!(parse_time("12:34:56.abc"), None);
    assert_eq!(parse_time("12:34:56.12a"), None);
}

#[test]
fn parse_ascii_u32_accepts_ascii_digits() {
    assert_eq!(parse_ascii_u32(b"0"), Some(0));
    assert_eq!(parse_ascii_u32(b"42"), Some(42));
    assert_eq!(parse_ascii_u32(b"0012"), Some(12));
}

#[test]
fn parse_ascii_u32_rejects_invalid_input() {
    assert_eq!(parse_ascii_u32(b""), None);
    assert_eq!(parse_ascii_u32(b"abc"), None);
    assert_eq!(parse_ascii_u32(b"12a"), None);
    assert_eq!(parse_ascii_u32(b"-12"), None);
}

#[test]
fn parse_ascii_i32_accepts_ascii_digits_without_sign() {
    assert_eq!(parse_ascii_i32(b"0"), Some(0));
    assert_eq!(parse_ascii_i32(b"42"), Some(42));
    assert_eq!(parse_ascii_i32(b"0012"), Some(12));
}

#[test]
fn parse_ascii_i32_rejects_invalid_input() {
    assert_eq!(parse_ascii_i32(b""), None);
    assert_eq!(parse_ascii_i32(b"abc"), None);
    assert_eq!(parse_ascii_i32(b"12a"), None);
    assert_eq!(parse_ascii_i32(b"-12"), None);
}

#[test]
fn days_in_month_returns_month_lengths() {
    assert_eq!(days_in_month(2025, 1), 31);
    assert_eq!(days_in_month(2025, 4), 30);
    assert_eq!(days_in_month(2025, 2), 28);
    assert_eq!(days_in_month(2024, 2), 29);
}

#[test]
fn days_in_month_returns_zero_for_invalid_month() {
    assert_eq!(days_in_month(2025, 0), 0);
    assert_eq!(days_in_month(2025, 13), 0);
}

#[test]
fn is_leap_year_uses_gregorian_rules() {
    assert!(is_leap_year(2024));
    assert!(!is_leap_year(2025));

    // Divisible by 100 is not enough.
    assert!(!is_leap_year(1900));

    // Divisible by 400 is a leap year.
    assert!(is_leap_year(2000));
}

#[test]
fn days_from_civil_returns_days_relative_to_unix_epoch() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(1970, 1, 2), 1);
    assert_eq!(days_from_civil(1969, 12, 31), -1);
    assert_eq!(days_from_civil(2000, 1, 1), 10_957);
}

#[test]
fn parse_time_rejects_single_digit_seconds_before_fraction() {
    assert_eq!(parse_time("12:34:5.789"), None);
}
