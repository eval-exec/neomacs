//! Time and date builtins for the Elisp interpreter.
//!
//! Implements `current-time`, `float-time`, `time-add`, `time-subtract`,
//! `time-less-p`, `time-equal-p`, `current-time-string`, `current-time-zone`,
//! `encode-time`, `decode-time`, `time-convert`, and `set-time-zone-rule`.
//!
//! Uses the compile-target wall clock for current-time operations.

use super::error::{EvalResult, Flow, signal};
use super::eval::Context;
use super::intern::{intern, resolve_sym};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_args;
use crate::emacs_core::value::{ValueKind, VecLikeType};
use malachite::base::num::conversion::traits::RoundingFrom;
use malachite::base::rounding_modes::RoundingMode;
use malachite::integer::Integer;
use std::cell::RefCell;
use std::ffi::c_int;
use std::ffi::{CStr, OsString};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
enum TimeConvertSymbolForm {
    #[strum(serialize = "integer")]
    Integer,
    #[strum(serialize = "list")]
    List,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TimeConvertForm {
    Integer,
    List,
    InputHz,
    ExplicitHz(Integer),
}

/// Output representation selected by the dynamically bound
/// `current-time-list`.
///
/// Keep this as a semantic enum rather than passing a boolean between modules:
/// callers cannot accidentally invert what `true` means, and all producers of
/// Lisp timestamps share the same GNU-compatible constructor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LispTimeOutput {
    LegacyList,
    TicksHz,
}

impl LispTimeOutput {
    pub(crate) fn from_context(eval: &Context) -> Result<Self, Flow> {
        eval.eval_symbol_by_id(intern("current-time-list"))
            .map(|value| {
                if value.is_truthy() {
                    Self::LegacyList
                } else {
                    Self::TicksHz
                }
            })
    }

