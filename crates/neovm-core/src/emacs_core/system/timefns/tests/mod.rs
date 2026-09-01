use super::*;
use crate::emacs_core::Context;
use crate::emacs_core::value::ValueKind;
use crate::heap_types::LispString;
use crate::test_utils::runtime_startup_eval_all;
use std::sync::{Mutex, OnceLock};

fn tz_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("tz test lock poisoned")
}

fn reset_tz_rule() {
    let _ = builtin_set_time_zone_rule(vec![Value::NIL]);
}

fn bootstrap_eval(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

/// Render a builtin's `Result<Value, Flow>` the way `(condition-case e EXPR
/// (error e))` followed by `prin1` would in Elisp: a success prints the value,
/// a signal prints the error object `(SYMBOL . DATA)`. This lets behavioral
/// tests assert against GNU Emacs `--batch` output byte-for-byte without
/// depending on the (pdump-gated) bootstrap evaluator.
fn condition_case_print(result: Result<Value, Flow>) -> String {
    match result {
        Ok(value) => crate::emacs_core::print::print_value(&value),
        Err(Flow::Signal(sig)) => {
            let symbol = Value::symbol(sig.symbol_name());
            let err_obj = if let Some(raw) = sig.raw_data {
                Value::cons(symbol, raw)
            } else {
                Value::cons(symbol, Value::list(sig.data.clone()))
            };
            crate::emacs_core::print::print_value(&err_obj)
        }
        Err(other) => panic!("expected signal or value, got {other:?}"),
    }
}

fn assert_invalid_time_frequency(flow: Flow) {
    match flow {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|value| value.as_utf8_str()),
                Some("Invalid time frequency")
            );
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

// -----------------------------------------------------------------------
// Internal helpers
// -----------------------------------------------------------------------

#[test]
fn time_micros_roundtrip_to_list() {
    crate::test_utils::init_test_tracing();
    let tm = TimeMicros {
        secs: 1_700_000_000,
        usecs: 123_456,
        psecs: 0,
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
    crate::test_utils::init_test_tracing();
    let tm = TimeMicros {
        secs: 1000,
        usecs: 500_000,
        psecs: 0,
    };
    let f = tm.to_float();
    assert!((f - 1000.5).abs() < 1e-6);
}

#[test]
fn time_micros_add() {
    crate::test_utils::init_test_tracing();
    let a = TimeMicros {
        secs: 10,
        usecs: 800_000,
        psecs: 0,
    };
    let b = TimeMicros {
        secs: 5,
        usecs: 400_000,
        psecs: 0,
    };
    let c = a.add(b);
    assert_eq!(c.secs, 16);
    assert_eq!(c.usecs, 200_000);
}

#[test]
fn time_micros_sub() {
    crate::test_utils::init_test_tracing();
    let a = TimeMicros {
        secs: 10,
        usecs: 200_000,
        psecs: 0,
    };
    let b = TimeMicros {
        secs: 5,
        usecs: 400_000,
        psecs: 0,
    };
    let c = a.sub(b);
    assert_eq!(c.secs, 4);
    assert_eq!(c.usecs, 800_000);
}

#[test]
fn time_micros_less_than() {
    crate::test_utils::init_test_tracing();
    let a = TimeMicros {
        secs: 10,
        usecs: 0,
        psecs: 0,
    };
    let b = TimeMicros {
        secs: 10,
        usecs: 1,
        psecs: 0,
    };
    assert!(a.less_than(b));
    assert!(!b.less_than(a));
    assert!(!a.less_than(a));
}

#[test]
fn time_micros_equal() {
    crate::test_utils::init_test_tracing();
    let a = TimeMicros {
        secs: 42,
        usecs: 123,
        psecs: 0,
    };
    let b = TimeMicros {
        secs: 42,
        usecs: 123,
        psecs: 0,
    };
    assert!(a.equal(b));
    let c = TimeMicros {
        secs: 42,
        usecs: 124,
        psecs: 0,
    };
    assert!(!a.equal(c));
}

// -----------------------------------------------------------------------
// parse_time
// -----------------------------------------------------------------------

#[test]
fn parse_time_nil() {
    crate::test_utils::init_test_tracing();
    let tm = parse_time(&Value::NIL).unwrap();
    // Just check it returns something reasonable (recent epoch).
    assert!(tm.secs > 1_000_000_000);
}

#[test]
fn parse_time_integer() {
    crate::test_utils::init_test_tracing();
    let tm = parse_time(&Value::fixnum(1_700_000_000)).unwrap();
    assert_eq!(tm.secs, 1_700_000_000);
    assert_eq!(tm.usecs, 0);
}

#[test]
fn parse_time_float() {
    crate::test_utils::init_test_tracing();
    let tm = parse_time(&Value::make_float(1000.5)).unwrap();
    assert_eq!(tm.secs, 1000);
    assert_eq!(tm.usecs, 500_000);
}

#[test]
fn parse_time_list_two() {
    crate::test_utils::init_test_tracing();
    // (HIGH LOW) format: 25939 * 65536 + 34304 = 1700000000
    let high = 1_700_000_000i64 >> 16;
    let low = 1_700_000_000i64 & 0xFFFF;
    let list = Value::list(vec![Value::fixnum(high), Value::fixnum(low)]);
    let tm = parse_time(&list).unwrap();
    assert_eq!(tm.secs, 1_700_000_000);
    assert_eq!(tm.usecs, 0);
}

#[test]
fn parse_time_list_four() {
    crate::test_utils::init_test_tracing();
    let high = 1_700_000_000i64 >> 16;
    let low = 1_700_000_000i64 & 0xFFFF;
    let list = Value::list(vec![
        Value::fixnum(high),
        Value::fixnum(low),
        Value::fixnum(42),
        Value::fixnum(0),
    ]);
    let tm = parse_time(&list).unwrap();
    assert_eq!(tm.secs, 1_700_000_000);
    assert_eq!(tm.usecs, 42);
}

#[test]
fn parse_time_bad_type() {
    crate::test_utils::init_test_tracing();
    // GNU `decode_lisp_time` signals `(error "Invalid time specification")` for a
    // non-number/non-cons TIME value (e.g. a string), NOT `wrong-type-argument
    // numberp`.
    let err = parse_time(&Value::string("not a time")).expect_err("string is not a time");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|value| value.as_utf8_str()),
                Some("Invalid time specification")
            );
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

// -----------------------------------------------------------------------
// Date computation helpers
// -----------------------------------------------------------------------

#[test]
fn leap_years() {
    crate::test_utils::init_test_tracing();
    assert!(is_leap_year(2000));
    assert!(!is_leap_year(1900));
    assert!(is_leap_year(2024));
    assert!(!is_leap_year(2023));
    assert!(is_leap_year(2400));
}

#[test]
fn decode_epoch_zero() {
    crate::test_utils::init_test_tracing();
    let dt = decode_epoch_secs(0).unwrap();
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
    crate::test_utils::init_test_tracing();
    // 2024-01-15 12:30:45 UTC -> epoch = 1705318245
    let epoch = encode_to_epoch_secs(45, 30, 12, 15, 1, 2024);
    let dt = decode_epoch_secs(epoch).unwrap();
    assert_eq!(dt.year, 2024);
    assert_eq!(dt.month, 1);
    assert_eq!(dt.day, 15);
    assert_eq!(dt.hour, 12);
    assert_eq!(dt.min, 30);
    assert_eq!(dt.sec, 45);
}

#[test]
fn encode_decode_roundtrip() {
    crate::test_utils::init_test_tracing();
    let epoch = encode_to_epoch_secs(30, 15, 10, 25, 6, 2023);
    let dt = decode_epoch_secs(epoch).unwrap();
    assert_eq!(dt.sec, 30);
    assert_eq!(dt.min, 15);
    assert_eq!(dt.hour, 10);
    assert_eq!(dt.day, 25);
    assert_eq!(dt.month, 6);
    assert_eq!(dt.year, 2023);
}

#[test]
fn encode_decode_roundtrip_leap_day() {
    crate::test_utils::init_test_tracing();
    let epoch = encode_to_epoch_secs(0, 0, 0, 29, 2, 2024);
    let dt = decode_epoch_secs(epoch).unwrap();
    assert_eq!(dt.day, 29);
    assert_eq!(dt.month, 2);
    assert_eq!(dt.year, 2024);
}

#[test]
fn decode_y2k() {
    crate::test_utils::init_test_tracing();
    // 2000-01-01 00:00:00 UTC = 946684800
    let dt = decode_epoch_secs(946_684_800).unwrap();
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
    crate::test_utils::init_test_tracing();
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
    crate::test_utils::init_test_tracing();
    let result = builtin_current_time(vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn builtin_float_time_no_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_float_time(vec![]).unwrap();
    match result.kind() {
        ValueKind::Float => {
            let f = result.as_float().unwrap();
            assert!(f > 1_000_000_000.0);
        }
        _ => panic!("expected float"),
    }
}

#[test]
fn builtin_float_time_from_list() {
    crate::test_utils::init_test_tracing();
    let high = 1_700_000_000i64 >> 16;
    let low = 1_700_000_000i64 & 0xFFFF;
    let list = Value::list(vec![
        Value::fixnum(high),
        Value::fixnum(low),
        Value::fixnum(500_000),
        Value::fixnum(0),
    ]);
    let result = builtin_float_time(vec![list]).unwrap();
    match result.kind() {
        ValueKind::Float => {
            let f = result.as_float().unwrap();
            assert!((f - 1_700_000_000.5).abs() < 1e-3);
        }
        _ => panic!("expected float"),
    }
}

#[test]
fn builtin_float_time_from_integer() {
    crate::test_utils::init_test_tracing();
    let result = builtin_float_time(vec![Value::fixnum(42)]).unwrap();
    match result.kind() {
        ValueKind::Float => {
            let f = result.as_float().unwrap();
            assert!((f - 42.0).abs() < 1e-9);
        }
        _ => panic!("expected float"),
    }
}

#[test]
fn builtin_time_add_basic() {
    crate::test_utils::init_test_tracing();
    let a = Value::fixnum(100);
    let b = Value::fixnum(200);
    let result = builtin_time_add(vec![a, b]).unwrap();
    assert_eq!(result.as_int(), Some(300));
}

#[test]
fn builtin_time_subtract_basic() {
    crate::test_utils::init_test_tracing();
    let a = Value::fixnum(300);
    let b = Value::fixnum(100);
    let result = builtin_time_subtract(vec![a, b]).unwrap();
    assert_eq!(result.as_int(), Some(200));
}

#[test]
fn time_add_preserves_gnu_list_timestamp_form_for_seconds_to_time_inputs() {
    crate::test_utils::init_test_tracing();
    let a = Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(0),
        Value::fixnum(0),
    ]);
    let b = Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(2),
        Value::fixnum(0),
        Value::fixnum(0),
    ]);
    let result = builtin_time_add(vec![a, b]).unwrap();
    assert_eq!(
        list_to_vec(&result).unwrap(),
        vec![
            Value::fixnum(0),
            Value::fixnum(3),
            Value::fixnum(0),
            Value::fixnum(0)
        ]
    );
}

