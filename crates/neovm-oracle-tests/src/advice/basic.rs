//! Oracle parity tests for advice functions.

use crate::common::{
    assert_oracle_parity, assert_oracle_parity_with_load_raw,
    return_if_neovm_enable_oracle_proptest_not_set,
};

#[test]
fn oracle_prop_advice_add_remove_member_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--adv-target) (adv 'neovm--adv-fn)) (fset target (lambda (x) x)) (fset adv (lambda (&rest _) nil)) (unwind-protect (progn (advice-add target :before adv) (list (not (null (advice-member-p adv target))) (progn (advice-remove target adv) (not (null (advice-member-p adv target)))))) (fmakunbound target) (fmakunbound adv)))";
    let expect = expect_test::expect![r#""OK (t nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_advice_unknown_where_keyword_error_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (error \"Unknown add-function location ‘:neovm-unknown’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (advice-add 'car :neovm-unknown #'ignore) (error err))",
        expect,
    );
}

#[test]
fn oracle_prop_advice_wrong_arity_error_shapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (wrong-number-of-arguments (3 . 4) 2)""#];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (advice-add 'car :before) (error err))",
        expect,
    );
    let expect = expect_test::expect![r#""OK (wrong-number-of-arguments (2 . 2) 1)""#];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (advice-remove 'car) (error err))",
        expect,
    );
    let expect = expect_test::expect![r#""OK (wrong-number-of-arguments (2 . 2) 1)""#];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (advice-member-p 'ignore) (error err))",
        expect,
    );
}

#[test]
fn oracle_prop_advice_target_type_error_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (wrong-type-argument symbolp 1)""#];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (advice-add 1 :before #'ignore) (error err))",
        expect,
    );
}

