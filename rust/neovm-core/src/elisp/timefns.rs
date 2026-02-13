//! Time and date builtins for the Elisp interpreter.
//!
//! Implements `current-time`, `float-time`, `time-add`, `time-subtract`,
//! `time-less-p`, `time-equal-p`, `current-time-string`, `current-time-zone`,
//! `encode-time`, `decode-time`, `time-convert`, `set-time-zone-rule`, and
//! `safe-date-to-time`.
//!
//! Uses only `std::time::SystemTime` and `std::time::UNIX_EPOCH`; no external
//! crates.

use super::error::{signal, EvalResult, Flow};
use super::value::*;
use std::time::{SystemTime, UNIX_EPOCH};

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

// ---------------------------------------------------------------------------
// Internal time representation
// ---------------------------------------------------------------------------

/// Internal microsecond-precision time (seconds + microseconds since epoch).
/// Allows negative values for times before the epoch.
#[derive(Clone, Copy, Debug)]
struct TimeMicros {
    /// Total seconds (may be negative).
    secs: i64,
    /// Microseconds within the current second, always in [0, 999_999].
    usecs: i64,
}

impl TimeMicros {
    fn now() -> Self {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(dur) => TimeMicros {
                secs: dur.as_secs() as i64,
                usecs: dur.subsec_micros() as i64,
            },
            Err(e) => {
                let dur = e.duration();
                TimeMicros {
                    secs: -(dur.as_secs() as i64),
                    usecs: -(dur.subsec_micros() as i64),
                }
            }
        }
    }

    fn to_list(&self) -> Value {
        let high = (self.secs >> 16) & 0xFFFF_FFFF;
        let low = self.secs & 0xFFFF;
        Value::list(vec![
            Value::Int(high),
            Value::Int(low),
            Value::Int(self.usecs),
            Value::Int(0), // PSEC
        ])
    }

    fn to_float(&self) -> f64 {
        self.secs as f64 + self.usecs as f64 / 1_000_000.0
    }

    fn add(self, other: TimeMicros) -> TimeMicros {
        let mut usecs = self.usecs + other.usecs;
        let mut secs = self.secs + other.secs;
        if usecs >= 1_000_000 {
            usecs -= 1_000_000;
            secs += 1;
        } else if usecs < 0 {
            usecs += 1_000_000;
            secs -= 1;
        }
        TimeMicros { secs, usecs }
    }

    fn sub(self, other: TimeMicros) -> TimeMicros {
        let mut usecs = self.usecs - other.usecs;
        let mut secs = self.secs - other.secs;
        if usecs < 0 {
            usecs += 1_000_000;
            secs -= 1;
        } else if usecs >= 1_000_000 {
            usecs -= 1_000_000;
            secs += 1;
        }
        TimeMicros { secs, usecs }
    }

    fn less_than(self, other: TimeMicros) -> bool {
        if self.secs != other.secs {
            self.secs < other.secs
        } else {
            self.usecs < other.usecs
        }
    }

    fn equal(self, other: TimeMicros) -> bool {
        self.secs == other.secs && self.usecs == other.usecs
    }
}

