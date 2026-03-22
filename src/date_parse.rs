use chrono::{Datelike, NaiveDate};

/// Parse a Navision-style date shorthand into a full date string.
///
/// Shortcuts (relative to `reference_date`, typically work_date or today):
///   - `t`, `T`, `d`, `D`     → reference date ("today" / "dags dato")
///   - `t+N`, `d+N`           → reference date + N days
///   - `t-N`, `d-N`           → reference date - N days
///   - `12`                   → 12th of current month/year
///   - `1203`                 → 12th March, current year
///   - `12-03`                → 12th March, current year
///   - `120326`               → 12th March 2026
///   - `12-03-26`             → 12th March 2026
///   - `12-03-2026`           → 12th March 2026
///   - Already formatted      → pass through if valid
///
/// `date_order` is `DayFirst` (DD-MM) or `MonthFirst` (MM-DD).
/// Returns `Ok(NaiveDate)` or `Err(reason)`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOrder {
    DayFirst,   // DD-MM-YYYY (European)
    MonthFirst, // MM-DD-YYYY (US)
}

impl DateOrder {
    pub fn from_format(fmt: &str) -> Self {
        if fmt.starts_with("MM") {
            DateOrder::MonthFirst
        } else {
            DateOrder::DayFirst
        }
    }
}

pub fn parse_date_shorthand(
    input: &str,
    reference: NaiveDate,
    order: DateOrder,
) -> Result<NaiveDate, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Tom dato".into());
    }

    // t/d shortcuts (today, today+N, today-N)
    let first = input.chars().next().unwrap();
    if first == 't' || first == 'T' || first == 'd' || first == 'D' {
        let rest = &input[1..];
        if rest.is_empty() {
            return Ok(reference);
        }
        if let Some(offset_str) = rest.strip_prefix('+') {
            let days: i64 = offset_str.parse().map_err(|_| "Ugyldigt offset".to_string())?;
            return reference
                .checked_add_signed(chrono::Duration::days(days))
                .ok_or_else(|| "Dato uden for rækkevidde".to_string());
        }
        if let Some(offset_str) = rest.strip_prefix('-') {
            let days: i64 = offset_str.parse().map_err(|_| "Ugyldigt offset".to_string())?;
            return reference
                .checked_sub_signed(chrono::Duration::days(days))
                .ok_or_else(|| "Dato uden for rækkevidde".to_string());
        }
        return Err("Ugyldigt datoformat".into());
    }

    // Strip separators to get pure digits, but also try with separators
    let digits_only: String = input.chars().filter(|c| c.is_ascii_digit()).collect();

    // Try full format with separators first (DD-MM-YYYY, DD-MM-YY, etc.)
    if let Some(date) = try_parse_with_separators(input, reference, order) {
        return Ok(date);
    }

    // Pure digit parsing
    let d = &digits_only;
    match d.len() {
        1 | 2 => {
            // Day only → current month/year
            let day: u32 = d.parse().map_err(|_| "Ugyldig dag".to_string())?;
            make_date(reference.year(), reference.month(), day)
        }
        3 | 4 => {
            // Day + month (e.g., "123" = 12th March or 1st 23rd?  "1203" = 12-03)
            let (a, b) = if d.len() == 3 {
                // 3 digits: first 1 is field A, last 2 is field B
                (d[..1].parse::<u32>().unwrap(), d[1..].parse::<u32>().unwrap())
            } else {
                // 4 digits: first 2 is field A, last 2 is field B
                (d[..2].parse::<u32>().unwrap(), d[2..].parse::<u32>().unwrap())
            };
            let (day, month) = match order {
                DateOrder::DayFirst => (a, b),
                DateOrder::MonthFirst => (b, a),
            };
            make_date(reference.year(), month, day)
        }
        5 | 6 => {
            // Day + month + 2-digit year (e.g., "120326")
            let (a, b, y) = if d.len() == 5 {
                (d[..1].parse::<u32>().unwrap(), d[1..3].parse::<u32>().unwrap(), d[3..].parse::<i32>().unwrap())
            } else {
                (d[..2].parse::<u32>().unwrap(), d[2..4].parse::<u32>().unwrap(), d[4..].parse::<i32>().unwrap())
            };
            let (day, month) = match order {
                DateOrder::DayFirst => (a, b),
                DateOrder::MonthFirst => (b, a),
            };
            let year = expand_year(y);
            make_date(year, month, day)
        }
        7 | 8 => {
            // Day + month + 4-digit year
            let (a, b, y) = if d.len() == 7 {
                (d[..1].parse::<u32>().unwrap(), d[1..3].parse::<u32>().unwrap(), d[3..].parse::<i32>().unwrap())
            } else {
                (d[..2].parse::<u32>().unwrap(), d[2..4].parse::<u32>().unwrap(), d[4..].parse::<i32>().unwrap())
            };
            let (day, month) = match order {
                DateOrder::DayFirst => (a, b),
                DateOrder::MonthFirst => (b, a),
            };
            make_date(y, month, day)
        }
        _ => Err("Ugyldigt datoformat".into()),
    }
}

