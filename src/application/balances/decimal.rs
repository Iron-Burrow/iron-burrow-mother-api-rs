#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecimalError {
    InvalidUnsignedInteger,
    InvalidUnsignedDecimal,
}

pub fn is_unsigned_integer(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn format_amount(raw_amount: &str, decimals: u8) -> Result<String, DecimalError> {
    if !is_unsigned_integer(raw_amount) {
        return Err(DecimalError::InvalidUnsignedInteger);
    }

    Ok(format_scaled_digits(
        normalize_integer_digits(raw_amount),
        usize::from(decimals),
    ))
}

pub fn multiply_amount_by_price(
    raw_amount: &str,
    balance_decimals: u8,
    unit_price: &str,
) -> Result<String, DecimalError> {
    if !is_unsigned_integer(raw_amount) {
        return Err(DecimalError::InvalidUnsignedInteger);
    }
    let price = parse_unsigned_decimal(unit_price)?;
    let mut product = multiply_integer_digits(raw_amount, &price.digits);
    let minimum_scale = usize::from(balance_decimals);
    let mut scale = minimum_scale + price.scale;

    if product == "0" {
        scale = minimum_scale;
    } else {
        while scale > minimum_scale && product.ends_with('0') {
            product.pop();
            scale -= 1;
        }
    }

    Ok(format_scaled_digits(product, scale))
}

struct ParsedDecimal {
    digits: String,
    scale: usize,
}

fn parse_unsigned_decimal(value: &str) -> Result<ParsedDecimal, DecimalError> {
    let (integer, fraction) = match value.split_once('.') {
        Some((integer, fraction))
            if !integer.is_empty()
                && !fraction.is_empty()
                && !fraction.contains('.')
                && integer.bytes().all(|byte| byte.is_ascii_digit())
                && fraction.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            (integer, Some(fraction))
        }
        None if is_unsigned_integer(value) => (value, None),
        _ => return Err(DecimalError::InvalidUnsignedDecimal),
    };

    let scale = fraction.map_or(0, str::len);
    let mut digits = String::with_capacity(integer.len() + scale);
    digits.push_str(integer);
    if let Some(fraction) = fraction {
        digits.push_str(fraction);
    }

    Ok(ParsedDecimal {
        digits: normalize_integer_digits(&digits),
        scale,
    })
}

fn normalize_integer_digits(value: &str) -> String {
    let normalized = value.trim_start_matches('0');
    if normalized.is_empty() {
        "0".to_string()
    } else {
        normalized.to_string()
    }
}

fn multiply_integer_digits(left: &str, right: &str) -> String {
    let left = normalize_integer_digits(left);
    let right = normalize_integer_digits(right);
    if left == "0" || right == "0" {
        return "0".to_string();
    }

    let left = left
        .bytes()
        .rev()
        .map(|byte| byte - b'0')
        .collect::<Vec<_>>();
    let right = right
        .bytes()
        .rev()
        .map(|byte| byte - b'0')
        .collect::<Vec<_>>();
    let mut product = vec![0u32; left.len() + right.len()];

    for (left_index, left_digit) in left.iter().enumerate() {
        for (right_index, right_digit) in right.iter().enumerate() {
            product[left_index + right_index] += u32::from(*left_digit) * u32::from(*right_digit);
        }
    }

    for index in 0..product.len() - 1 {
        let carry = product[index] / 10;
        product[index] %= 10;
        product[index + 1] += carry;
    }
    while product.last() == Some(&0) {
        product.pop();
    }

    product
        .into_iter()
        .rev()
        .map(|digit| char::from(b'0' + u8::try_from(digit).expect("normalized decimal digit")))
        .collect()
}

fn format_scaled_digits(digits: String, scale: usize) -> String {
    if scale == 0 {
        return digits;
    }

    if digits.len() <= scale {
        let mut value = String::with_capacity(2 + scale);
        value.push_str("0.");
        value.push_str(&"0".repeat(scale - digits.len()));
        value.push_str(&digits);
        return value;
    }

    let split = digits.len() - scale;
    format!("{}.{}", &digits[..split], &digits[split..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_raw_amounts_with_exact_catalog_scale() {
        assert_eq!(format_amount("0", 6).unwrap(), "0.000000");
        assert_eq!(format_amount("42", 0).unwrap(), "42");
        assert_eq!(format_amount("42", 6).unwrap(), "0.000042");
        assert_eq!(format_amount("450000000", 6).unwrap(), "450.000000");
        assert_eq!(format_amount("000450000000", 6).unwrap(), "450.000000");
    }

    #[test]
    fn multiplies_without_floating_point_and_preserves_balance_scale() {
        assert_eq!(
            multiply_amount_by_price("450000000", 6, "18.45").unwrap(),
            "8302.500000"
        );
        assert_eq!(
            multiply_amount_by_price("8000123456", 6, "18.45").unwrap(),
            "147602.2777632"
        );
        assert_eq!(
            multiply_amount_by_price("780000000", 6, "1.00").unwrap(),
            "780.000000"
        );
        assert_eq!(multiply_amount_by_price("42", 0, "2.50").unwrap(), "105");
        assert_eq!(
            multiply_amount_by_price("0", 18, "3187.123456789").unwrap(),
            "0.000000000000000000"
        );
    }

    #[test]
    fn handles_arbitrary_length_evm_values() {
        assert_eq!(
            multiply_amount_by_price(
                "80001234560000000000000000000000000000",
                18,
                "3187.123456789"
            )
            .unwrap(),
            "254973811238254813427840.000000000000000000"
        );
    }

    #[test]
    fn rejects_malformed_integer_and_decimal_inputs() {
        for raw_amount in ["", "-1", "+1", "1.0", "1e3", " 1"] {
            assert_eq!(
                format_amount(raw_amount, 6),
                Err(DecimalError::InvalidUnsignedInteger)
            );
        }
        for unit_price in ["", "-1", "+1", ".5", "1.", "1.2.3", "1e3", " 1"] {
            assert_eq!(
                multiply_amount_by_price("1", 0, unit_price),
                Err(DecimalError::InvalidUnsignedDecimal)
            );
        }
    }
}