    fn encode(self, time: TimeMicros) -> Value {
        match self {
            Self::LegacyList => time.to_list(),
            Self::TicksHz => time.to_ticks_hz(1_000_000_000),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
enum TimeZoneSymbol {
    #[strum(serialize = "wall")]
    Wall,
}

impl TimeZoneSymbol {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
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
    /// Picoseconds within the current microsecond, always in [0, 999_999].
    psecs: i64,
}

impl TimeMicros {
    fn from_timespec(secs: i64, nanos: i64) -> Self {
        let secs = secs + nanos.div_euclid(1_000_000_000);
        let nanos = nanos.rem_euclid(1_000_000_000);
        Self {
            secs,
            usecs: nanos / 1_000,
            psecs: (nanos % 1_000) * 1_000,
        }
    }

    fn now() -> Self {
        // GNU `current_timespec` has nanosecond resolution; `Ftime_convert`
        // projects it as USEC = ns / 1000 and PSEC = (ns % 1000) * 1000
        // (`src/timefns.c` timespec_to_lisp / decode_lisp_time). Keeping the
        // nanosecond remainder matters observably: timer vectors built by
        // `run-at-time` carry a nonzero PSEC field in GNU.
        match neomacs_host_runtime::time::wall_time_since_unix_epoch() {
            Ok(dur) => {
                let nanos = dur.subsec_nanos() as i64;
                TimeMicros {
                    secs: dur.as_secs() as i64,
                    usecs: nanos / 1_000,
                    psecs: (nanos % 1_000) * 1_000,
                }
            }
            Err(e) => {
                let dur = e.duration();
                let nanos = dur.subsec_nanos() as i64;
                TimeMicros {
                    secs: -(dur.as_secs() as i64),
                    usecs: -(nanos / 1_000),
                    psecs: -((nanos % 1_000) * 1_000),
                }
            }
        }
    }

    fn to_list(self) -> Value {
        let high = self.secs >> 16;
        let low = self.secs & 0xFFFF;
        Value::list(vec![
            Value::fixnum(high),
            Value::fixnum(low),
            Value::fixnum(self.usecs),
            Value::fixnum(self.psecs),
        ])
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn to_float(self) -> f64 {
        self.secs as f64 + self.usecs as f64 / 1_000_000.0
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn add(self, other: TimeMicros) -> TimeMicros {
        let mut psecs = self.psecs + other.psecs;
        let mut usecs = self.usecs + other.usecs;
        let mut secs = self.secs + other.secs;
        if psecs >= 1_000_000 {
            psecs -= 1_000_000;
            usecs += 1;
        } else if psecs < 0 {
            psecs += 1_000_000;
            usecs -= 1;
        }
        if usecs >= 1_000_000 {
            usecs -= 1_000_000;
            secs += 1;
        } else if usecs < 0 {
            usecs += 1_000_000;
            secs -= 1;
        }
        TimeMicros { secs, usecs, psecs }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn sub(self, other: TimeMicros) -> TimeMicros {
        let mut psecs = self.psecs - other.psecs;
        let mut usecs = self.usecs - other.usecs;
        let mut secs = self.secs - other.secs;
        if psecs < 0 {
            psecs += 1_000_000;
            usecs -= 1;
        } else if psecs >= 1_000_000 {
            psecs -= 1_000_000;
            usecs += 1;
        }
        if usecs < 0 {
            usecs += 1_000_000;
            secs -= 1;
        } else if usecs >= 1_000_000 {
            usecs -= 1_000_000;
            secs += 1;
        }
        TimeMicros { secs, usecs, psecs }
    }

    fn from_ticks_hz(ticks: i64, hz: i64) -> Result<TimeMicros, Flow> {
        if hz <= 0 {
            return Err(signal(
                "error",
                vec![Value::string("Invalid time specification")],
            ));
        }

        let secs = ticks.div_euclid(hz);
        let rem = ticks.rem_euclid(hz) as i128;
        let hz = hz as i128;
        let micros_total = rem * 1_000_000;
        let usecs = (micros_total / hz) as i64;
        let psecs = (((micros_total % hz) * 1_000_000) / hz) as i64;
        Ok(TimeMicros { secs, usecs, psecs })
    }

    fn from_exact_ticks_hz(ticks: &Integer, hz: &Integer) -> Result<TimeMicros, Flow> {
        if hz <= &Integer::from(0) {
            return Err(signal(
                "error",
                vec![Value::string("Invalid time specification")],
            ));
        }

        let trillion = Integer::from(1_000_000_000_000i64);
        let million = Integer::from(1_000_000i64);
        let total_psecs = integer_div_floor(&(ticks * &trillion), hz);
        let secs = integer_div_floor(&total_psecs, &trillion);
        let rem_psecs = &total_psecs - &secs * &trillion;
        let usecs = &rem_psecs / &million;
        let psecs = &rem_psecs - &usecs * &million;

        let secs = i64::try_from(&secs)
            .map_err(|_| signal("error", vec![Value::string("Time value out of range")]))?;
        let usecs = i64::try_from(&usecs)
            .map_err(|_| signal("error", vec![Value::string("Time value out of range")]))?;
        let psecs = i64::try_from(&psecs)
            .map_err(|_| signal("error", vec![Value::string("Time value out of range")]))?;

        Ok(TimeMicros { secs, usecs, psecs })
    }

    fn to_ticks_hz(self, hz: i64) -> Value {
        self.to_ticks_hz_integer(&Integer::from(hz))
    }

    fn to_ticks_hz_integer(self, hz: &Integer) -> Value {
        let trillion = Integer::from(1_000_000_000_000i64);
        let total_psecs = Integer::from(self.secs) * &trillion
            + Integer::from(self.usecs) * Integer::from(1_000_000i64)
            + Integer::from(self.psecs);
        let ticks = integer_div_floor(&(total_psecs * hz), &trillion);
        Value::cons(Value::make_integer(ticks), Value::make_integer(hz.clone()))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn less_than(self, other: TimeMicros) -> bool {
        if self.secs != other.secs {
            self.secs < other.secs
        } else if self.usecs != other.usecs {
            self.usecs < other.usecs
        } else {
            self.psecs < other.psecs
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn equal(self, other: TimeMicros) -> bool {
        self.secs == other.secs && self.usecs == other.usecs && self.psecs == other.psecs
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TimeInputForm {
    Scalar,
    List,
    TicksHz,
}

// ---------------------------------------------------------------------------
// Exact rational time core, mirroring GNU `src/timefns.c`.
//
// GNU represents every decoded timestamp as an exact rational `(TICKS . HZ)`
// (a `struct ticks_hz`, `src/timefns.c:516`) and performs all arithmetic,
// comparison, and float conversion on that exact pair so that arbitrary
// frequencies (HZ) — including the power-of-two HZ produced by decoding a
// float, or a caller-supplied `(TICKS . HZ)` whose HZ is neither 1, 10**6,
// nor 10**12 — round-trip without loss. The previous Neomacs code reduced
// every value to microsecond/picosecond `TimeMicros` first, which silently
// discarded precision (e.g. `(time-add 0.5 '(1 . 3))` could not be 5/6) and
// could not read back bignum TICKS at all. This module reproduces the exact
// path. `TimeMicros` survives only as a derived display view (used by
// `decode-time` / `current-time-string` / `%N`), computed from the exact
// pair.
// ---------------------------------------------------------------------------

/// An exact Lisp timestamp `(TICKS . HZ)`, where `ticks / hz` is the number of
/// seconds since the epoch and `hz > 0`. Mirrors GNU `struct ticks_hz`.
#[derive(Clone, Debug)]
struct TicksHz {
    ticks: Integer,
    hz: Integer,
}

/// A decoded Lisp time: the exact `(TICKS . HZ)` value plus the syntactic form
/// of the input, which GNU's `time_arith` consults when choosing the result
/// representation (`src/timefns.c:1211`). Named distinctly from the calendar
/// `DecodedTime` (broken-down sec/min/hour/...) used by `decode-time`.
#[derive(Clone, Debug)]
struct DecodedLispTime {
    th: TicksHz,
    form: TimeInputForm,
}

fn trillion_int() -> Integer {
    Integer::from(1_000_000_000_000i64)
}

fn million_int() -> Integer {
    Integer::from(1_000_000i64)
}

/// Extract a malachite `Integer` from any Lisp integer (fixnum or bignum).
/// Returns `None` for non-integers, mirroring GNU `INTEGERP`.
fn value_to_integer(val: &Value) -> Option<Integer> {
    if let Some(n) = val.as_fixnum() {
        Some(Integer::from(n))
    } else {
        val.as_bignum().cloned()
    }
}

/// GNU `time_spec_invalid` (`src/timefns.c`): an ill-formed time value signals
/// `(error "Invalid time specification")`, NOT a `wrong-type-argument`.
fn time_spec_invalid() -> Flow {
    signal("error", vec![Value::string("Invalid time specification")])
}

fn time_error_overflow() -> Flow {
    signal(
        "error",
        vec![Value::string("Specified time is not representable")],
    )
}

#[derive(Clone, Debug)]
struct ParsedTime {
    time: TimeMicros,
    hz: i64,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    form: TimeInputForm,
    exact_ticks_hz: Option<(Integer, Integer)>,
}

/// Parse a time value from a Lisp argument.
///
/// Accepts:
///   - nil            -> current time
///   - integer        -> seconds since epoch
///   - float          -> seconds since epoch (with fractional part)
///   - (TICKS . HZ)   -> modern GNU timestamp cons
///   - (HIGH LOW)     -> high*65536 + low seconds, 0 usecs
///   - (HIGH LOW USEC)       -> with microseconds
///   - (HIGH LOW USEC PSEC)  -> with picoseconds
fn parse_time(val: &Value) -> Result<TimeMicros, Flow> {
    Ok(parse_time_detailed(val)?.time)
}

/// Decode a Lisp time value into whole `(seconds, nanoseconds)`, using the same
/// full parser as the other time functions so every accepted form ((TICKS .
/// HZ), (HIGH LOW USEC PSEC), integer, float, nil) is handled identically.
/// `nanoseconds` is the subsecond fraction in `[0, 999_999_999]`, matching the
/// `int ns = t.tv_nsec` that GNU's `format_time_string` passes to nstrftime
/// (`src/timefns.c:1391`). This is what the `%N` directive consumes.
pub(crate) fn time_value_seconds_and_nanos(val: &Value) -> Result<(i64, i64), Flow> {
    // Decode exactly, then floor-divide into whole seconds + nanoseconds the
    // same way GNU `ticks_hz_to_timespec` does (`src/timefns.c:529`): the
    // nanosecond count is floor((ticks * 10**9) / hz) reduced mod 10**9. Doing
    // this on the exact pair (rather than a microsecond-rounded intermediate)
    // keeps `%N`/`%3N`/`%6N` byte-exact for arbitrary HZ.
    let th = decode_lisp_time(val)?.th;
    let timespec_hz = Integer::from(1_000_000_000i64);
    let total_nanos = integer_div_floor(&(&th.ticks * &timespec_hz), &th.hz);
    let (secs, nanos) = integer_fdiv_qr(&total_nanos, &timespec_hz);
    let secs = i64::try_from(&secs).map_err(|_| time_error_overflow())?;
    let nanos = i64::try_from(&nanos).unwrap_or(0);
    Ok((secs, nanos))
}

fn parse_time_detailed(val: &Value) -> Result<ParsedTime, Flow> {
    // Bignum seconds-since-epoch values get truncated to i64;
    // Emacs's GNU encoding of large times uses (HIGH LOW) cons
    // pairs anyway, so a bignum here usually only occurs for
    // tests that compute (1+ most-positive-fixnum) etc.
    if let ValueKind::Veclike(VecLikeType::Bignum) = val.kind() {
        let f = f64::rounding_from(val.as_bignum().unwrap(), RoundingMode::Nearest).0;
        return Ok(ParsedTime {
            time: TimeMicros {
                secs: f as i64,
                usecs: 0,
                psecs: 0,
            },
            hz: 1,
            form: TimeInputForm::Scalar,
            exact_ticks_hz: None,
        });
    }
    match val.kind() {
        ValueKind::Nil => Ok(ParsedTime {
            time: TimeMicros::now(),
            hz: 1_000_000_000_000,
            form: TimeInputForm::List,
            exact_ticks_hz: None,
        }),
        ValueKind::Fixnum(n) => Ok(ParsedTime {
            time: TimeMicros {
                secs: n,
                usecs: 0,
                psecs: 0,
            },
            hz: 1,
            form: TimeInputForm::Scalar,
            exact_ticks_hz: Some((Integer::from(n), Integer::from(1))),
        }),
        ValueKind::Float => {
            let f = val.xfloat();
            if !f.is_finite() {
                // GNU `decode_lisp_time` (`src/timefns.c:1037`) routes a non-finite
                // float through `time_error (isnan ? EDOM : EOVERFLOW)`: NaN ->
                // "Invalid time specification", ±inf -> "Specified time is not
                // representable".
                return Err(if f.is_nan() {
                    time_spec_invalid()
                } else {
                    time_error_overflow()
                });
            }
            let (ticks, hz) = float_to_exact_ticks_hz(f)?;
            let hz_i64 = i64::try_from(&hz)
                .map_err(|_| signal("error", vec![Value::string("Time value out of range")]))?;
            Ok(ParsedTime {
                time: TimeMicros::from_exact_ticks_hz(&ticks, &hz)?,
                hz: hz_i64,
                form: TimeInputForm::List,
                exact_ticks_hz: Some((ticks, hz)),
            })
        }
        ValueKind::Cons => {
            let high = val.cons_car();
            let low_or_tail = val.cons_cdr();
            if !low_or_tail.is_cons() {
                let ticks = high.as_int().ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("integerp"), high],
                    )
                })?;
                let hz = low_or_tail.as_int().ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("integerp"), low_or_tail],
                    )
                })?;
                return Ok(ParsedTime {
                    time: TimeMicros::from_ticks_hz(ticks, hz)?,
                    hz,
                    form: TimeInputForm::TicksHz,
                    exact_ticks_hz: Some((Integer::from(ticks), Integer::from(hz))),
                });
            }

            let items = list_to_vec(val).ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), *val],
                )
            })?;
            if items.len() < 2 {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), *val],
                ));
            }
            let high = items[0].as_int().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), items[0]],
                )
            })?;
            let low = items[1].as_int().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), items[1]],
                )
            })?;
            let usec = if items.len() > 2 {
                items[2].as_int().unwrap_or(0)
            } else {
                0
            };
            let psec = if items.len() > 3 {
                items[3].as_int().unwrap_or(0)
            } else {
                0
            };
            let secs = high * 65536 + low;
            Ok(ParsedTime {
                time: TimeMicros {
                    secs,
                    usecs: usec,
                    psecs: psec,
                },
                hz: time_convert_default_hz(val),
                form: TimeInputForm::List,
                exact_ticks_hz: None,
            })
        }
        // GNU `decode_lisp_time` (`src/timefns.c:1041`) signals
        // `time_spec_invalid` ("Invalid time specification") for any TIME value
        // that is not nil/cons/integer/float (e.g. a string), NOT a
        // `wrong-type-argument numberp`.
        _ => Err(time_spec_invalid()),
    }
}

