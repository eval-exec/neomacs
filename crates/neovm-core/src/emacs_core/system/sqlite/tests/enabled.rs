use super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::value::Value;

#[test]
fn only_execute_promotes_busy_and_locked_errors() {
    for code in [ffi::SQLITE_BUSY, ffi::SQLITE_LOCKED] {
        assert_eq!(
            SqliteOperation::Execute.condition_for_code(code),
            LispCondition::SqliteLockedError
        );
        assert_eq!(
            SqliteOperation::Select.condition_for_code(code),
            LispCondition::SqliteError
        );
        assert_eq!(
            SqliteOperation::Next.condition_for_code(code),
            LispCondition::SqliteError
        );
    }

    for operation in [
        SqliteOperation::Execute,
        SqliteOperation::Select,
        SqliteOperation::Next,
    ] {
        assert_eq!(
            operation.condition_for_code(ffi::SQLITE_ERROR),
            LispCondition::SqliteError
        );
    }
}

#[test]
fn sqlite_symbol_domains_match_gnu_symbols() {
    assert_eq!(
        SqliteReturnType::from_value(&Value::symbol("set")),
        Some(SqliteReturnType::Set)
    );
    assert_eq!(
        SqliteReturnType::from_value(&Value::symbol("full")),
        Some(SqliteReturnType::Full)
    );
    assert_eq!(SqliteReturnType::Set.symbol_name(), "set");
    assert_eq!(SqliteReturnType::Full.symbol_name(), "full");
    assert_eq!(SqliteReturnType::from_value(&Value::symbol("rows")), None);
    assert_eq!(SqliteReturnType::from_value(&Value::NIL), None);

    assert_eq!(
        SqliteBindSymbol::from_value(&Value::symbol("false")),
        Some(SqliteBindSymbol::False)
    );
    assert_eq!(SqliteBindSymbol::False.symbol_name(), "false");
    assert!(value_is_false_symbol(&Value::symbol("false")));
    assert!(!value_is_false_symbol(&Value::keyword(":false")));
    assert_eq!(SqliteBindSymbol::from_value(&Value::T), None);
}

#[test]
fn version_and_capability_report_bundled_sqlite() {
    crate::test_utils::init_test_tracing();
    assert!(version(vec![]).unwrap().is_string());
    assert_eq!(available_p(vec![]).unwrap(), Value::T);
}

#[test]
fn open_and_close_round_trip() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let db = open(&mut eval, vec![]).unwrap();
    assert_eq!(is_sqlite_object(vec![db]).unwrap(), Value::T);
    assert_eq!(close(vec![db]).unwrap(), Value::T);
    assert_eq!(close(vec![db]).unwrap(), Value::T);
}

#[test]
fn execute_rejects_non_handle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let err = execute(&mut eval, vec![Value::NIL, Value::string("select 1")]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn execute_values_validation_signals_sqlite_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let db = open(&mut eval, vec![]).unwrap();

    let err = execute(
        &mut eval,
        vec![db, Value::string("select ?"), Value::fixnum(9)],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "sqlite-error"),
        other => panic!("expected signal, got {other:?}"),
    }

    let err = execute(
        &mut eval,
        vec![
            db,
            Value::string("select ?"),
            Value::vector(vec![Value::cons(Value::fixnum(1), Value::fixnum(2))]),
        ],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "sqlite-error"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn select_values_validation_signals_sqlite_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let db = open(&mut eval, vec![]).unwrap();
    let err = select(
        &mut eval,
        vec![db, Value::string("select ?"), Value::fixnum(9)],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "sqlite-error"),
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn materialized_select_discards_terminal_step_errors_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let db = open(&mut eval, vec![]).unwrap();

    let rows = select(
        &mut eval,
        vec![db, Value::string("select abs(-9223372036854775808)")],
    )
    .unwrap();

    assert_eq!(rows, Value::NIL);
}