#[test]
fn time_arithmetic_preserves_gnu_numeric_and_ticks_hz_forms() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        builtin_time_add(vec![Value::fixnum(1), Value::fixnum(2)])
            .unwrap()
            .as_int(),
        Some(3)
    );

    let cons_result = builtin_time_add(vec![
        Value::cons(Value::fixnum(3), Value::fixnum(2)),
        Value::cons(Value::fixnum(1), Value::fixnum(2)),
    ])
    .unwrap();
    assert_eq!(cons_result.cons_car().as_int(), Some(4));
    assert_eq!(cons_result.cons_cdr().as_int(), Some(2));
}

#[test]
fn builtin_time_less_p_true() {
    crate::test_utils::init_test_tracing();
    let result = builtin_time_less_p(vec![Value::fixnum(1), Value::fixnum(2)]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn builtin_time_less_p_false() {
    crate::test_utils::init_test_tracing();
    let result = builtin_time_less_p(vec![Value::fixnum(2), Value::fixnum(1)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn builtin_time_equal_p_true() {
    crate::test_utils::init_test_tracing();
    let result = builtin_time_equal_p(vec![Value::fixnum(42), Value::fixnum(42)]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn builtin_time_equal_p_false() {
    crate::test_utils::init_test_tracing();
    let result = builtin_time_equal_p(vec![Value::fixnum(42), Value::fixnum(43)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn builtin_current_time_string_known_time() {
    crate::test_utils::init_test_tracing();
    // 2024-01-15 12:30:45 UTC
    let epoch = encode_to_epoch_secs(45, 30, 12, 15, 1, 2024);
    let result = builtin_current_time_string(vec![Value::fixnum(epoch), Value::T]).unwrap();
    let s = result.as_utf8_str().unwrap();
    assert!(s.contains("Jan"));
    assert!(s.contains("12:30:45"));
    assert!(s.contains("2024"));
    assert!(s.contains("15"));
}

#[test]
fn builtin_current_time_string_no_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_current_time_string(vec![]).unwrap();
    assert!(result.is_string());
}

#[test]
fn builtin_current_time_zone_default() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();
    let result = builtin_current_time_zone(vec![]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 2);
    assert!(items[0].is_integer());
    assert!(items[1].is_string());
}

#[test]
fn builtin_current_time_string_honors_explicit_zone_argument() {
    crate::test_utils::init_test_tracing();
    let result = builtin_current_time_string(vec![
        Value::list(vec![Value::fixnum(0), Value::fixnum(0)]),
        Value::fixnum(3600),
    ])
    .unwrap();
    assert_eq!(result.as_utf8_str().unwrap(), "Thu Jan  1 01:00:00 1970");
}

#[test]
fn builtin_encode_time_known() {
    crate::test_utils::init_test_tracing();
    let result = builtin_encode_time(vec![
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(1),
        Value::fixnum(1970),
        Value::T,
    ])
    .unwrap();
    let items = list_to_vec(&result).unwrap();
    let high = items[0].as_int().unwrap();
    let low = items[1].as_int().unwrap();
    assert_eq!(high * 65536 + low, 0);
}

/// GNU `encode-time` (mktime/timegm) rolls an out-of-range MONTH into the year:
/// month 0 -> December of the previous year, month 13 -> January of the next
/// year, month -1 -> November of the previous year. Verify the out-of-range
/// form encodes to the same instant as the explicitly-normalized form. (This is
/// what made org-timestamp-to-time on a string diverge: -001-12-31 vs GNU's
/// -001-11-30.)
#[test]
fn builtin_encode_time_normalizes_out_of_range_month_like_gnu() {
    crate::test_utils::init_test_tracing();
    let enc = |day: i64, month: i64, year: i64| -> i64 {
        let r = builtin_encode_time(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(day),
            Value::fixnum(month),
            Value::fixnum(year),
            Value::T,
        ])
        .unwrap();
        let items = list_to_vec(&r).unwrap();
        items[0].as_int().unwrap() * 65536 + items[1].as_int().unwrap()
    };
    // month 0, year Y  ==  month 12, year Y-1
    assert_eq!(enc(1, 0, 1), enc(1, 12, 0));
    // month 13, year Y  ==  month 1, year Y+1
    assert_eq!(enc(1, 13, 1), enc(1, 1, 2));
    // month -1, year Y  ==  month 11, year Y-1
    assert_eq!(enc(1, -1, 1), enc(1, 11, 0));
    // The degenerate org-timestamp case: (day 0 month 0 year 0) normalizes
    // through month 0 -> Dec of year -1, then day 0 -> Nov 30.
    assert_eq!(enc(0, 0, 0), enc(0, 12, -1));
}

#[test]
fn builtin_encode_time_y2k() {
    crate::test_utils::init_test_tracing();
    let result = builtin_encode_time(vec![
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(1),
        Value::fixnum(2000),
        Value::T,
    ])
    .unwrap();
    let items = list_to_vec(&result).unwrap();
    let high = items[0].as_int().unwrap();
    let low = items[1].as_int().unwrap();
    assert_eq!(high * 65536 + low, 946_684_800);
}

#[test]
fn builtin_encode_time_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let result = builtin_encode_time(vec![]);
    assert!(result.is_err());
}

#[test]
fn builtin_encode_time_decoded_time_list() {
    crate::test_utils::init_test_tracing();
    let result = builtin_encode_time(vec![Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(1),
        Value::fixnum(1970),
        Value::NIL,
        Value::fixnum(-1),
        Value::T,
    ])])
    .unwrap();
    let items = list_to_vec(&result).unwrap();
    let high = items[0].as_int().unwrap();
    let low = items[1].as_int().unwrap();
    assert_eq!(high * 65536 + low, 0);
}

#[test]
fn builtin_encode_time_honors_zone_offset() {
    crate::test_utils::init_test_tracing();
    let result = builtin_encode_time(vec![Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(1),
        Value::fixnum(1970),
        Value::NIL,
        Value::fixnum(-1),
        Value::fixnum(-3600),
    ])])
    .unwrap();
    let items = list_to_vec(&result).unwrap();
    let high = items[0].as_int().unwrap();
    let low = items[1].as_int().unwrap();
    assert_eq!(high * 65536 + low, 3600);
}

#[test]
fn builtin_decode_time_epoch_zero() {
    crate::test_utils::init_test_tracing();
    let result = builtin_decode_time(vec![Value::fixnum(0), Value::T]).unwrap();
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
fn builtin_decode_time_honors_explicit_zone_argument() {
    crate::test_utils::init_test_tracing();

    let plus = builtin_decode_time(vec![Value::fixnum(0), Value::fixnum(3600)]).unwrap();
    let plus_items = list_to_vec(&plus).unwrap();
    assert_eq!(plus_items[2].as_int(), Some(1)); // hour
    assert_eq!(plus_items[3].as_int(), Some(1)); // day
    assert_eq!(plus_items[4].as_int(), Some(1)); // month
    assert_eq!(plus_items[5].as_int(), Some(1970)); // year
    assert_eq!(plus_items[7], Value::NIL);
    assert_eq!(plus_items[8].as_int(), Some(3600));

    let minus = builtin_decode_time(vec![Value::fixnum(0), Value::fixnum(-3600)]).unwrap();
    let minus_items = list_to_vec(&minus).unwrap();
    assert_eq!(minus_items[2].as_int(), Some(23)); // hour
    assert_eq!(minus_items[3].as_int(), Some(31)); // day
    assert_eq!(minus_items[4].as_int(), Some(12)); // month
    assert_eq!(minus_items[5].as_int(), Some(1969)); // year
    assert_eq!(minus_items[6].as_int(), Some(3)); // Wednesday
    assert_eq!(minus_items[7], Value::NIL);
    assert_eq!(minus_items[8].as_int(), Some(-3600));
}

#[test]
fn builtin_decode_time_nil_zone_uses_time_zone_rule() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();

    builtin_set_time_zone_rule(vec![Value::fixnum(-3600)]).unwrap();
    let implicit = builtin_decode_time(vec![Value::fixnum(0)]).unwrap();
    let explicit_nil = builtin_decode_time(vec![Value::fixnum(0), Value::NIL]).unwrap();
    assert_eq!(implicit, explicit_nil);

    let items = list_to_vec(&implicit).unwrap();
    assert_eq!(items[2].as_int(), Some(23));
    assert_eq!(items[3].as_int(), Some(31));
    assert_eq!(items[4].as_int(), Some(12));
    assert_eq!(items[5].as_int(), Some(1969));
    assert_eq!(items[8].as_int(), Some(-3600));

    reset_tz_rule();
}

#[test]
fn builtin_decode_time_form_t_preserves_input_precision() {
    crate::test_utils::init_test_tracing();

    let integer = builtin_decode_time(vec![Value::fixnum(42), Value::T, Value::T]).unwrap();
    let integer_items = list_to_vec(&integer).unwrap();
    assert_eq!(integer_items[0].as_int(), Some(42));

    let micro = builtin_decode_time(vec![
        Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(1),
            Value::fixnum(250_000),
        ]),
        Value::T,
        Value::T,
    ])
    .unwrap();
    let micro_items = list_to_vec(&micro).unwrap();
    assert_eq!(micro_items[0].cons_car().as_int(), Some(1_250_000));
    assert_eq!(micro_items[0].cons_cdr().as_int(), Some(1_000_000));

    let pico = builtin_decode_time(vec![
        Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(1),
            Value::fixnum(250_000),
            Value::fixnum(123_456),
        ]),
        Value::T,
        Value::T,
    ])
    .unwrap();
    let pico_items = list_to_vec(&pico).unwrap();
    assert_eq!(pico_items[0].cons_car().as_int(), Some(1_250_000_123_456));
    assert_eq!(pico_items[0].cons_cdr().as_int(), Some(1_000_000_000_000));

    let default_form =
        builtin_decode_time(vec![Value::make_float(1.25), Value::T, Value::NIL]).unwrap();
    let default_items = list_to_vec(&default_form).unwrap();
    assert_eq!(default_items[0].as_int(), Some(1));
}

