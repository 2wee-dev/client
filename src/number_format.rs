/// Format a numeric string for display using locale separators.
///
/// Input: raw value like "95851.00" or "1234567.50"
/// Output: locale-formatted like "95.851,00" (Danish) or "95,851.00" (US)

pub fn format_decimal(
    raw: &str,
    decimal_sep: &str,
    thousand_sep: &str,
    decimals: Option<u8>,
) -> String {
    let value: f64 = match raw.parse() {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };

    let dec = decimals.unwrap_or(2) as usize;
    let formatted = format!("{:.prec$}", value, prec = dec);

    // Split on '.' (Rust always uses '.' for f64 formatting)
    let parts: Vec<&str> = formatted.split('.').collect();
    let int_part = parts[0];
    let frac_part = parts.get(1).unwrap_or(&"");

    // Add thousand separators to integer part
    let negative = int_part.starts_with('-');
    let digits: &str = if negative { &int_part[1..] } else { int_part };

    let with_thousands = add_thousand_separators(digits, thousand_sep);

    let mut result = String::new();
    if negative {
        result.push('-');
    }
    result.push_str(&with_thousands);
    if dec > 0 {
        result.push_str(decimal_sep);
        result.push_str(frac_part);
    }
    result
}

pub fn format_integer(raw: &str, thousand_sep: &str) -> String {
    let value: i64 = match raw.parse::<f64>() {
        Ok(v) => v as i64,
        Err(_) => return raw.to_string(),
    };

    let formatted = value.to_string();
    let negative = formatted.starts_with('-');
    let digits: &str = if negative { &formatted[1..] } else { &formatted };

    let with_thousands = add_thousand_separators(digits, thousand_sep);

    if negative {
        format!("-{}", with_thousands)
    } else {
        with_thousands
    }
}

/// Parse a locale-formatted decimal string back to a raw f64-parseable string.
///
/// Input: locale string like "1.200,50" (Danish) or "1,200.50" (US)
/// Output: Ok("1200.50") or Err if the input is not a valid number.
pub fn parse_locale_decimal(
    input: &str,
    decimal_sep: &str,
    thousand_sep: &str,
) -> Result<String, ()> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    // Strip thousand separators, replace locale decimal separator with '.'
    let mut raw = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        let s = ch.to_string();
        if !thousand_sep.is_empty() && s == *thousand_sep {
            continue; // skip thousand separators
        } else if s == *decimal_sep {
            raw.push('.');
        } else if ch.is_ascii_digit() || ch == '-' {
            raw.push(ch);
        } else {
            return Err(()); // invalid character
        }
    }

    // Validate it parses as f64
    if raw.is_empty() || raw == "-" || raw == "." {
        return Err(());
    }
    let _: f64 = raw.parse().map_err(|_| ())?;
    Ok(raw)
}

fn add_thousand_separators(digits: &str, sep: &str) -> String {
    let len = digits.len();
    if len <= 3 || sep.is_empty() {
        return digits.to_string();
    }

    let mut result = String::with_capacity(len + (len / 3) * sep.len());
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push_str(sep);
        }
        result.push(ch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn danish_decimal() {
        assert_eq!(format_decimal("95851.00", ",", ".", None), "95.851,00");
        assert_eq!(format_decimal("1234567.50", ",", ".", None), "1.234.567,50");
        assert_eq!(format_decimal("0.00", ",", ".", None), "0,00");
        assert_eq!(format_decimal("42.10", ",", ".", None), "42,10");
    }

    #[test]
    fn us_decimal() {
        assert_eq!(format_decimal("95851.00", ".", ",", None), "95,851.00");
        assert_eq!(format_decimal("1234567.50", ".", ",", None), "1,234,567.50");
    }

    #[test]
    fn negative() {
        assert_eq!(format_decimal("-3475.00", ",", ".", None), "-3.475,00");
        assert_eq!(format_decimal("-42.10", ",", ".", None), "-42,10");
    }

    #[test]
    fn integer_format() {
        assert_eq!(format_integer("1234567", "."), "1.234.567");
        assert_eq!(format_integer("42", "."), "42");
        assert_eq!(format_integer("-1000", "."), "-1.000");
    }

    #[test]
    fn small_numbers() {
        assert_eq!(format_decimal("0.50", ",", ".", None), "0,50");
        assert_eq!(format_decimal("999.99", ",", ".", None), "999,99");
        assert_eq!(format_decimal("1000.00", ",", ".", None), "1.000,00");
    }

    #[test]
    fn non_numeric_passthrough() {
        assert_eq!(format_decimal("hello", ",", ".", None), "hello");
        assert_eq!(format_integer("hello", "."), "hello");
    }

    #[test]
    fn parse_danish_decimal() {
        assert_eq!(parse_locale_decimal("5,00", ",", "."), Ok("5.00".to_string()));
        assert_eq!(parse_locale_decimal("1.200,50", ",", "."), Ok("1200.50".to_string()));
        assert_eq!(parse_locale_decimal("42", ",", "."), Ok("42".to_string()));
        assert_eq!(parse_locale_decimal("", ",", "."), Ok("".to_string()));
        assert_eq!(parse_locale_decimal("-3,5", ",", "."), Ok("-3.5".to_string()));
    }

    #[test]
    fn parse_us_decimal() {
        assert_eq!(parse_locale_decimal("1,200.50", ".", ","), Ok("1200.50".to_string()));
        assert_eq!(parse_locale_decimal("42.10", ".", ","), Ok("42.10".to_string()));
    }

    #[test]
    fn parse_invalid() {
        assert!(parse_locale_decimal("abc", ",", ".").is_err());
        assert!(parse_locale_decimal("12a3", ",", ".").is_err());
    }
}
