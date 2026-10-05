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
        assert_eq!(
            format_eval_result(&ctx.eval_str(form)),
            "ERR (overflow-error)",
            "{form}"
        );
    }
    assert_eq!(
        format_eval_result(&ctx.eval_str("(let ((integer-width 0)) (logb (ash 1 127)))")),
        "OK 127"
    );
}

#[test]
fn gdl_integer_width_preserves_existing_operand_identities() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    assert_eq!(format_eval_result(&ctx.eval_str("(let ((x (expt 2 200))) (let ((integer-width 128)) (list (eq (ash x 0) x) (eq (truncate x) x) (eq (+ x) x) (eq (* x) x) (eq (abs x) x))))")), "OK (t t t t t)");
}