// Bug 8: `encode-time` must honor an explicit `nil` DST slot by forcing
// standard time (GNU passes `tm_isdst = 0` to `mktime_z`), rather than treating
// it like the auto-detecting `-1`. With a DST-observing zone in summer, an
// explicit nil keeps EST (-5h) while `t`/`-1` give EDT (-4h).
//
// GNU oracle (TZ-string slot, July 10 2023 12:00):
//   (encode-time '(0 0 12 10 7 2023 nil nil "EST5EDT,M3.2.0,M11.1.0"))
//     => (25772 14608)            ; explicit nil -> standard time (EST)
//   (encode-time '(0 0 12 10 7 2023 nil t   "EST5EDT,M3.2.0,M11.1.0"))
//     => (25772 11008)            ; explicit t   -> daylight (EDT)
//   (encode-time '(0 0 12 10 7 2023 nil -1  "EST5EDT,M3.2.0,M11.1.0"))
//     => (25772 11008)            ; -1 auto      -> daylight (EDT) in summer
#[test]
fn builtin_encode_time_honors_explicit_nil_dst() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();

    let encode = |dst: Value| -> i64 {
        let result = builtin_encode_time(vec![Value::list(vec![
            Value::fixnum(0),    // sec
            Value::fixnum(0),    // min
            Value::fixnum(12),   // hour
            Value::fixnum(10),   // day
            Value::fixnum(7),    // month (July)
            Value::fixnum(2023), // year
            Value::NIL,          // ignored (dow)
            dst,                 // DST slot
            Value::string("EST5EDT,M3.2.0,M11.1.0"),
        ])])
        .unwrap();
        let items = list_to_vec(&result).unwrap();
        items[0].as_int().unwrap() * 65536 + items[1].as_int().unwrap()
    };

    // 25772 * 65536 + 14608 = 1689004608 (EST, standard time forced by nil)
    assert_eq!(encode(Value::NIL), 25772 * 65536 + 14608);
    // 25772 * 65536 + 11008 = 1689001008 (EDT, daylight forced by t)
    assert_eq!(encode(Value::T), 25772 * 65536 + 11008);
    // -1 auto-detects: summer -> EDT, same as t
    assert_eq!(encode(Value::fixnum(-1)), 25772 * 65536 + 11008);

    reset_tz_rule();
}

