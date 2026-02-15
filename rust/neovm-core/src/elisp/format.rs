//! Format-spec and advanced string formatting builtins.
//!
//! Pure builtins (`Vec<Value> -> EvalResult`):
//! - `format-spec` — format a string with named %-specs from an alist
//! - `format-time-string` — format time like strftime
//! - `format-seconds` — format a time duration string
//! - `string-pad` — pad string to a given length
//! - `string-chop-newline` — remove trailing newline
//! - `string-lines` — split string into lines
//! - `string-clean-whitespace` — collapse whitespace and trim
//! - `string-fill` — fill/wrap text at a given column width
//! - `string-limit` — truncate string to a given length
//! - `string-pixel-width` — batch-compatible display-column width
//! - `string-glyph-split` — split string into grapheme clusters (chars)
//! - `string-equal-ignore-case` — case-insensitive string comparison

use super::error::{signal, EvalResult, Flow};
use super::value::*;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_max_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Flow> {
    if args.len() < min || args.len() > max {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn require_string(name: &str, val: &Value) -> Result<String, Flow> {
    match val {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

fn require_int(name: &str, val: &Value) -> Result<i64, Flow> {
    match val {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// format-spec
// ---------------------------------------------------------------------------

/// `(format-spec TEMPLATE SPEC-ALIST)` -- format a string with named %-specs.
///
/// TEMPLATE contains `%x` sequences where `x` is a character.  SPEC-ALIST is
/// an alist mapping characters (integers) to replacement strings.
///
/// Supports `%%` for a literal `%`, and formatting flags:
/// `%<flags><width><spec-char>` where flags can be `-` (left-align),
/// `0` (zero-pad), and width is a decimal integer.
///
/// Example:
/// ```elisp
/// (format-spec "%n is %a" '((?n . "Bob") (?a . "21")))
/// ;; => "Bob is 21"
/// ```
pub(crate) fn builtin_format_spec(args: Vec<Value>) -> EvalResult {
    expect_args("format-spec", &args, 2)?;
    let template = require_string("format-spec", &args[0])?;

    // Parse spec-alist: list of (CHAR . STRING) pairs.
    let alist_items = match list_to_vec(&args[1]) {
        Some(items) => items,
        None => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("listp"), args[1].clone()],
            ));
        }
    };

    // Build lookup: char -> replacement string.
    let mut specs: Vec<(char, String)> = Vec::new();
    for item in &alist_items {
        match item {
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                let ch = match &pair.car {
                    Value::Int(n) => char::from_u32(*n as u32).unwrap_or('?'),
                    Value::Char(c) => *c,
                    _ => continue,
                };
                let replacement = match &pair.cdr {
                    Value::Str(s) => (**s).clone(),
                    Value::Int(n) => n.to_string(),
                    Value::Float(f) => f.to_string(),
                    Value::Nil => "nil".to_string(),
                    Value::True => "t".to_string(),
                    Value::Symbol(s) => s.clone(),
                    other => format!("{}", other),
                };
                specs.push((ch, replacement));
            }
            _ => continue,
        }
    }

    let mut result = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '%' {
            i += 1;
            if i >= chars.len() {
                // Trailing % -- just emit it.
                result.push('%');
                break;
            }

            // Literal %%
            if chars[i] == '%' {
                result.push('%');
                i += 1;
                continue;
            }

            // Parse optional flags: '-' and '0'
            let mut left_align = false;
            let mut zero_pad = false;
            while i < chars.len() && (chars[i] == '-' || chars[i] == '0') {
                if chars[i] == '-' {
                    left_align = true;
                } else {
                    zero_pad = true;
                }
                i += 1;
            }

            // Parse optional width
            let mut width: Option<usize> = None;
            let width_start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i > width_start {
                let width_str: String = chars[width_start..i].iter().collect();
                width = width_str.parse::<usize>().ok();
            }

            if i >= chars.len() {
                // Malformed spec at end of string -- emit what we parsed.
                result.push('%');
                if left_align {
                    result.push('-');
                }
                if zero_pad {
                    result.push('0');
                }
                if let Some(w) = width {
                    result.push_str(&w.to_string());
                }
                break;
            }

            let spec_char = chars[i];
            i += 1;

            // Look up replacement.
            let replacement = specs
                .iter()
                .find(|(ch, _)| *ch == spec_char)
                .map(|(_, s)| s.clone());

            match replacement {
                Some(rep) => {
                    if let Some(w) = width {
                        if rep.len() < w {
                            let padding = w - rep.len();
                            let pad_char = if zero_pad && !left_align { '0' } else { ' ' };
                            if left_align {
                                result.push_str(&rep);
                                for _ in 0..padding {
                                    result.push(pad_char);
                                }
                            } else {
                                for _ in 0..padding {
                                    result.push(pad_char);
                                }
                                result.push_str(&rep);
                            }
                        } else {
                            result.push_str(&rep);
                        }
                    } else {
                        result.push_str(&rep);
                    }
                }
                None => {
                    // Unknown spec -- leave it as-is (Emacs behavior: signal error,
                    // but many callers rely on pass-through).
                    result.push('%');
                    if left_align {
                        result.push('-');
                    }
                    if zero_pad {
                        result.push('0');
                    }
                    if let Some(w) = width {
                        result.push_str(&w.to_string());
                    }
                    result.push(spec_char);
                }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    Ok(Value::string(result))
}

// ---------------------------------------------------------------------------
// format-time-string
// ---------------------------------------------------------------------------

/// Broken-down time fields computed from a Unix timestamp.
struct BrokenDownTime {
    year: i64,
    month: u32,   // 1..=12
    day: u32,     // 1..=31
    hour: u32,    // 0..=23
    minute: u32,  // 0..=59
    second: u32,  // 0..=60 (leap second)
    weekday: u32, // 0=Sunday .. 6=Saturday
    yearday: u32, // 0..=365
}

/// Whether a year is a leap year (Gregorian).
fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Days in each month for a given year.
fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 => 31,
        2 => {
            if is_leap_year(y) {
                29
            } else {
                28
            }
        }
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        12 => 31,
        _ => 30,
    }
}

/// Convert a Unix timestamp (seconds since 1970-01-01 00:00:00 UTC) into
/// broken-down UTC time fields.  No external crate needed.
fn unix_to_broken_down(timestamp: i64) -> BrokenDownTime {
    // Handle negative timestamps (before epoch).
    let mut remaining = timestamp;
    let second_of_day;
    let mut day_count; // days since epoch (can be negative)

    if remaining >= 0 {
        day_count = remaining / 86400;
        second_of_day = (remaining % 86400) as u32;
    } else {
        // For negative timestamps, adjust so second_of_day is non-negative.
        day_count = (remaining - 86399) / 86400; // floor division
        let rem = remaining - day_count * 86400;
        second_of_day = rem as u32;
    }

    let hour = second_of_day / 3600;
    let minute = (second_of_day % 3600) / 60;
    let second = second_of_day % 60;

    // Weekday: 1970-01-01 was a Thursday (4).
    let weekday = ((day_count % 7 + 4 + 7) % 7) as u32; // 0=Sunday

    // Convert day_count to year/month/day.
    // day_count is days since 1970-01-01.
    let mut year: i64 = 1970;

    if day_count >= 0 {
        loop {
            let days_in_year = if is_leap_year(year) { 366 } else { 365 };
            if day_count < days_in_year {
                break;
            }
            day_count -= days_in_year;
            year += 1;
        }
    } else {
        loop {
            year -= 1;
            let days_in_year = if is_leap_year(year) { 366 } else { 365 };
            day_count += days_in_year;
            if day_count >= 0 {
                break;
            }
        }
    }

    let yearday = day_count as u32;

    // Now day_count is the 0-based day within `year`.
    let mut month = 1u32;
    let mut remaining_days = day_count as u32;
    loop {
        let dim = days_in_month(year, month);
        if remaining_days < dim {
            break;
        }
        remaining_days -= dim;
        month += 1;
        if month > 12 {
            break;
        }
    }
    let day = remaining_days + 1;

    BrokenDownTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        weekday,
        yearday,
    }
}

