use chrono::NaiveTime;

/// Parse a Navision-style time shorthand into HH:MM:SS.
///
/// Shortcuts:
///   - `t`, `T`           → current system time
///   - `8`                → 08:00:00
///   - `14`               → 14:00:00
///   - `830`              → 08:30:00
///   - `1430`             → 14:30:00
///   - `83015`            → 08:30:15
///   - `143005`           → 14:30:05
///   - `14.30`, `14,30`   → 14:30:00 (separator auto-correction)
///   - `14:30`            → 14:30:00
///   - `14:30:05`         → 14:30:05

pub fn parse_time_shorthand(input: &str, now: NaiveTime) -> Result<NaiveTime, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Tom tid".into());
    }

    // t/T shortcut → current time
    if input == "t" || input == "T" {
        return Ok(now);
    }

    // Try parsing with separators first (: , .)
    if input.contains(':') || input.contains(',') || input.contains('.') {
        return parse_with_separators(input);
    }

    // Pure digits
    let digits: String = input.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != input.len() {
        return Err("Ugyldigt tidsformat".into());
    }

    match digits.len() {
        1 | 2 => {
            // Hours only
            let h: u32 = digits.parse().map_err(|_| "Ugyldig time".to_string())?;
            make_time(h, 0, 0)
        }
        3 => {
            // H + MM (e.g., "830" → 8:30)
            let h: u32 = digits[..1].parse().unwrap();
            let m: u32 = digits[1..].parse().unwrap();
            make_time(h, m, 0)
        }
        4 => {
            // HH + MM (e.g., "1430" → 14:30)
            let h: u32 = digits[..2].parse().unwrap();
            let m: u32 = digits[2..].parse().unwrap();
            make_time(h, m, 0)
        }
        5 => {
            // H + MM + SS (e.g., "83015" → 8:30:15)
            let h: u32 = digits[..1].parse().unwrap();
            let m: u32 = digits[1..3].parse().unwrap();
            let s: u32 = digits[3..].parse().unwrap();
            make_time(h, m, s)
        }
        6 => {
            // HH + MM + SS (e.g., "143005" → 14:30:05)
            let h: u32 = digits[..2].parse().unwrap();
            let m: u32 = digits[2..4].parse().unwrap();
            let s: u32 = digits[4..].parse().unwrap();
            make_time(h, m, s)
        }
        _ => Err("Ugyldigt tidsformat".into()),
    }
}

fn parse_with_separators(input: &str) -> Result<NaiveTime, String> {
    // Normalize all separators to ':'
    let normalized: String = input.chars().map(|c| {
        if c == ',' || c == '.' { ':' } else { c }
    }).collect();

    let parts: Vec<&str> = normalized.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return Err("Ugyldigt tidsformat".into());
    }

    let h: u32 = parts[0].parse().map_err(|_| "Ugyldig time".to_string())?;
    let m: u32 = parts[1].parse().map_err(|_| "Ugyldigt minut".to_string())?;
    let s: u32 = if parts.len() == 3 {
        parts[2].parse().map_err(|_| "Ugyldigt sekund".to_string())?
    } else {
        0
    };

    make_time(h, m, s)
}

fn make_time(h: u32, m: u32, s: u32) -> Result<NaiveTime, String> {
    NaiveTime::from_hms_opt(h, m, s)
        .ok_or_else(|| format!("Ugyldig tid: {:02}:{:02}:{:02}", h, m, s))
}

pub fn format_time(time: NaiveTime) -> String {
    time.format("%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn now() -> NaiveTime {
        NaiveTime::from_hms_opt(14, 30, 5).unwrap()
    }

    #[test]
    fn today_shortcut() {
        assert_eq!(parse_time_shorthand("t", now()).unwrap(), now());
        assert_eq!(parse_time_shorthand("T", now()).unwrap(), now());
    }

    #[test]
    fn hours_only() {
        assert_eq!(
            parse_time_shorthand("8", now()).unwrap(),
            NaiveTime::from_hms_opt(8, 0, 0).unwrap()
        );
        assert_eq!(
            parse_time_shorthand("14", now()).unwrap(),
            NaiveTime::from_hms_opt(14, 0, 0).unwrap()
        );
    }

    #[test]
    fn hours_and_minutes() {
        assert_eq!(
            parse_time_shorthand("830", now()).unwrap(),
            NaiveTime::from_hms_opt(8, 30, 0).unwrap()
        );
        assert_eq!(
            parse_time_shorthand("1430", now()).unwrap(),
            NaiveTime::from_hms_opt(14, 30, 0).unwrap()
        );
    }

    #[test]
    fn hours_minutes_seconds() {
        assert_eq!(
            parse_time_shorthand("83015", now()).unwrap(),
            NaiveTime::from_hms_opt(8, 30, 15).unwrap()
        );
        assert_eq!(
            parse_time_shorthand("143005", now()).unwrap(),
            NaiveTime::from_hms_opt(14, 30, 5).unwrap()
        );
    }

    #[test]
    fn separator_colon() {
        assert_eq!(
            parse_time_shorthand("14:30", now()).unwrap(),
            NaiveTime::from_hms_opt(14, 30, 0).unwrap()
        );
        assert_eq!(
            parse_time_shorthand("14:30:05", now()).unwrap(),
            NaiveTime::from_hms_opt(14, 30, 5).unwrap()
        );
    }

    #[test]
    fn separator_auto_correction() {
        // Comma (Danish numpad)
        assert_eq!(
            parse_time_shorthand("14,30", now()).unwrap(),
            NaiveTime::from_hms_opt(14, 30, 0).unwrap()
        );
        // Dot
        assert_eq!(
            parse_time_shorthand("14.30", now()).unwrap(),
            NaiveTime::from_hms_opt(14, 30, 0).unwrap()
        );
        // Comma with seconds
        assert_eq!(
            parse_time_shorthand("14,30,05", now()).unwrap(),
            NaiveTime::from_hms_opt(14, 30, 5).unwrap()
        );
    }

    #[test]
    fn invalid_inputs() {
        assert!(parse_time_shorthand("", now()).is_err());
        assert!(parse_time_shorthand("hello", now()).is_err());
        assert!(parse_time_shorthand("25", now()).is_err());
        assert!(parse_time_shorthand("1470", now()).is_err());
    }

    #[test]
    fn format() {
        assert_eq!(
            format_time(NaiveTime::from_hms_opt(8, 5, 0).unwrap()),
            "08:05:00"
        );
        assert_eq!(
            format_time(NaiveTime::from_hms_opt(14, 30, 5).unwrap()),
            "14:30:05"
        );
    }
}