fn try_parse_with_separators(
    input: &str,
    reference: NaiveDate,
    order: DateOrder,
) -> Option<NaiveDate> {
    let parts: Vec<&str> = input.split(|c: char| c == '-' || c == '/' || c == '.').collect();
    if parts.len() < 2 {
        return None;
    }

    let a: u32 = parts[0].parse().ok()?;
    let b: u32 = parts[1].parse().ok()?;

    let (day, month) = match order {
        DateOrder::DayFirst => (a, b),
        DateOrder::MonthFirst => (b, a),
    };

    if parts.len() == 2 {
        // DD-MM → current year
        make_date(reference.year(), month, day).ok()
    } else {
        let y: i32 = parts[2].parse().ok()?;
        let year = if parts[2].len() <= 2 { expand_year(y) } else { y };
        make_date(year, month, day).ok()
    }
}

fn expand_year(y: i32) -> i32 {
    if y >= 100 {
        y
    } else if y <= 49 {
        2000 + y
    } else {
        1900 + y
    }
}

fn make_date(year: i32, month: u32, day: u32) -> Result<NaiveDate, String> {
    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| format!("Ugyldig dato: {}-{}-{}", day, month, year))
}

/// Format a NaiveDate according to the date_format string.
pub fn format_date(date: NaiveDate, order: DateOrder) -> String {
    match order {
        DateOrder::DayFirst => date.format("%d-%m-%Y").to_string(),
        DateOrder::MonthFirst => date.format("%m-%d-%Y").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn ref_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()
    }

    #[test]
    fn today_shortcuts() {
        let r = ref_date();
        assert_eq!(parse_date_shorthand("t", r, DateOrder::DayFirst).unwrap(), r);
        assert_eq!(parse_date_shorthand("T", r, DateOrder::DayFirst).unwrap(), r);
        assert_eq!(parse_date_shorthand("d", r, DateOrder::DayFirst).unwrap(), r);
        assert_eq!(parse_date_shorthand("D", r, DateOrder::DayFirst).unwrap(), r);
    }

    #[test]
    fn today_offset() {
        let r = ref_date();
        assert_eq!(
            parse_date_shorthand("t+1", r, DateOrder::DayFirst).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()
        );
        assert_eq!(
            parse_date_shorthand("d-5", r, DateOrder::DayFirst).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 5).unwrap()
        );
    }

    #[test]
    fn day_only() {
        let r = ref_date();
        assert_eq!(
            parse_date_shorthand("12", r, DateOrder::DayFirst).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 12).unwrap()
        );
        assert_eq!(
            parse_date_shorthand("1", r, DateOrder::DayFirst).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()
        );
    }

    #[test]
    fn day_and_month() {
        let r = ref_date();
        // DD-MM format
        assert_eq!(
            parse_date_shorthand("1203", r, DateOrder::DayFirst).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 12).unwrap()
        );
        assert_eq!(
            parse_date_shorthand("12-03", r, DateOrder::DayFirst).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 12).unwrap()
        );
        // MM-DD format
        assert_eq!(
            parse_date_shorthand("0312", r, DateOrder::MonthFirst).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 12).unwrap()
        );
    }

    #[test]
    fn full_date_short_year() {
        let r = ref_date();
        assert_eq!(
            parse_date_shorthand("120326", r, DateOrder::DayFirst).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 12).unwrap()
        );
        assert_eq!(
            parse_date_shorthand("12-03-26", r, DateOrder::DayFirst).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 12).unwrap()
        );
    }

    #[test]
    fn full_date_long_year() {
        let r = ref_date();
        assert_eq!(
            parse_date_shorthand("12032026", r, DateOrder::DayFirst).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 12).unwrap()
        );
        assert_eq!(
            parse_date_shorthand("12-03-2026", r, DateOrder::DayFirst).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 12).unwrap()
        );
    }

    #[test]
    fn invalid_inputs() {
        let r = ref_date();
        assert!(parse_date_shorthand("hello", r, DateOrder::DayFirst).is_err());
        assert!(parse_date_shorthand("32", r, DateOrder::DayFirst).is_err());
        assert!(parse_date_shorthand("", r, DateOrder::DayFirst).is_err());
    }

    #[test]
    fn year_expansion() {
        assert_eq!(expand_year(26), 2026);
        assert_eq!(expand_year(0), 2000);
        assert_eq!(expand_year(49), 2049);
        assert_eq!(expand_year(50), 1950);
        assert_eq!(expand_year(99), 1999);
        assert_eq!(expand_year(2026), 2026);
    }
}