#[test]
fn oracle_prop_advice_before_observes_call_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--adv-target) (before 'neovm--adv-before) (log nil)) (fset target (lambda (x) (setq log (cons (list 'orig x) log)) x)) (fset before (lambda (&rest args) (setq log (cons (cons 'before args) log)))) (unwind-protect (progn (advice-add target :before before) (funcall target 7) (nreverse log)) (fmakunbound target) (fmakunbound before)))";
    let expect = expect_test::expect![r#""OK ((before 7) (orig 7))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_advice_around_wraps_original_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--adv-around-target) (around 'neovm--adv-around)) (fset target (lambda (x) (* x 2))) (fset around (lambda (orig x) (+ 10 (funcall orig (1+ x))))) (unwind-protect (progn (advice-add target :around around) (funcall target 3)) (fmakunbound target) (fmakunbound around)))";
    let expect = expect_test::expect![r#""OK 18""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_advice_override_replaces_original_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--adv-override-target) (override 'neovm--adv-override)) (fset target (lambda (x) (+ x 1))) (fset override (lambda (&rest _) 'override-hit)) (unwind-protect (progn (advice-add target :override override) (funcall target 11)) (fmakunbound target) (fmakunbound override)))";
    let expect = expect_test::expect![r#""OK override-hit""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_advice_filter_args_rewrites_argument_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--adv-filter-args-target) (filter 'neovm--adv-filter-args)) (fset target (lambda (a b) (+ a b))) (fset filter (lambda (args) (list (* 2 (car args)) (* 3 (car (cdr args)))))) (unwind-protect (progn (advice-add target :filter-args filter) (funcall target 2 5)) (fmakunbound target) (fmakunbound filter)))";
    let expect = expect_test::expect![r#""OK 19""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_advice_filter_return_rewrites_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--adv-filter-ret-target) (filter 'neovm--adv-filter-ret)) (fset target (lambda (x) (* x 2))) (fset filter (lambda (ret) (+ ret 9))) (unwind-protect (progn (advice-add target :filter-return filter) (funcall target 3)) (fmakunbound target) (fmakunbound filter)))";
    let expect = expect_test::expect![r#""OK 15""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_advice_runs_when_target_is_called_via_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--adv-apply-target) (before 'neovm--adv-apply-before) (log nil)) (fset target (lambda (a b) (setq log (cons (list 'orig a b) log)) (+ a b))) (fset before (lambda (&rest args) (setq log (cons (cons 'before args) log)))) (unwind-protect (progn (advice-add target :before before) (list (apply target '(4 9)) (nreverse log))) (fmakunbound target) (fmakunbound before)))";
    let expect = expect_test::expect![r#""OK (13 ((before 4 9) (orig 4 9)))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_advice_remove_restores_unadvised_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--adv-restore-target) (before 'neovm--adv-restore-before) (log nil)) (fset target (lambda (x) (setq log (cons (list 'orig x) log)) x)) (fset before (lambda (&rest args) (setq log (cons (cons 'before args) log)))) (unwind-protect (progn (advice-add target :before before) (funcall target 1) (advice-remove target before) (funcall target 2) (nreverse log)) (fmakunbound target) (fmakunbound before)))";
    let expect = expect_test::expect![r#""OK ((before 1) (orig 1) (orig 2))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_advice_before_and_after_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--adv-order-target) (before 'neovm--adv-order-before) (after 'neovm--adv-order-after) (log nil)) (fset target (lambda (x) (setq log (cons (list 'orig x) log)) x)) (fset before (lambda (&rest args) (setq log (cons (cons 'before args) log)))) (fset after (lambda (&rest args) (setq log (cons (cons 'after args) log)))) (unwind-protect (progn (advice-add target :before before) (advice-add target :after after) (funcall target 5) (nreverse log)) (fmakunbound target) (fmakunbound before) (fmakunbound after)))";
    let expect = expect_test::expect![r#""OK ((before 5) (orig 5) (after 5))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_advice_non_callable_advice_function_error_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (invalid-function 1)""#];
    // `nadvice.el` is Elisp-defined. NeoVM loads the GNU source file directly,
    // so compare against GNU Emacs running the same source, not the default
    // byte-compiled dump where this path is accidentally masked.
    //
    // Use the raw parity helper here: source-loading nadvice can rewrite `car`,
    // so the recursive oracle normalizer itself is no longer a valid observer.
    crate::common::assert_oracle_parity_with_load_raw_expect(
        "(condition-case err (advice-add 'car :before 1) (error err))",
        &["emacs-lisp/oclosure.el", "emacs-lisp/nadvice.el"],
        expect,
    );
}

/// Temporary debug test to isolate where (advice-add 'car :before 1) fails.
///
/// This test evaluates sub-expressions step by step to pinpoint the failure.
/// GNU Emacs byte-compiled nadvice.el uses Bcar opcodes that bypass the
/// function cell, so `(advice-add 'car ...)` works.  NeoVM interprets .el
/// source, so after `fset 'car <advice-closure>`, internal `(car ref)` calls
/// inside `gv-deref` dispatch through the overridden function cell and break.
///
/// GNU Emacs has the SAME bug when forced to load nadvice.el as source
/// (without .elc).  The byte-compiler masks it.
#[test]
fn debug_advice_add_non_callable_steps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    use crate::common::eval_oracle_and_neovm;

    let cases: &[(&str, &str)] = &[
        // Step 1: advice--make on a user symbol — should succeed
        (
            "advice--make works on user symbol",
            "(let ((target 'neovm--dbg-target)) (fset target (lambda (x) x)) (unwind-protect (type-of (advice--make :before target (lambda (&rest _) nil) nil)) (fmakunbound target)))",
        ),
        // Step 2: advice-add on a user-defined function — should succeed
        (
            "advice-add on user-defined fn",
            "(let ((target 'neovm--dbg-target2)) (fset target (lambda (x) x)) (unwind-protect (condition-case err (progn (advice-add target :before #'ignore) 'ok) (error err)) (fmakunbound target)))",
        ),
        // Step 3: advice-add on 'car — the problematic case
        (
            "advice-add on 'car with non-callable",
            "(condition-case err (advice-add 'car :before 1) (error err))",
        ),
        // Step 4: type-of car's function cell (subr in GNU Emacs)
        (
            "type-of symbol-function car",
            "(type-of (symbol-function 'car))",
        ),
    ];

    for (label, form) in cases {
        let (oracle, neovm) = eval_oracle_and_neovm(form);
        eprintln!("[debug] {label}:");
        eprintln!("  oracle: {oracle}");
        eprintln!("  neovm:  {neovm}");
        if oracle != neovm {
            eprintln!("  ** MISMATCH **");
        }
    }
}
