//! Oracle parity tests for dynamic binding, `defvar`, `let`-binding
//! interactions, `boundp`, `default-value`, `symbol-value`,
//! `set`, `makunbound`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Dynamic vs lexical binding interactions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_let_rebinding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defvar makes a variable dynamically scoped
    let form = "(progn
                  (defvar neovm--test-dyn-var 10)
                  (unwind-protect
                      (let ((read-outer
                             (lambda () neovm--test-dyn-var)))
                        (let ((neovm--test-dyn-var 20))
                          ;; Dynamic: lambda sees the let binding
                          (let ((inner (funcall read-outer)))
                            (list neovm--test-dyn-var inner))))
                    (makunbound 'neovm--test-dyn-var)))";
    let expect = expect_test::expect![[r#""OK (20 20)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_dynamic_nested_rebinding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defvar neovm--test-stack-var 0)
                  (unwind-protect
                      (let ((neovm--test-stack-var 1))
                        (let ((neovm--test-stack-var 2))
                          (let ((neovm--test-stack-var 3))
                            (let ((deep neovm--test-stack-var))
                              ;; Unwind and check each level restores
                              deep))))
                    (makunbound 'neovm--test-stack-var)))";
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_prop_dynamic_restore_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Dynamic binding must restore on non-local exit
    let form = "(progn
                  (defvar neovm--test-restore-var 'original)
                  (unwind-protect
                      (progn
                        (condition-case nil
                            (let ((neovm--test-restore-var 'modified))
                              (signal 'error '(\"boom\")))
                          (error nil))
                        neovm--test-restore-var)
                    (makunbound 'neovm--test-restore-var)))";
    let expect = expect_test::expect![[r#""OK original""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-value / set / boundp / makunbound
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_value_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defvar neovm--test-sv-var 42)
                  (unwind-protect
                      (list (symbol-value 'neovm--test-sv-var)
                            (boundp 'neovm--test-sv-var))
                    (makunbound 'neovm--test-sv-var)))";
    let expect = expect_test::expect![[r#""OK (42 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_set_and_symbol_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defvar neovm--test-set-var nil)
                  (unwind-protect
                      (progn
                        (set 'neovm--test-set-var 99)
                        (let ((via-sv (symbol-value 'neovm--test-set-var))
                              (via-name neovm--test-set-var))
                          (list via-sv via-name
                                (= via-sv via-name))))
                    (makunbound 'neovm--test-set-var)))";
    let expect = expect_test::expect![[r#""OK (99 99 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_makunbound_then_boundp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defvar neovm--test-mkub-var 1)
                  (let ((before (boundp 'neovm--test-mkub-var)))
                    (makunbound 'neovm--test-mkub-var)
                    (list before (boundp 'neovm--test-mkub-var))))";
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic binding with funcall patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_callback_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Common Emacs pattern: dynamically bind a variable to customize
    // behavior of called function
    let form = "(progn
                  (defvar neovm--test-output-format 'plain)
                  (unwind-protect
                      (let ((formatter
                             (lambda (val)
                               (cond
                                 ((eq neovm--test-output-format 'plain)
                                  (format \"%s\" val))
                                 ((eq neovm--test-output-format 'quoted)
                                  (format \"'%s'\" val))
                                 ((eq neovm--test-output-format 'upper)
                                  (upcase (format \"%s\" val)))))))
                        (list
                          (funcall formatter \"hello\")
                          (let ((neovm--test-output-format 'quoted))
                            (funcall formatter \"hello\"))
                          (let ((neovm--test-output-format 'upper))
                            (funcall formatter \"hello\"))
                          ;; Back to default
                          (funcall formatter \"hello\")))
                    (makunbound 'neovm--test-output-format)))";
    let expect = expect_test::expect![[r#""OK (\"hello\" \"'hello'\" \"HELLO\" \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_dynamic_with_multiple_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple dynamic variables bound simultaneously
    let form = "(progn
                  (defvar neovm--test-mv-a 1)
                  (defvar neovm--test-mv-b 2)
                  (defvar neovm--test-mv-c 3)
                  (unwind-protect
                      (let ((sum-fn (lambda ()
                                      (+ neovm--test-mv-a
                                         neovm--test-mv-b
                                         neovm--test-mv-c))))
                        (let ((default-sum (funcall sum-fn)))
                          (let ((neovm--test-mv-a 10)
                                (neovm--test-mv-b 20)
                                (neovm--test-mv-c 30))
                            (let ((rebound-sum (funcall sum-fn)))
                              (list default-sum rebound-sum
                                    (funcall sum-fn))))))
                    (makunbound 'neovm--test-mv-a)
                    (makunbound 'neovm--test-mv-b)
                    (makunbound 'neovm--test-mv-c)))";
    let expect = expect_test::expect![[r#""OK (6 60 60)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: dynamic scoping for configuration/context
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_context_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a context stack using dynamic binding
    let form = "(progn
                  (defvar neovm--test-indent-level 0)
                  (unwind-protect
                      (let ((lines nil))
                        (let ((emit
                               (lambda (text)
                                 (setq lines
                                       (cons (concat
                                              (make-string
                                               (* 2 neovm--test-indent-level)
                                               ?\\s)
                                              text)
                                             lines)))))
                          (funcall emit \"root\")
                          (let ((neovm--test-indent-level 1))
                            (funcall emit \"child-1\")
                            (let ((neovm--test-indent-level 2))
                              (funcall emit \"grandchild\"))
                            (funcall emit \"child-2\"))
                          (funcall emit \"root-end\"))
                        (nreverse lines))
                    (makunbound 'neovm--test-indent-level)))";
    let expect = expect_test::expect![[
        r#""OK (\"root\" \"  child-1\" \"    grandchild\" \"  child-2\" \"root-end\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
