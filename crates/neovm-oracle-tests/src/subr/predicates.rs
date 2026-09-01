//! Oracle parity tests for subr/function predicates and introspection:
//! `subrp`, `subr-arity`, `commandp`, `interactive-form`,
//! `byte-code-function-p`, `autoloadp`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// subrp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subrp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (subrp (symbol-function '+))
                        (subrp (symbol-function 'car))
                        (subrp (symbol-function 'cons))
                        (subrp (lambda (x) x))
                        (subrp 42)
                        (subrp nil))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// subr-arity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (subr-arity (symbol-function 'car))
                        (subr-arity (symbol-function 'cons))
                        (subr-arity (symbol-function '+))
                        (subr-arity (symbol-function 'list)))"#;
    let expect = expect_test::expect![[r#""OK ((1 . 1) (2 . 2) (0 . many) (0 . many))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// commandp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_commandp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; Regular lambda is NOT a command
                    (commandp (lambda (x) x))
                    ;; Lambda with interactive IS a command
                    (commandp (lambda () (interactive) 42))
                    ;; Symbols
                    (commandp '+))"#;
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// functionp with various types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_functionp_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (functionp (lambda (x) x))
                        (functionp #'car)
                        (functionp '+)
                        (functionp (symbol-function '+))
                        (functionp nil)
                        (functionp t)
                        (functionp 42)
                        (functionp "hello")
                        (functionp '(1 2 3)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// byte-code-function-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_byte_code_function_p_and_make_byte_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU data.c:Fbyte_code_function_p is true only for byte-code function
    // objects. GNU alloc.c:Fmake_byte_code creates those objects after
    // validating the slot layout.
    let form = r#"
(let ((bc (make-byte-code '() "\300\207" [42] 1)))
  (list
   (byte-code-function-p bc)
   (functionp bc)
   (closurep bc)
   (compiled-function-p bc)
   (funcall bc)
   (byte-code-function-p (lambda (x) x))
   (byte-code-function-p '(lambda (x) x))
   (byte-code-function-p (symbol-function 'car))
   (byte-code-function-p nil)
   (condition-case err
       (make-byte-code)
     (error (list (car err) (cdr err))))
   (condition-case err
       (make-byte-code '() "not-byte-code" [] 0)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t t t 42 nil nil nil nil (wrong-number-of-arguments (make-byte-code 0)) #[nil \"not-byte-code\" [] 0])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: function introspection framework
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_introspect_framework() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Introspect a set of functions and categorize them
    let form = r#"(let ((fns '(+ car cons list length append
                               mapcar format concat))
                        (results nil))
                    (dolist (f fns)
                      (let ((def (symbol-function f)))
                        (let ((arity (when (subrp def)
                                       (subr-arity def))))
                          (setq results
                                (cons (list f
                                            (subrp def)
                                            (when arity (car arity))
                                            (when arity (cdr arity)))
                                      results)))))
                    (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ t 0 many) (car t 1 1) (cons t 2 2) (list t 0 many) (length t 1 1) (append t 0 many) (mapcar t 2 2) (format t 1 many) (concat t 0 many))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: arity-based dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_arity_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Choose how to call function based on its arity
    let form = r#"(let ((call-with-defaults
                         (lambda (fn args defaults)
                           (let* ((def (if (symbolp fn)
                                           (symbol-function fn)
                                         fn))
                                  (arity (when (subrp def)
                                           (subr-arity def)))
                                  (min-args (if arity (car arity) 0))
                                  (padded args))
                             ;; Pad with defaults if needed
                             (while (< (length padded) min-args)
                               (let ((idx (length padded)))
                                 (setq padded
                                       (append padded
                                               (list (nth idx defaults))))))
                             (apply fn padded)))))
                    (list
                     ;; cons needs exactly 2 args
                     (funcall call-with-defaults
                              'cons '(hello) '(nil))
                     ;; + works with 0 args
                     (funcall call-with-defaults
                              '+ nil nil)
                     ;; concat works with 0 args
                     (funcall call-with-defaults
                              'concat nil nil)))"#;
    let expect = expect_test::expect![[r#""OK ((hello) 0 \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