fn float_to_exact_ticks_hz(f: f64) -> Result<(Integer, Integer), Flow> {
    if f == 0.0 {
        return Ok((Integer::from(0), Integer::from(1)));
    }

    let bits = f.to_bits();
    let sign = if bits >> 63 == 0 { 1i128 } else { -1i128 };
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (mantissa, unbiased_exponent) = if exponent_bits == 0 {
        (Integer::from(fraction), -1022)
    } else {
        (Integer::from((1u64 << 52) | fraction), exponent_bits - 1023)
    };
    let scale = (52 - unbiased_exponent).max(0);

    let mut ticks = if unbiased_exponent >= 52 {
        let shift = (unbiased_exponent - 52) as u32;
        mantissa << shift
    } else {
        mantissa
    };
    if sign < 0 {
        ticks = -ticks;
    }

    let hz = Integer::from(1) << (scale as u32);
    Ok((ticks, hz))
}

fn integer_div_floor(n: &Integer, d: &Integer) -> Integer {
    let q = n / d;
    let r = n - &q * d;
    if r != 0 && n < &Integer::from(0) {
        q - Integer::from(1)
    } else {
        q
    }
}

/// Floor-divide `n` by `d` (d > 0), also returning the non-negative remainder.
fn integer_fdiv_qr(n: &Integer, d: &Integer) -> (Integer, Integer) {
    let q = integer_div_floor(n, d);
    let r = n - &q * d;
    (q, r)
}

