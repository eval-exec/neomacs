//! Oracle parity tests for `fset`, `symbol-function`, `fboundp`, `fmakunbound`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_fset_and_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--test-fset-fn (lambda (x) (+ x 1)))
                  (unwind-protect
                      (funcall 'neovm--test-fset-fn 5)
                    (fmakunbound 'neovm--test-fset-fn)))";
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("6", &o, &n);
}

#[test]
fn oracle_prop_symbol_function_retrieves_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--test-sf-fn (lambda (x) (* x 2)))
                  (unwind-protect
                      (funcall (symbol-function 'neovm--test-sf-fn) 7)
                    (fmakunbound 'neovm--test-sf-fn)))";
    let expect = expect_test::expect![[r#""OK 14""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("14", &o, &n);
}

#[test]
fn oracle_prop_fboundp_true() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--test-fbp-fn (lambda () 'hi))
                  (unwind-protect
                      (fboundp 'neovm--test-fbp-fn)
                    (fmakunbound 'neovm--test-fbp-fn)))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_fboundp_false() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(fboundp 'neovm--surely-unbound-symbol-xyz)",
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_fmakunbound_makes_unbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--test-fmub-fn (lambda () 42))
                  (fmakunbound 'neovm--test-fmub-fn)
                  (fboundp 'neovm--test-fmub-fn))";
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_fset_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--test-fow-fn (lambda (x) (+ x 1)))
                  (unwind-protect
                      (progn
                        (let ((r1 (funcall 'neovm--test-fow-fn 5)))
                          (fset 'neovm--test-fow-fn (lambda (x) (* x 10)))
                          (list r1 (funcall 'neovm--test-fow-fn 5))))
                    (fmakunbound 'neovm--test-fow-fn)))";
    let expect = expect_test::expect![[r#""OK (6 50)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(6 50)", &o, &n);
}

#[test]
fn oracle_prop_symbol_function_on_unbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(condition-case err
                  (symbol-function 'neovm--definitely-unbound-xyz)
                  (void-function (car err)))";
    let expect = expect_test::expect![[r#""OK nil""#]];
    // GNU Emacs `symbol-function` on an unbound symbol signals void-function,
    // and condition-case catches it. The oracle returns OK nil because
    // `(car err)` on a void-function error yields the symbol `void-function`,
    // but Emacs normalizes the condition-case result to nil in this context.
    // Both should agree on the result.
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(n, o, "neovm and oracle should match");
}

#[test]
fn oracle_prop_fset_with_builtin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Assign a built-in function to a new symbol
    let form = "(progn
                  (fset 'neovm--test-alias (symbol-function '+))
                  (unwind-protect
                      (funcall 'neovm--test-alias 1 2 3)
                    (fmakunbound 'neovm--test-alias)))";
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("6", &o, &n);
}

#[test]
fn oracle_prop_indirect_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Function aliasing via fset to a symbol
    let form = "(progn
                  (fset 'neovm--test-orig (lambda (x) (+ x 100)))
                  (fset 'neovm--test-alias2 'neovm--test-orig)
                  (unwind-protect
                      (funcall 'neovm--test-alias2 5)
                    (fmakunbound 'neovm--test-orig)
                    (fmakunbound 'neovm--test-alias2)))";
    let expect = expect_test::expect![[r#""OK 105""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_indirect_function_follows_long_function_alias_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/data.c:indirect_function follows SYMBOLP function-cell
    // indirections until a non-symbol or nil is reached; it has no 128-hop
    // semantic cutoff.
    let form = r#"
(let ((syms nil))
  (unwind-protect
      (progn
        (dotimes (i 140)
          (push (intern (format "neovm--oracle-if-long-%d" i)) syms))
        (setq syms (nreverse syms))
        (dotimes (i 139)
          (fset (nth i syms) (nth (1+ i) syms)))
        (fset (nth 139 syms) (lambda (x) (list 'tail x)))
        (let ((resolved (indirect-function (car syms))))
          (list
           (functionp resolved)
           (funcall resolved 17)
           (eq resolved (symbol-function (nth 139 syms)))
           (indirect-function (car syms) 'ignored-noerror))))
    (dolist (sym syms)
      (ignore-errors (fmakunbound sym)))))
"#;

    let expect = expect_test::expect![[r#""OK (t (tail 17) t (closure (t) (x) (list 'tail x)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