/// Parse a time value from a Lisp argument.
///
/// Accepts:
///   - nil            -> current time
///   - integer        -> seconds since epoch
///   - float          -> seconds since epoch (with fractional part)
///   - (HIGH LOW)     -> high*65536 + low seconds, 0 usecs
///   - (HIGH LOW USEC)       -> with microseconds
///   - (HIGH LOW USEC PSEC)  -> with microseconds (PSEC ignored)
fn parse_time(val: &Value) -> Result<TimeMicros, Flow> {
    match val {
        Value::Nil => Ok(TimeMicros::now()),
        Value::Int(n) => Ok(TimeMicros {
            secs: *n,
            usecs: 0,
        }),
        Value::Float(f) => {
            let secs = f.floor() as i64;
            let usecs = ((f - f.floor()) * 1_000_000.0).round() as i64;
            Ok(TimeMicros { secs, usecs })
        }
        Value::Cons(_) => {
            let items = list_to_vec(val).ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), val.clone()],
                )
            })?;
            if items.len() < 2 {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), val.clone()],
                ));
            }
            let high = items[0].as_int().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), items[0].clone()],
                )
            })?;
            let low = items[1].as_int().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), items[1].clone()],
                )
            })?;
            let usec = if items.len() > 2 {
                items[2].as_int().unwrap_or(0)
            } else {
                0
            };
            // PSEC (items[3]) is ignored
            let secs = high * 65536 + low;
            Ok(TimeMicros { secs, usecs: usec })
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("numberp"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// Date/time breakdown helpers (UTC only, no chrono)
// ---------------------------------------------------------------------------

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(month: i64, year: i64) -> i64 {
    match month {
        1 => 31,
        2 => {
            if is_leap_year(year) {
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

fn days_in_year(year: i64) -> i64 {
    if is_leap_year(year) {
        366
    } else {
        365
    }
}

/// Decoded time in UTC: (sec min hour day month year dow dst utcoff).
struct DecodedTime {
    sec: i64,
    min: i64,
    hour: i64,
    day: i64,   // 1-based
    month: i64, // 1-based
    year: i64,
    dow: i64,   // 0=Sunday, 1=Monday, ..., 6=Saturday
}

/// Break epoch seconds into UTC date/time components.
fn decode_epoch_secs(total_secs: i64) -> DecodedTime {
    // Handle the time-of-day part
    let mut days = total_secs.div_euclid(86400);
    let day_secs = total_secs.rem_euclid(86400);

    let sec = day_secs % 60;
    let min = (day_secs / 60) % 60;
    let hour = day_secs / 3600;

    // Day of week: epoch (1970-01-01) was Thursday (4).
    // dow: 0=Sunday
    let dow = ((days % 7) + 4).rem_euclid(7);

    // Compute year, month, day from days since epoch.
    let mut year: i64 = 1970;
    if days >= 0 {
        loop {
            let dy = days_in_year(year);
            if days < dy {
                break;
            }
            days -= dy;
            year += 1;
        }
    } else {
        loop {
            year -= 1;
            let dy = days_in_year(year);
            days += dy;
            if days >= 0 {
                break;
            }
        }
    }

    // Now `days` is day-of-year (0-based).
    let mut month: i64 = 1;
    loop {
        let dm = days_in_month(month, year);
        if days < dm {
            break;
        }
        days -= dm;
        month += 1;
        if month > 12 {
            break;
        }
    }
    let day = days + 1; // 1-based

    DecodedTime {
        sec,
        min,
        hour,
        day,
        month,
        year,
        dow,
    }
}

/// Encode date/time components to epoch seconds (UTC).
fn encode_to_epoch_secs(sec: i64, min: i64, hour: i64, day: i64, month: i64, year: i64) -> i64 {
    // Count days from epoch (1970-01-01) to the given date.
    let mut total_days: i64 = 0;

    if year >= 1970 {
        for y in 1970..year {
            total_days += days_in_year(y);
        }
    } else {
        for y in year..1970 {
            total_days -= days_in_year(y);
        }
    }

    // Add days for months in the target year.
    for m in 1..month {
        total_days += days_in_month(m, year);
    }

    // Add days within month (day is 1-based).
    total_days += day - 1;

    total_days * 86400 + hour * 3600 + min * 60 + sec
}

// ---------------------------------------------------------------------------
// Day/month name tables
// ---------------------------------------------------------------------------

const DAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// `(current-time)` -> `(HIGH LOW USEC PSEC)`
pub(crate) fn builtin_current_time(args: Vec<Value>) -> EvalResult {
    expect_args("current-time", &args, 0)?;
    Ok(TimeMicros::now().to_list())
}

/// `(float-time &optional TIME)` -> float seconds since epoch.
pub(crate) fn builtin_float_time(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("float-time", &args, 0, 1)?;
    let tm = if args.is_empty() || args[0].is_nil() {
        TimeMicros::now()
    } else {
        parse_time(&args[0])?
    };
    Ok(Value::Float(tm.to_float()))
}

/// `(time-add A B)` -> `(HIGH LOW USEC PSEC)`
pub(crate) fn builtin_time_add(args: Vec<Value>) -> EvalResult {
    expect_args("time-add", &args, 2)?;
    let a = parse_time(&args[0])?;
    let b = parse_time(&args[1])?;
    Ok(a.add(b).to_list())
}

/// `(time-subtract A B)` -> `(HIGH LOW USEC PSEC)`
pub(crate) fn builtin_time_subtract(args: Vec<Value>) -> EvalResult {
    expect_args("time-subtract", &args, 2)?;
    let a = parse_time(&args[0])?;
    let b = parse_time(&args[1])?;
    Ok(a.sub(b).to_list())
}

/// `(time-less-p A B)` -> t or nil
pub(crate) fn builtin_time_less_p(args: Vec<Value>) -> EvalResult {
    expect_args("time-less-p", &args, 2)?;
    let a = parse_time(&args[0])?;
    let b = parse_time(&args[1])?;
    Ok(Value::bool(a.less_than(b)))
}

/// `(time-equal-p A B)` -> t or nil
pub(crate) fn builtin_time_equal_p(args: Vec<Value>) -> EvalResult {
    expect_args("time-equal-p", &args, 2)?;
    let a = parse_time(&args[0])?;
    let b = parse_time(&args[1])?;
    Ok(Value::bool(a.equal(b)))
}

/// `(current-time-string &optional TIME ZONE)` -> human-readable string.
///
/// Returns a string like `"Mon Jan  2 15:04:05 2006"`.
/// ZONE is ignored; UTC is always used.
pub(crate) fn builtin_current_time_string(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("current-time-string", &args, 0, 2)?;
    let tm = if args.is_empty() || args[0].is_nil() {
        TimeMicros::now()
    } else {
        parse_time(&args[0])?
    };
    let dt = decode_epoch_secs(tm.secs);

    // Format: "Dow Mon DD HH:MM:SS YYYY"
    // Day of month is right-justified in a 2-char field (space-padded).
    let s = format!(
        "{} {} {:2} {:02}:{:02}:{:02} {}",
        DAY_NAMES[dt.dow as usize],
        MONTH_NAMES[(dt.month - 1) as usize],
        dt.day,
        dt.hour,
        dt.min,
        dt.sec,
        dt.year,
    );
    Ok(Value::string(s))
}

/// `(current-time-zone &optional TIME ZONE)` -> `(OFFSET NAME)`.
///
/// Stub: always returns UTC (offset 0, name "UTC").
pub(crate) fn builtin_current_time_zone(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("current-time-zone", &args, 0, 2)?;
    // Always UTC.
    Ok(Value::list(vec![Value::Int(0), Value::string("UTC")]))
}

/// `(encode-time SECONDS MINUTES HOURS DAY MONTH YEAR &optional ZONE)`
/// -> `(HIGH LOW)`
pub(crate) fn builtin_encode_time(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("encode-time", &args, 6, 7)?;
    let sec = args[0].as_int().ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), args[0].clone()],
        )
    })?;
    let min = args[1].as_int().ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), args[1].clone()],
        )
    })?;
    let hour = args[2].as_int().ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), args[2].clone()],
        )
    })?;
    let day = args[3].as_int().ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), args[3].clone()],
        )
    })?;
    let month = args[4].as_int().ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), args[4].clone()],
        )
    })?;
    let year = args[5].as_int().ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), args[5].clone()],
        )
    })?;
    // ZONE (args[6]) is ignored; UTC assumed.

    let total_secs = encode_to_epoch_secs(sec, min, hour, day, month, year);
    let high = (total_secs >> 16) & 0xFFFF_FFFF;
    let low = total_secs & 0xFFFF;
    Ok(Value::list(vec![Value::Int(high), Value::Int(low)]))
}