fn integer_gcd(a: &Integer, b: &Integer) -> Integer {
    use malachite::base::num::arithmetic::traits::Gcd;
    // GCD is defined on the magnitudes; `unsigned_abs_ref` borrows the
    // underlying `Natural` without copying.
    Integer::from(a.unsigned_abs_ref().gcd(b.unsigned_abs_ref()))
}

// ---------------------------------------------------------------------------
// Exact (TICKS . HZ) decoding — GNU `decode_lisp_time` and friends.
// ---------------------------------------------------------------------------

/// GNU `decode_float_time` (`src/timefns.c:612`): convert a finite double into
/// the exact `(TICKS . HZ)` pair whose value equals it, with HZ the float's
/// frequency (a power of two) or 1, whichever is greater. Reuses the existing
/// bit-exact `float_to_exact_ticks_hz` decomposition.
fn decode_float_time(t: f64) -> Result<TicksHz, Flow> {
    let (ticks, hz) = float_to_exact_ticks_hz(t)?;
    Ok(TicksHz { ticks, hz })
}

/// GNU `decode_time_components` (`src/timefns.c:855`): combine the HIGH, LOW,
/// USEC and PSEC components at resolution HZ (1, 10**6 or 10**12) into an exact
/// `(TICKS . HZ)`. USEC/PSEC out-of-range values carry into higher-order
/// components exactly as GNU does.
fn decode_time_components(
    high: &Integer,
    low: &Integer,
    usec: &Integer,
    psec: &Integer,
    hz: &Integer,
) -> TicksHz {
    let million = million_int();
    let trillion = trillion_int();

    // us += ps / 1000000 (floor); ps, us reduced to [0, 1000000).
    let (ps_carry, ps_norm) = integer_fdiv_qr(psec, &million);
    let us_adj = usec + ps_carry;
    let (s_from_us_ps, us_norm) = integer_fdiv_qr(&us_adj, &million);

    // seconds = high * 2**16 + low + s_from_us_ps.
    let lo_time_bits = Integer::from(1i64 << 16);
    let s = high * &lo_time_bits + low + &s_from_us_ps;

    let ticks = if *hz == trillion {
        &s * &trillion + &us_norm * &million + &ps_norm
    } else if *hz == million {
        &s * &million + &us_norm
    } else {
        // hz == 1 (the (A B) / (A B . C-as-list) forms): drop sub-second.
        s
    };

    TicksHz {
        ticks,
        hz: hz.clone(),
    }
}

