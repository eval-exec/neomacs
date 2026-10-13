//! Coding conversion CHECK_SYMBOL and EQ retain positioned-symbol policy.
use super::*;
use crate::emacs_core::{error::FlowKind, eval::Context, print::print_value};
use crate::tagged::SymbolMode;

#[test]
fn issue554_positioned_pure_coding_fallbacks_honor_explicit_policy() {
    let mut ctx = Context::new();
    let coding = ctx
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("utf-8-unix"), Value::fixnum(4));
    for mode in [SymbolMode::Bare, SymbolMode::Positioned] {
        for encode in [false, true] {
            let args = vec![Value::string("abc"), coding];
            let result = if encode {
                builtin_encode_coding_string_with_known(
                    args,
                    known_coding_system,
                    crate::emacs_core::coding::EolConversion::Enabled,
                    mode,
                )
            } else {
                builtin_decode_coding_string_with_known(
                    args,
                    known_coding_system,
                    crate::emacs_core::coding::EolConversion::Enabled,
                    mode,
                )
            };
            if mode == SymbolMode::Positioned {
                assert_eq!(
                    result
                        .expect("enabled coding symbol")
                        .as_lisp_string()
                        .expect("coding text")
                        .as_bytes(),
                    b"abc"
                );
            } else {
                match result.expect_err("disabled CHECK_SYMBOL").into_kind() {
                    FlowKind::Signal(signal) => {
                        assert_eq!(signal.symbol_name(), "wrong-type-argument");
                        assert_eq!(signal.data, vec![Value::symbol("symbolp"), coding]);
                    }
                    other => panic!("expected symbolp condition, got {other:?}"),
                }
            }
        }
    }
}

#[test]
fn issue554_positioned_coding_runtime_conversion_and_destination_policy() {
    let mut ctx = Context::new();
    let result = ctx
        .eval_str(
            r#"(let ((coding (position-symbol 'utf-8-unix 4))
                 (destination (position-symbol t 5)))
             (mapcar
              (lambda (enabled)
                (let ((symbols-with-pos-enabled enabled))
                  (list
                   (condition-case err
                       (equal (decode-coding-string "abc" coding) "abc")
                     (error (car err)))
                   (condition-case err
                       (equal (encode-coding-string "abc" coding) "abc")
                     (error (car err)))
                   (condition-case err
                       (equal (decode-coding-string "abc" 'utf-8-unix nil destination)
                              "abc")
                     (error (car err))))))
              '(nil t)))"#,
        )
        .expect("coding runtime policy form");
    assert_eq!(
        print_value(&result),
        "((wrong-type-argument wrong-type-argument wrong-type-argument) (t t t))"
    );
}

#[test]
fn issue554_positioned_coding_ascii_identity_records_original_symbol() {
    let mut ctx = Context::new();
    let result = ctx
        .eval_str(
            r#"(let ((coding (position-symbol 'utf-8-unix 4))
                 (symbols-with-pos-enabled t))
             (decode-coding-string "abc" coding)
             (symbol-with-pos-p last-coding-system-used))"#,
        )
        .expect("coding original identity form");
    assert_eq!(
        result,
        Value::T,
        "GNU coding.c:9621 records original coding_system"
    );
}