const DAY_NAMES: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

const DAY_ABBREVS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const MONTH_ABBREVS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `(format-time-string FORMAT-STRING &optional TIME ZONE)` -- format time
/// like C `strftime`.
///
/// Supported directives:
/// `%Y` year, `%m` month (01-12), `%d` day (01-31), `%H` hour (00-23),
/// `%M` minute (00-59), `%S` second (00-60), `%A` full day name,
/// `%a` abbreviated day name, `%B` full month name, `%b`/`%h` abbreviated
/// month name, `%Z` timezone name, `%z` numeric timezone offset,
/// `%j` day of year (001-366), `%e` day space-padded, `%k` hour space-padded,
/// `%l` 12-hour space-padded, `%I` 12-hour zero-padded, `%p` AM/PM,
/// `%P` am/pm, `%n` newline, `%t` tab, `%%` literal `%`.
///
/// If TIME is nil, uses current system time.  ZONE is currently ignored (UTC
/// assumed).
pub(crate) fn builtin_format_time_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("format-time-string", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("format-time-string"),
                Value::Int(args.len() as i64),
            ],
        ));
    }

    let format_str = require_string("format-time-string", &args[0])?;

    // Determine timestamp.
    let timestamp: i64 = if args.len() >= 2 && !args[1].is_nil() {
        match &args[1] {
            Value::Int(n) => *n,
            Value::Float(f) => *f as i64,
            Value::Cons(_) => {
                // Emacs time value: (HIGH LOW) or (HIGH LOW USEC) or (HIGH LOW USEC PSEC).
                // Decode as HIGH * 65536 + LOW.
                let items = list_to_vec(&args[1]).unwrap_or_default();
                if items.len() >= 2 {
                    let high = items[0].as_int().unwrap_or(0);
                    let low = items[1].as_int().unwrap_or(0);
                    high * 65536 + low
                } else {
                    current_unix_timestamp()
                }
            }
            _ => current_unix_timestamp(),
        }
    } else {
        current_unix_timestamp()
    };

    let tm = unix_to_broken_down(timestamp);
    let formatted = format_time(&format_str, &tm);
    Ok(Value::string(formatted))
}