// Bug 9: `decode-time` with FORM=t on a sub-second `(TICKS . HZ)` timestamp
// whose HZ is a nanosecond clock must put the seconds as a `(TICKS . HZ)` pair
// preserving sub-second precision, not a truncated integer.
//
// GNU oracle:
//   (decode-time '(1700000000123456789 . 1000000000) 0 t)
//     => SEC slot = (20123456789 . 1000000000)
#[test]
fn builtin_decode_time_form_t_preserves_nanosecond_subsec() {
    crate::test_utils::init_test_tracing();

    // 1700000000 epoch seconds = 2023-11-14 22:13:20 UTC, so tm_sec = 20.
    let time = Value::cons(
        Value::make_int(1_700_000_000_123_456_789),
        Value::make_int(1_000_000_000),
    );
    let result = builtin_decode_time(vec![time, Value::fixnum(0), Value::T]).unwrap();
    let items = list_to_vec(&result).unwrap();

    // SEC slot is a (TICKS . HZ) pair: HZ * 20 + (ticks mod HZ)
    //   = 1_000_000_000 * 20 + 123_456_789 = 20_123_456_789
    let sec = items[0];
    assert!(
        sec.is_cons(),
        "SEC slot must be a (TICKS . HZ) pair, got {sec:?}"
    );
    assert_eq!(sec.cons_car().as_int(), Some(20_123_456_789));
    assert_eq!(sec.cons_cdr().as_int(), Some(1_000_000_000));

    // The minute/second context must still be right (22:13:20 UTC).
    assert_eq!(items[1].as_int(), Some(13)); // min
    assert_eq!(items[2].as_int(), Some(22)); // hour
}

#[test]
fn builtin_decode_time_no_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_decode_time(vec![]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 9);
}

#[test]
fn builtin_encode_decode_roundtrip() {
    crate::test_utils::init_test_tracing();
    // Encode a specific time.
    let encoded = builtin_encode_time(vec![
        Value::fixnum(30),
        Value::fixnum(45),
        Value::fixnum(14),
        Value::fixnum(20),
        Value::fixnum(3),
        Value::fixnum(2025),
        Value::T,
    ])
    .unwrap();

    // Decode it back.
    let decoded = builtin_decode_time(vec![encoded, Value::T]).unwrap();
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
    crate::test_utils::init_test_tracing();
    let result = builtin_time_convert(vec![Value::fixnum(1000)]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 4);
    let high = items[0].as_int().unwrap();
    let low = items[1].as_int().unwrap();
    assert_eq!(high * 65536 + low, 1000);
}

#[test]
fn builtin_time_convert_to_integer() {
    crate::test_utils::init_test_tracing();
    let result = builtin_time_convert(vec![Value::fixnum(1000), Value::symbol("integer")]).unwrap();
    assert_eq!(result.as_int(), Some(1000));
}

#[test]
fn builtin_time_convert_rejects_float_output_form_like_gnu() {
    crate::test_utils::init_test_tracing();
    let err = builtin_time_convert(vec![Value::fixnum(1000), Value::symbol("float")])
        .expect_err("GNU rejects `float' as a time-convert output form");
    assert_invalid_time_frequency(err);
}

#[test]
fn builtin_time_convert_with_t() {
    crate::test_utils::init_test_tracing();
    // GNU: exact integer seconds preserve one-second resolution.
    let result = builtin_time_convert(vec![Value::fixnum(42), Value::T]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let ticks = result.cons_car().as_int().expect("expected int ticks");
            let hz = result.cons_cdr().as_int().expect("expected int hz");
            assert_eq!(hz, 1);
            assert_eq!(ticks, 42);
        }
        _ => panic!("expected cons, got {:?}", result),
    }
}

#[test]
fn builtin_time_convert_rejects_unknown_frequency_forms_like_gnu() {
    crate::test_utils::init_test_tracing();
    for form in [
        Value::symbol("other"),
        Value::string("integer"),
        Value::fixnum(0),
        Value::fixnum(-1),
    ] {
        let err = builtin_time_convert(vec![Value::fixnum(1), form])
            .expect_err("invalid FORM should signal");
        assert_invalid_time_frequency(err);
    }
}

#[test]
fn current_time_and_time_convert_respect_current_time_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_variable("current-time-list", Value::NIL);

    let current = builtin_current_time_in_context(&mut eval, vec![]).unwrap();
    assert!(current.is_cons());
    assert_eq!(current.cons_cdr().as_int(), Some(1_000_000_000));

    let converted =
        builtin_time_convert_in_context(&mut eval, vec![Value::fixnum(1), Value::NIL]).unwrap();
    assert!(converted.is_cons());
    assert_eq!(converted.cons_car().as_int(), Some(1));
    assert_eq!(converted.cons_cdr().as_int(), Some(1));
}

#[test]
fn make_lisp_time_uses_typed_output_representation() {
    let list = make_lisp_time(1_234_567_890, 123_456_789, LispTimeOutput::LegacyList);
    let items = list_to_vec(&list).expect("legacy time must be a list");
    assert_eq!(items.len(), 4);
    let high = items[0].as_int().expect("high seconds");
    let low = items[1].as_int().expect("low seconds");
    assert_eq!((high << 16) | low, 1_234_567_890);
    assert_eq!(items[2].as_int(), Some(123_456));
    assert_eq!(items[3].as_int(), Some(789_000));

    let ticks_hz = make_lisp_time(1_000, 123_456_789, LispTimeOutput::TicksHz);
    assert_eq!(ticks_hz.cons_car().as_int(), Some(1_000_123_456_789));
    assert_eq!(ticks_hz.cons_cdr().as_int(), Some(1_000_000_000));
}

#[test]
fn builtin_time_convert_float_preserves_gnu_binary_precision() {
    crate::test_utils::init_test_tracing();

    // GNU `decode_float_time' treats all significand bits as significant.
    let result = builtin_time_convert(vec![Value::make_float(3.5), Value::T]).unwrap();
    assert_eq!(result.cons_car().as_int(), Some(7_881_299_347_898_368));
    assert_eq!(result.cons_cdr().as_int(), Some(2_251_799_813_685_248));

    let result =
        builtin_time_convert(vec![Value::make_float(1_760_000_000.123456), Value::T]).unwrap();
    assert_eq!(result.cons_car().as_int(), Some(7_381_975_040_517_812));
    assert_eq!(result.cons_cdr().as_int(), Some(4_194_304));

    let result = builtin_time_convert(vec![Value::make_float(-0.1), Value::T]).unwrap();
    assert_eq!(result.cons_car().as_int(), Some(-7_205_759_403_792_794));
    assert_eq!(result.cons_cdr().as_int(), Some(72_057_594_037_927_936));

    let result =
        builtin_time_convert(vec![Value::make_float(-0.1), Value::symbol("list")]).unwrap();
    assert_eq!(
        list_to_vec(&result).unwrap(),
        vec![
            Value::fixnum(-1),
            Value::fixnum(65_535),
            Value::fixnum(899_999),
            Value::fixnum(999_999),
        ]
    );
}

