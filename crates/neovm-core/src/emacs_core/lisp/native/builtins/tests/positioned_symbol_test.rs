use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn issue554_positioned_coding_arguments_honor_symbol_mode() {
    let mut ctx = Context::new();
    for form in [
        "(let ((symbols-with-pos-enabled t)) (coding-system-p (position-symbol 'utf-8-unix 0)))",
        "(let ((symbols-with-pos-enabled t)) (eq (check-coding-system (position-symbol 'utf-8-unix 0)) 'utf-8-unix))",
        "(let ((symbols-with-pos-enabled t)) (equal (decode-coding-string \"abc\" (position-symbol 'utf-8-unix 0)) \"abc\"))",
    ] {
        let result = ctx.eval_str(form);
        assert!(
            matches!(result, Ok(value) if value == Value::T),
            "{form}: {result:?}"
        );
    }
}

#[test]
fn issue554_positioned_feature_membership_and_original_storage() {
    let mut ctx = Context::new();
    let result = ctx
        .eval_str(
            "(let ((features nil) (sp (position-symbol 'issue554-feature 0)))
           (list (let ((symbols-with-pos-enabled t))
                   (provide sp)
                   (list (featurep 'issue554-feature)
                         (symbol-with-pos-p (car features))))
                 (let ((symbols-with-pos-enabled nil)) (featurep 'issue554-feature))
                 (let ((symbols-with-pos-enabled t)) (featurep 'issue554-feature))))",
        )
        .expect("positioned feature membership");
    assert_eq!(
        crate::emacs_core::print::print_value(&result),
        "((t t) nil t)"
    );
}

#[test]
fn issue554_intern_soft_preserves_position_and_exact_symbol_identity() {
    let mut ctx = Context::new();
    let result = ctx
        .eval_str(
            "(let* ((oa (obarray-make 2)) (s (intern \"issue554-intern\" oa))
                (sp (position-symbol s 0)) (other (make-symbol \"issue554-intern\")))
           (let ((symbols-with-pos-enabled t))
             (list (symbol-with-pos-p (intern-soft sp oa))
                   (intern-soft other oa)
                   (symbol-with-pos-p (intern-soft (position-symbol 'car 0)))
                   (intern-soft (make-symbol \"car\")))))",
        )
        .expect("intern-soft exact identity");
    assert_eq!(
        crate::emacs_core::print::print_value(&result),
        "(t nil t nil)"
    );
}

#[test]
fn issue554_functionp_rejects_non_nil_autoload_type_in_both_modes() {
    let mut ctx = Context::new();
    let result = ctx
        .eval_str(
            "(progn (fset 'issue554-autoload-function
                   (list 'autoload \"issue554-missing\" nil nil (position-symbol 'macro 0)))
           (list (let ((symbols-with-pos-enabled nil)) (functionp 'issue554-autoload-function))
                 (let ((symbols-with-pos-enabled t)) (functionp 'issue554-autoload-function))))",
        )
        .expect("autoload functionp classification");
    assert_eq!(crate::emacs_core::print::print_value(&result), "(nil nil)");
}