/// Get current Unix timestamp using `std::time::SystemTime`.
fn current_unix_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Format a broken-down time according to a strftime-like format string.
fn format_time(fmt: &str, tm: &BrokenDownTime) -> String {
    let mut result = String::new();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '%' {
            i += 1;
            if i >= chars.len() {
                result.push('%');
                break;
            }

            // Handle optional '-' flag to suppress padding.
            let suppress_pad = if chars[i] == '-' {
                i += 1;
                true
            } else {
                false
            };

            if i >= chars.len() {
                result.push('%');
                if suppress_pad {
                    result.push('-');
                }
                break;
            }

            match chars[i] {
                '%' => result.push('%'),
                'Y' => result.push_str(&format!("{:04}", tm.year)),
                'y' => result.push_str(&format!("{:02}", tm.year % 100)),
                'C' => result.push_str(&format!("{:02}", tm.year / 100)),
                'm' => {
                    if suppress_pad {
                        result.push_str(&tm.month.to_string());
                    } else {
                        result.push_str(&format!("{:02}", tm.month));
                    }
                }
                'd' => {
                    if suppress_pad {
                        result.push_str(&tm.day.to_string());
                    } else {
                        result.push_str(&format!("{:02}", tm.day));
                    }
                }
                'e' => result.push_str(&format!("{:2}", tm.day)),
                'H' => {
                    if suppress_pad {
                        result.push_str(&tm.hour.to_string());
                    } else {
                        result.push_str(&format!("{:02}", tm.hour));
                    }
                }
                'k' => result.push_str(&format!("{:2}", tm.hour)),
                'I' => {
                    let h12 = if tm.hour == 0 {
                        12
                    } else if tm.hour > 12 {
                        tm.hour - 12
                    } else {
                        tm.hour
                    };
                    if suppress_pad {
                        result.push_str(&h12.to_string());
                    } else {
                        result.push_str(&format!("{:02}", h12));
                    }
                }
                'l' => {
                    let h12 = if tm.hour == 0 {
                        12
                    } else if tm.hour > 12 {
                        tm.hour - 12
                    } else {
                        tm.hour
                    };
                    result.push_str(&format!("{:2}", h12));
                }
                'M' => {
                    if suppress_pad {
                        result.push_str(&tm.minute.to_string());
                    } else {
                        result.push_str(&format!("{:02}", tm.minute));
                    }
                }
                'S' => {
                    if suppress_pad {
                        result.push_str(&tm.second.to_string());
                    } else {
                        result.push_str(&format!("{:02}", tm.second));
                    }
                }
                'A' => result.push_str(DAY_NAMES[tm.weekday as usize % 7]),
                'a' => result.push_str(DAY_ABBREVS[tm.weekday as usize % 7]),
                'B' => result.push_str(MONTH_NAMES[(tm.month as usize).saturating_sub(1) % 12]),
                'b' | 'h' => {
                    result.push_str(MONTH_ABBREVS[(tm.month as usize).saturating_sub(1) % 12])
                }
                'p' => result.push_str(if tm.hour < 12 { "AM" } else { "PM" }),
                'P' => result.push_str(if tm.hour < 12 { "am" } else { "pm" }),
                'Z' => result.push_str("UTC"),
                'z' => result.push_str("+0000"),
                'j' => {
                    if suppress_pad {
                        result.push_str(&(tm.yearday + 1).to_string());
                    } else {
                        result.push_str(&format!("{:03}", tm.yearday + 1));
                    }
                }
                'u' => {
                    // ISO weekday: 1=Monday .. 7=Sunday
                    let iso_wd = if tm.weekday == 0 { 7 } else { tm.weekday };
                    result.push_str(&iso_wd.to_string());
                }
                'w' => result.push_str(&tm.weekday.to_string()),
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'R' => result.push_str(&format!("{:02}:{:02}", tm.hour, tm.minute)),
                'T' => {
                    result.push_str(&format!("{:02}:{:02}:{:02}", tm.hour, tm.minute, tm.second))
                }
                'F' => result.push_str(&format!("{:04}-{:02}-{:02}", tm.year, tm.month, tm.day)),
                'D' => result.push_str(&format!(
                    "{:02}/{:02}/{:02}",
                    tm.month,
                    tm.day,
                    tm.year % 100
                )),
                other => {
                    // Unknown directive -- emit as-is.
                    result.push('%');
                    if suppress_pad {
                        result.push('-');
                    }
                    result.push(other);
                }
            }
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

// ---------------------------------------------------------------------------
// format-seconds
// ---------------------------------------------------------------------------

/// `(format-seconds STRING SECONDS)` -- format a time duration.
///
/// STRING is a format template containing these directives:
/// - `%y` years (365.25 days), `%d` days, `%h` hours, `%m` minutes,
///   `%s` seconds, `%z` suppress trailing zero-value fields,
///   `%%` literal `%`.
///
/// Each field consumes from the total remaining seconds.
pub(crate) fn builtin_format_seconds(args: Vec<Value>) -> EvalResult {
    expect_args("format-seconds", &args, 2)?;
    let format_str = require_string("format-seconds", &args[0])?;

    let total_seconds: f64 = match &args[1] {
        Value::Int(n) => *n as f64,
        Value::Float(f) => *f,
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("numberp"), other.clone()],
            ));
        }
    };

    let mut remaining = total_seconds.abs();
    let negative = total_seconds < 0.0;

    // First pass: determine which fields are present and compute values.
    // We need to scan the format string to know which units are requested,
    // then allocate seconds top-down.
    let chars: Vec<char> = format_str.chars().collect();

    let mut directives: Vec<(usize, char)> = Vec::new(); // (index in chars, kind)
    {
        let mut j = 0;
        while j < chars.len() {
            if chars[j] == '%' {
                j += 1;
                if j < chars.len() {
                    match chars[j] {
                        'y' | 'd' | 'h' | 'm' | 's' => {
                            directives.push((j, chars[j]));
                        }
                        _ => {}
                    }
                    j += 1;
                }
            } else {
                j += 1;
            }
        }
    }

    // Compute values top-down.
    let mut values: Vec<(char, u64)> = Vec::new();
    for &(_idx, kind) in &directives {
        let divisor: f64 = match kind {
            'y' => 365.25 * 24.0 * 3600.0,
            'd' => 24.0 * 3600.0,
            'h' => 3600.0,
            'm' => 60.0,
            's' => 1.0,
            _ => 1.0,
        };
        let val = (remaining / divisor).floor() as u64;
        remaining -= val as f64 * divisor;
        if remaining < 0.0 {
            remaining = 0.0;
        }
        values.push((kind, val));
    }

    // Check for %z -- suppress trailing zero-value fields.
    let has_z = format_str.contains("%z");

    // Determine which fields to suppress (trailing zeros).
    let mut suppress_from = values.len();
    if has_z {
        // Find the last non-zero value.
        let mut last_nonzero = None;
        for (idx, &(_, val)) in values.iter().enumerate() {
            if val > 0 {
                last_nonzero = Some(idx);
            }
        }
        suppress_from = match last_nonzero {
            Some(idx) => idx + 1,
            None => 0, // All zero -- show at least the last field.
        };
        if suppress_from == 0 && !values.is_empty() {
            suppress_from = values.len(); // Show the last field at minimum.
        }
    }

    // Second pass: build result.
    let mut result = String::new();
    if negative {
        result.push('-');
    }
    let mut value_idx = 0;
    let mut suppressing = false;
    let mut j = 0;

    while j < chars.len() {
        if chars[j] == '%' {
            j += 1;
            if j >= chars.len() {
                result.push('%');
                break;
            }
            match chars[j] {
                '%' => result.push('%'),
                'z' => {
                    // %z itself produces no output; it just marks suppression.
                    // Suppression of trailing zeros is handled above.
                }
                'y' | 'd' | 'h' | 'm' | 's' => {
                    if value_idx < values.len() {
                        let (_kind, val) = values[value_idx];
                        if has_z && value_idx >= suppress_from {
                            // Suppress this field and any following literal text
                            // until the next directive.
                            suppressing = true;
                        } else {
                            suppressing = false;
                            result.push_str(&val.to_string());
                        }
                        value_idx += 1;
                    }
                }
                other => {
                    result.push('%');
                    result.push(other);
                }
            }
            j += 1;
        } else {
            if !suppressing {
                result.push(chars[j]);
            }
            j += 1;
        }
    }

    Ok(Value::string(result))
}