#[test]
fn builtin_set_time_zone_rule_t() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();

    let result = builtin_set_time_zone_rule(vec![Value::T]).unwrap();
    assert!(result.is_nil());
    let tz = builtin_current_time_zone(vec![]).unwrap();
    assert_eq!(
        tz,
        Value::list(vec![Value::fixnum(0), Value::string("GMT")])
    );
    reset_tz_rule();
}

#[test]
fn builtin_set_time_zone_rule_fixed_offsets() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();

    builtin_set_time_zone_rule(vec![Value::fixnum(3600)]).unwrap();
    let plus = builtin_current_time_zone(vec![]).unwrap();
    assert_eq!(
        plus,
        Value::list(vec![Value::fixnum(3600), Value::string("+01")])
    );

    builtin_set_time_zone_rule(vec![Value::fixnum(-3600)]).unwrap();
    let minus = builtin_current_time_zone(vec![]).unwrap();
    assert_eq!(
        minus,
        Value::list(vec![Value::fixnum(-3600), Value::string("-01")])
    );

    builtin_set_time_zone_rule(vec![Value::fixnum(1)]).unwrap();
    let one = builtin_current_time_zone(vec![]).unwrap();
    assert_eq!(
        one,
        Value::list(vec![Value::fixnum(1), Value::string("+000001")])
    );
    reset_tz_rule();
}

#[test]
fn builtin_set_time_zone_rule_string_specs() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();

    builtin_set_time_zone_rule(vec![Value::string("UTC")]).unwrap();
    let utc = builtin_current_time_zone(vec![]).unwrap();
    assert_eq!(
        utc,
        Value::list(vec![Value::fixnum(0), Value::string("UTC")])
    );

    builtin_set_time_zone_rule(vec![Value::string("JST-9")]).unwrap();
    let jst = builtin_current_time_zone(vec![]).unwrap();
    assert_eq!(
        jst,
        Value::list(vec![Value::fixnum(32400), Value::string("JST")])
    );
    reset_tz_rule();
}

#[test]
fn builtin_set_time_zone_rule_invalid_spec() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();

    match builtin_set_time_zone_rule(vec![Value::keyword(":x")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|v| v.as_utf8_str()),
                Some("Invalid time zone specification")
            );
        }
        other => panic!("expected invalid time zone specification error, got {other:?}"),
    }
    reset_tz_rule();
}

/// GNU delegates an explicit numeric/named ZONE to libc `tzalloc` via a POSIX
/// TZ string (`timefns.c` `tzlookup`), so the offset is clamped to +/-24h and a
/// too-short/invalid abbreviation falls back to UTC. Every expectation below was
/// captured from `emacs -Q --batch` (host-TZ-independent, since the ZONE is
/// explicit).
#[test]
#[cfg(unix)]
fn builtin_current_time_zone_validates_and_clamps_explicit_zone() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();

    let t = Value::fixnum(1_700_000_000);
    let named = |off: i64, name: &str| Value::list(vec![Value::fixnum(off), Value::string(name)]);

    // (OFFSET NAME) with NAME shorter than 3 chars is invalid -> UTC fallback
    // with the numeric "+00" abbreviation (libc empties the zone; GNU
    // synthesizes "+00").
    assert_eq!(
        builtin_current_time_zone(vec![t, named(5400, "X")]).unwrap(),
        Value::list(vec![Value::fixnum(0), Value::string("+00")]),
    );

    // A valid (>=3 char) NAME is kept; its offset still passes through libc.
    assert_eq!(
        builtin_current_time_zone(vec![t, named(5400, "ABC")]).unwrap(),
        Value::list(vec![Value::fixnum(5400), Value::string("ABC")]),
    );

    // Named offset beyond +/-24h: the magnitude is clamped, the name retained.
    assert_eq!(
        builtin_current_time_zone(vec![t, named(90000, "ABC")]).unwrap(),
        Value::list(vec![Value::fixnum(86400), Value::string("ABC")]),
    );

    // Integer offset clamped to [-86400, 86400]; the numeric abbreviation
    // reflects the *requested* hour count ("+25"/"+48"), not the clamped one.
    assert_eq!(
        builtin_current_time_zone(vec![t, Value::fixnum(-90000)]).unwrap(),
        Value::list(vec![Value::fixnum(-86400), Value::string("-25")]),
    );
    assert_eq!(
        builtin_current_time_zone(vec![t, Value::fixnum(172800)]).unwrap(),
        Value::list(vec![Value::fixnum(86400), Value::string("+48")]),
    );
    assert_eq!(
        builtin_current_time_zone(vec![t, Value::fixnum(86400)]).unwrap(),
        Value::list(vec![Value::fixnum(86400), Value::string("+24")]),
    );

    // Sub-hour integer offsets format the full numeric abbreviation.
    assert_eq!(
        builtin_current_time_zone(vec![t, Value::fixnum(5400)]).unwrap(),
        Value::list(vec![Value::fixnum(5400), Value::string("+0130")]),
    );
    assert_eq!(
        builtin_current_time_zone(vec![t, Value::fixnum(3661)]).unwrap(),
        Value::list(vec![Value::fixnum(3661), Value::string("+010101")]),
    );

    // A malformed POSIX TZ string falls back to UTC with the "+00" abbreviation
    // (not the empty string libc reports).
    assert_eq!(
        builtin_current_time_zone(vec![t, Value::string("X1Y,J1,J365")]).unwrap(),
        Value::list(vec![Value::fixnum(0), Value::string("+00")]),
    );

    reset_tz_rule();
}

/// `decode-time` with a too-short (OFFSET NAME) ZONE likewise falls back to UTC,
/// so the broken-down time and UTCOFF use offset 0 (GNU reports hour 22, UTCOFF
/// 0 for `(decode-time 1700000000 (list -18000 "E"))`).
#[test]
#[cfg(unix)]
fn builtin_decode_time_short_named_zone_falls_back_to_utc() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();

    let decoded = builtin_decode_time(vec![
        Value::fixnum(1_700_000_000),
        Value::list(vec![Value::fixnum(-18000), Value::string("E")]),
    ])
    .unwrap();
    let items = list_to_vec(&decoded).unwrap();
    assert_eq!(items[2].as_int(), Some(22)); // hour: UTC, not 17
    assert_eq!(items[8].as_int(), Some(0)); // UTCOFF clamped to 0

    // A valid named offset still applies normally.
    let valid = builtin_decode_time(vec![
        Value::fixnum(1_700_000_000),
        Value::list(vec![Value::fixnum(-18000), Value::string("EST")]),
    ])
    .unwrap();
    let valid_items = list_to_vec(&valid).unwrap();
    assert_eq!(valid_items[2].as_int(), Some(17)); // hour: 22 - 5
    assert_eq!(valid_items[8].as_int(), Some(-18000));

    reset_tz_rule();
}

#[test]
fn builtin_current_time_zone_with_zone_arg() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();

    let gmt = builtin_current_time_zone(vec![Value::NIL, Value::T]).unwrap();
    assert_eq!(
        gmt,
        Value::list(vec![Value::fixnum(0), Value::string("GMT")])
    );

    let plus = builtin_current_time_zone(vec![Value::NIL, Value::fixnum(3600)]).unwrap();
    assert_eq!(
        plus,
        Value::list(vec![Value::fixnum(3600), Value::string("+01")])
    );

    match builtin_current_time_zone(vec![Value::NIL, Value::keyword(":x")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|v| v.as_utf8_str()),
                Some("Invalid time zone specification")
            );
        }
        other => panic!("expected invalid time zone specification error, got {other:?}"),
    }
    reset_tz_rule();
}