/// GNU `decode_lisp_time` (`src/timefns.c:959`), returning the exact
/// `(TICKS . HZ)` and the syntactic form. Accepts the same four canonical and
/// three compatibility forms GNU does:
///   nil                -> current time
///   integer            -> SEC . 1
///   float              -> exact power-of-two pair
///   (TICKS . HZ)       -> as-is (TICKS/HZ may be bignums)
///   (HIGH LOW)         -> hz 1
///   (HIGH LOW . USEC)  -> hz 10**6
///   (HIGH LOW USEC)    -> hz 10**6
///   (HIGH LOW USEC PSEC) -> hz 10**12
fn decode_lisp_time(val: &Value) -> Result<DecodedLispTime, Flow> {
    match val.kind() {
        ValueKind::Nil => {
            let now = TimeMicros::now();
            // current_time uses a 10**9 (nanosecond) clock; mirror GNU's
            // timespec-derived (TICKS . timespec_hz) for the current time.
            // `psecs` holds the sub-microsecond part as (ns % 1000) * 1000, so
            // its nanosecond contribution is psecs / 1000 — dropping it made
            // `(time-add nil N)` (and thus timer.el's timer vectors) lose the
            // PSEC field GNU carries.
            let hz = Integer::from(1_000_000_000i64);
            let ticks = Integer::from(now.secs) * &hz
                + Integer::from(now.usecs) * Integer::from(1000i64)
                + Integer::from(now.psecs / 1000);
            Ok(DecodedLispTime {
                th: TicksHz { ticks, hz },
                form: TimeInputForm::List,
            })
        }
        ValueKind::Fixnum(n) => Ok(DecodedLispTime {
            th: TicksHz {
                ticks: Integer::from(n),
                hz: Integer::from(1),
            },
            form: TimeInputForm::Scalar,
        }),
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(DecodedLispTime {
            th: TicksHz {
                ticks: val.as_bignum().expect("bignum payload").clone(),
                hz: Integer::from(1),
            },
            form: TimeInputForm::Scalar,
        }),
        ValueKind::Float => {
            let f = val.xfloat();
            if !f.is_finite() {
                // GNU `decode_lisp_time` (`src/timefns.c:1037`):
                // `time_error (isnan (d) ? EDOM : EOVERFLOW)`. EDOM (NaN) maps to
                // `time_spec_invalid` ("Invalid time specification"); EOVERFLOW
                // (±inf) maps to `time_overflow` ("Specified time is not
                // representable").
                return Err(if f.is_nan() {
                    time_spec_invalid()
                } else {
                    time_error_overflow()
                });
            }
            Ok(DecodedLispTime {
                th: decode_float_time(f)?,
                form: TimeInputForm::List,
            })
        }
        ValueKind::Cons => {
            let high = val.cons_car();
            let low = val.cons_cdr();
            if low.is_cons() {
                // (HIGH LOW ...) old-format list.
                let mut usec = Integer::from(0);
                let mut psec = Integer::from(0);
                let mut hz = Integer::from(1);
                let low_car = low.cons_car();
                let low_tail = low.cons_cdr();
                if low_tail.is_cons() {
                    usec = value_to_integer(&low_tail.cons_car()).ok_or_else(time_spec_invalid)?;
                    let tail2 = low_tail.cons_cdr();
                    if tail2.is_cons() {
                        psec = value_to_integer(&tail2.cons_car()).ok_or_else(time_spec_invalid)?;
                        hz = trillion_int();
                    } else {
                        hz = million_int();
                    }
                } else if !low_tail.is_nil() {
                    // (HIGH LOW . USEC) dotted form.
                    usec = value_to_integer(&low_tail).ok_or_else(time_spec_invalid)?;
                    hz = million_int();
                }
                let high_i = value_to_integer(&high).ok_or_else(time_spec_invalid)?;
                let low_i = value_to_integer(&low_car).ok_or_else(time_spec_invalid)?;
                Ok(DecodedLispTime {
                    th: decode_time_components(&high_i, &low_i, &usec, &psec, &hz),
                    form: TimeInputForm::List,
                })
            } else {
                // (TICKS . HZ): TICKS integer, HZ positive integer.
                let ticks = value_to_integer(&high).ok_or_else(time_spec_invalid)?;
                let hz = value_to_integer(&low).ok_or_else(time_spec_invalid)?;
                if hz <= 0 {
                    return Err(time_spec_invalid());
                }
                Ok(DecodedLispTime {
                    th: TicksHz { ticks, hz },
                    form: TimeInputForm::TicksHz,
                })
            }
        }
        _ => Err(time_spec_invalid()),
    }
}

/// GNU `frac_to_double` (`src/timefns.c:408`): convert the exact rational
/// `numerator / denominator` to the nearest double (round to even). malachite's
/// `Rational` -> `f64` rounding-from gives the correctly-rounded result.
fn frac_to_double(numerator: &Integer, denominator: &Integer) -> f64 {
    use malachite::base::num::conversion::traits::RoundingFrom;
    use malachite::rational::Rational;
    let q = Rational::from_integers(numerator.clone(), denominator.clone());
    f64::rounding_from(&q, RoundingMode::Nearest).0
}

/// GNU `ticks_hz_list4` (`src/timefns.c:664`): render an exact `(TICKS . HZ)`
/// as the backward-compatible `(HI LO US PS)` list, dropping any excess
/// precision below 10**-12 s (floor).
fn ticks_hz_list4(ticks: &Integer, hz: &Integer) -> Value {
    let trillion = trillion_int();
    let million = million_int();
    // floor((ticks * trillion) / hz).
    let scaled = integer_div_floor(&(ticks * &trillion), hz);
    // Split into seconds and the 12-digit sub-second remainder.
    let (secs, rem) = integer_fdiv_qr(&scaled, &trillion);
    let us = &rem / &million;
    let ps = &rem - &us * &million;
    // Split seconds into HI/LO at 16 bits (LO non-negative).
    let lo_time_bits = Integer::from(1i64 << 16);
    let (hi, lo) = integer_fdiv_qr(&secs, &lo_time_bits);
    Value::list(vec![
        Value::make_integer(hi),
        Value::make_integer(lo),
        Value::make_integer(us),
        Value::make_integer(ps),
    ])
}

/// True if the positive integer HZ divides evenly into a trillion
/// (GNU `trillion_factor`, `src/timefns.c:98`).
fn trillion_factor(hz: &Integer) -> bool {
    &trillion_int() % hz == 0
}

/// GNU `ticks_hz_seconds` (`src/timefns.c:800`): floor(ticks / hz) — the whole
/// seconds of an exact `(TICKS . HZ)` value.
fn ticks_hz_seconds(t: &TicksHz) -> Integer {
    integer_div_floor(&t.ticks, &t.hz)
}

