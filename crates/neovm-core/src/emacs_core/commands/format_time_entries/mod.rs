//! `format-time-string` (GNU `timefns.c`), a core-side Lisp entry.
//!
//! The strftime engine and its argument helpers are text-layer code; this
//! entry stays in core because it binds the zone unit through
//! `timefns::zone_offset_name_for_time` (the peer T2 ruling keeps the zone
//! helper in core). It calls the text engine downward; the registered subr's
//! name, arity and module_path are unchanged (the declaration site stays in
//! `subrs/mod.rs`).

use crate::emacs_core::format::{
    current_unix_timestamp, format_time, require_string, unix_to_broken_down,
};
use crate::emacs_core::timefns::zone_offset_name_for_time;
use neovm_value::error::{EvalResult, LispCondition, expect_min_args, signal};
use neovm_value::value::*;

/// Format TIME like C `strftime`.
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
        neovm_value::time_core::time_value_seconds_and_nanos(&args[1])?
    } else {
        (current_unix_timestamp(), 0)
    };

    let (offset_secs, zone_name) = zone_offset_name_for_time(args.get(2), timestamp)?;
    let tm = unix_to_broken_down(timestamp.saturating_add(offset_secs));
    let formatted = format_time(&format_str, &tm, timestamp, offset_secs, &zone_name, nanos);
    Ok(Value::string(formatted))
}