#[test]
fn safe_date_to_time_bootstrap_matches_gnu_elisp() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (safe-date-to-time "1970-01-01 00:00:00 +0000")
        (safe-date-to-time "Thu, 01 Jan 1970 00:00:00 +0000")
        (safe-date-to-time "1970-01-01 00:00:00 -0100")
        (safe-date-to-time "not a date")
        (safe-date-to-time nil)
        (condition-case err (safe-date-to-time) (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK (0 0)");
    assert_eq!(results[1], "OK (0 0)");
    assert_eq!(results[2], "OK (0 3600)");
    assert_eq!(results[3], "OK 0");
    assert_eq!(results[4], "OK 0");
    assert_eq!(results[5], "OK wrong-number-of-arguments");
}

// -----------------------------------------------------------------------
// Edge cases
// -----------------------------------------------------------------------

#[test]
fn time_add_with_usec_overflow() {
    crate::test_utils::init_test_tracing();
    let a = Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(10),
        Value::fixnum(999_000),
        Value::fixnum(0),
    ]);
    let b = Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(5),
        Value::fixnum(500_000),
        Value::fixnum(0),
    ]);
    let result = builtin_time_add(vec![a, b]).unwrap();
    assert_eq!(
        list_to_vec(&result).unwrap(),
        vec![
            Value::fixnum(0),
            Value::fixnum(16),
            Value::fixnum(499_000),
            Value::fixnum(0)
        ]
    );
}

#[test]
fn time_subtract_with_usec_borrow() {
    crate::test_utils::init_test_tracing();
    let a = Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(10),
        Value::fixnum(100_000),
        Value::fixnum(0),
    ]);
    let b = Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(5),
        Value::fixnum(500_000),
        Value::fixnum(0),
    ]);
    let result = builtin_time_subtract(vec![a, b]).unwrap();
    assert_eq!(
        list_to_vec(&result).unwrap(),
        vec![
            Value::fixnum(0),
            Value::fixnum(4),
            Value::fixnum(600_000),
            Value::fixnum(0)
        ]
    );
}

#[test]
fn float_time_nil_arg() {
    crate::test_utils::init_test_tracing();
    let result = builtin_float_time(vec![Value::NIL]).unwrap();
    match result.kind() {
        ValueKind::Float => {
            let f = result.as_float().unwrap();
            assert!(f > 1_000_000_000.0);
        }
        _ => panic!("expected float"),
    }
}

#[test]
fn time_operations_with_mixed_formats() {
    crate::test_utils::init_test_tracing();
    // Add an integer to a list-format time.
    let a = Value::fixnum(100);
    let b = Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(50),
        Value::fixnum(250_000),
        Value::fixnum(0),
    ]);
    let result = builtin_time_add(vec![a, b]).unwrap();
    assert_eq!(
        list_to_vec(&result).unwrap(),
        vec![
            Value::fixnum(0),
            Value::fixnum(150),
            Value::fixnum(250_000),
            Value::fixnum(0)
        ]
    );
}

#[test]
fn current_time_string_epoch() {
    crate::test_utils::init_test_tracing();
    let result = builtin_current_time_string(vec![Value::fixnum(0), Value::T]).unwrap();
    let s = result.as_utf8_str().unwrap();
    // 1970-01-01 00:00:00 UTC, Thursday
    assert!(s.contains("Thu"));
    assert!(s.contains("Jan"));
    assert!(s.contains("1970"));
    assert!(s.contains("00:00:00"));
}

#[test]
fn time_zone_symbol_domain_matches_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        TimeZoneSymbol::from_value(&Value::symbol("wall")),
        Some(TimeZoneSymbol::Wall)
    );
    assert_eq!(TimeZoneSymbol::Wall.name(), "wall");
    assert_eq!(TimeZoneSymbol::from_value(&Value::NIL), None);
    assert_eq!(TimeZoneSymbol::from_value(&Value::T), None);
    assert_eq!(TimeZoneSymbol::from_value(&Value::symbol("local")), None);

    assert!(matches!(
        parse_zone_rule(&Value::NIL).unwrap(),
        ZoneRule::Local
    ));
    assert!(matches!(
        parse_zone_rule(&Value::symbol("wall")).unwrap(),
        ZoneRule::Local
    ));
    assert!(matches!(parse_zone_rule(&Value::T).unwrap(), ZoneRule::Utc));
    assert!(parse_zone_rule(&Value::symbol("local")).is_err());
}

#[test]
fn parse_zone_rule_accepts_raw_unibyte_string_without_panicking() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    match parse_zone_rule(&raw).unwrap() {
        ZoneRule::TzString(spec) => assert_eq!(spec.chars().count(), 1),
        other => panic!("expected TzString, got {other:?}"),
    }
}

// -----------------------------------------------------------------------
// Exact (TICKS . HZ) rational time core — byte-exact vs GNU Emacs 31.0.50.
// Each assertion was reproduced against
// `/home/exec/Projects/github.com/emacs-mirror/emacs/src/emacs --batch`.
// -----------------------------------------------------------------------

#[test]
fn float_time_reads_back_bignum_ticks_hz_from_encode_time_float() {
    crate::test_utils::init_test_tracing();
    // GNU: (float-time (encode-time 30.5 30 14 16 6 2026 t)) => 1781620230.5
    // Use an explicit UTC zone so this regression is independent of the host TZ.
    // The encode-time result has bignum TICKS at HZ=2**48; the old i64 path
    // signalled `wrong-type-argument integerp <bignum>` instead of decoding it.
    let results = bootstrap_eval(
        r#"
        (float-time (encode-time 30.5 30 14 16 6 2026 t))
        (float-time (encode-time 30.0 30 14 16 6 2026 t))
        "#,
    );
    assert_eq!(results[0], "OK 1781620230.5");
    assert_eq!(results[1], "OK 1781620230.0");
}

#[test]
fn float_time_of_symbol_t_signals_invalid_time_specification() {
    crate::test_utils::init_test_tracing();
    // GNU: (float-time t) => (error "Invalid time specification"); previously
    // Neomacs raised `wrong-type-argument`.
    let results = bootstrap_eval(
        r#"
        (condition-case e (float-time t) (error (car e)))
        (condition-case e (float-time t) (error (car (cdr e))))
        "#,
    );
    assert_eq!(results[0], "OK error");
    assert_eq!(results[1], "OK \"Invalid time specification\"");
}

#[test]
fn time_add_mixed_resolutions_uses_exact_rational() {
    crate::test_utils::init_test_tracing();
    // GNU: (time-add 0.5 '(1 . 3)) => (5 . 6); the lossy microsecond path
    // produced a garbage high-HZ pair.
    let results = bootstrap_eval(
        r#"
        (time-add 0.5 '(1 . 3))
        (time-add '(3 . 4) '(1 . 8))
        "#,
    );
    assert_eq!(results[0], "OK (5 . 6)");
    assert_eq!(results[1], "OK (7 . 8)");
}

#[test]
fn time_arith_collapses_to_integer_for_whole_second_lists() {
    crate::test_utils::init_test_tracing();
    // GNU collapses HZ back to 1 when the result is whole seconds:
    // Use an explicit UTC zone so the epoch assertions do not depend on the
    // test runner's local timezone.
    // (time-add (encode-time 0 0 0 1 1 2024 t) (seconds-to-time 86400))
    //   => 1704153600 (a plain integer, NOT a (HI LO ...) list).
    let results = bootstrap_eval(
        r#"
        (let ((t1 (encode-time 0 0 0 1 1 2024 t)))
          (time-add t1 (seconds-to-time 86400)))
        (let ((t1 (encode-time 0 0 0 1 1 2024 t)))
          (time-add t1 (seconds-to-time 3600)))
        (let ((t1 (encode-time 0 0 0 2 1 2024 t))
              (t2 (encode-time 0 0 0 1 1 2024 t)))
          (time-subtract t1 t2))
        "#,
    );
    assert_eq!(results[0], "OK 1704153600");
    assert_eq!(results[1], "OK 1704070800");
    assert_eq!(results[2], "OK 86400");
}

#[test]
fn time_add_preserves_hi_lo_us_ps_list_form() {
    crate::test_utils::init_test_tracing();
    // GNU keeps the (HI LO US PS) form when sub-second precision survives and
    // neither input is a (TICKS . HZ) cons.
    let results = bootstrap_eval(
        r#"
        (time-add '(0 1 0 0) '(0 2 0 0))
        (time-add '(0 1 5 0) '(0 2 0 7))
        "#,
    );
    assert_eq!(results[0], "OK (0 3 0 0)");
    assert_eq!(results[1], "OK (0 3 5 7)");
}

#[test]
fn time_less_p_and_equal_p_on_encode_time_values() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (let ((t1 (encode-time 0 0 0 1 1 2024 nil))
              (t2 (encode-time 0 0 0 2 1 2024 nil)))
          (list (time-less-p t1 t2) (time-less-p t2 t1) (time-equal-p t1 t1)))
        "#,
    );
    assert_eq!(results[0], "OK (t nil t)");
}