/// `(decode-time &optional TIME ZONE)`
/// -> `(SECONDS MINUTES HOURS DAY MONTH YEAR DOW DST UTCOFF)`
///
/// DOW is 0=Sunday .. 6=Saturday.  DST is nil.  UTCOFF is 0 (UTC).
pub(crate) fn builtin_decode_time(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("decode-time", &args, 0, 2)?;
    let tm = if args.is_empty() || args[0].is_nil() {
        TimeMicros::now()
    } else {
        parse_time(&args[0])?
    };
    let dt = decode_epoch_secs(tm.secs);
    Ok(Value::list(vec![
        Value::Int(dt.sec),
        Value::Int(dt.min),
        Value::Int(dt.hour),
        Value::Int(dt.day),
        Value::Int(dt.month),
        Value::Int(dt.year),
        Value::Int(dt.dow),
        Value::Nil,    // DST
        Value::Int(0), // UTCOFF
    ]))
}

/// `(time-convert TIME &optional FORM)`
///
/// FORM controls the output format:
///   - nil or `list`   -> `(HIGH LOW USEC PSEC)`
///   - `integer`       -> integer seconds
///   - `t` or `float`  -> float seconds
pub(crate) fn builtin_time_convert(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("time-convert", &args, 1, 2)?;
    let tm = parse_time(&args[0])?;

    let form = if args.len() > 1 { &args[1] } else { &Value::Nil };

    match form {
        Value::Nil => Ok(tm.to_list()),
        Value::True => Ok(Value::Float(tm.to_float())),
        Value::Symbol(s) => match s.as_str() {
            "list" => Ok(tm.to_list()),
            "integer" => Ok(Value::Int(tm.secs)),
            "float" => Ok(Value::Float(tm.to_float())),
            _ => Ok(tm.to_list()),
        },
        Value::Int(_) => {
            // When FORM is an integer, Emacs returns a cons (TICKS . HZ).
            // We approximate by returning (TICKS . 1) where TICKS = seconds.
            Ok(Value::cons(Value::Int(tm.secs), Value::Int(1)))
        }
        _ => Ok(tm.to_list()),
    }
}

