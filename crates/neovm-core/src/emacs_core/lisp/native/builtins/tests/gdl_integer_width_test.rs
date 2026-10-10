use crate::emacs_core::{Context, format_eval_result};

#[test]
fn gdl_integer_width_checks_new_results_and_large_requests() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    for form in [
        "(let ((integer-width 64)) (ash 1 128))",
        "(expt 2 65536)",
        "(let ((integer-width 128)) (truncate 1e40))",
        "(let ((integer-width 128)) (* (ash 1 100) (ash 1 100)))",
        "(let ((integer-width 128)) (ash 1 4294967296))",
    ] {
        match ctx.eval_str(form) {
            Err(crate::emacs_core::error::EvalError::Signal {
                symbol,
                data,
                raw_data,
                ..
            }) => {
                assert_eq!(
                    symbol,
                    crate::emacs_core::intern::intern("overflow-error"),
                    "{form}"
                );
                assert!(data.is_empty(), "overflow data must be nil: {form}");
                assert!(
                    raw_data.is_none_or(|raw| raw.is_nil()),
                    "raw overflow data must be nil: {form}"
                );
            }
            result => panic!("expected overflow-error for {form}, got {result:?}"),
        }
    }
    let floor = ctx
        .eval_str("(let ((integer-width 0)) (ash 1 127))")
        .expect("128-bit floor permits this result");
    assert!(floor.as_bignum().is_some());
    // GNU alloc.c:7506 and data.c:1475-1483 permit intmax_t-sized bignum
    // bindings; bignum.c:94-100 reads their full signed slot value.
    for form in [
        "(let ((integer-width (1+ most-positive-fixnum))) (ash 1 70000))",
        "(let ((integer-width (1- most-negative-fixnum))) (ash 1 70000))",
    ] {
        let result = ctx.eval_str(form).expect("intmax_t-sized width binding");
        assert!(result.as_bignum().is_some(), "{form}");
    }
}

#[test]
fn gdl_integer_width_preserves_existing_operand_identities() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    assert_eq!(format_eval_result(&ctx.eval_str("(let ((x (expt 2 200))) (let ((integer-width 128)) (list (eq (ash x 0) x) (eq (truncate x) x) (eq (+ x) x) (eq (* x) x) (eq (abs x) x))))")), "OK (t t t t t)");
}
