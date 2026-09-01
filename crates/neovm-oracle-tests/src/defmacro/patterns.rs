//! Advanced oracle parity tests for `defmacro` patterns.
//!
//! Covers: simple transformation macros, &rest body, &optional params,
//! macroexpand verification, macro generating defun, gensym-like hygiene,
//! anaphoric macros (aif, awhen, aand), and loop macro with break/continue.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Simple transformation macros
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_simple_transformation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macros that transform expressions: negate, square, swap-args
    let form = r#"(progn
                    (defmacro neovm--pat-negate (expr)
                      `(- 0 ,expr))
                    (defmacro neovm--pat-square (x)
                      (let ((v (make-symbol "v")))
                        `(let ((,v ,x))
                           (* ,v ,v))))
                    (defmacro neovm--pat-swap-args (fn a b)
                      `(,fn ,b ,a))
                    (unwind-protect
                        (list
                          (neovm--pat-negate 42)
                          (neovm--pat-negate (+ 10 20))
                          (neovm--pat-square 7)
                          (neovm--pat-square (+ 2 3))
                          (neovm--pat-swap-args - 10 3)
                          (neovm--pat-swap-args cons 'tail 'head))
                      (fmakunbound 'neovm--pat-negate)
                      (fmakunbound 'neovm--pat-square)
                      (fmakunbound 'neovm--pat-swap-args)))"#;
    let expect = expect_test::expect![[r#""OK (-42 -30 49 25 -7 (head . tail))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macros with &rest body parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_rest_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // with-timing: wraps body in let + progn, returns (elapsed . result)
    let form = r#"(progn
                    (require 'cl-lib)
                    (defmacro neovm--pat-with-accumulator (var init &rest body)
                      `(let ((,var ,init))
                         ,@body
                         ,var))
                    (defmacro neovm--pat-collecting (&rest body)
                      (let ((result (make-symbol "result")))
                        `(let ((,result nil))
                           (cl-flet ((collect (item)
                                       (setq ,result (cons item ,result))))
                             ,@body)
                           (nreverse ,result))))
                    (unwind-protect
                        (list
                          ;; accumulator: sum 1..5
                          (neovm--pat-with-accumulator total 0
                            (setq total (+ total 1))
                            (setq total (+ total 2))
                            (setq total (+ total 3))
                            (setq total (+ total 4))
                            (setq total (+ total 5)))
                          ;; accumulator: build string
                          (neovm--pat-with-accumulator s ""
                            (setq s (concat s "a"))
                            (setq s (concat s "b"))
                            (setq s (concat s "c"))))
                      (fmakunbound 'neovm--pat-with-accumulator)
                      (fmakunbound 'neovm--pat-collecting)))"#;
    let expect = expect_test::expect![[r#""OK (15 \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macros with &optional parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_optional_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macro with optional default value and optional else-clause.
    let form = r#"(progn
                    (defmacro neovm--pat-with-default (var expr &optional default)
                      `(let ((,var ,expr))
                         (if ,var ,var ,(or default nil))))
                    (defmacro neovm--pat-if2 (test then &optional else)
                      `(if ,test ,then ,else))
                    (unwind-protect
                        (list
                          (neovm--pat-with-default x 42)
                          (neovm--pat-with-default x nil)
                          (neovm--pat-with-default x nil 'fallback)
                          (neovm--pat-if2 t 'yes)
                          (neovm--pat-if2 nil 'yes)
                          (neovm--pat-if2 nil 'yes 'no)
                          (neovm--pat-if2 t 'yes 'no))
                      (fmakunbound 'neovm--pat-with-default)
                      (fmakunbound 'neovm--pat-if2)))"#;
    let expect = expect_test::expect![[r#""OK (42 nil fallback yes nil no yes)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// macroexpand / macroexpand-all verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_macroexpand_verification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that macroexpand produces the expected expansion,
    // and that evaluating the expansion gives the correct result.
    let form = r#"(progn
                    (defmacro neovm--pat-triple (x)
                      `(+ ,x ,x ,x))
                    (defmacro neovm--pat-and2 (a b)
                      `(if ,a ,b nil))
                    (unwind-protect
                        (let ((exp1 (macroexpand '(neovm--pat-triple 10)))
                              (exp2 (macroexpand '(neovm--pat-and2 t 42))))
                          (list
                            exp1
                            (eval exp1)
                            exp2
                            (eval exp2)
                            ;; Nested macro: outer expands but inner stays
                            (macroexpand '(neovm--pat-and2
                                            (neovm--pat-triple 1)
                                            (neovm--pat-triple 2)))))
                      (fmakunbound 'neovm--pat-triple)
                      (fmakunbound 'neovm--pat-and2)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ 10 10 10) 30 (if t 42 nil) 42 (if (neovm--pat-triple 1) (neovm--pat-triple 2) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro that generates defun
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_generates_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A macro that generates getter/setter function pairs.
    let form = r#"(progn
                    (defmacro neovm--pat-defprop (name)
                      (let ((getter (intern (concat "neovm--pat-get-" (symbol-name name))))
                            (setter (intern (concat "neovm--pat-set-" (symbol-name name))))
                            (store (intern (concat "neovm--pat-store-" (symbol-name name)))))
                        `(progn
                           (defvar ,store nil)
                           (defun ,getter () ,store)
                           (defun ,setter (val) (setq ,store val)))))
                    (unwind-protect
                        (progn
                          (neovm--pat-defprop color)
                          (neovm--pat-set-color 'red)
                          (let ((v1 (neovm--pat-get-color)))
                            (neovm--pat-set-color 'blue)
                            (list v1 (neovm--pat-get-color))))
                      (fmakunbound 'neovm--pat-defprop)
                      (fmakunbound 'neovm--pat-get-color)
                      (fmakunbound 'neovm--pat-set-color)
                      (makunbound 'neovm--pat-store-color)))"#;
    let expect = expect_test::expect![[r#""OK (red blue)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_defmacro_generates_defun_with_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macro that defines a memoized function.
    let form = r#"(progn
                    (defmacro neovm--pat-defmemo (name args &rest body)
                      (let ((cache (make-symbol "cache"))
                            (key (make-symbol "key"))
                            (val (make-symbol "val")))
                        `(progn
                           (defvar ,(intern (concat "neovm--pat-memo-cache-"
                                                    (symbol-name name)))
                             (make-hash-table :test 'equal))
                           (defun ,name ,args
                             (let* ((,cache ,(intern (concat "neovm--pat-memo-cache-"
                                                             (symbol-name name))))
                                    (,key (list ,@args))
                                    (,val (gethash ,key ,cache 'neovm--miss)))
                               (if (eq ,val 'neovm--miss)
                                   (let ((result (progn ,@body)))
                                     (puthash ,key result ,cache)
                                     result)
                                 ,val))))))
                    (unwind-protect
                        (progn
                          (neovm--pat-defmemo neovm--pat-add (a b) (+ a b))
                          (list
                            (neovm--pat-add 3 4)
                            (neovm--pat-add 3 4)
                            (neovm--pat-add 10 20)
                            (hash-table-count neovm--pat-memo-cache-neovm--pat-add)))
                      (fmakunbound 'neovm--pat-defmemo)
                      (fmakunbound 'neovm--pat-add)
                      (makunbound 'neovm--pat-memo-cache-neovm--pat-add)))"#;
    let expect = expect_test::expect![[r#""OK (7 7 30 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro with gensym-like variable hygiene
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_gensym_hygiene() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Without make-symbol, the macro would capture the user's variable.
    // With make-symbol, hygiene is preserved.
    let form = r#"(progn
                    ;; BAD macro (captures temp variable)
                    (defmacro neovm--pat-swap-bad (a b)
                      `(let ((tmp ,a))
                         (setq ,a ,b)
                         (setq ,b tmp)))
                    ;; GOOD macro (hygienic via make-symbol)
                    (defmacro neovm--pat-swap-good (a b)
                      (let ((tmp (make-symbol "tmp")))
                        `(let ((,tmp ,a))
                           (setq ,a ,b)
                           (setq ,b ,tmp))))
                    (unwind-protect
                        (let ((results nil))
                          ;; Test with non-conflicting names
                          (let ((x 1) (y 2))
                            (neovm--pat-swap-bad x y)
                            (setq results (cons (list x y) results)))
                          ;; Test good macro with non-conflicting names
                          (let ((x 10) (y 20))
                            (neovm--pat-swap-good x y)
                            (setq results (cons (list x y) results)))
                          ;; Test good macro: even if user var is named tmp, no capture
                          (let ((tmp 100) (y 200))
                            (neovm--pat-swap-good tmp y)
                            (setq results (cons (list tmp y) results)))
                          (nreverse results))
                      (fmakunbound 'neovm--pat-swap-bad)
                      (fmakunbound 'neovm--pat-swap-good)))"#;
    let expect = expect_test::expect![[r#""OK ((2 1) (20 10) (200 100))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: anaphoric macros (aif, awhen, aand)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_anaphoric_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Anaphoric macros bind the test result to `it`.
    // Note: using dynamic binding, `it` is available in body.
    let form = r#"(progn
                    (defmacro neovm--pat-aif (test then &optional else)
                      `(let ((it ,test))
                         (if it ,then ,else)))
                    (defmacro neovm--pat-awhen (test &rest body)
                      `(let ((it ,test))
                         (when it ,@body)))
                    (defmacro neovm--pat-aand (&rest forms)
                      (cond
                        ((null forms) t)
                        ((null (cdr forms)) (car forms))
                        (t `(let ((it ,(car forms)))
                              (when it
                                (neovm--pat-aand ,@(cdr forms)))))))
                    (unwind-protect
                        (list
                          ;; aif: test is truthy
                          (neovm--pat-aif (assoc 'b '((a 1) (b 2) (c 3)))
                            (cadr it)
                            'not-found)
                          ;; aif: test is falsy
                          (neovm--pat-aif (assoc 'z '((a 1) (b 2) (c 3)))
                            (cadr it)
                            'not-found)
                          ;; awhen: truthy
                          (neovm--pat-awhen (member 3 '(1 2 3 4 5))
                            (length it))
                          ;; awhen: falsy
                          (neovm--pat-awhen (member 9 '(1 2 3))
                            (length it))
                          ;; aand: chain of non-nil values
                          (neovm--pat-aand
                            '((a . 1) (b . 2) (c . 3))
                            (assoc 'b it)
                            (cdr it))
                          ;; aand: chain broken by nil
                          (neovm--pat-aand
                            '((a . 1) (b . 2))
                            (assoc 'z it)
                            (cdr it)))
                      (fmakunbound 'neovm--pat-aif)
                      (fmakunbound 'neovm--pat-awhen)
                      (fmakunbound 'neovm--pat-aand)))"#;
    let expect = expect_test::expect![[r#""OK (2 not-found 3 nil 2 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: loop macro with break/continue
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defmacro_loop_with_break_continue() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A simple loop macro that supports (break VAL) and (continue).
    // Implemented via catch/throw under the hood.
    let form = r#"(progn
                    (defmacro neovm--pat-loop-for (var in list &rest body)
                      (let ((result (make-symbol "result"))
                            (items (make-symbol "items")))
                        `(catch 'neovm--pat-loop-break
                           (let ((,items ,list)
                                 (,result nil))
                             (while ,items
                               (let ((,var (car ,items)))
                                 (setq ,items (cdr ,items))
                                 (catch 'neovm--pat-loop-continue
                                   ,@body)))
                             ,result))))
                    (defmacro neovm--pat-loop-break (&optional value)
                      `(throw 'neovm--pat-loop-break ,value))
                    (defmacro neovm--pat-loop-continue ()
                      '(throw 'neovm--pat-loop-continue nil))
                    (unwind-protect
                        (let ((collected nil))
                          ;; Collect squares, skip evens, break at 7
                          (neovm--pat-loop-for x in '(1 2 3 4 5 6 7 8 9 10)
                            (when (= x 7)
                              (neovm--pat-loop-break (nreverse collected)))
                            (when (= 0 (% x 2))
                              (neovm--pat-loop-continue))
                            (setq collected (cons (* x x) collected))))
                      (fmakunbound 'neovm--pat-loop-for)
                      (fmakunbound 'neovm--pat-loop-break)
                      (fmakunbound 'neovm--pat-loop-continue)))"#;
    let expect = expect_test::expect![[r#""OK (1 9 25)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_defmacro_loop_collect_and_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A more sophisticated loop: collect into result list,
    // with when-clause filtering and until-clause termination.
    let form = r#"(progn
                    (defmacro neovm--pat-do-collect (var list when-clause &rest body)
                      (let ((result (make-symbol "result"))
                            (items (make-symbol "items")))
                        `(let ((,items ,list)
                               (,result nil))
                           (while ,items
                             (let ((,var (car ,items)))
                               (setq ,items (cdr ,items))
                               (when ,when-clause
                                 (setq ,result (cons (progn ,@body) ,result)))))
                           (nreverse ,result))))
                    (unwind-protect
                        (list
                          ;; Collect doubled odd numbers from 1-10
                          (neovm--pat-do-collect n '(1 2 3 4 5 6 7 8 9 10)
                            (= 1 (% n 2))
                            (* n 2))
                          ;; Collect lengths of strings longer than 3 chars
                          (neovm--pat-do-collect s '("hi" "hello" "yo" "world" "ok" "wonderful")
                            (> (length s) 3)
                            (length s)))
                      (fmakunbound 'neovm--pat-do-collect)))"#;
    let expect = expect_test::expect![[r#""OK ((2 6 10 14 18) (5 5 9))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