/// `(set-time-zone-rule ZONE)` -> nil (stub).
pub(crate) fn builtin_set_time_zone_rule(args: Vec<Value>) -> EvalResult {
    expect_args("set-time-zone-rule", &args, 1)?;
    Ok(Value::Nil)
}

/// `(safe-date-to-time DATE-STRING)` -> current-time (stub).
pub(crate) fn builtin_safe_date_to_time(args: Vec<Value>) -> EvalResult {
    expect_args("safe-date-to-time", &args, 1)?;
    // Stub: ignore the date string, return current time.
    Ok(TimeMicros::now().to_list())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    #[test]
    fn time_micros_roundtrip_to_list() {
        let tm = TimeMicros {
            secs: 1_700_000_000,
            usecs: 123_456,
        };
        let list = tm.to_list();
        let items = list_to_vec(&list).unwrap();
        assert_eq!(items.len(), 4);
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        let usec = items[2].as_int().unwrap();
        let psec = items[3].as_int().unwrap();
        assert_eq!(high * 65536 + low, 1_700_000_000);
        assert_eq!(usec, 123_456);
        assert_eq!(psec, 0);
    }

    #[test]
    fn time_micros_to_float() {
        let tm = TimeMicros {
            secs: 1000,
            usecs: 500_000,
        };
        let f = tm.to_float();
        assert!((f - 1000.5).abs() < 1e-6);
    }

    #[test]
    fn time_micros_add() {
        let a = TimeMicros {
            secs: 10,
            usecs: 800_000,
        };
        let b = TimeMicros {
            secs: 5,
            usecs: 400_000,
        };
        let c = a.add(b);
        assert_eq!(c.secs, 16);
        assert_eq!(c.usecs, 200_000);
    }

    #[test]
    fn time_micros_sub() {
        let a = TimeMicros {
            secs: 10,
            usecs: 200_000,
        };
        let b = TimeMicros {
            secs: 5,
            usecs: 400_000,
        };
        let c = a.sub(b);
        assert_eq!(c.secs, 4);
        assert_eq!(c.usecs, 800_000);
    }

    #[test]
    fn time_micros_less_than() {
        let a = TimeMicros {
            secs: 10,
            usecs: 0,
        };
        let b = TimeMicros {
            secs: 10,
            usecs: 1,
        };
        assert!(a.less_than(b));
        assert!(!b.less_than(a));
        assert!(!a.less_than(a));
    }

    #[test]
    fn time_micros_equal() {
        let a = TimeMicros {
            secs: 42,
            usecs: 123,
        };
        let b = TimeMicros {
            secs: 42,
            usecs: 123,
        };
        assert!(a.equal(b));
        let c = TimeMicros {
            secs: 42,
            usecs: 124,
        };
        assert!(!a.equal(c));
    }

    // -----------------------------------------------------------------------
    // parse_time
    // -----------------------------------------------------------------------

    #[test]
    fn parse_time_nil() {
        let tm = parse_time(&Value::Nil).unwrap();
        // Just check it returns something reasonable (recent epoch).
        assert!(tm.secs > 1_000_000_000);
    }

    #[test]
    fn parse_time_integer() {
        let tm = parse_time(&Value::Int(1_700_000_000)).unwrap();
        assert_eq!(tm.secs, 1_700_000_000);
        assert_eq!(tm.usecs, 0);
    }

    #[test]
    fn parse_time_float() {
        let tm = parse_time(&Value::Float(1000.5)).unwrap();
        assert_eq!(tm.secs, 1000);
        assert_eq!(tm.usecs, 500_000);
    }

    #[test]
    fn parse_time_list_two() {
        // (HIGH LOW) format: 25939 * 65536 + 34304 = 1700000000
        let high = 1_700_000_000i64 >> 16;
        let low = 1_700_000_000i64 & 0xFFFF;
        let list = Value::list(vec![Value::Int(high), Value::Int(low)]);
        let tm = parse_time(&list).unwrap();
        assert_eq!(tm.secs, 1_700_000_000);
        assert_eq!(tm.usecs, 0);
    }

    #[test]
    fn parse_time_list_four() {
        let high = 1_700_000_000i64 >> 16;
        let low = 1_700_000_000i64 & 0xFFFF;
        let list = Value::list(vec![
            Value::Int(high),
            Value::Int(low),
            Value::Int(42),
            Value::Int(0),
        ]);
        let tm = parse_time(&list).unwrap();
        assert_eq!(tm.secs, 1_700_000_000);
        assert_eq!(tm.usecs, 42);
    }

    #[test]
    fn parse_time_bad_type() {
        let result = parse_time(&Value::string("not a time"));
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Date computation helpers
    // -----------------------------------------------------------------------

    #[test]
    fn leap_years() {
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2023));
        assert!(is_leap_year(2400));
    }

    #[test]
    fn decode_epoch_zero() {
        let dt = decode_epoch_secs(0);
        assert_eq!(dt.year, 1970);
        assert_eq!(dt.month, 1);
        assert_eq!(dt.day, 1);
        assert_eq!(dt.hour, 0);
        assert_eq!(dt.min, 0);
        assert_eq!(dt.sec, 0);
        assert_eq!(dt.dow, 4); // Thursday
    }

    #[test]
    fn decode_known_date() {
        // 2024-01-15 12:30:45 UTC -> epoch = 1705318245
        let epoch = encode_to_epoch_secs(45, 30, 12, 15, 1, 2024);
        let dt = decode_epoch_secs(epoch);
        assert_eq!(dt.year, 2024);
        assert_eq!(dt.month, 1);
        assert_eq!(dt.day, 15);
        assert_eq!(dt.hour, 12);
        assert_eq!(dt.min, 30);
        assert_eq!(dt.sec, 45);
    }

    #[test]
    fn encode_decode_roundtrip() {
        let epoch = encode_to_epoch_secs(30, 15, 10, 25, 6, 2023);
        let dt = decode_epoch_secs(epoch);
        assert_eq!(dt.sec, 30);
        assert_eq!(dt.min, 15);
        assert_eq!(dt.hour, 10);
        assert_eq!(dt.day, 25);
        assert_eq!(dt.month, 6);
        assert_eq!(dt.year, 2023);
    }

    #[test]
    fn encode_decode_roundtrip_leap_day() {
        let epoch = encode_to_epoch_secs(0, 0, 0, 29, 2, 2024);
        let dt = decode_epoch_secs(epoch);
        assert_eq!(dt.day, 29);
        assert_eq!(dt.month, 2);
        assert_eq!(dt.year, 2024);
    }

    #[test]
    fn decode_y2k() {
        // 2000-01-01 00:00:00 UTC = 946684800
        let dt = decode_epoch_secs(946_684_800);
        assert_eq!(dt.year, 2000);
        assert_eq!(dt.month, 1);
        assert_eq!(dt.day, 1);
        assert_eq!(dt.hour, 0);
        assert_eq!(dt.min, 0);
        assert_eq!(dt.sec, 0);
        assert_eq!(dt.dow, 6); // Saturday
    }

    // -----------------------------------------------------------------------
    // Builtins
    // -----------------------------------------------------------------------

    #[test]
    fn builtin_current_time_returns_four_element_list() {
        let result = builtin_current_time(vec![]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 4);
        // All should be integers.
        for item in &items {
            assert!(item.is_integer());
        }
        // Reconstruct and check sanity.
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        let secs = high * 65536 + low;
        assert!(secs > 1_000_000_000);
    }

    #[test]
    fn builtin_current_time_wrong_arity() {
        let result = builtin_current_time(vec![Value::Int(1)]);
        assert!(result.is_err());
    }

    #[test]
    fn builtin_float_time_no_args() {
        let result = builtin_float_time(vec![]).unwrap();
        match result {
            Value::Float(f) => assert!(f > 1_000_000_000.0),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn builtin_float_time_from_list() {
        let high = 1_700_000_000i64 >> 16;
        let low = 1_700_000_000i64 & 0xFFFF;
        let list = Value::list(vec![
            Value::Int(high),
            Value::Int(low),
            Value::Int(500_000),
            Value::Int(0),
        ]);
        let result = builtin_float_time(vec![list]).unwrap();
        match result {
            Value::Float(f) => assert!((f - 1_700_000_000.5).abs() < 1e-3),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn builtin_float_time_from_integer() {
        let result = builtin_float_time(vec![Value::Int(42)]).unwrap();
        match result {
            Value::Float(f) => assert!((f - 42.0).abs() < 1e-9),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn builtin_time_add_basic() {
        let a = Value::Int(100);
        let b = Value::Int(200);
        let result = builtin_time_add(vec![a, b]).unwrap();
        let items = list_to_vec(&result).unwrap();
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        assert_eq!(high * 65536 + low, 300);
    }

    #[test]
    fn builtin_time_subtract_basic() {
        let a = Value::Int(300);
        let b = Value::Int(100);
        let result = builtin_time_subtract(vec![a, b]).unwrap();
        let items = list_to_vec(&result).unwrap();
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        assert_eq!(high * 65536 + low, 200);
    }

    #[test]
    fn builtin_time_less_p_true() {
        let result = builtin_time_less_p(vec![Value::Int(1), Value::Int(2)]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn builtin_time_less_p_false() {
        let result = builtin_time_less_p(vec![Value::Int(2), Value::Int(1)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn builtin_time_equal_p_true() {
        let result = builtin_time_equal_p(vec![Value::Int(42), Value::Int(42)]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn builtin_time_equal_p_false() {
        let result = builtin_time_equal_p(vec![Value::Int(42), Value::Int(43)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn builtin_current_time_string_known_time() {
        // 2024-01-15 12:30:45 UTC
        let epoch = encode_to_epoch_secs(45, 30, 12, 15, 1, 2024);
        let result = builtin_current_time_string(vec![Value::Int(epoch)]).unwrap();
        let s = result.as_str().unwrap();
        assert!(s.contains("Jan"));
        assert!(s.contains("12:30:45"));
        assert!(s.contains("2024"));
        assert!(s.contains("15"));
    }

    #[test]
    fn builtin_current_time_string_no_args() {
        let result = builtin_current_time_string(vec![]).unwrap();
        assert!(result.is_string());
    }

    #[test]
    fn builtin_current_time_zone_default() {
        let result = builtin_current_time_zone(vec![]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_int(), Some(0));
        assert_eq!(items[1].as_str(), Some("UTC"));
    }

    #[test]
    fn builtin_encode_time_known() {
        let result = builtin_encode_time(vec![
            Value::Int(0),
            Value::Int(0),
            Value::Int(0),
            Value::Int(1),
            Value::Int(1),
            Value::Int(1970),
        ])
        .unwrap();
        let items = list_to_vec(&result).unwrap();
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        assert_eq!(high * 65536 + low, 0);
    }

    #[test]
    fn builtin_encode_time_y2k() {
        let result = builtin_encode_time(vec![
            Value::Int(0),
            Value::Int(0),
            Value::Int(0),
            Value::Int(1),
            Value::Int(1),
            Value::Int(2000),
        ])
        .unwrap();
        let items = list_to_vec(&result).unwrap();
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        assert_eq!(high * 65536 + low, 946_684_800);
    }

    #[test]
    fn builtin_encode_time_wrong_arity() {
        let result = builtin_encode_time(vec![Value::Int(0)]);
        assert!(result.is_err());
    }

    #[test]
    fn builtin_decode_time_epoch_zero() {
        let result = builtin_decode_time(vec![Value::Int(0)]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 9);
        assert_eq!(items[0].as_int(), Some(0)); // sec
        assert_eq!(items[1].as_int(), Some(0)); // min
        assert_eq!(items[2].as_int(), Some(0)); // hour
        assert_eq!(items[3].as_int(), Some(1)); // day
        assert_eq!(items[4].as_int(), Some(1)); // month
        assert_eq!(items[5].as_int(), Some(1970)); // year
        assert_eq!(items[6].as_int(), Some(4)); // dow (Thursday)
        assert!(items[7].is_nil()); // DST
        assert_eq!(items[8].as_int(), Some(0)); // utcoff
    }

    #[test]
    fn builtin_decode_time_no_args() {
        let result = builtin_decode_time(vec![]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 9);
    }

    #[test]
    fn builtin_encode_decode_roundtrip() {
        // Encode a specific time.
        let encoded = builtin_encode_time(vec![
            Value::Int(30),
            Value::Int(45),
            Value::Int(14),
            Value::Int(20),
            Value::Int(3),
            Value::Int(2025),
        ])
        .unwrap();

        // Decode it back.
        let decoded = builtin_decode_time(vec![encoded]).unwrap();
        let items = list_to_vec(&decoded).unwrap();
        assert_eq!(items[0].as_int(), Some(30)); // sec
        assert_eq!(items[1].as_int(), Some(45)); // min
        assert_eq!(items[2].as_int(), Some(14)); // hour
        assert_eq!(items[3].as_int(), Some(20)); // day
        assert_eq!(items[4].as_int(), Some(3)); // month
        assert_eq!(items[5].as_int(), Some(2025)); // year
    }

    #[test]
    fn builtin_time_convert_to_list() {
        let result = builtin_time_convert(vec![Value::Int(1000)]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 4);
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        assert_eq!(high * 65536 + low, 1000);
    }

    #[test]
    fn builtin_time_convert_to_integer() {
        let result =
            builtin_time_convert(vec![Value::Int(1000), Value::symbol("integer")]).unwrap();
        assert_eq!(result.as_int(), Some(1000));
    }

    #[test]
    fn builtin_time_convert_to_float() {
        let result =
            builtin_time_convert(vec![Value::Int(1000), Value::symbol("float")]).unwrap();
        match result {
            Value::Float(f) => assert!((f - 1000.0).abs() < 1e-9),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn builtin_time_convert_with_t() {
        let result = builtin_time_convert(vec![Value::Int(42), Value::True]).unwrap();
        match result {
            Value::Float(f) => assert!((f - 42.0).abs() < 1e-9),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn builtin_set_time_zone_rule_stub() {
        let result = builtin_set_time_zone_rule(vec![Value::string("UTC")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn builtin_safe_date_to_time_stub() {
        let result =
            builtin_safe_date_to_time(vec![Value::string("Mon Jan  2 15:04:05 2006")]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn builtin_safe_date_to_time_wrong_arity() {
        let result = builtin_safe_date_to_time(vec![]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn time_add_with_usec_overflow() {
        let a = Value::list(vec![
            Value::Int(0),
            Value::Int(10),
            Value::Int(999_000),
            Value::Int(0),
        ]);
        let b = Value::list(vec![
            Value::Int(0),
            Value::Int(5),
            Value::Int(500_000),
            Value::Int(0),
        ]);
        let result = builtin_time_add(vec![a, b]).unwrap();
        let items = list_to_vec(&result).unwrap();
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        let usec = items[2].as_int().unwrap();
        assert_eq!(high * 65536 + low, 16); // 10 + 5 + 1 carry
        assert_eq!(usec, 499_000); // 999000 + 500000 - 1000000
    }

    #[test]
    fn time_subtract_with_usec_borrow() {
        let a = Value::list(vec![
            Value::Int(0),
            Value::Int(10),
            Value::Int(100_000),
            Value::Int(0),
        ]);
        let b = Value::list(vec![
            Value::Int(0),
            Value::Int(5),
            Value::Int(500_000),
            Value::Int(0),
        ]);
        let result = builtin_time_subtract(vec![a, b]).unwrap();
        let items = list_to_vec(&result).unwrap();
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        let usec = items[2].as_int().unwrap();
        assert_eq!(high * 65536 + low, 4); // 10 - 5 - 1 borrow
        assert_eq!(usec, 600_000); // 100000 - 500000 + 1000000
    }

    #[test]
    fn float_time_nil_arg() {
        let result = builtin_float_time(vec![Value::Nil]).unwrap();
        match result {
            Value::Float(f) => assert!(f > 1_000_000_000.0),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn time_operations_with_mixed_formats() {
        // Add an integer to a list-format time.
        let a = Value::Int(100);
        let b = Value::list(vec![
            Value::Int(0),
            Value::Int(50),
            Value::Int(250_000),
            Value::Int(0),
        ]);
        let result = builtin_time_add(vec![a, b]).unwrap();
        let items = list_to_vec(&result).unwrap();
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        let usec = items[2].as_int().unwrap();
        assert_eq!(high * 65536 + low, 150);
        assert_eq!(usec, 250_000);
    }

    #[test]
    fn current_time_string_epoch() {
        let result = builtin_current_time_string(vec![Value::Int(0)]).unwrap();
        let s = result.as_str().unwrap();
        // 1970-01-01 00:00:00 UTC, Thursday
        assert!(s.contains("Thu"));
        assert!(s.contains("Jan"));
        assert!(s.contains("1970"));
        assert!(s.contains("00:00:00"));
    }
}
