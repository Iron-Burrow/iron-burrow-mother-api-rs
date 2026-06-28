use super::*;

#[test]
fn parse_rfc3339_accepts_utc_timestamp() {
    assert_eq!(
        parse_rfc3339("1970-01-01T00:00:00Z"),
        Some(ParsedRfc3339 {
            epoch_seconds: 0,
            fraction: String::new(),
        })
    );
}

#[test]
fn parse_rfc3339_preserves_fractional_seconds() {
    assert_eq!(
        parse_rfc3339("1970-01-01T00:00:00.123Z"),
        Some(ParsedRfc3339 {
            epoch_seconds: 0,
            fraction: "123".to_string(),
        })
    );
}

#[test]
fn parse_rfc3339_normalizes_positive_offset_to_utc() {
    assert_eq!(
        parse_rfc3339("1970-01-01T01:00:00+01:00"),
        Some(ParsedRfc3339 {
            epoch_seconds: 0,
            fraction: String::new(),
        })
    );
}

#[test]
fn parse_rfc3339_normalizes_negative_offset_to_utc() {
    assert_eq!(
        parse_rfc3339("1969-12-31T23:00:00-01:00"),
        Some(ParsedRfc3339 {
            epoch_seconds: 0,
            fraction: String::new(),
        })
    );
}

#[test]
fn parse_rfc3339_rejects_missing_or_duplicate_t_separator() {
    assert_eq!(parse_rfc3339("1970-01-01 00:00:00Z"), None);
    assert_eq!(parse_rfc3339("1970-01-01T00:00:00TZ"), None);
}

#[test]
fn parse_rfc3339_rejects_invalid_dates() {
    assert_eq!(parse_rfc3339("2025-00-01T00:00:00Z"), None);
    assert_eq!(parse_rfc3339("2025-13-01T00:00:00Z"), None);
    assert_eq!(parse_rfc3339("2025-02-29T00:00:00Z"), None);
    assert_eq!(parse_rfc3339("2024-02-30T00:00:00Z"), None);
}

#[test]
fn parse_rfc3339_accepts_leap_day() {
    assert_eq!(
        parse_rfc3339("2024-02-29T00:00:00Z"),
        Some(ParsedRfc3339 {
            epoch_seconds: 1_709_164_800,
            fraction: String::new(),
        })
    );
}

#[test]
fn parse_rfc3339_rejects_invalid_times() {
    assert_eq!(parse_rfc3339("1970-01-01T24:00:00Z"), None);
    assert_eq!(parse_rfc3339("1970-01-01T23:60:00Z"), None);
    assert_eq!(parse_rfc3339("1970-01-01T23:59:60Z"), None);
}

#[test]
fn parse_rfc3339_rejects_invalid_offsets() {
    assert_eq!(parse_rfc3339("1970-01-01T00:00:00+24:00"), None);
    assert_eq!(parse_rfc3339("1970-01-01T00:00:00+00:60"), None);
    assert_eq!(parse_rfc3339("1970-01-01T00:00:00+0000"), None);
    assert_eq!(parse_rfc3339("1970-01-01T00:00:00UTC"), None);
}

#[test]
fn compare_rfc3339_orders_by_epoch_seconds_first() {
    let earlier = parse_rfc3339("1970-01-01T00:00:00Z").unwrap();
    let later = parse_rfc3339("1970-01-01T00:00:01Z").unwrap();

    assert_eq!(compare_rfc3339(&earlier, &later), Ordering::Less);
    assert_eq!(compare_rfc3339(&later, &earlier), Ordering::Greater);
}

#[test]
fn compare_rfc3339_orders_by_fraction_when_epoch_seconds_match() {
    let earlier = parse_rfc3339("1970-01-01T00:00:00.123Z").unwrap();
    let later = parse_rfc3339("1970-01-01T00:00:00.124Z").unwrap();

    assert_eq!(compare_rfc3339(&earlier, &later), Ordering::Less);
    assert_eq!(compare_rfc3339(&later, &earlier), Ordering::Greater);
}

#[test]
fn compare_rfc3339_treats_missing_fraction_digits_as_zeroes() {
    let left = parse_rfc3339("1970-01-01T00:00:00.1Z").unwrap();
    let right = parse_rfc3339("1970-01-01T00:00:00.100Z").unwrap();

    assert_eq!(compare_rfc3339(&left, &right), Ordering::Equal);
}

#[test]
fn parse_date_accepts_valid_date() {
    assert_eq!(parse_date("2025-12-31"), Some((2025, 12, 31)));
}

#[test]
fn parse_date_rejects_invalid_layout() {
    assert_eq!(parse_date("2025/12/31"), None);
    assert_eq!(parse_date("25-12-31"), None);
    assert_eq!(parse_date("2025-1-31"), None);
}

#[test]
fn parse_offset_accepts_positive_and_negative_offsets() {
    assert_eq!(parse_offset("+01:30"), Some(5_400));
    assert_eq!(parse_offset("-01:30"), Some(-5_400));
}

#[test]
fn parse_offset_rejects_invalid_layout() {
    assert_eq!(parse_offset("01:30"), None);
    assert_eq!(parse_offset("+0130"), None);
    assert_eq!(parse_offset("+01-30"), None);
}

#[test]
fn parse_rfc3339_rejects_single_digit_seconds_before_fraction() {
    assert_eq!(parse_rfc3339("1970-01-01T00:00:0.123Z"), None);
}