// ---------------------------------------------------------------------------
// string-pad
// ---------------------------------------------------------------------------

/// `(string-pad STRING LENGTH &optional PADDING START)` -- pad STRING to
/// LENGTH characters.
///
/// PADDING defaults to space.  If START is non-nil, pad at the start
/// (right-align); otherwise pad at the end (left-align).
pub(crate) fn builtin_string_pad(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("string-pad", &args, 2, 4)?;
    let s = require_string("string-pad", &args[0])?;
    let length = require_int("string-pad", &args[1])? as usize;

    let padding = if args.len() >= 3 && !args[2].is_nil() {
        match &args[2] {
            Value::Char(c) => *c,
            Value::Int(n) => char::from_u32(*n as u32).unwrap_or(' '),
            _ => ' ',
        }
    } else {
        ' '
    };

    let at_start = args.len() >= 4 && args[3].is_truthy();

    let char_count = s.chars().count();
    if char_count >= length {
        // Truncate to length if longer.
        let truncated: String = s.chars().take(length).collect();
        Ok(Value::string(truncated))
    } else {
        let pad_count = length - char_count;
        let pad_str: String = std::iter::repeat(padding).take(pad_count).collect();
        if at_start {
            Ok(Value::string(format!("{}{}", pad_str, s)))
        } else {
            Ok(Value::string(format!("{}{}", s, pad_str)))
        }
    }
}

// ---------------------------------------------------------------------------
// string-chop-newline
// ---------------------------------------------------------------------------

/// `(string-chop-newline STRING)` -- remove a single trailing newline, if
/// present.
pub(crate) fn builtin_string_chop_newline(args: Vec<Value>) -> EvalResult {
    expect_args("string-chop-newline", &args, 1)?;
    let s = require_string("string-chop-newline", &args[0])?;
    if s.ends_with('\n') {
        Ok(Value::string(&s[..s.len() - 1]))
    } else {
        Ok(Value::string(s))
    }
}

// ---------------------------------------------------------------------------
// string-lines
// ---------------------------------------------------------------------------

/// `(string-lines STRING &optional OMIT-NULLS)` -- split STRING into a list
/// of lines.  If OMIT-NULLS is non-nil, empty strings are omitted.
pub(crate) fn builtin_string_lines(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("string-lines", &args, 1, 2)?;
    let s = require_string("string-lines", &args[0])?;
    let omit_nulls = args.len() >= 2 && args[1].is_truthy();

    let lines: Vec<Value> = s
        .split('\n')
        .filter(|line| !omit_nulls || !line.is_empty())
        .map(|line| Value::string(line))
        .collect();

    Ok(Value::list(lines))
}

// ---------------------------------------------------------------------------
// string-clean-whitespace
// ---------------------------------------------------------------------------

/// `(string-clean-whitespace STRING)` -- collapse runs of whitespace into
/// single spaces and trim leading/trailing whitespace.
pub(crate) fn builtin_string_clean_whitespace(args: Vec<Value>) -> EvalResult {
    expect_args("string-clean-whitespace", &args, 1)?;
    let s = require_string("string-clean-whitespace", &args[0])?;

    let mut result = String::new();
    let mut in_whitespace = false;

    for ch in s.chars() {
        if ch.is_whitespace() {
            if !in_whitespace && !result.is_empty() {
                result.push(' ');
            }
            in_whitespace = true;
        } else {
            in_whitespace = false;
            result.push(ch);
        }
    }

    // Trim trailing space that might have been added.
    if result.ends_with(' ') {
        result.pop();
    }

    Ok(Value::string(result))
}

// ---------------------------------------------------------------------------
// string-fill
// ---------------------------------------------------------------------------

