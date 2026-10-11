//! Source-backed GNU contract only: these tests never deliver a signal.
//!
//! GNU process.c checks FIXNUM SIGCODE with check_integer_range(INT_MIN,
//! INT_MAX); bignum.c's check_integer_range emits (args-out-of-range VALUE
//! LOW HIGH). Bignum SIGCODE instead reaches CHECK_SYMBOL in process.c.
//! Running the signal-process builtin is deliberately forbidden.

use super::*;
use crate::emacs_core::error::FlowKind;
use crate::emacs_core::eval::Context;

#[test]
fn parse_signal_number_rejects_truncating_fixnums() {
    let _context = Context::new();
    for code in [
        i64::from(i32::MIN) - 1,
        i64::from(i32::MAX) + 1,
        (1_i64 << 32) + 15,
        (1_i64 << 32) + 9,
        Value::MOST_NEGATIVE_FIXNUM,
        Value::MOST_POSITIVE_FIXNUM,
    ] {
        let value = Value::fixnum(code);
        let error = parse_signal_number(&value).expect_err("GNU rejects a non-C-int SIGCODE");
        let FlowKind::Signal(error) = error.into_kind() else {
            panic!("expected GNU range signal");
        };
        assert_eq!(error.symbol_name(), "args-out-of-range");
        assert_eq!(
            error.data,
            vec![
                value,
                Value::fixnum(i64::from(i32::MIN)),
                Value::fixnum(i64::from(i32::MAX)),
            ],
        );
    }
}

#[test]
fn parse_signal_number_accepts_c_int_boundaries() {
    let _context = Context::new();
    for code in [i32::MIN, -1, 0, 1, i32::MAX] {
        assert_eq!(
            i32::from(SignalNumber::try_from(i64::from(code)).unwrap()),
            code
        );
        let parsed = parse_signal_number(&Value::fixnum(i64::from(code))).unwrap();
        assert_eq!(i32::from(parsed), code);
    }
}

#[test]
fn parse_signal_number_preserves_names_and_string_type_errors() {
    let _context = Context::new();
    #[cfg(unix)]
    assert_eq!(
        i32::from(parse_signal_number(&Value::symbol("TERM")).unwrap()),
        libc::SIGTERM,
    );
    let value = Value::string("TERM");
    let error = parse_signal_number(&value).unwrap_err();
    let FlowKind::Signal(error) = error.into_kind() else {
        panic!("expected GNU type signal");
    };
    assert_eq!(error.symbol_name(), "wrong-type-argument");
    assert_eq!(error.data, vec![Value::symbol("symbolp"), value]);
}

/// GNU rejects bignum SIGCODE through CHECK_SYMBOL, even though integerp is true.
#[test]
fn parse_signal_number_rejects_bignums_as_symbols() {
    use malachite::integer::Integer;

    let _context = Context::new();
    for integer in [
        Integer::from(Value::MOST_NEGATIVE_FIXNUM) - Integer::from(1),
        Integer::from(Value::MOST_POSITIVE_FIXNUM) + Integer::from(1),
        Integer::from(-(1_i128 << 100)),
        Integer::from(1_i128 << 100),
    ] {
        let value = Value::make_integer(integer);
        assert!(value.is_bignum());
        let error =
            parse_signal_number(&value).expect_err("GNU requires symbolp for bignum SIGCODE");
        let FlowKind::Signal(error) = error.into_kind() else {
            panic!("expected GNU type signal");
        };
        assert_eq!(error.symbol_name(), "wrong-type-argument");
        assert_eq!(error.data, vec![Value::symbol("symbolp"), value]);
    }
}
