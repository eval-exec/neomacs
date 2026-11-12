//! Time functions (GNU `timefns.c`).
//!
//! The exact `(TICKS . HZ)` conversion core — decode, arithmetic,
//! comparisons, the TimeMicros display view, and the current-time-list
//! output encoders — moved below the evaluator with the stage-3 split
//! (C16): `neovm_value::time_core`. This module keeps the evaluator-bound
//! layer: the subrs, the calendar and zone machinery (encode-time,
//! decode-time, current-time-zone), the world-state swap for tests, and
//! the `current-time-list` selector (`lisp_time_output_from_context`),
//! and re-exports the core's public surface so every historical
//! `timefns::` path is unchanged.

// ---------------------------------------------------------------------------
// Conversion-core facade (stage-3 C16): the moved substrate's public surface
// ---------------------------------------------------------------------------

pub use neovm_value::time_core::{
    LispTimeOutput, TimeConvertForm, TimeMicros, current_time_value, decode_lisp_time,
    frac_to_double, integer_fdiv_qr, make_lisp_time, parse_time, parse_time_detailed,
    ticks_hz_hz_ticks, ticks_hz_list4, ticks_hz_seconds, time_arith, time_cmp, time_error_overflow,
    time_spec_invalid, time_value_seconds_and_nanos, value_to_integer,
};

/// `current-time-list` selects the timestamp representation (GNU
/// `timefns.c`): the selector reads a dynamically bound variable through
/// the evaluator, so it stays above the split as a free function.
#[doc(hidden)]
pub fn lisp_time_output_from_context(eval: &Context) -> Result<LispTimeOutput, Flow> {
    eval.eval_symbol_by_id(intern("current-time-list"))
        .map(|value| {
            if value.is_truthy() {
                LispTimeOutput::LegacyList
            } else {
                LispTimeOutput::TicksHz
            }
        })
}

use malachite::integer::Integer;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::error::LispCondition;
use neovm_value::error::expect_args;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::intern::{intern, resolve_sym};
use neovm_value::value::*;
use neovm_value::value::{ValueKind, VecLikeType};
use std::cell::RefCell;
use std::ffi::{CStr, OsString};
use std::sync::{Mutex, OnceLock};
#[cfg(not(feature = "test-support"))]
mod calendar;
#[cfg(feature = "test-support")]
pub mod calendar;
use calendar::{CalendarFieldOverflow, CalendarTime};
#[cfg(all(unix, not(feature = "test-support")))]
mod mktime;
#[cfg(all(unix, feature = "test-support"))]
pub mod mktime;
#[cfg(not(feature = "test-support"))]
mod time_zone_spec;
#[cfg(feature = "test-support")]
pub mod time_zone_spec;
use time_zone_spec::TimeZoneSpec;
#[cfg(not(feature = "test-support"))]
mod unix_timestamp;
#[cfg(feature = "test-support")]
pub mod unix_timestamp;
use neovm_edit::emacs_core::edit_host::EditHost;
pub(crate) use unix_timestamp::{TimestampError, UnixTimestamp};
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
enum TimeConvertSymbolForm {
    #[strum(serialize = "integer")]
    Integer,
    #[strum(serialize = "list")]
    List,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
enum TimeZoneSymbol {
    #[strum(serialize = "wall")]
    Wall,
}
impl TimeZoneSymbol {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn from_value(value: &Value) -> Option<Self> {
        value.as_bare_symbol_name()?.parse().ok()
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn name(self) -> &'static str {
        self.into()
    }
}
// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_min_max_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Flow> {
    if args.len() < min || args.len() > max {
        Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol(name), Value::fixnum(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}
// ---------------------------------------------------------------------------
// Date/time breakdown helpers (UTC only, no chrono)
// ---------------------------------------------------------------------------

/// Offset between a calendar year and glibc's `struct tm` `tm_year` field
/// (GNU `TM_YEAR_BASE`): `tm_year = year - 1900`.
const TM_YEAR_BASE: i64 = 1900;
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
/// Decoded time in UTC: (sec min hour day month year dow dst utcoff).
#[cfg_attr(feature = "test-support", visibility::make(pub))]
struct DecodedTime {
    sec: i64,
    min: i64,
    hour: i64,
    day: i64,   // 1-based
    month: i64, // 1-based
    year: i64,
    dow: i64, // 0=Sunday, 1=Monday, ..., 6=Saturday
}

/// Read surfaces for the moved timefns oracle (peer #1005 field
/// remediation): every field is private in every arm; the oracle reads the
/// decoded UTC tuple through these getters.
#[cfg(any(test, feature = "test-support"))]
impl DecodedTime {
    /// Copied seconds component.
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    fn sec_for_test(&self) -> i64 {
        self.sec
    }

    /// Copied minutes component.
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    fn min_for_test(&self) -> i64 {
        self.min
    }

    /// Copied hours component.
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    fn hour_for_test(&self) -> i64 {
        self.hour
    }

    /// Copied day-of-month component.
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    fn day_for_test(&self) -> i64 {
        self.day
    }

    /// Copied month component.
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    fn month_for_test(&self) -> i64 {
        self.month
    }

    /// Copied year component.
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    fn year_for_test(&self) -> i64 {
        self.year
    }

    /// Copied day-of-week component.
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    fn dow_for_test(&self) -> i64 {
        self.dow
    }
}
struct ZonedDecodedTime {
    time: DecodedTime,
    dst: Value,
    utcoff: i64,
}
/// Convert a count of days since the Unix epoch (1970-01-01) into a proleptic
/// Gregorian `(year, month, day)`, in O(1). This is Howard Hinnant's
/// `civil_from_days` algorithm <http://howardhinnant.github.io/date_algorithms.html>,
/// which is the same closed form glibc's `__offtime`/`gmtime_r` uses; it
/// replaces the previous O(years) year-counting loop that took ~7e13 iterations
/// (and effectively hung) for inputs like `most-positive-fixnum`.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    // Shift the epoch to 0000-03-01 so the leap day falls at the end of a
    // 400-year era and the month arithmetic below is uniform.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    // Year-of-era, accounting for the 100-year and 400-year leap rules.
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11] (March-based month index)
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}
/// Break epoch seconds into UTC date/time components.
///
/// Returns an error (GNU `time_overflow`, "Specified time is not
/// representable") when the resulting year is not representable as glibc's
/// `tm_year` (an `int`); GNU's `emacs_localtime_rz`/`gmtime_r` fails the same
/// way, so e.g. `(decode-time most-positive-fixnum t)` signals rather than
/// returning an out-of-range year.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn decode_epoch_secs(total_secs: i64) -> Result<DecodedTime, Flow> {
    // Handle the time-of-day part
    let days = total_secs.div_euclid(86400);
    let day_secs = total_secs.rem_euclid(86400);

    let sec = day_secs % 60;
    let min = (day_secs / 60) % 60;
    let hour = day_secs / 3600;

    // Day of week: epoch (1970-01-01) was Thursday (4).
    // dow: 0=Sunday
    let dow = ((days % 7) + 4).rem_euclid(7);

    // Compute year, month, day from days since epoch via the closed form.
    let (year, month, day) = civil_from_days(days);

    // GNU `gmtime_r` stores the year in `tm_year` (= year - 1900) as an `int`,
    // and fails (errno = EOVERFLOW) when it would not fit. Reproduce that limit.
    if i32::try_from(year - TM_YEAR_BASE).is_err() {
        return Err(time_error_overflow());
    }

    Ok(DecodedTime {
        sec,
        min,
        hour,
        day,
        month,
        year,
        dow,
    })
}
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn encode_to_epoch_secs(sec: i64, min: i64, hour: i64, day: i64, month: i64, year: i64) -> i64 {
    CalendarTime::try_from([sec, min, hour, day, month, year])
        .unwrap()
        .epoch_seconds()
        .unwrap()
}
// ---------------------------------------------------------------------------
// Day/month name tables
// ---------------------------------------------------------------------------

const DAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
#[derive(Clone, Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
enum ZoneRule {
    Local,
    Utc,
    FixedOffset(i64),
    FixedNamedOffset(i64, String),
    TzString(String),
}
thread_local! {
    static TIME_ZONE_RULE: RefCell<ZoneRule> = const { RefCell::new(ZoneRule::Local) };
}
/// Round 3w (plan-3w.md section 7, C2): the time-zone rule is one value for
/// the World (GNU's TZ is process-wide; a process runs one World). It lives
/// in this thread-local while the Baton holder runs and in the World while
/// it is parked, so reads take no lock; the Green runtime moves it at each
/// park and resume. Several Worlds in one process each keep their own rule.
#[cold]
#[doc(hidden)]
pub fn world_state_placeholder() -> Box<dyn std::any::Any> {
    Box::new(ZoneRule::Local)
}
#[cold]
#[doc(hidden)]
pub fn swap_world_state(slot: &mut dyn std::any::Any) {
    let slot = slot
        .downcast_mut::<ZoneRule>()
        .expect("carried World state");
    TIME_ZONE_RULE.with(|rule| std::mem::swap(&mut *rule.borrow_mut(), slot));
}
/// Reset timezone rule to default (called from Context::new).
#[doc(hidden)]
pub fn reset_timefns_thread_locals() {
    TIME_ZONE_RULE.with(|slot| *slot.borrow_mut() = ZoneRule::Local);
}
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn tz_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
fn invalid_time_zone_spec(spec: &Value) -> Flow {
    signal(
        "error",
        vec![Value::string("Invalid time zone specification"), *spec],
    )
}
fn format_fixed_offset_name(offset_secs: i64) -> String {
    if offset_secs == 0 {
        return "GMT".to_string();
    }
    let sign = if offset_secs < 0 { '-' } else { '+' };
    let abs_secs = offset_secs.abs();
    if abs_secs % 3600 == 0 {
        format!("{}{abs_hours:02}", sign, abs_hours = abs_secs / 3600)
    } else if abs_secs % 60 == 0 {
        let total_minutes = abs_secs / 60;
        format!(
            "{}{hours:02}{mins:02}",
            sign,
            hours = total_minutes / 60,
            mins = total_minutes % 60
        )
    } else {
        format!(
            "{}{hours:02}{mins:02}{secs:02}",
            sign,
            hours = abs_secs / 3600,
            mins = (abs_secs % 3600) / 60,
            secs = abs_secs % 60
        )
    }
}
/// GNU's numeric-zone abbreviation, synthesized when libc reports an empty
/// zone name (`Fcurrent_time_zone`, `src/timefns.c:1937`). Unlike
/// `format_fixed_offset_name`, an offset of 0 yields the numeric `"+00"` rather
/// than `"GMT"`, and the sign always reflects the (already clamped) offset.
fn numeric_zone_abbrev(offset: i64) -> String {
    let hour = offset / 3600;
    let min_sec = offset % 3600;
    let mut buf = String::new();
    buf.push(if offset < 0 { '-' } else { '+' });
    buf.push_str(&format!("{:02}", hour.abs()));
    if min_sec != 0 {
        let amin_sec = min_sec.abs();
        let min = amin_sec / 60;
        let sec = amin_sec % 60;
        buf.push_str(&format!("{:02}", min));
        if sec != 0 {
            buf.push_str(&format!("{:02}", sec));
        }
    }
    buf
}
/// Build the POSIX TZ string GNU's `tzlookup` hands to libc `tzalloc` for an
/// explicit integer ZONE (`src/timefns.c:277`). The bracketed part is the
/// numeric abbreviation `<+HH[MM[SS]]>`; the body is always `HH:MM:SS`. Note the
/// POSIX body sign is inverted relative to the Emacs offset: a leading body sign
/// is "time to add to local to reach UTC", so a *positive* (east-of-UTC) offset
/// gets a "-" body and a negative offset an empty body — matching GNU's
/// `&"-"[XFIXNUM (zone) < 0]`.
fn tz_string_for_fixed_offset(offset: i64) -> String {
    let abszone = offset.abs();
    let hour = abszone / 3600;
    let hour_remainder = abszone % 3600;
    let min = hour_remainder / 60;
    let sec = hour_remainder % 60;

    // numzone packs HHMMSS to whatever precision the offset needs, matching
    // GNU's prec/numzone construction.
    let (prec, numzone) = if hour_remainder == 0 {
        (2usize, hour)
    } else if sec == 0 {
        (4usize, 100 * hour + min)
    } else {
        (6usize, 100 * (100 * hour + min) + sec)
    };
    let signed_numzone = if offset < 0 { -numzone } else { numzone };
    let body_sign = if offset < 0 { "" } else { "-" };
    format!(
        "<{:+0width$}>{}{}:{:02}:{:02}",
        signed_numzone,
        body_sign,
        hour,
        min,
        sec,
        // The `+` flag already counts toward the field width, so add 1 for it.
        width = prec + 1,
    )
}
/// Build GNU's POSIX TZ string for an explicit `(OFFSET NAME)` ZONE
/// (`src/timefns.c:292`): `<NAME>` followed by the signed `HH:MM:SS` body. The
/// body sign is inverted relative to the Emacs offset, exactly as for the
/// integer case (`&"-"[XFIXNUM (zone) < 0]`).
fn tz_string_for_named_offset(offset: i64, name: &str) -> String {
    let abszone = offset.abs();
    let hour = abszone / 3600;
    let hour_remainder = abszone % 3600;
    let min = hour_remainder / 60;
    let sec = hour_remainder % 60;
    let body_sign = if offset < 0 { "" } else { "-" };
    format!("<{}>{}{}:{:02}:{:02}", name, body_sign, hour, min, sec)
}
/// Resolve an explicit numeric/named ZONE through libc exactly as GNU does:
/// build the POSIX TZ string GNU's `tzlookup` would, then read back the
/// clamped `tm_gmtoff` and validated `tm_zone`. libc clamps the offset to
/// +/-24h and rejects abbreviations shorter than 3 characters (empty zone),
/// for which GNU substitutes the numeric `+HH[MM[SS]]` abbreviation.
#[cfg(unix)]
fn fixed_zone_offset_name(tz_string: &str, epoch_secs: i64) -> (i64, String) {
    let (offset, name) = with_tz_env(Some(tz_string), || {
        let mut time_val: libc::time_t = epoch_secs as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let tm_ptr = unsafe { libc::localtime_r(&mut time_val as *mut _, &mut tm as *mut _) };
        if tm_ptr.is_null() {
            return (0i64, String::new());
        }
        let offset = tm.tm_gmtoff as i64;
        let name = if tm.tm_zone.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(tm.tm_zone) }
                .to_string_lossy()
                .into_owned()
        };
        (offset, name)
    });
    let name = if name.is_empty() {
        numeric_zone_abbrev(offset)
    } else {
        name
    };
    (offset, name)
}
#[cfg(unix)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn local_offset_name_at_epoch(epoch_secs: i64) -> (i64, String) {
    let mut time_val: libc::time_t = epoch_secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let tm_ptr = unsafe { libc::localtime_r(&mut time_val as *mut _, &mut tm as *mut _) };
    if tm_ptr.is_null() {
        return (0, "UTC".to_string());
    }
    let offset = tm.tm_gmtoff as i64;
    let name = if tm.tm_zone.is_null() {
        format_fixed_offset_name(offset)
    } else {
        unsafe { CStr::from_ptr(tm.tm_zone) }
            .to_string_lossy()
            .into_owned()
    };
    (offset, name)
}
#[cfg(not(unix))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn local_offset_name_at_epoch(_epoch_secs: i64) -> (i64, String) {
    (0, "UTC".to_string())
}
#[cfg(unix)]
fn local_decoded_time_at_epoch(epoch_secs: i64) -> Result<ZonedDecodedTime, Flow> {
    let mut time_val: libc::time_t = epoch_secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let tm_ptr = unsafe { libc::localtime_r(&mut time_val as *mut _, &mut tm as *mut _) };
    if tm_ptr.is_null() {
        return Err(signal(
            "error",
            vec![Value::string("Invalid time specification")],
        ));
    }

    let dst = match tm.tm_isdst {
        n if n < 0 => Value::fixnum(-1),
        0 => Value::NIL,
        _ => Value::T,
    };

    Ok(ZonedDecodedTime {
        time: DecodedTime {
            sec: tm.tm_sec as i64,
            min: tm.tm_min as i64,
            hour: tm.tm_hour as i64,
            day: tm.tm_mday as i64,
            month: tm.tm_mon as i64 + 1,
            year: tm.tm_year as i64 + 1900,
            dow: tm.tm_wday as i64,
        },
        utcoff: tm.tm_gmtoff as i64,
        dst,
    })
}
#[cfg(not(unix))]
fn local_decoded_time_at_epoch(epoch_secs: i64) -> Result<ZonedDecodedTime, Flow> {
    let time = decode_epoch_secs(epoch_secs)?;
    Ok(ZonedDecodedTime {
        time,
        dst: Value::NIL,
        utcoff: 0,
    })
}
#[cfg(unix)]
fn refresh_tz_env() {
    unsafe extern "C" {
        fn tzset();
    }
    // SAFETY: tzset has no pointer arguments. Timezone mutations using this
    // helper are serialized by ScopedTzEnv's process-wide lock.
    unsafe {
        tzset();
    }
}
#[cfg(not(unix))]
fn refresh_tz_env() {}
/// Serializes local libc timezone reads with temporary timezone writers.
/// This lock-only guard preserves the process TZ and stays on its mutator thread.
#[cfg(unix)]
#[must_use = "dropping the guard permits temporary timezone writers"]
#[derive(Debug)]
struct LocalTzReadGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
#[cfg(unix)]
static_assertions::assert_not_impl_any!(LocalTzReadGuard: Send, Sync);
#[cfg(unix)]
impl LocalTzReadGuard {
    fn new() -> Self {
        Self {
            _lock: tz_env_lock()
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
            _thread: std::marker::PhantomData,
        }
    }
}
/// Restores the process timezone while retaining the serialization lock.
/// The owned lock and thread marker keep the guard confined to its creating thread.
#[must_use = "dropping the guard restores the previous timezone"]
#[derive(Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
struct ScopedTzEnv {
    previous: Option<OsString>,
    _lock: std::sync::MutexGuard<'static, ()>,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
static_assertions::assert_not_impl_any!(ScopedTzEnv: Send, Sync);
impl ScopedTzEnv {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn new(spec: Option<TimeZoneSpec<'_>>) -> Self {
        let lock = tz_env_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let previous = std::env::var_os("TZ");
        match spec {
            // SAFETY: the guard owns the timezone serialization lock, and
            // TimeZoneSpec guarantees there is no embedded NUL.
            Some(v) => unsafe { std::env::set_var("TZ", v.as_ref()) },
            // SAFETY: the guard owns the timezone serialization lock.
            None => unsafe { std::env::remove_var("TZ") },
        }
        refresh_tz_env();
        Self {
            previous,
            _lock: lock,
            _thread: std::marker::PhantomData,
        }
    }
}
impl Drop for ScopedTzEnv {
    fn drop(&mut self) {
        match &self.previous {
            // SAFETY: the serialization lock is still held, and an OS
            // environment value contains no embedded NUL.
            Some(v) => unsafe { std::env::set_var("TZ", v) },
            // SAFETY: the serialization lock is still held.
            None => unsafe { std::env::remove_var("TZ") },
        }
        refresh_tz_env();
    }
}
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn with_tz_env<T>(spec: Option<&str>, f: impl FnOnce() -> T) -> T {
    let _guard = ScopedTzEnv::new(spec.map(TimeZoneSpec::from));
    f()
}
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn parse_zone_rule(zone: &Value) -> Result<ZoneRule, Flow> {
    match zone.kind() {
        ValueKind::Nil => Ok(ZoneRule::Local),
        ValueKind::T => Ok(ZoneRule::Utc),
        ValueKind::Symbol(_) => match TimeZoneSymbol::from_value(zone) {
            Some(TimeZoneSymbol::Wall) => Ok(ZoneRule::Local),
            None => Err(invalid_time_zone_spec(zone)),
        },
        // GNU `tzlookup` (timefns.c:250) treats the integer 0 as a special
        // UTC alias, identical to `t`: its zone abbreviation is "GMT", not the
        // numeric "+00" that a generic fixed offset would produce. All other
        // integers are genuine fixed offsets.
        ValueKind::Fixnum(0) => Ok(ZoneRule::Utc),
        ValueKind::Fixnum(n) => Ok(ZoneRule::FixedOffset(n)),
        ValueKind::String => Ok(ZoneRule::TzString(
            zone.as_lisp_string()
                .map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
                .expect("ValueKind::String must carry LispString payload"),
        )),
        ValueKind::Cons => {
            let items = list_to_vec(zone).ok_or_else(|| invalid_time_zone_spec(zone))?;
            if items.len() != 2 {
                return Err(invalid_time_zone_spec(zone));
            }
            let Some(offset) = items[0].as_int() else {
                return Err(invalid_time_zone_spec(zone));
            };
            let name = match items[1].kind() {
                ValueKind::String => items[1]
                    .as_lisp_string()
                    .map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
                    .expect("ValueKind::String must carry LispString payload"),
                ValueKind::Symbol(id) => resolve_sym(id).to_owned(),
                _ => return Err(invalid_time_zone_spec(zone)),
            };
            Ok(ZoneRule::FixedNamedOffset(offset, name))
        }
        _ => Err(invalid_time_zone_spec(zone)),
    }
}
fn effective_zone_rule(zone: Option<&Value>) -> Result<ZoneRule, Flow> {
    match zone {
        None => TIME_ZONE_RULE.with(|slot| Ok(slot.borrow().clone())),
        Some(value) if value.is_nil() => TIME_ZONE_RULE.with(|slot| Ok(slot.borrow().clone())),
        Some(value) => parse_zone_rule(value),
    }
}
/// GNU's UTC zone has no `timezone_t`: `tzlookup` returns `utc_tz`, which is
/// NULL, so `emacs_localtime_rz` falls through to `gmtime_r`
/// (`src/timefns.c:198-199`, :249-254, :206-218).  The abbreviation is
/// therefore the C library's own: glibc says "GMT", Darwin says "UTC".
#[cfg(unix)]
fn utc_zone_abbreviation(epoch_secs: i64) -> String {
    let value: libc::time_t = epoch_secs as libc::time_t;
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    // SAFETY: `tm` is a valid out-parameter for the call.
    let converted = unsafe { libc::gmtime_r(&value, &mut tm) };
    if converted.is_null() || tm.tm_zone.is_null() {
        return numeric_zone_abbrev(0);
    }
    // SAFETY: `tm_zone` points at a NUL-terminated string owned by libc.
    let name = unsafe { CStr::from_ptr(tm.tm_zone) }
        .to_string_lossy()
        .into_owned();
    if name.is_empty() {
        numeric_zone_abbrev(0)
    } else {
        name
    }
}
#[cfg(unix)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn zone_rule_to_offset_name(rule: &ZoneRule, epoch_secs: i64) -> (i64, String) {
    match rule {
        ZoneRule::Local => {
            let _guard = LocalTzReadGuard::new();
            local_offset_name_at_epoch(epoch_secs)
        }
        ZoneRule::Utc => (0, utc_zone_abbreviation(epoch_secs)),
        // GNU `tzlookup` builds a POSIX TZ string and hands it to libc
        // `tzalloc`; the resulting offset is therefore clamped to +/-24h and a
        // too-short/invalid abbreviation falls back to UTC.
        ZoneRule::FixedOffset(offset) => {
            fixed_zone_offset_name(&tz_string_for_fixed_offset(*offset), epoch_secs)
        }
        ZoneRule::FixedNamedOffset(offset, name) => {
            fixed_zone_offset_name(&tz_string_for_named_offset(*offset, name), epoch_secs)
        }
        ZoneRule::TzString(spec) => {
            with_tz_env(Some(spec), || local_offset_name_at_epoch(epoch_secs))
        }
    }
}
/// Clamp a fixed zone offset (in seconds) to +/-24h. GNU routes an explicit
/// numeric/named ZONE through libc `tzalloc`, which enforces this bound; the
/// non-Unix fallback (which does not call libc) applies it directly.
#[cfg(not(unix))]
fn fixed_offset_clamped(offset: i64) -> i64 {
    offset.clamp(-86_400, 86_400)
}
#[cfg(not(unix))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn zone_rule_to_offset_name(rule: &ZoneRule, epoch_secs: i64) -> (i64, String) {
    match rule {
        ZoneRule::Local => local_offset_name_at_epoch(epoch_secs),
        ZoneRule::Utc => (0, "GMT".to_string()),
        ZoneRule::FixedOffset(offset) => {
            let clamped = fixed_offset_clamped(*offset);
            (clamped, numeric_zone_abbrev(clamped))
        }
        ZoneRule::FixedNamedOffset(offset, name) => {
            let clamped = fixed_offset_clamped(*offset);
            // Short names are invalid: libc would fall back to UTC.
            if name.chars().count() < 3 {
                (0, numeric_zone_abbrev(0))
            } else {
                (clamped, name.clone())
            }
        }
        ZoneRule::TzString(spec) => {
            with_tz_env(Some(spec), || local_offset_name_at_epoch(epoch_secs))
        }
    }
}
#[doc(hidden)]
pub fn zone_offset_name_for_time(
    zone: Option<&Value>,
    epoch_secs: i64,
) -> Result<(i64, String), Flow> {
    let rule = effective_zone_rule(zone)?;
    Ok(zone_rule_to_offset_name(&rule, epoch_secs))
}
fn decode_time_for_zone(rule: &ZoneRule, epoch_secs: i64) -> Result<ZonedDecodedTime, Flow> {
    match rule {
        ZoneRule::Local => {
            #[cfg(unix)]
            let _guard = LocalTzReadGuard::new();
            local_decoded_time_at_epoch(epoch_secs)
        }
        ZoneRule::Utc => Ok(ZonedDecodedTime {
            time: decode_epoch_secs(epoch_secs)?,
            dst: Value::NIL,
            utcoff: 0,
        }),
        // Mirror GNU: route the explicit numeric/named ZONE through libc so the
        // offset (and the broken-down time it produces) is clamped to +/-24h
        // and an invalid/too-short abbreviation falls back to UTC.
        ZoneRule::FixedOffset(offset) => {
            decode_time_for_fixed_zone(&tz_string_for_fixed_offset(*offset), *offset, epoch_secs)
        }
        ZoneRule::FixedNamedOffset(offset, name) => decode_time_for_fixed_zone(
            &tz_string_for_named_offset(*offset, name),
            *offset,
            epoch_secs,
        ),
        ZoneRule::TzString(spec) => {
            with_tz_env(Some(spec), || local_decoded_time_at_epoch(epoch_secs))
        }
    }
}
/// Decode a time at an explicit numeric/named ZONE, honoring libc's clamping
/// and short-name fallback. On Unix this defers to libc (`localtime_r` under
/// the GNU-style TZ string); elsewhere it applies the clamp directly.
#[cfg(unix)]
fn decode_time_for_fixed_zone(
    tz_string: &str,
    _offset: i64,
    epoch_secs: i64,
) -> Result<ZonedDecodedTime, Flow> {
    with_tz_env(Some(tz_string), || local_decoded_time_at_epoch(epoch_secs))
}
#[cfg(not(unix))]
fn decode_time_for_fixed_zone(
    _tz_string: &str,
    offset: i64,
    epoch_secs: i64,
) -> Result<ZonedDecodedTime, Flow> {
    let clamped = fixed_offset_clamped(offset);
    Ok(ZonedDecodedTime {
        time: decode_epoch_secs(epoch_secs.saturating_add(clamped))?,
        dst: Value::NIL,
        utcoff: clamped,
    })
}
/// Extract a time component as a fixnum, mirroring GNU `encode-time`'s
/// `CHECK_FIXNUM`: nil, non-numbers, AND bignums all signal
/// `wrong-type-argument fixnump VALUE`. Verified against GNU 31.0.50 — both
/// `(encode-time 0 0 0 2026 5 nil)` and a bignum year signal `fixnump`, not
/// `integerp`. (This is what made org-datetree-find-month-create on a 2-element
/// date — where the extracted year is nil — diverge: integerp vs fixnump.)
fn require_fixnum_component(value: &Value) -> Result<i64, Flow> {
    value.as_fixnum().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), *value],
        )
    })
}
/// Decode `encode-time`'s SECOND field, which (unlike the other components)
/// accepts any Lisp time value, mirroring GNU `Fencode_time` /
/// `decode_lisp_time (secarg, CFORM_TICKS_HZ)` (`src/timefns.c:1659`).
///
/// Returns `(sec, subsec_ticks, hz)` where `sec = floor(ticks / hz)` is the
/// integer-seconds contribution folded into the broken-down time, and
/// `subsec_ticks` is the remainder carried into the resulting timestamp at the
/// resolution `hz`. A plain fixnum keeps the historical fast path (`hz == 1`).
/// As with GNU's later `check_tm_member`, `sec` must be representable as a
/// fixnum or `wrong-type-argument fixnump` is signalled.
fn decode_encode_time_second(value: &Value) -> Result<(i64, Integer, Integer), Flow> {
    if let Some(n) = value.as_fixnum() {
        return Ok((n, Integer::from(0), Integer::from(1)));
    }
    // GNU `Fencode_time` decodes the SECOND field with `decode_lisp_time
    // (secarg, CFORM_TICKS_HZ)` (`src/timefns.c:1736`), so it accepts every time
    // form `decode_lisp_time` does: nil means the current time, numbers and time
    // lists/conses decode normally, and any other value (a symbol, a string, ...)
    // signals `time_spec_invalid` ("Invalid time specification") — NOT a
    // `wrong-type-argument fixnump`.
    if !matches!(
        value.kind(),
        ValueKind::Nil | ValueKind::Float | ValueKind::Cons | ValueKind::Veclike(_)
    ) {
        return Err(time_spec_invalid());
    }
    // GNU splits the exact (TICKS . HZ) with `fdiv_qr' (`src/timefns.c:1745`),
    // so HZ (and the remainder) may be bignums.
    let th = decode_lisp_time(value)?.th;
    let (sec, subsec_ticks) = integer_fdiv_qr(&th.ticks, &th.hz);
    // `sec' becomes `tm.tm_sec` via GNU `check_tm_member (sec, 0)`
    // (`src/timefns.c:1752`), which requires it fit in an `int` and otherwise
    // signals `time_overflow` ("Specified time is not representable").
    let sec = i64::try_from(&sec)
        .ok()
        .filter(|sec| i32::try_from(*sec).is_ok())
        .ok_or_else(time_error_overflow)?;
    Ok((sec, subsec_ticks, th.hz))
}
/// GNU `Fencode_time`'s broken-down-time daylight-saving flag (the input
/// `tm.tm_isdst` it hands to `mktime_z`, `src/timefns.c:1696`/`1718`/`1761`):
/// `-1` auto-detects DST from the wall clock, `0` forces standard time, and `1`
/// forces daylight saving time. An explicit `nil` DST slot means `0`, an
/// explicit `t` means `1`, and a missing/`-1`/non-symbol slot stays `-1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TmIsDst {
    Auto,
    Standard,
    Daylight,
}
impl TmIsDst {
    fn to_c(self) -> libc::c_int {
        match self {
            TmIsDst::Auto => -1,
            TmIsDst::Standard => 0,
            TmIsDst::Daylight => 1,
        }
    }
}
/// Compute the epoch seconds for a broken-down local time in a DST-aware zone
/// (`Local`/`TzString`) honoring the forced `isdst` flag, mirroring GNU's
/// `mktime_z` (`src/timefns.c:1761`). Unlike `localtime_r`, libc `mktime`
/// treats `tm_isdst` as an *input* that disambiguates which UTC offset applies,
/// so an explicit `nil` DST slot forces standard time even during the summer.
#[cfg(unix)]
fn mktime_with_isdst(calendar: CalendarTime, isdst: TmIsDst) -> Result<i64, Flow> {
    let mut tm = calendar.into_tm();
    tm.tm_isdst = isdst.to_c();
    tm.tm_wday = -1;
    // GNU's `Fencode_time` calls `mktime_z`, which on Apple resolves to
    // gnulib's replacement and on glibc to the same algorithm in libc.  The
    // port keeps the answer GNU's on both (Darwin's own `mktime` rejects every
    // time before 1900; libc/mktime.c is the reference).
    match mktime::mktime_local(&mut tm) {
        Ok(seconds) => Ok(seconds),
        Err(mktime::MktimeError::Overflow) => Err(time_error_overflow()),
        Err(mktime::MktimeError::Invalid) => Err(time_spec_invalid()),
    }
}
#[cfg(not(unix))]
fn mktime_with_isdst(calendar: CalendarTime, _isdst: TmIsDst) -> Result<i64, Flow> {
    calendar.epoch_seconds()
}
#[cfg(unix)]
fn encode_calendar_in_fixed_zone(
    calendar: CalendarTime,
    spec: &str,
    _offset: i64,
) -> Result<i64, Flow> {
    with_tz_env(Some(spec), || mktime_with_isdst(calendar, TmIsDst::Auto))
}
#[cfg(not(unix))]
fn encode_calendar_in_fixed_zone(
    calendar: CalendarTime,
    _spec: &str,
    offset: i64,
) -> Result<i64, Flow> {
    let seconds = calendar
        .epoch_seconds()?
        .checked_sub(fixed_offset_clamped(offset))
        .ok_or_else(time_error_overflow)?;
    Ok(seconds)
}
/// An offset that POSIX timezone parsing leaves unchanged. The private field
/// permits direct calendar arithmetic without bypassing GNU's offset clamping.
/// This immutable scalar contains no mutator state and is Send + Sync.
#[derive(Clone, Copy, Debug)]
struct DirectZoneOffset(i32);
#[derive(Clone, Copy, Debug, thiserror::Error)]
enum OffsetNormalization {
    #[error("Offset requires timezone normalization")]
    Required,
}
impl TryFrom<i64> for DirectZoneOffset {
    type Error = OffsetNormalization;