/// `(string-fill STRING LENGTH)` -- fill (word-wrap) STRING at LENGTH columns.
///
/// Breaks at word boundaries where possible.  Preserves existing newlines.
pub(crate) fn builtin_string_fill(args: Vec<Value>) -> EvalResult {
    expect_args("string-fill", &args, 2)?;
    let s = require_string("string-fill", &args[0])?;
    let length = require_int("string-fill", &args[1])? as usize;

    if length == 0 {
        return Ok(Value::string(s));
    }

    let mut result = String::new();

    // Process each paragraph (split by existing newlines) separately.
    for (para_idx, paragraph) in s.split('\n').enumerate() {
        if para_idx > 0 {
            result.push('\n');
        }

        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }

        let mut line_len: usize = 0;
        for (word_idx, word) in words.iter().enumerate() {
            let word_len = word.chars().count();
            if word_idx == 0 {
                result.push_str(word);
                line_len = word_len;
            } else if line_len + 1 + word_len <= length {
                result.push(' ');
                result.push_str(word);
                line_len += 1 + word_len;
            } else {
                result.push('\n');
                result.push_str(word);
                line_len = word_len;
            }
        }
    }

    Ok(Value::string(result))
}

// ---------------------------------------------------------------------------
// string-limit
// ---------------------------------------------------------------------------

/// `(string-limit STRING LENGTH &optional END CODING-SYSTEM)` -- truncate
/// STRING to at most LENGTH characters.
///
/// If END is non-nil, take from the end instead of the beginning.
/// CODING-SYSTEM is accepted but ignored (we always count characters).
pub(crate) fn builtin_string_limit(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("string-limit", &args, 2, 4)?;
    let s = require_string("string-limit", &args[0])?;
    let length = require_int("string-limit", &args[1])? as usize;
    let from_end = args.len() >= 3 && args[2].is_truthy();

    let char_count = s.chars().count();
    if char_count <= length {
        Ok(Value::string(s))
    } else if from_end {
        let skip = char_count - length;
        let truncated: String = s.chars().skip(skip).collect();
        Ok(Value::string(truncated))
    } else {
        let truncated: String = s.chars().take(length).collect();
        Ok(Value::string(truncated))
    }
}

// ---------------------------------------------------------------------------
// string-pixel-width
// ---------------------------------------------------------------------------

fn string_pixel_width(s: &str) -> i64 {
    let mut columns = 0usize;
    for ch in s.chars() {
        if ch == '\t' {
            let tab_width = 8usize;
            columns += tab_width - (columns % tab_width);
        } else {
            columns += crate::encoding::char_width(ch);
        }
    }
    columns as i64
}

/// `(string-pixel-width STRING)` -- return the display-column width of STRING
/// in batch mode.
///
/// Oracle behavior in batch is column-based (not GUI pixel metrics), including
/// tab expansion to the next 8-column boundary.
pub(crate) fn builtin_string_pixel_width(args: Vec<Value>) -> EvalResult {
    expect_args("string-pixel-width", &args, 1)?;
    let s = require_string("string-pixel-width", &args[0])?;
    Ok(Value::Int(string_pixel_width(&s)))
}

// ---------------------------------------------------------------------------
// string-glyph-split
// ---------------------------------------------------------------------------

/// `(string-glyph-split STRING)` -- split STRING into a list of grapheme
/// clusters.
///
/// Simplified implementation: splits into individual characters (a full
/// implementation would use Unicode grapheme cluster segmentation).
pub(crate) fn builtin_string_glyph_split(args: Vec<Value>) -> EvalResult {
    expect_args("string-glyph-split", &args, 1)?;
    let s = require_string("string-glyph-split", &args[0])?;
    let chars: Vec<Value> = s.chars().map(|c| Value::string(c.to_string())).collect();
    Ok(Value::list(chars))
}

// ---------------------------------------------------------------------------
// string-equal-ignore-case
// ---------------------------------------------------------------------------