#[test]
fn float_time_list_form_matches_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU: (float-time '(1 2 3 4)) => 65538.00000300001
    let results = bootstrap_eval(r#"(float-time '(1 2 3 4))"#);
    assert_eq!(results[0], "OK 65538.00000300001");
}

#[test]
fn format_time_string_subsecond_from_cons_second() {
    crate::test_utils::init_test_tracing();
    // GNU: encode-time with a cons SECOND (30 0 0) (= (HIGH LOW USEC)) yields a
    // (... . 1000000) timestamp; %3N/%6N read its sub-second field exactly.
    // Use an explicit UTC zone so the timestamp is independent of the host TZ.
    let results = bootstrap_eval(
        r#"
        (encode-time '(30 0 0) 0 12 15 6 2024 t)
        (let ((t0 (encode-time '(30 0 0) 0 12 15 6 2024 t)))
          (list (format-time-string "%S.%3N" t0 t)
                (format-time-string "%S.%6N" t0 t)))
        "#,
    );
    assert_eq!(results[0], "OK (1720418880000000 . 1000000)");
    assert_eq!(results[1], "OK (\"00.000\" \"00.000000\")");
}

#[test]
fn time_convert_accepts_bignum_ticks_hz_for_all_forms() {
    crate::test_utils::init_test_tracing();
    // Regression: timer arithmetic on a float delay (e.g. (run-with-timer 0.1
    // ...)) produces a (TICKS . HZ) cons with a *bignum* TICKS, which
    // timer--time-setter then feeds to `(time-convert TIME 'list)`. The old
    // fixnum-only decoder signalled `wrong-type-argument integerp <bignum>`.
    // All forms verified byte-exact vs GNU Emacs 31.0.50.
    let big = "1003201560221054835793619586101";
    let results = bootstrap_eval(&format!(
        r#"
        (time-convert '({big} . 562949953421312000000) 'list)
        (time-convert '({big} . 562949953421312000000) 'integer)
        (time-convert '({big} . 562949953421312000000) t)
        (time-convert '({big} . 562949953421312000000) 1000000)
        "#
    ));
    assert_eq!(results[0], "OK (27191 54576 795673 0)");
    assert_eq!(results[1], "OK 1782043952");
    assert_eq!(
        results[2],
        "OK (1003201560221054835793619586101 . 562949953421312000000)"
    );
    assert_eq!(results[3], "OK (1782043952795673 . 1000000)");
}

// -----------------------------------------------------------------------
// GROUP=timefns behavioral fixes (mirroring GNU src/timefns.c). Each output
// below was captured from GNU Emacs 31 `--batch`.
// -----------------------------------------------------------------------

// NB: these behavioral tests call the builtins directly (not `bootstrap_eval`),
// because the bootstrap evaluator is gated on a generated `.pdump` that a fresh
// debug build lacks. `condition_case_print` renders the result/error object so
// it can be compared byte-for-byte against GNU Emacs `--batch` output.

const MOST_POSITIVE_FIXNUM: i64 = 2_305_843_009_213_693_951; // GNU 61-bit fixnum

/// Bug #1: `(decode-time most-positive-fixnum t)` and `(decode-time 1e18 t)`
/// used to hang (the year was found by an O(years) loop, ~7e13 iterations) and
/// then return an out-of-range year. GNU's `gmtime_r`/`emacs_localtime_rz`
/// fails for such inputs, so `decode-time` signals
/// `(error "Specified time is not representable")`. The closed-form
/// civil-from-days conversion plus the `tm_year`-range check reproduces that.
#[test]
fn decode_time_huge_input_signals_not_representable_no_hang() {
    crate::test_utils::init_test_tracing();
    // (decode-time most-positive-fixnum t)
    assert_eq!(
        condition_case_print(builtin_decode_time(vec![
            Value::make_int(MOST_POSITIVE_FIXNUM),
            Value::T,
        ])),
        "(error \"Specified time is not representable\")"
    );
    // (decode-time 1e18 t)
    assert_eq!(
        condition_case_print(builtin_decode_time(
            vec![Value::make_float(1e18), Value::T,]
        )),
        "(error \"Specified time is not representable\")"
    );
}

/// Bug #2: the first out-of-range epoch second (year 2147485548, one past the
/// largest representable `tm_year` = INT_MAX) signals; the last in-range second
/// (year 2147485547-12-31 23:59:59 UTC) decodes; and the negative boundary
/// behaves the same way. Matches GNU.
#[test]
fn decode_time_year_range_boundary_matches_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        condition_case_print(builtin_decode_time(vec![
            Value::make_int(67_768_036_191_676_800),
            Value::T,
        ])),
        "(error \"Specified time is not representable\")"
    );
    assert_eq!(
        condition_case_print(builtin_decode_time(vec![
            Value::make_int(67_768_036_191_676_799),
            Value::T,
        ])),
        "(59 59 23 31 12 2147485547 3 nil 0)"
    );
    assert_eq!(
        condition_case_print(builtin_decode_time(vec![
            Value::make_int(-67_768_040_609_740_801),
            Value::T,
        ])),
        "(error \"Specified time is not representable\")"
    );
    assert_eq!(
        condition_case_print(builtin_decode_time(vec![
            Value::make_int(-67_768_040_609_740_800),
            Value::T,
        ])),
        "(0 0 0 1 1 -2147481748 4 nil 0)"
    );
}

/// The closed-form civil conversion must agree with the previous loop on
/// ordinary dates, including negative epochs (pre-1970) and leap days.
#[test]
fn civil_from_days_matches_known_dates() {
    crate::test_utils::init_test_tracing();
    // 1970-01-01.
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    // 2000-02-29 (leap day).
    let leap = encode_to_epoch_secs(0, 0, 0, 29, 2, 2000) / 86400;
    assert_eq!(civil_from_days(leap), (2000, 2, 29));
    // 1969-12-31 (one day before epoch).
    assert_eq!(civil_from_days(-1), (1969, 12, 31));
    // 1900-01-01 (century, non-leap) — negative epoch.
    let y1900 = encode_to_epoch_secs(0, 0, 0, 1, 1, 1900).div_euclid(86400);
    assert_eq!(civil_from_days(y1900), (1900, 1, 1));
}

/// Bug #7: `encode-time` of a decoded-time list whose SECOND field is nil. GNU
/// decodes SECOND with `decode_lisp_time`, where nil means the current time, so
/// the call returns a `(TICKS . HZ)` value rather than signalling
/// `(wrong-type-argument fixnump nil)`.
#[test]
fn encode_time_nil_second_returns_ticks_hz_like_gnu() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();
    let result = builtin_encode_time(vec![Value::list(vec![
        Value::NIL, // SECOND = nil -> current time
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(1),
        Value::fixnum(1970),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
    ])])
    .expect("nil SECOND is allowed");
    // GNU returns (TICKS . HZ); the exact value depends on the current time, so
    // just assert it is a (integer . positive-integer) cons. TICKS is a bignum
    // here (now.secs * HZ), so accept fixnum-or-bignum via `value_to_integer`.
    assert!(result.is_cons(), "expected (TICKS . HZ), got {result:?}");
    assert!(
        value_to_integer(&result.cons_car()).is_some(),
        "TICKS must be an integer, got {:?}",
        result.cons_car()
    );
    let hz = value_to_integer(&result.cons_cdr()).expect("HZ is an integer");
    assert!(hz > Integer::from(0), "HZ must be positive, got {hz}");
    reset_tz_rule();
}

/// Bug #7 (companion): a non-number, non-nil SECOND (e.g. a symbol) signals the
/// `decode_lisp_time` error `(error "Invalid time specification")`, not
/// `wrong-type-argument fixnump`.
#[test]
fn encode_time_symbol_second_signals_invalid_time_specification() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();
    let result = builtin_encode_time(vec![Value::list(vec![
        Value::symbol("foo"), // SECOND = symbol -> invalid time spec
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(1),
        Value::fixnum(1970),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
    ])]);
    assert_eq!(
        condition_case_print(result),
        "(error \"Invalid time specification\")"
    );
    reset_tz_rule();
}

/// Bugs #8-11: `encode-time` field-list validation walks the list cons-by-cons
/// (`CHECK_CONS`), so a malformed list signals `(wrong-type-argument consp
/// OFFENDING-CELL)` on the specific cell — not a blanket `listp` error.
#[test]
fn encode_time_field_list_reports_consp_per_cell_like_gnu() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();
    // (encode-time 0)
    assert_eq!(
        condition_case_print(builtin_encode_time(vec![Value::fixnum(0)])),
        "(wrong-type-argument consp 0)"
    );
    // (encode-time '(0 . 1000))
    assert_eq!(
        condition_case_print(builtin_encode_time(vec![Value::cons(
            Value::fixnum(0),
            Value::fixnum(1000),
        )])),
        "(wrong-type-argument consp 1000)"
    );
    // (encode-time '(0 0 0))
    assert_eq!(
        condition_case_print(builtin_encode_time(vec![Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
        ])])),
        "(wrong-type-argument consp nil)"
    );
    // (encode-time '(0 0 0 1 1 . 1970)) — improper list with a fixnum tail.
    let improper = Value::cons(
        Value::fixnum(0),
        Value::cons(
            Value::fixnum(0),
            Value::cons(
                Value::fixnum(0),
                Value::cons(
                    Value::fixnum(1),
                    Value::cons(Value::fixnum(1), Value::fixnum(1970)),
                ),
            ),
        ),
    );
    assert_eq!(
        condition_case_print(builtin_encode_time(vec![improper])),
        "(wrong-type-argument consp 1970)"
    );
    reset_tz_rule();
}