/// GNU `ticks_hz_hz_ticks` (`src/timefns.c:747`): convert T to a count of
/// `hz_out` ticks, taking the floor — floor((t.ticks * hz_out) / t.hz). HZ_OUT
/// must be a positive integer.
fn ticks_hz_hz_ticks(t: &TicksHz, hz_out: &Integer) -> Result<Integer, Flow> {
    if hz_out <= &Integer::from(0) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid time frequency")],
        ));
    }
    if t.hz == *hz_out {
        return Ok(t.ticks.clone());
    }
    Ok(integer_div_floor(&(&t.ticks * hz_out), &t.hz))
}

/// GNU `time_arith` (`src/timefns.c:1127`): add (or subtract) two Lisp time
/// values exactly and choose the GNU result representation.
fn time_arith(a: &Value, b: &Value, subtract: bool) -> Result<Value, Flow> {
    let da = decode_lisp_time(a)?;
    let db = decode_lisp_time(b)?;

    let (ticks, hz) = if da.th.hz == db.th.hz {
        let ticks = if subtract {
            &da.th.ticks - &db.th.ticks
        } else {
            &da.th.ticks + &db.th.ticks
        };
        (ticks, da.th.hz.clone())
    } else {
        // Compute (na*(db/g) OP nb*(da/g)) / lcm(da,db), then normalize by the
        // gcd of numerator and denominator, rescaling up so the denominator is
        // never coarser than the finer of the two inputs (GNU hzmin rule).
        let g = integer_gcd(&da.th.hz, &db.th.hz);
        let fa = &da.th.hz / &g; // da/g
        let fb = &db.th.hz / &g; // db/g
        let mut ihz = &fa * &db.th.hz; // lcm(da, db)
        let mut iticks = &fb * &da.th.ticks;
        if subtract {
            iticks -= &fa * &db.th.ticks;
        } else {
            iticks += &fa * &db.th.ticks;
        }

        let ig = integer_gcd(&iticks, &ihz);
        if ig > 1 {
            iticks /= &ig;
            ihz /= &ig;
            let hzmin = if da.th.hz < db.th.hz {
                &da.th.hz
            } else {
                &db.th.hz
            };
            if ihz < *hzmin {
                // rescale = ceil(hzmin / ihz).
                let rescale = {
                    let (q, r) = integer_fdiv_qr(hzmin, &ihz);
                    if r == 0 { q } else { q + Integer::from(1) }
                };
                iticks *= &rescale;
                ihz *= &rescale;
            }
        }
        (iticks, ihz)
    };

    Ok(time_arith_to_lisp(ticks, hz, &da, &db))
}

/// Select the result form, mirroring the final `return` of GNU `time_arith`
/// (`src/timefns.c:1211`). `current-time-list` defaults to t in Neomacs, so an
/// integer HZ != 1 yields the list form unless an input used `(TICKS . HZ)` or
/// HZ does not divide a trillion.
fn time_arith_to_lisp(
    ticks: Integer,
    hz: Integer,
    da: &DecodedLispTime,
    db: &DecodedLispTime,
) -> Value {
    if hz == 1 {
        return Value::make_integer(ticks);
    }
    let a_is_ticks_hz = da.form == TimeInputForm::TicksHz;
    let b_is_ticks_hz = db.form == TimeInputForm::TicksHz;
    if a_is_ticks_hz || b_is_ticks_hz || !trillion_factor(&hz) {
        Value::cons(Value::make_integer(ticks), Value::make_integer(hz))
    } else {
        ticks_hz_list4(&ticks, &hz)
    }
}

/// GNU `time_cmp` (`src/timefns.c:1250`): compare two exact time values by
/// cross-multiplying ATICKS*BHZ vs BTICKS*AHZ.
fn time_cmp(a: &Value, b: &Value) -> Result<std::cmp::Ordering, Flow> {
    let da = decode_lisp_time(a)?;
    let db = decode_lisp_time(b)?;
    let lhs = &da.th.ticks * &db.th.hz;
    let rhs = &db.th.ticks * &da.th.hz;
    Ok(lhs.cmp(&rhs))
}

// ---------------------------------------------------------------------------
// Date/time breakdown helpers (UTC only, no chrono)
// ---------------------------------------------------------------------------

/// Offset between a calendar year and glibc's `struct tm` `tm_year` field
/// (GNU `TM_YEAR_BASE`): `tm_year = year - 1900`.
const TM_YEAR_BASE: i64 = 1900;

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
    if is_leap_year(year) { 366 } else { 365 }
}

/// Decoded time in UTC: (sec min hour day month year dow dst utcoff).
struct DecodedTime {
    sec: i64,
    min: i64,
    hour: i64,
    day: i64,   // 1-based
    month: i64, // 1-based
    year: i64,
    dow: i64, // 0=Sunday, 1=Monday, ..., 6=Saturday
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

/// Encode date/time components to epoch seconds (UTC).
fn encode_to_epoch_secs(sec: i64, min: i64, hour: i64, day: i64, month: i64, year: i64) -> i64 {
    // Normalize an out-of-range MONTH into 1..=12, rolling the YEAR, exactly
    // like GNU's `encode-time` (mktime/timegm semantics): month 0 -> December of
    // the previous year, month 13 -> January of the next year, month -1 ->
    // November of the previous year, and so on. DAY is handled by the
    // day-of-year arithmetic below (it works for any integer via `day - 1`), so
    // only MONTH needs an explicit rollover here; without it `for m in 1..month`
    // silently treats month 0 as January of the same year and mis-indexes
    // `days_in_month` for month > 12.
    let m0 = month - 1;
    let year = year + m0.div_euclid(12);
    let month = m0.rem_euclid(12) + 1;

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

#[derive(Clone, Debug)]
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

/// Reset timezone rule to default (called from Context::new).
pub(crate) fn reset_timefns_thread_locals() {
    TIME_ZONE_RULE.with(|slot| *slot.borrow_mut() = ZoneRule::Local);
}

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
    unsafe {
        tzset();
    }
}

#[cfg(not(unix))]
fn refresh_tz_env() {}

struct ScopedTzEnv {
    previous: Option<OsString>,
}

impl ScopedTzEnv {
    fn new(spec: Option<&str>) -> Self {
        let previous = std::env::var_os("TZ");
        match spec {
            Some(v) => unsafe { std::env::set_var("TZ", v) },
            None => unsafe { std::env::remove_var("TZ") },
        }
        refresh_tz_env();
        Self { previous }
    }
}

impl Drop for ScopedTzEnv {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => unsafe { std::env::set_var("TZ", v) },
            None => unsafe { std::env::remove_var("TZ") },
        }
        refresh_tz_env();
    }
}

