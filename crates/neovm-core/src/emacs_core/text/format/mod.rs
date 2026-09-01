//! Advanced string formatting builtins.
//!
//! Pure builtins (`Vec<Value> -> EvalResult`):
//! - `format-time-string` — format time like strftime
//! - `string-clean-whitespace` — collapse whitespace and trim
//! - `string-pixel-width` — batch-compatible display-column width

use super::error::{EvalResult, Flow, signal};
use super::timefns::zone_offset_name_for_time;
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_min_args;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn require_string(_name: &str, val: &Value) -> Result<String, Flow> {
    val.as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), *val],
            )
        })
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

fn days_in_year(y: i64) -> i32 {
    if is_leap_year(y) { 366 } else { 365 }
}

fn iso_week_days(yday: i32, wday: i32) -> i32 {
    const ISO_WEEK_START_WDAY: i32 = 1;
    const ISO_WEEK1_WDAY: i32 = 4;
    const YDAY_MINIMUM: i32 = -366;
    let big_enough_multiple_of_7 = (-YDAY_MINIMUM / 7 + 2) * 7;
    yday - (yday - wday + ISO_WEEK1_WDAY + big_enough_multiple_of_7) % 7 + ISO_WEEK1_WDAY
        - ISO_WEEK_START_WDAY
}

fn iso_week_year_and_number(tm: &BrokenDownTime) -> (i64, i32) {
    let mut year_adjust = 0;
    let mut days = iso_week_days(tm.yearday as i32, tm.weekday as i32);

    if days < 0 {
        year_adjust = -1;
        days = iso_week_days(
            tm.yearday as i32 + days_in_year(tm.year - 1),
            tm.weekday as i32,
        );
    } else {
        let next_year_days =
            iso_week_days(tm.yearday as i32 - days_in_year(tm.year), tm.weekday as i32);
        if next_year_days >= 0 {
            year_adjust = 1;
            days = next_year_days;
        }
    }

    (tm.year + year_adjust, days / 7 + 1)
}

