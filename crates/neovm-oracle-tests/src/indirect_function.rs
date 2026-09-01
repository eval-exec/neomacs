//! Oracle parity tests for `indirect-function`, `symbol-function`,
//! `fset`, `fmakunbound`, `fboundp`, `defalias`, and function
//! indirection chains.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// indirect-function basic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_indirect_function_direct() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // indirect-function on a lambda returns the lambda itself
    let form = r#"(let ((f (lambda (x) (* x x))))
                    (eq f (indirect-function f)))"#;
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_indirect_function_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // indirect-function follows symbol-function chain
    let form = r#"(progn
                    (fset 'neovm--test-indf-a (lambda (x) (+ x 1)))
                    (unwind-protect
                        (list (functionp (indirect-function 'neovm--test-indf-a))
                              (funcall (indirect-function 'neovm--test-indf-a) 10))
                      (fmakunbound 'neovm--test-indf-a)))"#;
    let expect = expect_test::expect![[r#""OK (t 11)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_indirect_function_alias_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // indirect-function resolves a chain of aliases: a -> b -> c -> lambda
    let form = r#"(progn
                    (fset 'neovm--test-chain-c (lambda (x) (* x 2)))
                    (fset 'neovm--test-chain-b 'neovm--test-chain-c)
                    (fset 'neovm--test-chain-a 'neovm--test-chain-b)
                    (unwind-protect
                        (list (funcall (indirect-function 'neovm--test-chain-a) 5)
                              (funcall (indirect-function 'neovm--test-chain-b) 5)
                              (funcall (indirect-function 'neovm--test-chain-c) 5))
                      (fmakunbound 'neovm--test-chain-a)
                      (fmakunbound 'neovm--test-chain-b)
                      (fmakunbound 'neovm--test-chain-c)))"#;
    let expect = expect_test::expect![[r#""OK (10 10 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_function_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
                    (fset 'neovm--test-sf (lambda (a b) (+ a b)))
                    (unwind-protect
                        (funcall (symbol-function 'neovm--test-sf) 3 4)
                      (fmakunbound 'neovm--test-sf)))"#;
    let expect = expect_test::expect![[r#""OK 7""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("7", &o, &n);
}

#[test]
fn oracle_prop_symbol_function_builtin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // symbol-function on built-in returns the subr
    let form = r#"(list (functionp (symbol-function '+))
                        (functionp (symbol-function 'car))
                        (functionp (symbol-function 'cons)))"#;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fboundp / fmakunbound
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fboundp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
                    (fset 'neovm--test-fbp (lambda () t))
                    (unwind-protect
                        (let ((before (fboundp 'neovm--test-fbp)))
                          (fmakunbound 'neovm--test-fbp)
                          (let ((after (fboundp 'neovm--test-fbp)))
                            (list before after)))
                      (when (fboundp 'neovm--test-fbp)
                        (fmakunbound 'neovm--test-fbp))))"#;
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_fboundp_builtins() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (fboundp '+)
                        (fboundp 'car)
                        (fboundp 'mapcar)
                        (fboundp 'neovm--nonexistent-fn-xyz-987))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defalias
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
                    (fset 'neovm--test-original (lambda (x) (1+ x)))
                    (defalias 'neovm--test-alias 'neovm--test-original)
                    (unwind-protect
                        (list (funcall 'neovm--test-alias 10)
                              (funcall 'neovm--test-original 10)
                              (eq (indirect-function 'neovm--test-alias)
                                  (indirect-function 'neovm--test-original)))
                      (fmakunbound 'neovm--test-alias)
                      (fmakunbound 'neovm--test-original)))"#;
    let expect = expect_test::expect![[r#""OK (11 11 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_defalias_to_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
                    (defalias 'neovm--test-da-lambda
                              (lambda (a b) (format "%s-%s" a b)))
                    (unwind-protect
                        (funcall 'neovm--test-da-lambda "hello" "world")
                      (fmakunbound 'neovm--test-da-lambda)))"#;
    let expect = expect_test::expect![[r#""OK \"hello-world\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: dispatch table using indirect-function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_indirect_function_dispatch_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a dispatch table mapping operation names to functions
    let form = r#"(progn
                    (fset 'neovm--test-op-add (lambda (a b) (+ a b)))
                    (fset 'neovm--test-op-mul (lambda (a b) (* a b)))
                    (fset 'neovm--test-op-sub (lambda (a b) (- a b)))
                    (unwind-protect
                        (let ((dispatch '((add . neovm--test-op-add)
                                          (mul . neovm--test-op-mul)
                                          (sub . neovm--test-op-sub))))
                          (let ((run (lambda (op a b)
                                       (let ((fn (cdr (assq op dispatch))))
                                         (when fn
                                           (funcall (indirect-function fn)
                                                    a b))))))
                            (list (funcall run 'add 3 4)
                                  (funcall run 'mul 3 4)
                                  (funcall run 'sub 10 3)
                                  (funcall run 'unknown 1 2))))
                      (fmakunbound 'neovm--test-op-add)
                      (fmakunbound 'neovm--test-op-mul)
                      (fmakunbound 'neovm--test-op-sub)))"#;
    let expect = expect_test::expect![[r#""OK (7 12 7 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: advice-like wrapping via fset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_indirect_function_advice_wrapper() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Wrap an existing function to add logging (advice pattern)
    let form = r#"(progn
                    (fset 'neovm--test-base-fn (lambda (x) (* x x)))
                    (let ((log nil))
                      (let ((original (symbol-function 'neovm--test-base-fn)))
                        ;; Wrap: log args and result
                        (fset 'neovm--test-base-fn
                              (lambda (x)
                                (setq log (cons (list 'call x) log))
                                (let ((result (funcall original x)))
                                  (setq log (cons (list 'result result) log))
                                  result))))
                      (unwind-protect
                          (let ((r1 (funcall 'neovm--test-base-fn 5))
                                (r2 (funcall 'neovm--test-base-fn 3)))
                            (list r1 r2 (nreverse log)))
                        (fmakunbound 'neovm--test-base-fn))))"#;
    let expect =
        expect_test::expect![[r#""OK (25 9 ((call 5) (result 25) (call 3) (result 9)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-level alias resolution with condition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_indirect_function_conditional_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate method dispatch: resolve function based on type tag
    let form = r#"(progn
                    (fset 'neovm--test-format-int
                          (lambda (v) (number-to-string v)))
                    (fset 'neovm--test-format-str
                          (lambda (v) (format "\"%s\"" v)))
                    (fset 'neovm--test-format-list
                          (lambda (v) (format "(%s)"
                                       (mapconcat
                                        (lambda (x) (format "%s" x))
                                        v " "))))
                    (unwind-protect
                        (let ((format-val
                               (lambda (v)
                                 (let ((fn-sym (cond
                                                ((integerp v) 'neovm--test-format-int)
                                                ((stringp v) 'neovm--test-format-str)
                                                ((listp v) 'neovm--test-format-list))))
                                   (when fn-sym
                                     (funcall (indirect-function fn-sym) v))))))
                          (list (funcall format-val 42)
                                (funcall format-val "hello")
                                (funcall format-val '(a b c))
                                (funcall format-val 0)))
                      (fmakunbound 'neovm--test-format-int)
                      (fmakunbound 'neovm--test-format-str)
                      (fmakunbound 'neovm--test-format-list)))"#;
    let expect = expect_test::expect![[r#""OK (\"42\" \"\\\"hello\\\"\" \"(a b c)\" \"0\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