fn with_tz_env<T>(spec: Option<&str>, f: impl FnOnce() -> T) -> T {
    let _lock = tz_env_lock().lock().expect("time zone env lock poisoned");
    let _guard = ScopedTzEnv::new(spec);
    f()
}

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
                .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
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
                    .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
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

#[cfg(unix)]
fn zone_rule_to_offset_name(rule: &ZoneRule, epoch_secs: i64) -> (i64, String) {
    match rule {
        ZoneRule::Local => local_offset_name_at_epoch(epoch_secs),
        ZoneRule::Utc => (0, "GMT".to_string()),
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

pub(crate) fn zone_offset_name_for_time(
    zone: Option<&Value>,
    epoch_secs: i64,
) -> Result<(i64, String), Flow> {
    let rule = effective_zone_rule(zone)?;
    Ok(zone_rule_to_offset_name(&rule, epoch_secs))
}

fn decode_time_for_zone(rule: &ZoneRule, epoch_secs: i64) -> Result<ZonedDecodedTime, Flow> {
    match rule {
        ZoneRule::Local => local_decoded_time_at_epoch(epoch_secs),
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

fn time_convert_default_hz(value: &Value) -> i64 {
    match value.kind() {
        ValueKind::Fixnum(_) => 1,
        ValueKind::Float => 1_000_000,
        ValueKind::Cons => {
            let tail = value.cons_cdr();
            if !tail.is_cons() {
                return tail.as_int().filter(|hz| *hz > 0).unwrap_or(1);
            }
            if let Some(items) = list_to_vec(value) {
                match items.len() {
                    0..=2 => 1,
                    3 => 1_000_000,
                    _ => 1_000_000_000_000,
                }
            } else {
                1
            }
        }
        _ => 1,
    }
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
fn decode_encode_time_second(value: &Value) -> Result<(i64, i64, i64), Flow> {
    if let Some(n) = value.as_fixnum() {
        return Ok((n, 0, 1));
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
    let parsed = parse_time_detailed(value)?;
    let hz = parsed.hz.max(1);
    let sec = parsed.time.secs;
    // Subsecond remainder expressed at HZ.
    let subsec_ticks = match hz {
        1 => 0,
        1_000_000 => parsed.time.usecs,
        1_000_000_000_000 => parsed.time.usecs * 1_000_000 + parsed.time.psecs,
        _ => {
            // (TICKS . HZ)/float: recover the remainder from the exact pair.
            if let Some((ticks, exact_hz)) = parsed.exact_ticks_hz.as_ref() {
                let hz_i = i64::try_from(exact_hz).unwrap_or(1).max(1);
                i64::try_from(&(ticks.clone() % Integer::from(hz_i))).unwrap_or(0)
            } else {
                0
            }
        }
    };
    // `sec' becomes `tm.tm_sec` via GNU `check_tm_member (sec, 0)`
    // (`src/timefns.c:1752`), which requires it fit in an `int` and otherwise
    // signals `time_overflow` ("Specified time is not representable").
    if i32::try_from(sec).is_err() {
        return Err(time_error_overflow());
    }
    Ok((sec, subsec_ticks.rem_euclid(hz), hz))
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
    fn to_c(self) -> c_int {
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
fn mktime_with_isdst(
    sec: i64,
    min: i64,
    hour: i64,
    day: i64,
    month: i64,
    year: i64,
    isdst: TmIsDst,
) -> Result<i64, Flow> {
    // `mktime` normalizes out-of-range fields itself, but `tm_year`/`tm_mon`
    // must fit in a C `int`; bail out the same way GNU `check_tm_member` does.
    let tm_year = year.checked_sub(1900).filter(|v| i32::try_from(*v).is_ok());
    let Some(tm_year) = tm_year else {
        return Err(time_error_overflow());
    };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_sec = sec as libc::c_int;
    tm.tm_min = min as libc::c_int;
    tm.tm_hour = hour as libc::c_int;
    tm.tm_mday = day as libc::c_int;
    tm.tm_mon = (month - 1) as libc::c_int;
    tm.tm_year = tm_year as libc::c_int;
    tm.tm_isdst = isdst.to_c();
    // GNU sets `tm.tm_wday = -1` and treats a still-negative `tm_wday` after
    // `mktime_z` as an error; libc `mktime` returns `(time_t)-1` on failure.
    tm.tm_wday = -1;
    let value = unsafe { libc::mktime(&mut tm as *mut _) };
    if value == -1 && tm.tm_wday < 0 {
        return Err(time_error_overflow());
    }
    Ok(value as i64)
}

#[cfg(not(unix))]
fn mktime_with_isdst(
    sec: i64,
    min: i64,
    hour: i64,
    day: i64,
    month: i64,
    year: i64,
    _isdst: TmIsDst,
) -> Result<i64, Flow> {
    Ok(encode_to_epoch_secs(sec, min, hour, day, month, year))
}

/// Encode a broken-down local time to epoch seconds honoring the zone's DST
/// rules and the forced `isdst` flag, mirroring GNU `Fencode_time` +
/// `mktime_z`. For zones without DST transitions (`Utc`/fixed offsets) the flag
/// is irrelevant and a plain offset subtraction suffices.
#[allow(clippy::too_many_arguments)] // broken-down time fields mirror encode-time's positional contract
fn encode_time_to_epoch(
    sec: i64,
    min: i64,
    hour: i64,
    day: i64,
    month: i64,
    year: i64,
    zone: &Value,
    isdst: TmIsDst,
) -> Result<i64, Flow> {
    let rule = effective_zone_rule(Some(zone))?;
    let local_secs = encode_to_epoch_secs(sec, min, hour, day, month, year);
    match rule {
        ZoneRule::Local => mktime_with_isdst(sec, min, hour, day, month, year, isdst),
        ZoneRule::TzString(spec) => with_tz_env(Some(&spec), || {
            mktime_with_isdst(sec, min, hour, day, month, year, isdst)
        }),
        ZoneRule::Utc => Ok(local_secs),
        ZoneRule::FixedOffset(offset) | ZoneRule::FixedNamedOffset(offset, _) => {
            Ok(local_secs - offset)
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

fn current_time_value(output: LispTimeOutput) -> Value {
    output.encode(TimeMicros::now())
}

/// Convert a system `(seconds, nanoseconds)` timestamp to the representation
/// selected by `current-time-list`, matching GNU `timefns.c:make_lisp_time`.
pub(crate) fn make_lisp_time(secs: i64, nanos: i64, output: LispTimeOutput) -> Value {
    output.encode(TimeMicros::from_timespec(secs, nanos))
}

/// `(current-time)` -> `(HIGH LOW USEC PSEC)` or `(TICKS . HZ)`.
pub(crate) fn builtin_current_time(args: Vec<Value>) -> EvalResult {
    expect_args("current-time", &args, 0)?;
    Ok(current_time_value(LispTimeOutput::LegacyList))
}

pub(crate) fn builtin_current_time_in_context(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("current-time", &args, 0)?;
    Ok(current_time_value(LispTimeOutput::from_context(eval)?))
}

/// `(float-time &optional TIME)` -> float seconds since epoch.
///
/// GNU `Ffloat_time` returns the float argument unchanged when given a float,
/// and otherwise `frac_to_double` of the exact `(TICKS . HZ)` decoding
/// (`src/timefns.c:1309`). Crucially the decoding is exact, so a bignum-TICKS
/// timestamp (such as the one `encode-time` produces from a float SECOND)
/// converts losslessly instead of overflowing an i64.
pub(crate) fn builtin_float_time(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("float-time", &args, 0, 1)?;
    let arg = if args.is_empty() { Value::NIL } else { args[0] };
    if matches!(arg.kind(), ValueKind::Float) {
        return Ok(arg);
    }
    let th = decode_lisp_time(&arg)?.th;
    Ok(Value::make_float(frac_to_double(&th.ticks, &th.hz)))
}

/// `(time-add A B)` -> integer seconds, `(HI LO US PS)`, or `(TICKS . HZ)`.
pub(crate) fn builtin_time_add(args: Vec<Value>) -> EvalResult {
    expect_args("time-add", &args, 2)?;
    time_arith(&args[0], &args[1], false)
}

/// `(time-subtract A B)` -> integer seconds, `(HI LO US PS)`, or `(TICKS . HZ)`.
pub(crate) fn builtin_time_subtract(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_time_less_p(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_time_equal_p(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_current_time_string(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_current_time_zone(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_encode_time(args: Vec<Value>) -> EvalResult {
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
            let isdst = if dstflag.is_symbol() && !zoneval.is_fixnum() && !zoneval.is_cons() {
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

    let total_secs = encode_time_to_epoch(sec, min, hour, day, month, year, &zone, isdst)?;
    if hz <= 1 {
        // Integer-second SECOND field: keep GNU's (HIGH LOW) result form
        // (`current-time-list' defaults to t).
        let high = total_secs >> 16;
        let low = total_secs & 0xFFFF;
        return Ok(Value::list(vec![Value::fixnum(high), Value::fixnum(low)]));
    }
    // Sub-second resolution: GNU returns the (TICKS . HZ) form
    // (time_form_to_lisp / CFORM_TICKS_HZ) carrying the original HZ.
    let ticks = Integer::from(total_secs) * Integer::from(hz) + Integer::from(subsec_ticks);
    Ok(Value::cons(
        Value::make_integer(ticks),
        Value::make_integer(Integer::from(hz)),
    ))
}

/// `(decode-time &optional TIME ZONE FORM)`
/// -> `(SECONDS MINUTES HOURS DAY MONTH YEAR DOW DST UTCOFF)`
pub(crate) fn builtin_decode_time(args: Vec<Value>) -> EvalResult {
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
        let tm = if time_arg.is_nil() {
            TimeMicros::now()
        } else {
            parse_time(&time_arg)?
        };
        (tm.secs, None)
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
pub(crate) fn builtin_time_convert(args: Vec<Value>) -> EvalResult {
    builtin_time_convert_with_default_output(args, LispTimeOutput::LegacyList)
}

pub(crate) fn builtin_time_convert_in_context(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let default_output = LispTimeOutput::from_context(eval)?;
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
pub(crate) fn builtin_set_time_zone_rule(args: Vec<Value>) -> EvalResult {
    expect_args("set-time-zone-rule", &args, 1)?;
    let rule = parse_zone_rule(&args[0])?;
    TIME_ZONE_RULE.with(|slot| *slot.borrow_mut() = rule);
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