/// Convert a Unix timestamp (seconds since 1970-01-01 00:00:00 UTC) into
/// broken-down UTC time fields.  No external crate needed.
fn unix_to_broken_down(timestamp: i64) -> BrokenDownTime {
    // Handle negative timestamps (before epoch).
    let remaining = timestamp;
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

/// Whether conversion `conversion` accepts the `E`/`O` locale modifier.
///
/// One decode of the modifier guards scattered through GNU
/// `lib/strftime.c`'s big `switch (format_char)`, where a conversion that has
/// no era (`E`) or alternative-digit (`O`) form does `goto bad_format` instead
/// of ignoring the modifier. `bad_format` copies the whole directive out
/// literally, so `(format-time-string "%Ed" ...)` yields the string `"%Ed"`,
/// not the day of month.
///
/// In the C locale the ACCEPTED combinations all fall back to the plain
/// conversion (`_NL_CURRENT` era strings are empty), so only the rejections are
/// observable — `%Ex` formats like `%x` while `%Ox` stays literal. The table
/// was extracted from GNU's source and then confirmed against the GNU binary
/// for every conversion character.
fn modifier_accepted_by(modifier: char, conversion: char) -> bool {
    match conversion {
        // `if (modifier != 0) goto bad_format;`
        // NB `A` is spelled `case 'A':` in GNU's source, not `case L_('A'):`
        // like its neighbours -- easy to miss when reading the table off.
        'A' | 'D' | 'F' | 'a' => false,
        // `if (modifier == L_('E')) goto bad_format;`
        'B' | 'G' | 'H' | 'I' | 'M' | 'N' | 'S' | 'U' | 'V' | 'W' | 'b' | 'd' | 'e' | 'g' | 'h'
        | 'j' | 'k' | 'l' | 'm' | 'w' => modifier != 'E',
        // `if (modifier == L_('O')) goto bad_format;`
        'X' | 'c' | 'x' => modifier != 'O',
        _ => true,
    }
}

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
/// If TIME is nil, uses current system time.  ZONE follows GNU Emacs
/// `format-time-string`.
pub(crate) fn builtin_format_time_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("format-time-string", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("format-time-string"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let format_str = require_string("format-time-string", &args[0])?;

    // Determine timestamp. Use the shared time-value parser so every Lisp time
    // form ((TICKS . HZ), (HIGH LOW USEC PSEC), integer, float, nil) decodes
    // identically to the other time functions, and so the subsecond fraction
    // is available for the `%N' directive (GNU passes `t.tv_nsec' to nstrftime,
    // src/timefns.c:1391).
    let (timestamp, nanos): (i64, i64) = if args.len() >= 2 && !args[1].is_nil() {
        crate::emacs_core::timefns::time_value_seconds_and_nanos(&args[1])?
    } else {
        (current_unix_timestamp(), 0)
    };

    let (offset_secs, zone_name) = zone_offset_name_for_time(args.get(2), timestamp)?;
    let tm = unix_to_broken_down(timestamp.saturating_add(offset_secs));
    let formatted = format_time(&format_str, &tm, timestamp, offset_secs, &zone_name, nanos);
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

/// Padding style for a strftime numeric field. Mirrors GNU `enum pad_style`
/// (`lib/strftime.c`): the flag characters `_`/`-`/`+`/`0` select these, and
/// `ZERO_PAD` is the default that each directive may override.
#[derive(Clone, Copy, PartialEq, Eq)]
// Names mirror GNU `enum pad_style`; retaining that vocabulary makes the
// strftime port directly auditable.
#[allow(clippy::enum_variant_names)]
enum Pad {
    Zero,       // ZERO_PAD: directive default; turns into AlwaysZero (or SpacePad)
    SpacePad,   // SPACE_PAD: `_` flag
    NoPad,      // NO_PAD: `-` flag (suppresses padding *and* the field width)
    SignPad,    // SIGN_PAD: `+` flag
    AlwaysZero, // ALWAYS_ZERO_PAD: `0` flag
}

/// State accumulated while parsing one `%`-directive's leading flags/width.
struct DirectiveFlags {
    pad: Pad,
    to_uppcase: bool,
    change_case: bool,
    /// Field width, or -1 when unspecified (GNU's `width = -1`).
    width: i64,
}

/// Format a single broken-down numeric field exactly like GNU's
/// `do_number_sign_and_padding` block. `digits` is the directive's default
/// minimum width, `value` the number, `negative`/`always_sign` control the
/// emitted sign, and `tz_colon_mask` inserts ':' before selected digits for the
/// `%:z` family (bit i set => ':' before the i'th digit, counting from the
/// least-significant digit).
fn do_number(
    result: &mut String,
    flags: &DirectiveFlags,
    digits: i64,
    value: i64,
    negative: bool,
    always_sign: bool,
    mut tz_colon_mask: u32,
) {
    // Build the digit string (with embedded colons) right-to-left.
    let mut uval = (value as i128).unsigned_abs();
    let mut buf: Vec<char> = Vec::new();
    loop {
        if tz_colon_mask & 1 != 0 {
            buf.push(':');
        }
        tz_colon_mask >>= 1;
        buf.push((b'0' + (uval % 10) as u8) as char);
        uval /= 10;
        if uval == 0 && tz_colon_mask == 0 {
            break;
        }
    }
    buf.reverse();
    let number: String = buf.into_iter().collect();
    // GNU sets `number_digits = number_bytes`, i.e. it counts the embedded
    // ':' separators toward the field width (see `do_number_sign_and_padding`).
    let number_digits = number.chars().count() as i64;

    // GNU: `if (pad == ZERO_PAD) pad = ALWAYS_ZERO_PAD;` then default width.
    let pad = if flags.pad == Pad::Zero {
        Pad::AlwaysZero
    } else {
        flags.pad
    };
    let width = if flags.width < 0 { digits } else { flags.width };

    let sign_char = if negative {
        Some('-')
    } else if always_sign {
        Some('+')
    } else {
        None
    };
    let shortage = width - i64::from(sign_char.is_some()) - number_digits;
    let padding = if pad == Pad::NoPad || shortage <= 0 {
        0
    } else {
        shortage
    };

    let pad_char = if pad == Pad::AlwaysZero || pad == Pad::SignPad {
        '0'
    } else {
        ' '
    };
    if let Some(sign) = sign_char {
        if pad == Pad::SpacePad {
            // Space padding goes before the sign.
            for _ in 0..padding {
                result.push(' ');
            }
            result.push(sign);
        } else {
            result.push(sign);
            for _ in 0..padding {
                result.push(pad_char);
            }
        }
    } else {
        for _ in 0..padding {
            result.push(pad_char);
        }
    }
    result.push_str(&number);
}

/// Emit a text field, applying the GNU `cpy`/`width_add` rules: pad on the left
/// to `width` (space-padded; `NoPad` suppresses width entirely) and apply the
/// requested case folding.
fn do_text(result: &mut String, flags: &DirectiveFlags, to_lowcase: bool, s: &str) {
    let text: String = if to_lowcase {
        s.to_lowercase()
    } else if flags.to_uppcase {
        s.to_uppercase()
    } else {
        s.to_string()
    };
    let n = text.chars().count() as i64;
    let w = if flags.pad == Pad::NoPad || flags.width < 0 {
        0
    } else {
        flags.width
    };
    for _ in n..w {
        result.push(' ');
    }
    result.push_str(&text);
}

/// Format a broken-down time according to a strftime-like format string,
/// mirroring GNU's `__strftime_internal` (`lib/strftime.c`).
fn format_time(
    fmt: &str,
    tm: &BrokenDownTime,
    timestamp: i64,
    zone_offset_secs: i64,
    zone_name: &str,
    nanos: i64,
) -> String {
    let mut result = String::new();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;

    // 12-hour clock value used by %I/%l/%r.
    let h12 = |hour: u32| -> i64 {
        if hour == 0 {
            12
        } else if hour > 12 {
            (hour - 12) as i64
        } else {
            hour as i64
        }
    };
    // Render a subformat (e.g. %r -> "%I:%M:%S %p") with no inherited flags,
    // honoring an inherited upcase from the `^` flag the way GNU does.
    let subformat = |sub: &str, upcase: bool| -> String {
        let s = format_time(sub, tm, timestamp, zone_offset_secs, zone_name, nanos);
        if upcase { s.to_uppercase() } else { s }
    };

    while i < chars.len() {
        if chars[i] != '%' {
            result.push(chars[i]);
            i += 1;
            continue;
        }

        let percent = i;
        i += 1;

        // GNU: parse the flag characters `_ - + 0 ^ #` (any order, repeated).
        let mut flags = DirectiveFlags {
            pad: Pad::Zero,
            to_uppcase: false,
            change_case: false,
            width: -1,
        };
        while i < chars.len() {
            match chars[i] {
                '_' => flags.pad = Pad::SpacePad,
                '-' => flags.pad = Pad::NoPad,
                '+' => flags.pad = Pad::SignPad,
                '0' => flags.pad = Pad::AlwaysZero,
                '^' => flags.to_uppcase = true,
                '#' => flags.change_case = true,
                _ => break,
            }
            i += 1;
        }

        // GNU: parse an optional decimal field width.
        if i < chars.len() && chars[i].is_ascii_digit() {
            let mut w: i64 = 0;
            while i < chars.len() && chars[i].is_ascii_digit() {
                w = w
                    .saturating_mul(10)
                    .saturating_add((chars[i] as i64) - ('0' as i64));
                i += 1;
            }
            flags.width = w;
        }

        // GNU: parse the `E`/`O` locale modifiers (`lib/strftime.c`).  They are
        // NOT simply skipped: each conversion decides whether it accepts one,
        // and a conversion that does not `goto bad_format`, which copies the
        // whole directive out literally.
        let modifier = match chars.get(i) {
            Some(&m @ ('E' | 'O')) => {
                i += 1;
                Some(m)
            }
            _ => None,
        };

        if i >= chars.len() {
            // GNU "% at end of format": emit the literal text from `%`.
            result.extend(chars[percent..].iter());
            break;
        }

        let fc = chars[i];

        // GNU `bad_format`: a conversion that does not accept the modifier it
        // was given emits the ENTIRE directive literally, `%` included
        // (`cpy (f - percent + 1, percent)`), rather than ignoring the
        // modifier and formatting anyway.
        if let Some(m) = modifier
            && !modifier_accepted_by(m, fc)
        {
            result.extend(chars[percent..=i].iter());
            i += 1;
            continue;
        }

        // GNU: `:`, `::`, `:::` are valid only just before `z`.
        if fc == ':' {
            let mut colons = 0usize;
            while i + colons < chars.len() && chars[i + colons] == ':' {
                colons += 1;
            }
            if i + colons < chars.len() && chars[i + colons] == 'z' {
                emit_tz_offset(&mut result, &flags, zone_offset_secs, colons as u32);
                i += colons + 1;
                continue;
            }
            // Not a valid `%:z`: echo the directive verbatim (bad_format).
            result.extend(chars[percent..=i].iter());
            i += 1;
            continue;
        }

        match fc {
            '%' => {
                // GNU `bad_percent`: a bare "%%" only — flags before it are
                // echoed literally.
                if i - 1 != percent {
                    result.extend(chars[percent..=i].iter());
                } else {
                    result.push('%');
                }
            }
            // ---- Numeric directives (routed through do_number) -------------
            'Y' => do_number(&mut result, &flags, 4, tm.year, tm.year < 0, false, 0),
            'y' => do_number(
                &mut result,
                &flags,
                2,
                tm.year.rem_euclid(100),
                false,
                false,
                0,
            ),
            'C' => do_number(
                &mut result,
                &flags,
                2,
                tm.year.div_euclid(100),
                tm.year < 0,
                false,
                0,
            ),
            'G' | 'g' | 'V' => {
                let (iso_year, iso_week) = iso_week_year_and_number(tm);
                match fc {
                    'G' => do_number(&mut result, &flags, 4, iso_year, iso_year < 0, false, 0),
                    'g' => do_number(
                        &mut result,
                        &flags,
                        2,
                        iso_year.rem_euclid(100),
                        false,
                        false,
                        0,
                    ),
                    _ => do_number(&mut result, &flags, 2, iso_week as i64, false, false, 0),
                }
            }
            'm' => do_number(&mut result, &flags, 2, tm.month as i64, false, false, 0),
            'd' => do_number(&mut result, &flags, 2, tm.day as i64, false, false, 0),
            'e' => {
                // %e is space-padded by default.
                let mut f = rebuild(&flags);
                if f.pad == Pad::Zero {
                    f.pad = Pad::SpacePad;
                }
                do_number(&mut result, &f, 2, tm.day as i64, false, false, 0);
            }
            'H' => do_number(&mut result, &flags, 2, tm.hour as i64, false, false, 0),
            'k' => {
                let mut f = rebuild(&flags);
                if f.pad == Pad::Zero {
                    f.pad = Pad::SpacePad;
                }
                do_number(&mut result, &f, 2, tm.hour as i64, false, false, 0);
            }
            'I' => do_number(&mut result, &flags, 2, h12(tm.hour), false, false, 0),
            'l' => {
                let mut f = rebuild(&flags);
                if f.pad == Pad::Zero {
                    f.pad = Pad::SpacePad;
                }
                do_number(&mut result, &f, 2, h12(tm.hour), false, false, 0);
            }
            'M' => do_number(&mut result, &flags, 2, tm.minute as i64, false, false, 0),
            'S' => do_number(&mut result, &flags, 2, tm.second as i64, false, false, 0),
            's' => do_number(&mut result, &flags, 1, timestamp, timestamp < 0, false, 0),
            'j' => do_number(
                &mut result,
                &flags,
                3,
                tm.yearday as i64 + 1,
                false,
                false,
                0,
            ),
            'u' => {
                let iso_wd = if tm.weekday == 0 {
                    7
                } else {
                    tm.weekday as i64
                };
                do_number(&mut result, &flags, 1, iso_wd, false, false, 0);
            }
            'w' => do_number(&mut result, &flags, 1, tm.weekday as i64, false, false, 0),
            'U' => {
                let wnum = (tm.yearday + 7 - tm.weekday) / 7;
                do_number(&mut result, &flags, 2, wnum as i64, false, false, 0);
            }
            'W' => {
                let monday_weekday = if tm.weekday == 0 { 6 } else { tm.weekday - 1 };
                let wnum = (tm.yearday + 7 - monday_weekday) / 7;
                do_number(&mut result, &flags, 2, wnum as i64, false, false, 0);
            }
            'q' => do_number(
                &mut result,
                &flags,
                1,
                (((tm.month as i64 - 1) * 11) >> 5) + 1,
                false,
                false,
                0,
            ),
            // ---- Time-zone offset (%z; the %:z family handled above) -------
            'z' => emit_tz_offset(&mut result, &flags, zone_offset_secs, 0),
            // ---- Text directives (routed through do_text) ------------------
            'A' => {
                let f = with_change_case_upcase(&flags);
                do_text(&mut result, &f, false, DAY_NAMES[tm.weekday as usize % 7]);
            }
            'a' => {
                let f = with_change_case_upcase(&flags);
                do_text(&mut result, &f, false, DAY_ABBREVS[tm.weekday as usize % 7]);
            }
            'B' => {
                let f = with_change_case_upcase(&flags);
                do_text(
                    &mut result,
                    &f,
                    false,
                    MONTH_NAMES[(tm.month as usize).saturating_sub(1) % 12],
                );
            }
            'b' | 'h' => {
                let f = with_change_case_upcase(&flags);
                do_text(
                    &mut result,
                    &f,
                    false,
                    MONTH_ABBREVS[(tm.month as usize).saturating_sub(1) % 12],
                );
            }
            'p' => {
                // `#` flag => lowercase for %p.
                let lower = flags.change_case;
                do_text(
                    &mut result,
                    &flags,
                    lower,
                    if tm.hour < 12 { "AM" } else { "PM" },
                );
            }
            'P' => {
                // %P is always lowercased; `^`/`#` cannot upcase it.
                do_text(
                    &mut result,
                    &flags,
                    true,
                    if tm.hour < 12 { "AM" } else { "PM" },
                );
            }
            'Z' => {
                // `#` flag => lowercase for %Z.
                let lower = flags.change_case;
                do_text(&mut result, &flags, lower, zone_name);
            }
            'n' => do_text(&mut result, &flags, false, "\n"),
            't' => do_text(&mut result, &flags, false, "\t"),
            'N' => {
                // GNU extension: subsecond digits (`lib/strftime.c`, `case 'N'`).
                // GNU always computes the nanosecond digits, strips trailing
                // zeros (while `1 < ndigs && n % 10 == 0`), then pads the field
                // to `width` according to the directive's pad style:
                //   `-` (NoPad)  -> no padding (just the stripped digits);
                //   `_` (SpacePad) -> space-pad on the right to `width`;
                //   default/`0`  -> zero-pad on the right to `width`.
                // A field width caps the number of significant digits emitted.
                const NS_DIGITS: i64 = 9;
                let mut n = nanos.clamp(0, 999_999_999);
                let width = if flags.width <= 0 {
                    NS_DIGITS
                } else {
                    flags.width
                };
                // GNU: `while (width < ndigs || (1 < ndigs && n % 10 == 0)) ndigs--, n /= 10;`
                let mut ndigs = NS_DIGITS;
                while width < ndigs || (1 < ndigs && n % 10 == 0) {
                    ndigs -= 1;
                    n /= 10;
                }
                // Emit the `ndigs` significant digits (zero-padded to `ndigs`),
                // which corresponds to GNU's `width_cpy (0, ndigs, buf)`.
                let digits = format!("{:0width$}", n, width = ndigs as usize);
                result.push_str(&digits);
                // GNU: `if (pad == ZERO_PAD) pad = ALWAYS_ZERO_PAD;` then
                // `width_add (width - ndigs, 0, ...)` -- pad the remainder.
                let pad = if flags.pad == Pad::Zero {
                    Pad::AlwaysZero
                } else {
                    flags.pad
                };
                let extra = width - ndigs;
                if pad != Pad::NoPad && extra > 0 {
                    let pad_char = if pad == Pad::AlwaysZero || pad == Pad::SignPad {
                        '0'
                    } else {
                        ' '
                    };
                    result.extend(std::iter::repeat_n(pad_char, extra as usize));
                }
            }
            // ---- Compound (subformat) directives ---------------------------
            'R' => result.push_str(&subformat("%H:%M", flags.to_uppcase)),
            'T' | 'X' => result.push_str(&subformat("%H:%M:%S", flags.to_uppcase)),
            'r' => result.push_str(&subformat("%I:%M:%S %p", flags.to_uppcase)),
            'F' => result.push_str(&subformat("%Y-%m-%d", flags.to_uppcase)),
            'D' | 'x' => result.push_str(&subformat("%m/%d/%y", flags.to_uppcase)),
            'c' => result.push_str(&subformat("%a %b %e %H:%M:%S %Y", flags.to_uppcase)),
            other => {
                // GNU `bad_format`: echo the directive verbatim, including `%`,
                // flags and width.
                result.extend(chars[percent..i].iter());
                result.push(other);
            }
        }
        i += 1;
    }

    result
}

/// Clone a `DirectiveFlags`. Used where a directive needs to override a default
/// (e.g. `%e`/`%k`/`%l` space-pad) without mutating the shared parse result.
fn rebuild(f: &DirectiveFlags) -> DirectiveFlags {
    DirectiveFlags {
        pad: f.pad,
        to_uppcase: f.to_uppcase,
        change_case: f.change_case,
        width: f.width,
    }
}

/// For text directives where the `#` (change_case) flag means "upcase" (e.g.
/// `%a`, `%A`, `%b`, `%B`): return flags with `to_uppcase` forced when `#` set.
fn with_change_case_upcase(f: &DirectiveFlags) -> DirectiveFlags {
    let mut out = rebuild(f);
    if out.change_case {
        out.to_uppcase = true;
    }
    out
}

/// Emit the `%z`/`%:z`/`%::z`/`%:::z` time-zone offset, mirroring GNU's
/// `do_z_conversion` switch on the number of colons.
fn emit_tz_offset(result: &mut String, flags: &DirectiveFlags, offset_secs: i64, colons: u32) {
    let diff = offset_secs;
    let negative = diff < 0;
    let abs = diff.abs();
    let hour_diff = abs / 3600;
    let min_diff = abs / 60 % 60;
    let sec_diff = abs % 60;
    // GNU's mask: bit i set => insert ':' before the i'th digit (from the
    // least-significant digit). 04 (octal) and 024 (octal) for %:z / %::z.
    match colons {
        0 => do_number(
            result,
            flags,
            5,
            hour_diff * 100 + min_diff,
            negative,
            true,
            0,
        ),
        1 => do_number(
            result,
            flags,
            6,
            hour_diff * 100 + min_diff,
            negative,
            true,
            0o04,
        ),
        2 => do_number(
            result,
            flags,
            9,
            hour_diff * 10000 + min_diff * 100 + sec_diff,
            negative,
            true,
            0o24,
        ),
        _ => {
            // colons == 3: +hh if possible, else +hh:mm, else +hh:mm:ss.
            if sec_diff != 0 {
                do_number(
                    result,
                    flags,
                    9,
                    hour_diff * 10000 + min_diff * 100 + sec_diff,
                    negative,
                    true,
                    0o24,
                );
            } else if min_diff != 0 {
                do_number(
                    result,
                    flags,
                    6,
                    hour_diff * 100 + min_diff,
                    negative,
                    true,
                    0o04,
                );
            } else {
                do_number(result, flags, 3, hour_diff, negative, true, 0);
            }
        }
    }
}
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