/// `(string-equal-ignore-case S1 S2)` -- case-insensitive string comparison.
///
/// Returns `t` if S1 and S2 are equal when compared case-insensitively,
/// `nil` otherwise.
pub(crate) fn builtin_string_equal_ignore_case(args: Vec<Value>) -> EvalResult {
    expect_args("string-equal-ignore-case", &args, 2)?;
    let s1 = require_string("string-equal-ignore-case", &args[0])?;
    let s2 = require_string("string-equal-ignore-case", &args[1])?;

    if s1.eq_ignore_ascii_case(&s2) {
        Ok(Value::True)
    } else {
        // Fall back to full Unicode casefold comparison.
        let fold1: String = s1.chars().flat_map(|c| c.to_lowercase()).collect();
        let fold2: String = s2.chars().flat_map(|c| c.to_lowercase()).collect();
        Ok(Value::bool(fold1 == fold2))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ===================================================================
    // format-spec tests
    // ===================================================================

    #[test]
    fn format_spec_basic() {
        let spec_alist = Value::list(vec![
            Value::cons(Value::Char('n'), Value::string("Bob")),
            Value::cons(Value::Char('a'), Value::string("21")),
        ]);
        let result = builtin_format_spec(vec![Value::string("%n is %a"), spec_alist]);
        assert_eq!(result.unwrap().as_str().unwrap(), "Bob is 21");
    }

    #[test]
    fn format_spec_literal_percent() {
        let result = builtin_format_spec(vec![
            Value::string("100%% done"),
            Value::Nil, // empty alist
        ]);
        assert_eq!(result.unwrap().as_str().unwrap(), "100% done");
    }

    #[test]
    fn format_spec_width_right_align() {
        let spec_alist = Value::list(vec![Value::cons(Value::Char('n'), Value::string("hi"))]);
        let result = builtin_format_spec(vec![Value::string("[%10n]"), spec_alist]);
        assert_eq!(result.unwrap().as_str().unwrap(), "[        hi]");
    }

    #[test]
    fn format_spec_width_left_align() {
        let spec_alist = Value::list(vec![Value::cons(Value::Char('n'), Value::string("hi"))]);
        let result = builtin_format_spec(vec![Value::string("[%-10n]"), spec_alist]);
        assert_eq!(result.unwrap().as_str().unwrap(), "[hi        ]");
    }

    #[test]
    fn format_spec_zero_pad() {
        let spec_alist = Value::list(vec![Value::cons(Value::Char('n'), Value::string("42"))]);
        let result = builtin_format_spec(vec![Value::string("[%05n]"), spec_alist]);
        assert_eq!(result.unwrap().as_str().unwrap(), "[00042]");
    }

    #[test]
    fn format_spec_no_match_passthrough() {
        let result = builtin_format_spec(vec![Value::string("hello %x world"), Value::Nil]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hello %x world");
    }

    #[test]
    fn format_spec_int_keys() {
        // Use integers instead of chars for the spec keys.
        let spec_alist = Value::list(vec![Value::cons(
            Value::Int('n' as i64),
            Value::string("Alice"),
        )]);
        let result = builtin_format_spec(vec![Value::string("Name: %n"), spec_alist]);
        assert_eq!(result.unwrap().as_str().unwrap(), "Name: Alice");
    }

    #[test]
    fn format_spec_wrong_args() {
        let result = builtin_format_spec(vec![Value::string("hi")]);
        assert!(result.is_err());
    }

    // ===================================================================
    // format-time-string tests
    // ===================================================================

    #[test]
    fn format_time_string_epoch() {
        // Unix epoch: 1970-01-01 00:00:00 UTC (Thursday)
        let result =
            builtin_format_time_string(vec![Value::string("%Y-%m-%d %H:%M:%S"), Value::Int(0)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "1970-01-01 00:00:00");
    }

    #[test]
    fn format_time_string_day_name() {
        // 1970-01-01 is a Thursday.
        let result = builtin_format_time_string(vec![Value::string("%A"), Value::Int(0)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "Thursday");
    }

    #[test]
    fn format_time_string_month_name() {
        let result = builtin_format_time_string(vec![Value::string("%B"), Value::Int(0)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "January");
    }

    #[test]
    fn format_time_string_known_date() {
        // 2000-01-01 00:00:00 UTC = 946684800
        let result =
            builtin_format_time_string(vec![Value::string("%Y-%m-%d %A"), Value::Int(946684800)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "2000-01-01 Saturday");
    }

    #[test]
    fn format_time_string_literal_percent() {
        let result = builtin_format_time_string(vec![Value::string("100%%"), Value::Int(0)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "100%");
    }

    #[test]
    fn format_time_string_timezone() {
        let result = builtin_format_time_string(vec![Value::string("%Z"), Value::Int(0)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "UTC");
    }

    #[test]
    fn format_time_string_iso_format() {
        let result =
            builtin_format_time_string(vec![Value::string("%F %T"), Value::Int(946684800)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "2000-01-01 00:00:00");
    }

    #[test]
    fn format_time_string_ampm() {
        // 2000-01-01 15:30:00 UTC = 946684800 + 15*3600 + 30*60 = 946740600
        let result =
            builtin_format_time_string(vec![Value::string("%I:%M %p"), Value::Int(946740600)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "03:30 PM");
    }

    #[test]
    fn format_time_string_no_time_uses_current() {
        // Should not error when TIME is nil.
        let result = builtin_format_time_string(vec![Value::string("%Y"), Value::Nil]);
        assert!(result.is_ok());
        // Should return a 4-digit year.
        let year_str = result.unwrap();
        assert_eq!(year_str.as_str().unwrap().len(), 4);
    }

    // ===================================================================
    // format-seconds tests
    // ===================================================================

    #[test]
    fn format_seconds_basic() {
        // 3661 seconds = 1 hour, 1 minute, 1 second
        let result = builtin_format_seconds(vec![Value::string("%h:%m:%s"), Value::Int(3661)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "1:1:1");
    }

    #[test]
    fn format_seconds_days() {
        // 90061 = 1 day + 1 hour + 1 minute + 1 second
        let result =
            builtin_format_seconds(vec![Value::string("%d days, %h:%m:%s"), Value::Int(90061)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "1 days, 1:1:1");
    }

    #[test]
    fn format_seconds_zero() {
        let result = builtin_format_seconds(vec![Value::string("%h:%m:%s"), Value::Int(0)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "0:0:0");
    }

    #[test]
    fn format_seconds_literal_percent() {
        let result = builtin_format_seconds(vec![Value::string("100%%"), Value::Int(0)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "100%");
    }

    // ===================================================================
    // string-pad tests
    // ===================================================================

    #[test]
    fn string_pad_right() {
        let result = builtin_string_pad(vec![Value::string("hi"), Value::Int(5)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hi   ");
    }

    #[test]
    fn string_pad_left() {
        let result = builtin_string_pad(vec![
            Value::string("hi"),
            Value::Int(5),
            Value::Nil,
            Value::True,
        ]);
        assert_eq!(result.unwrap().as_str().unwrap(), "   hi");
    }

    #[test]
    fn string_pad_custom_char() {
        let result = builtin_string_pad(vec![
            Value::string("hi"),
            Value::Int(5),
            Value::Char('.'),
            Value::Nil,
        ]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hi...");
    }

    #[test]
    fn string_pad_truncate() {
        let result = builtin_string_pad(vec![Value::string("hello world"), Value::Int(5)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hello");
    }

    #[test]
    fn string_pad_exact_length() {
        let result = builtin_string_pad(vec![Value::string("hello"), Value::Int(5)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hello");
    }

    // ===================================================================
    // string-chop-newline tests
    // ===================================================================

    #[test]
    fn string_chop_newline_present() {
        let result = builtin_string_chop_newline(vec![Value::string("hello\n")]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hello");
    }

    #[test]
    fn string_chop_newline_absent() {
        let result = builtin_string_chop_newline(vec![Value::string("hello")]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hello");
    }

    #[test]
    fn string_chop_newline_only_last() {
        let result = builtin_string_chop_newline(vec![Value::string("a\nb\n")]);
        assert_eq!(result.unwrap().as_str().unwrap(), "a\nb");
    }

    #[test]
    fn string_chop_newline_empty() {
        let result = builtin_string_chop_newline(vec![Value::string("")]);
        assert_eq!(result.unwrap().as_str().unwrap(), "");
    }

    // ===================================================================
    // string-lines tests
    // ===================================================================

    #[test]
    fn string_lines_basic() {
        let result = builtin_string_lines(vec![Value::string("a\nb\nc")]);
        let items = list_to_vec(&result.unwrap()).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].as_str().unwrap(), "a");
        assert_eq!(items[1].as_str().unwrap(), "b");
        assert_eq!(items[2].as_str().unwrap(), "c");
    }

    #[test]
    fn string_lines_trailing_newline() {
        let result = builtin_string_lines(vec![Value::string("a\nb\n")]);
        let items = list_to_vec(&result.unwrap()).unwrap();
        assert_eq!(items.len(), 3); // "a", "b", ""
        assert_eq!(items[2].as_str().unwrap(), "");
    }

    #[test]
    fn string_lines_omit_nulls() {
        let result = builtin_string_lines(vec![Value::string("a\n\nb\n"), Value::True]);
        let items = list_to_vec(&result.unwrap()).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_str().unwrap(), "a");
        assert_eq!(items[1].as_str().unwrap(), "b");
    }

    #[test]
    fn string_lines_empty_string() {
        let result = builtin_string_lines(vec![Value::string("")]);
        let items = list_to_vec(&result.unwrap()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_str().unwrap(), "");
    }

    // ===================================================================
    // string-clean-whitespace tests
    // ===================================================================

    #[test]
    fn string_clean_whitespace_basic() {
        let result = builtin_string_clean_whitespace(vec![Value::string("  hello   world  ")]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hello world");
    }

    #[test]
    fn string_clean_whitespace_tabs_and_newlines() {
        let result = builtin_string_clean_whitespace(vec![Value::string("a\t\tb\n\nc")]);
        assert_eq!(result.unwrap().as_str().unwrap(), "a b c");
    }

    #[test]
    fn string_clean_whitespace_no_change() {
        let result = builtin_string_clean_whitespace(vec![Value::string("hello world")]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hello world");
    }

    #[test]
    fn string_clean_whitespace_empty() {
        let result = builtin_string_clean_whitespace(vec![Value::string("")]);
        assert_eq!(result.unwrap().as_str().unwrap(), "");
    }

    #[test]
    fn string_clean_whitespace_only_spaces() {
        let result = builtin_string_clean_whitespace(vec![Value::string("   ")]);
        assert_eq!(result.unwrap().as_str().unwrap(), "");
    }

    // ===================================================================
    // string-fill tests
    // ===================================================================

    #[test]
    fn string_fill_basic() {
        let result =
            builtin_string_fill(vec![Value::string("hello world foo bar"), Value::Int(10)]);
        let filled = result.unwrap();
        let s = filled.as_str().unwrap();
        // "hello" + " " + "world" = 11 > 10, so "world" goes to next line.
        assert!(s.contains('\n'));
        let lines: Vec<&str> = s.split('\n').collect();
        assert!(lines[0].len() <= 11); // "hello world" fits in 11 but we set 10
    }

    #[test]
    fn string_fill_short_text() {
        let result = builtin_string_fill(vec![Value::string("hi"), Value::Int(80)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hi");
    }

    #[test]
    fn string_fill_preserves_newlines() {
        let result = builtin_string_fill(vec![
            Value::string("paragraph one\nparagraph two"),
            Value::Int(80),
        ]);
        let s = result.unwrap();
        assert!(s.as_str().unwrap().contains('\n'));
    }

    // ===================================================================
    // string-limit tests
    // ===================================================================

    #[test]
    fn string_limit_from_start() {
        let result = builtin_string_limit(vec![Value::string("hello world"), Value::Int(5)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hello");
    }

    #[test]
    fn string_limit_from_end() {
        let result = builtin_string_limit(vec![
            Value::string("hello world"),
            Value::Int(5),
            Value::True,
        ]);
        assert_eq!(result.unwrap().as_str().unwrap(), "world");
    }

    #[test]
    fn string_limit_no_truncation() {
        let result = builtin_string_limit(vec![Value::string("hi"), Value::Int(10)]);
        assert_eq!(result.unwrap().as_str().unwrap(), "hi");
    }

    #[test]
    fn string_limit_unicode() {
        // Unicode chars should be counted by character, not byte.
        let result = builtin_string_limit(vec![Value::string("cafe\u{0301}"), Value::Int(4)]);
        let s = result.unwrap();
        // "cafe\u{0301}" has 5 chars (c, a, f, e, combining accent).
        assert_eq!(s.as_str().unwrap().chars().count(), 4);
    }

    // ===================================================================
    // string-pixel-width tests
    // ===================================================================

    #[test]
    fn string_pixel_width_basic() {
        let result = builtin_string_pixel_width(vec![Value::string("hello")]);
        assert_eq!(result.unwrap().as_int().unwrap(), 5);
    }

    #[test]
    fn string_pixel_width_empty() {
        let result = builtin_string_pixel_width(vec![Value::string("")]);
        assert_eq!(result.unwrap().as_int().unwrap(), 0);
    }

    #[test]
    fn string_pixel_width_tabs_and_wide_chars() {
        assert_eq!(
            builtin_string_pixel_width(vec![Value::string("\t")])
                .unwrap()
                .as_int()
                .unwrap(),
            8
        );
        assert_eq!(
            builtin_string_pixel_width(vec![Value::string("a\t")])
                .unwrap()
                .as_int()
                .unwrap(),
            8
        );
        assert_eq!(
            builtin_string_pixel_width(vec![Value::string("a\tb")])
                .unwrap()
                .as_int()
                .unwrap(),
            9
        );
        assert_eq!(
            builtin_string_pixel_width(vec![Value::string("漢字")])
                .unwrap()
                .as_int()
                .unwrap(),
            4
        );
        assert_eq!(
            builtin_string_pixel_width(vec![Value::string("e\u{0301}")])
                .unwrap()
                .as_int()
                .unwrap(),
            1
        );
    }

    // ===================================================================
    // string-glyph-split tests
    // ===================================================================

    #[test]
    fn string_glyph_split_basic() {
        let result = builtin_string_glyph_split(vec![Value::string("abc")]);
        let items = list_to_vec(&result.unwrap()).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].as_str().unwrap(), "a");
        assert_eq!(items[1].as_str().unwrap(), "b");
        assert_eq!(items[2].as_str().unwrap(), "c");
    }

    #[test]
    fn string_glyph_split_empty() {
        let result = builtin_string_glyph_split(vec![Value::string("")]);
        let items = list_to_vec(&result.unwrap()).unwrap();
        assert_eq!(items.len(), 0);
    }

    #[test]
    fn string_glyph_split_unicode() {
        let result = builtin_string_glyph_split(vec![Value::string("\u{1F600}")]);
        let items = list_to_vec(&result.unwrap()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_str().unwrap(), "\u{1F600}");
    }

    // ===================================================================
    // string-equal-ignore-case tests
    // ===================================================================

    #[test]
    fn string_equal_ignore_case_equal() {
        let result =
            builtin_string_equal_ignore_case(vec![Value::string("Hello"), Value::string("hello")]);
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn string_equal_ignore_case_not_equal() {
        let result =
            builtin_string_equal_ignore_case(vec![Value::string("Hello"), Value::string("world")]);
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn string_equal_ignore_case_identical() {
        let result =
            builtin_string_equal_ignore_case(vec![Value::string("abc"), Value::string("abc")]);
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn string_equal_ignore_case_empty() {
        let result = builtin_string_equal_ignore_case(vec![Value::string(""), Value::string("")]);
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn string_equal_ignore_case_unicode() {
        let result = builtin_string_equal_ignore_case(vec![
            Value::string("\u{00DF}"), // German sharp s
            Value::string("\u{00DF}"),
        ]);
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn string_equal_ignore_case_wrong_type() {
        let result = builtin_string_equal_ignore_case(vec![Value::Int(42), Value::string("hello")]);
        assert!(result.is_err());
    }

    // ===================================================================
    // unix_to_broken_down internal tests
    // ===================================================================

    #[test]
    fn broken_down_epoch() {
        let tm = unix_to_broken_down(0);
        assert_eq!(tm.year, 1970);
        assert_eq!(tm.month, 1);
        assert_eq!(tm.day, 1);
        assert_eq!(tm.hour, 0);
        assert_eq!(tm.minute, 0);
        assert_eq!(tm.second, 0);
        assert_eq!(tm.weekday, 4); // Thursday
    }

    #[test]
    fn broken_down_y2k() {
        // 2000-01-01 00:00:00 UTC = 946684800
        let tm = unix_to_broken_down(946684800);
        assert_eq!(tm.year, 2000);
        assert_eq!(tm.month, 1);
        assert_eq!(tm.day, 1);
        assert_eq!(tm.weekday, 6); // Saturday
    }

    #[test]
    fn broken_down_leap_year() {
        // 2000-02-29 00:00:00 UTC = 946684800 + 59*86400 = 946684800 + 5097600 = 951782400
        let tm = unix_to_broken_down(951782400);
        assert_eq!(tm.year, 2000);
        assert_eq!(tm.month, 2);
        assert_eq!(tm.day, 29);
    }

    #[test]
    fn broken_down_end_of_day() {
        // 1970-01-01 23:59:59 = 86399
        let tm = unix_to_broken_down(86399);
        assert_eq!(tm.year, 1970);
        assert_eq!(tm.month, 1);
        assert_eq!(tm.day, 1);
        assert_eq!(tm.hour, 23);
        assert_eq!(tm.minute, 59);
        assert_eq!(tm.second, 59);
    }

    #[test]
    fn broken_down_2024() {
        // 2024-03-15 12:30:45 UTC
        // Compute: days from 1970 to 2024-03-15
        // Using known: 2024-01-01 = 1704067200
        // Jan has 31 days, Feb has 29 (2024 is leap), so Mar 15 = 31 + 29 + 14 = 74 days after Jan 1
        // 1704067200 + 74 * 86400 = 1704067200 + 6393600 = 1710460800
        // + 12*3600 + 30*60 + 45 = 43200 + 1800 + 45 = 45045
        // Total: 1710505845
        let tm = unix_to_broken_down(1710505845);
        assert_eq!(tm.year, 2024);
        assert_eq!(tm.month, 3);
        assert_eq!(tm.day, 15);
        assert_eq!(tm.hour, 12);
        assert_eq!(tm.minute, 30);
        assert_eq!(tm.second, 45);
    }
}