/// Bug #12: an invalid HZ/FORM passed to `time-convert` carries the offending
/// value in the error data (GNU `invalid_hz`: `xsignal2 (Qerror, "Invalid time
/// frequency", hz)`).
#[test]
fn time_convert_invalid_frequency_appends_offending_value_like_gnu() {
    crate::test_utils::init_test_tracing();
    // (time-convert 1 0)
    assert_eq!(
        condition_case_print(builtin_time_convert(vec![
            Value::fixnum(1),
            Value::fixnum(0),
        ])),
        "(error \"Invalid time frequency\" 0)"
    );
    // (time-convert 1 -5)
    assert_eq!(
        condition_case_print(builtin_time_convert(vec![
            Value::fixnum(1),
            Value::fixnum(-5),
        ])),
        "(error \"Invalid time frequency\" -5)"
    );
    // (time-convert 1 'bogus)
    assert_eq!(
        condition_case_print(builtin_time_convert(vec![
            Value::fixnum(1),
            Value::symbol("bogus"),
        ])),
        "(error \"Invalid time frequency\" bogus)"
    );
}

/// Bug #13: a non-finite float TIME routes through the overflow/not-representable
/// path, matching GNU `time_error (isnan ? EDOM : EOVERFLOW)`: ±inf signals
/// "Specified time is not representable", NaN signals "Invalid time
/// specification".
#[test]
fn time_convert_non_finite_float_matches_gnu() {
    crate::test_utils::init_test_tracing();
    // (time-convert 1.0e+INF t)
    assert_eq!(
        condition_case_print(builtin_time_convert(vec![
            Value::make_float(f64::INFINITY),
            Value::T,
        ])),
        "(error \"Specified time is not representable\")"
    );
    // (time-convert -1.0e+INF t)
    assert_eq!(
        condition_case_print(builtin_time_convert(vec![
            Value::make_float(f64::NEG_INFINITY),
            Value::T,
        ])),
        "(error \"Specified time is not representable\")"
    );
    // (time-convert 0.0e+NaN t)
    assert_eq!(
        condition_case_print(builtin_time_convert(vec![
            Value::make_float(f64::NAN),
            Value::T,
        ])),
        "(error \"Invalid time specification\")"
    );
    // (decode-time 1.0e+INF t) — the same overflow path via `parse_time`.
    let _guard = tz_test_lock();
    reset_tz_rule();
    assert_eq!(
        condition_case_print(builtin_decode_time(vec![
            Value::make_float(f64::INFINITY),
            Value::T,
        ])),
        "(error \"Specified time is not representable\")"
    );
    reset_tz_rule();
}

/// Bugs #14/#15: a bad (non-number) TIME value signals a plain
/// `(error "Invalid time specification")` via GNU's `time_spec_invalid` path —
/// not a `wrong-type-argument numberp`. `time-to-days` shares the `decode-time`
/// -> `parse_time` path exercised here.
#[test]
fn decode_time_bad_time_value_signals_invalid_time_specification() {
    crate::test_utils::init_test_tracing();
    let _guard = tz_test_lock();
    reset_tz_rule();
    // (decode-time "not-a-time" t)
    assert_eq!(
        condition_case_print(builtin_decode_time(vec![
            Value::string("not-a-time"),
            Value::T,
        ])),
        "(error \"Invalid time specification\")"
    );
    reset_tz_rule();
}

/// GNU timefns.c Fcurrent_cpu_time returns (clock() . CLOCKS_PER_SEC) —
/// PROCESS CPU time, not wall time: a process asleep on the wall clock
/// accrues (nearly) none of it. The pre-fix NeoMacs implementation returned
/// wall time since first call, so this asserts the distinguishing property.
#[test]
fn current_cpu_time_is_cpu_not_wall_time() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::builtins::misc_eval::builtin_current_cpu_time;
    let read_ticks = || {
        let pair = builtin_current_cpu_time(vec![]).expect("current-cpu-time succeeds");
        let ticks = pair.cons_car().as_fixnum().expect("ticks fixnum");
        let hz = pair.cons_cdr().as_fixnum().expect("hz fixnum");
        assert_eq!(hz, 1_000_000, "CLOCKS_PER_SEC ticks");
        ticks
    };
    let before = read_ticks();
    std::thread::sleep(std::time::Duration::from_millis(400));
    let after = read_ticks();
    assert!(after >= before, "CPU time is monotonic");
    // 400ms of wall sleep must not register as anything close to 400ms of
    // CPU time (allow generous slack for test-harness background threads).
    assert!(
        after - before < 200_000,
        "slept 400ms wall but CPU ticks advanced by {} us — current-cpu-time \
         is reporting wall time",
        after - before
    );
}