    fn try_from(offset: i64) -> Result<Self, Self::Error> {
        if !(-86_400..=86_400).contains(&offset) {
            return Err(OffsetNormalization::Required);
        }
        Ok(Self(
            i32::try_from(offset).map_err(|_| OffsetNormalization::Required)?,
        ))
    }
}
impl From<DirectZoneOffset> for i64 {
    fn from(offset: DirectZoneOffset) -> Self {
        i64::from(offset.0)
    }
}
static_assertions::assert_impl_all!(DirectZoneOffset: Send, Sync);
impl CalendarTime {
    fn at_offset(self, offset: DirectZoneOffset) -> Result<i64, Flow> {
        let seconds = self
            .epoch_seconds()?
            .checked_sub(i64::from(offset))
            .ok_or_else(time_error_overflow)?;
        // GNU also requires the resulting UTC year to fit tm_year.
        decode_epoch_secs(seconds)?;
        Ok(seconds)
    }
}
fn encode_calendar_numeric_zone(calendar: CalendarTime, offset: i64) -> Result<i64, Flow> {
    match DirectZoneOffset::try_from(offset) {
        Ok(offset) => calendar.at_offset(offset),
        Err(OffsetNormalization::Required) => {
            encode_calendar_in_fixed_zone(calendar, &tz_string_for_fixed_offset(offset), offset)
        }
    }
}
fn encode_calendar_named_zone(
    calendar: CalendarTime,
    offset: i64,
    name: &str,
) -> Result<i64, Flow> {
    if name.len() >= 3 && name.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        match DirectZoneOffset::try_from(offset) {
            Ok(offset) => return calendar.at_offset(offset),
            Err(OffsetNormalization::Required) => {}
        }
    }
    encode_calendar_in_fixed_zone(calendar, &tz_string_for_named_offset(offset, name), offset)
}
/// Encode a broken-down local time to epoch seconds honoring the zone's DST
/// rules and the forced `isdst` flag, mirroring GNU `Fencode_time` +
/// `mktime_z`. For zones without DST transitions (`Utc`/fixed offsets) the flag
/// is irrelevant and a plain offset subtraction suffices.
fn encode_time_to_epoch(calendar: CalendarTime, zone: &Value, isdst: TmIsDst) -> Result<i64, Flow> {
    let rule = effective_zone_rule(Some(zone))?;

    match rule {
        ZoneRule::Local => {
            #[cfg(unix)]
            let _guard = LocalTzReadGuard::new();
            mktime_with_isdst(calendar, isdst)
        }
        ZoneRule::TzString(spec) => with_tz_env(Some(&spec), || mktime_with_isdst(calendar, isdst)),
        ZoneRule::Utc => calendar.epoch_seconds(),
        ZoneRule::FixedOffset(offset) => encode_calendar_numeric_zone(calendar, offset),
        ZoneRule::FixedNamedOffset(offset, name) => {
            encode_calendar_named_zone(calendar, offset, &name)
        }
    }
}
// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// GNU `invalid_hz` (`src/timefns.c:376`): `xsignal2 (Qerror, "Invalid time
/// frequency", hz)`. The error data always carries the offending FORM/HZ value.
fn invalid_time_frequency_error(hz: &Value) -> Flow {
    signal("error", vec![Value::string("Invalid time frequency"), *hz])
}
fn parse_time_convert_form(
    form: &Value,
    default_output: LispTimeOutput,
) -> Result<TimeConvertForm, Flow> {
    match form.kind() {
        ValueKind::Nil => match default_output {
            LispTimeOutput::LegacyList => Ok(TimeConvertForm::List),
            LispTimeOutput::TicksHz => Ok(TimeConvertForm::InputHz),
        },
        ValueKind::T => Ok(TimeConvertForm::InputHz),
        ValueKind::Fixnum(hz) if hz > 0 => Ok(TimeConvertForm::ExplicitHz(Integer::from(hz))),
        ValueKind::Fixnum(_) => Err(invalid_time_frequency_error(form)),
        ValueKind::Veclike(VecLikeType::Bignum) => {
            let hz = form
                .as_bignum()
                .expect("ValueKind::Bignum must carry Integer payload")
                .clone();
            if hz > 0 {
                Ok(TimeConvertForm::ExplicitHz(hz))
            } else {
                Err(invalid_time_frequency_error(form))
            }
        }
        ValueKind::Symbol(id) => match resolve_sym(id).parse::<TimeConvertSymbolForm>().ok() {
            Some(TimeConvertSymbolForm::List) => Ok(TimeConvertForm::List),
            Some(TimeConvertSymbolForm::Integer) => Ok(TimeConvertForm::Integer),
            None => Err(invalid_time_frequency_error(form)),
        },
        _ => Err(invalid_time_frequency_error(form)),
    }
}
#[doc(hidden)]
pub fn builtin_current_time_in_context(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("current-time", &args, 0)?;
    Ok(current_time_value(lisp_time_output_from_context(eval)?))
}
/// `(float-time &optional TIME)` -> float seconds since epoch.
///
/// GNU `Ffloat_time` returns the float argument unchanged when given a float,
/// and otherwise `frac_to_double` of the exact `(TICKS . HZ)` decoding
/// (`src/timefns.c:1309`). Crucially the decoding is exact, so a bignum-TICKS
/// timestamp (such as the one `encode-time` produces from a float SECOND)
/// converts losslessly instead of overflowing an i64.
#[doc(hidden)]
pub fn builtin_float_time(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("float-time", &args, 0, 1)?;
    let arg = if args.is_empty() { Value::NIL } else { args[0] };
    if matches!(arg.kind(), ValueKind::Float) {
        return Ok(arg);
    }
    let th = decode_lisp_time(&arg)?.th;
    Ok(Value::make_float(frac_to_double(&th.ticks, &th.hz)))
}
/// `(time-add A B)` -> integer seconds, `(HI LO US PS)`, or `(TICKS . HZ)`.
#[doc(hidden)]
pub fn builtin_time_add(args: Vec<Value>) -> EvalResult {
    expect_args("time-add", &args, 2)?;
    time_arith(&args[0], &args[1], false)
}
/// `(time-subtract A B)` -> integer seconds, `(HI LO US PS)`, or `(TICKS . HZ)`.
#[doc(hidden)]
pub fn builtin_time_subtract(args: Vec<Value>) -> EvalResult {
    expect_args("time-subtract", &args, 2)?;
    // GNU subtracts identical objects to a zero timestamp without validating,
    // so `(time-subtract X X)` never errors (`src/timefns.c:1238`). The guard
    // uses `BASE_EQ` (object identity), so compare bits, not structural equal.
    if args[0].bits() == args[1].bits() {
        return Ok(TimeMicros {
            secs: 0,
            usecs: 0,
            psecs: 0,
        }
        .to_list());
    }
    time_arith(&args[0], &args[1], true)
}
/// `(time-less-p A B)` -> t or nil
#[doc(hidden)]
pub fn builtin_time_less_p(args: Vec<Value>) -> EvalResult {
    expect_args("time-less-p", &args, 2)?;
    // GNU time_cmp short-circuits identical objects (`BASE_EQ`) as equal, so
    // `<` is false (`src/timefns.c:1255`).
    if args[0].bits() == args[1].bits() {
        return Ok(Value::NIL);
    }
    Ok(Value::bool_val(
        time_cmp(&args[0], &args[1])? == std::cmp::Ordering::Less,
    ))
}
/// `(time-equal-p A B)` -> t or nil
#[doc(hidden)]
pub fn builtin_time_equal_p(args: Vec<Value>) -> EvalResult {
    expect_args("time-equal-p", &args, 2)?;
    // GNU timefns.c:time_cmp first treats identical Lisp objects (`BASE_EQ`,
    // object identity) as equal, so `(time-equal-p nil nil)' and other `eq'
    // inputs avoid validation.
    if args[0].bits() == args[1].bits() {
        return Ok(Value::T);
    }
    // GNU Ftime_equal_p also avoids interpreting one nil as "current time"
    // when the other argument is non-nil.
    if args[0].is_nil() || args[1].is_nil() {
        return Ok(Value::NIL);
    }
    Ok(Value::bool_val(
        time_cmp(&args[0], &args[1])? == std::cmp::Ordering::Equal,
    ))
}
/// `(current-time-string &optional TIME ZONE)` -> human-readable string.
///
/// Returns a string like `"Mon Jan  2 15:04:05 2006"`.
#[doc(hidden)]
pub fn builtin_current_time_string(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("current-time-string", &args, 0, 2)?;
    let tm = if args.is_empty() || args[0].is_nil() {
        TimeMicros::now()
    } else {
        parse_time(&args[0])?
    };
    let (offset_secs, _) = zone_offset_name_for_time(args.get(1), tm.secs)?;
    let dt = decode_epoch_secs(tm.secs.saturating_add(offset_secs))?;

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
#[doc(hidden)]
pub fn builtin_current_time_zone(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("current-time-zone", &args, 0, 2)?;
    let tm = if args.is_empty() || args[0].is_nil() {
        TimeMicros::now()
    } else {
        parse_time(&args[0])?
    };

    let rule = effective_zone_rule(args.get(1))?;

    let (offset, name) = zone_rule_to_offset_name(&rule, tm.secs);
    // GNU `Fcurrent_time_zone` (`src/timefns.c:1935`): when the zone name comes
    // back empty (e.g. an invalid/too-short abbreviation, or a malformed POSIX
    // TZ string that libc rejects to UTC), substitute the numeric `+HH[MM[SS]]`
    // abbreviation rather than reporting an empty string.
    let name = if name.is_empty() {
        numeric_zone_abbrev(offset)
    } else {
        name
    };
    Ok(Value::list(vec![
        Value::fixnum(offset),
        Value::string(name),
    ]))
}
/// GNU `CHECK_CONS`: signal `(wrong-type-argument consp OBJ)` unless OBJ is a
/// cons cell. Returns `(car, cdr)` for a valid cons.
fn check_cons(obj: &Value) -> Result<(Value, Value), Flow> {
    if obj.is_cons() {
        Ok((obj.cons_car(), obj.cons_cdr()))
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), *obj],
        ))
    }
}
/// `(encode-time TIME &rest OBSOLESCENT-ARGUMENTS)` -> `(HIGH LOW)`
#[doc(hidden)]
pub fn builtin_encode_time(args: Vec<Value>) -> EvalResult {
    // The SECOND field accepts any Lisp time value (GNU decodes it via
    // `decode_lisp_time'); its sub-second resolution carries into the result.
    let (sec, subsec_ticks, hz, min, hour, day, month, year, zone, isdst) = if args.len() == 1 {
        // GNU `Fencode_time` (`src/timefns.c:1698`) walks the single list
        // argument cons-by-cons: `for (i=0;i<6;i++) CHECK_CONS(tail)` then peels
        // SECOND..YEAR off the head. A malformed tail therefore signals
        // `(wrong-type-argument consp OFFENDING-CELL)` on the specific cell — not
        // a `listp` error on the whole argument.
        let a = args[0];
        let mut tail = a;
        for _ in 0..6 {
            let (_, cdr) = check_cons(&tail)?;
            tail = cdr;
        }
        let (secarg, a) = check_cons(&a)?;
        let (minarg, a) = check_cons(&a)?;
        let (hourarg, a) = check_cons(&a)?;
        let (mdayarg, a) = check_cons(&a)?;
        let (monarg, a) = check_cons(&a)?;
        let (yeararg, a) = check_cons(&a)?;
        // ZONE is the element after the IGNORED and DST fields, when present.
        // GNU `CHECK_CONS`-walks them too. The DST slot only forces the
        // broken-down time's `tm_isdst` when it is a symbol AND ZONE is neither
        // a fixnum nor a cons (`src/timefns.c:1717`): an explicit `nil` forces
        // standard time, an explicit `t` forces daylight saving, and any other
        // DST value leaves the default auto-detection (`tm_isdst = -1`).
        let (zone, isdst) = if a.is_nil() {
            (Value::NIL, TmIsDst::Auto)
        } else {
            let (_ignored, a) = check_cons(&a)?;
            let (dstflag, a) = check_cons(&a)?;
            let (zoneval, _) = check_cons(&a)?;
            let isdst = if dstflag.is_bare_symbol() && !zoneval.is_fixnum() && !zoneval.is_cons() {
                if dstflag.is_nil() {
                    TmIsDst::Standard
                } else {
                    TmIsDst::Daylight
                }
            } else {
                TmIsDst::Auto
            };
            (zoneval, isdst)
        };
        let (sec, subsec_ticks, hz) = decode_encode_time_second(&secarg)?;
        (
            sec,
            subsec_ticks,
            hz,
            require_fixnum_component(&minarg)?,
            require_fixnum_component(&hourarg)?,
            require_fixnum_component(&mdayarg)?,
            require_fixnum_component(&monarg)?,
            require_fixnum_component(&yeararg)?,
            zone,
            isdst,
        )
    } else if args.len() < 6 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("encode-time"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    } else {
        // Obsolescent 6+-argument convention: GNU keeps `tm_isdst = -1` and
        // ZONE defaults to nil (`src/timefns.c:1696`/`1723`).
        let (sec, subsec_ticks, hz) = decode_encode_time_second(&args[0])?;
        (
            sec,
            subsec_ticks,
            hz,
            require_fixnum_component(&args[1])?,
            require_fixnum_component(&args[2])?,
            require_fixnum_component(&args[3])?,
            require_fixnum_component(&args[4])?,
            require_fixnum_component(&args[5])?,
            if args.len() > 6 {
                args.last().copied().unwrap_or(Value::NIL)
            } else {
                Value::NIL
            },
            TmIsDst::Auto,
        )
    };

    let calendar = CalendarTime::try_from([sec, min, hour, day, month, year])
        .map_err(|CalendarFieldOverflow::OutOfRange| time_error_overflow())?;
    let total_secs = encode_time_to_epoch(calendar, &zone, isdst)?;
    if hz == 1 {
        // Integer-second SECOND field: keep GNU's (HIGH LOW) result form
        // (`current-time-list' defaults to t).
        let high = total_secs >> 16;
        let low = total_secs & 0xFFFF;
        return Ok(Value::list(vec![Value::fixnum(high), Value::fixnum(low)]));
    }
    // Sub-second resolution: GNU returns the (TICKS . HZ) form
    // (time_form_to_lisp / CFORM_TICKS_HZ) carrying the original HZ.
    let ticks = Integer::from(total_secs) * &hz + subsec_ticks;
    Ok(Value::cons(
        Value::make_integer(ticks),
        Value::make_integer(hz),
    ))
}
/// `(decode-time &optional TIME ZONE FORM)`
/// -> `(SECONDS MINUTES HOURS DAY MONTH YEAR DOW DST UTCOFF)`
#[doc(hidden)]
pub fn builtin_decode_time(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("decode-time", &args, 0, 3)?;
    // GNU `Fdecode_time` (`src/timefns.c:1542`): with FORM=t it decodes the
    // exact `(TICKS . HZ)` so the broken-down SEC member can carry sub-second
    // precision; otherwise HZ is 1 and SEC is a plain integer.
    let form_is_t = matches!(args.get(2).map(|v| v.kind()), Some(ValueKind::T));
    let time_arg = if args.is_empty() { Value::NIL } else { args[0] };
    let (broken_down_secs, sec_value): (i64, Option<(Integer, Integer)>) = if form_is_t {
        // CFORM_TICKS_HZ: keep the exact ticks/hz; `time_spec = tv_sec` is the
        // floor of ticks/hz (GNU `ticks_hz_to_timespec`).
        let th = decode_lisp_time(&time_arg)?.th;
        let secs = i64::try_from(&ticks_hz_seconds(&th)).map_err(|_| time_error_overflow())?;
        (secs, Some((th.ticks, th.hz)))
    } else {
        // GNU `lisp_seconds_argument`: the floor of the exact (TICKS . HZ), so TICKS may be a
        // bignum as long as the seconds fit.
        let th = decode_lisp_time(&time_arg)?.th;
        let secs = i64::try_from(&ticks_hz_seconds(&th)).map_err(|_| time_error_overflow())?;
        (secs, None)
    };

    let rule = effective_zone_rule(args.get(1))?;
    let decoded = decode_time_for_zone(&rule, broken_down_secs)?;
    let dt = decoded.time;

    // Compute the SEC member. For FORM=t with a sub-second clock (HZ != 1) GNU
    // returns `(HZ * tm_sec + mod(ticks, HZ)) . HZ` (`src/timefns.c:1596`);
    // otherwise SEC is the integer `tm_sec`.
    let sec = match sec_value {
        Some((ticks, hz)) if hz != 1 => {
            // GNU `mpz_fdiv_r`: the floor remainder is always non-negative.
            let (_, rem) = integer_fdiv_qr(&ticks, &hz);
            let sec_ticks = &hz * Integer::from(dt.sec) + rem;
            Value::cons(Value::make_integer(sec_ticks), Value::make_integer(hz))
        }
        _ => Value::fixnum(dt.sec),
    };
    Ok(Value::list(vec![
        sec,
        Value::fixnum(dt.min),
        Value::fixnum(dt.hour),
        Value::fixnum(dt.day),
        Value::fixnum(dt.month),
        Value::fixnum(dt.year),
        Value::fixnum(dt.dow),
        decoded.dst,
        Value::fixnum(decoded.utcoff),
    ]))
}
/// `(time-convert TIME &optional FORM)`
///
/// FORM controls the output format:
///   - nil             -> `current-time-list` dependent default
///   - `list`          -> `(HIGH LOW USEC PSEC)`
///   - `integer`       -> integer seconds
///   - `t`             -> `(TICKS . HZ)` (highest precision cons cell)
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_time_convert(args: Vec<Value>) -> EvalResult {
    builtin_time_convert_with_default_output(args, LispTimeOutput::LegacyList)
}
#[doc(hidden)]
pub fn builtin_time_convert_in_context(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let default_output = lisp_time_output_from_context(eval)?;
    builtin_time_convert_with_default_output(args, default_output)
}
fn builtin_time_convert_with_default_output(
    args: Vec<Value>,
    default_output: LispTimeOutput,
) -> EvalResult {
    expect_min_max_args("time-convert", &args, 1, 2)?;
    // Decode exactly so bignum TICKS (e.g. the high-resolution timestamps timer
    // arithmetic produces from a float delay) convert losslessly. Mirrors GNU
    // `Ftime_convert` (`src/timefns.c:1780`), which works entirely on the exact
    // `(TICKS . HZ)` pair.
    let t = decode_lisp_time(&args[0])?.th;

    let form = if args.len() > 1 {
        &args[1]
    } else {
        &Value::NIL
    };

    match parse_time_convert_form(form, default_output)? {
        TimeConvertForm::List => Ok(ticks_hz_list4(&t.ticks, &t.hz)),
        TimeConvertForm::Integer => {
            // GNU returns the input unchanged if it was already an integer.
            if matches!(
                args[0].kind(),
                ValueKind::Fixnum(_) | ValueKind::Veclike(VecLikeType::Bignum)
            ) {
                Ok(args[0])
            } else {
                Ok(Value::make_integer(ticks_hz_seconds(&t)))
            }
        }
        TimeConvertForm::InputHz => {
            // FORM t: result HZ is the input's own HZ. Return the input cons
            // unchanged when it already is (TICKS . HZ) at that frequency.
            if args[0].is_cons() && !args[0].cons_cdr().is_cons() {
                return Ok(args[0]);
            }
            Ok(Value::cons(
                Value::make_integer(t.ticks.clone()),
                Value::make_integer(t.hz.clone()),
            ))
        }
        TimeConvertForm::ExplicitHz(hz) => {
            // Fast path: input is (TICKS . HZ) with the requested HZ.
            if args[0].is_cons()
                && !args[0].cons_cdr().is_cons()
                && value_to_integer(&args[0].cons_cdr()) == Some(hz.clone())
            {
                return Ok(args[0]);
            }
            Ok(Value::cons(
                Value::make_integer(ticks_hz_hz_ticks(&t, &hz)?),
                Value::make_integer(hz),
            ))
        }
    }
}
/// `(set-time-zone-rule ZONE)` -> nil.
#[doc(hidden)]
pub fn builtin_set_time_zone_rule(args: Vec<Value>) -> EvalResult {
    expect_args("set-time-zone-rule", &args, 1)?;
    let rule = parse_zone_rule(&args[0])?;
    TIME_ZONE_RULE.with(|slot| *slot.borrow_mut() = rule);
    Ok(Value::NIL)
}
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
