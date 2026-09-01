//! Divergence tests: real symbol & obarray behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_intern_unintern_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 test-sym-cycle-xxx t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((sym (intern \"test-sym-cycle-xxx\" obarray)))
  (set sym 42)
  (list (symbol-value sym)
        (intern-soft \"test-sym-cycle-xxx\" obarray)
        (unintern sym obarray)
        (intern-soft \"test-sym-cycle-xxx\" obarray))) ",
        expect,
    );
}

#[test]
fn divergence_symbol_plist_real() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 nil (a 1 b 2 c 3 d 4) 4 4 8)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((sym (make-symbol \"temp\")))
  (setplist sym '(a 1 b 2 c 3))
  (list (get sym 'a)
        (get sym 'b)
        (get sym 'c)
        (get sym 'd)
        (symbol-plist sym)
        (put sym 'd 4)
        (get sym 'd)
        (length (symbol-plist sym)))) ",
        expect,
    );
}

#[test]
fn divergence_symbol_function_indirect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (car #<subr car> #<subr car> 1 t test-alias-xxx nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defalias 'test-alias-xxx 'car)
  (list (symbol-function 'test-alias-xxx)
        (indirect-function 'test-alias-xxx)
        (indirect-function 'car)
        (funcall 'test-alias-xxx '(1 2 3))
        (fboundp 'test-alias-xxx)
        (fmakunbound 'test-alias-xxx)
        (fboundp 'test-alias-xxx))) ",
        expect,
    );
}

#[test]
fn divergence_mapatoms_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (intern \"test-mapatoms-a-xxx\" obarray)
  (intern \"test-mapatoms-b-xxx\" obarray)
  (intern \"test-mapatoms-c-xxx\" obarray)
  (let ((names nil))
    (mapatoms (lambda (s)
                (when (string-prefix-p \"test-mapatoms-xxx\"
                                       (symbol-name s))
                  (push (symbol-name s) names))))
    (sort names #'string<))) ",
        expect,
    );
}

#[test]
fn divergence_keyword_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t \":hello\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (keywordp :hello)
  (keywordp 'hello)
  (keywordp ':hello)
  (symbolp :hello)
  (symbol-name :hello)
  (eq :hello :hello)
  (equal :hello ':hello)) ",
        expect,
    );
}

#[test]
fn deficiency_symbol_circular_naming() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t \"test-circ-1-xxx\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((s1 (intern \"test-circ-1-xxx\"))
        (s2 (intern \"test-circ-2-xxx\")))
  (list (eq s1 s2)
        (eq s1 (intern \"test-circ-1-xxx\"))
        (eq s1 (intern-soft \"test-circ-1-xxx\" obarray))
        (symbol-name s1)
        (string= (symbol-name s1) \"test-circ-1-xxx\"))) ",
        expect,
    );
}

#[test]
fn divergence_symbol_doctoring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (initial t test-doctor-xxx nil restored restored t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((sym (intern \"test-doctor-xxx\")))
  (set sym 'initial)
  (list (symbol-value sym)
        (boundp sym)
        (makunbound sym)
        (boundp sym)
        (set sym 'restored)
        (symbol-value sym)
        (boundp sym))) ",
        expect,
    );
}

#[test]
fn divergence_default_value_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (case-fold-search t (nil nil))""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((v (make-variable-buffer-local 'case-fold-search)))
  (list v
        (default-value 'case-fold-search)
        (let ((case-fold-search nil))
          (list case-fold-search
                (default-value 'case-fold-search))))) ",
        expect,
    );
}

#[test]
fn divergence_dynamic_binding_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 42 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-dyn-var-xxx nil)
  (defun test-dyn-check-xxx () test-dyn-var-xxx)
  (list (test-dyn-check-xxx)
        (let ((test-dyn-var-xxx 42))
          (test-dyn-check-xxx))
        (test-dyn-check-xxx))) ",
        expect,
    );
}

#[test]
fn deficiency_special_form_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (special-form-p 'if)
  (special-form-p 'let)
  (special-form-p 'progn)
  (special-form-p 'setq)
  (special-form-p 'car)
  (macrop 'when)
  (macrop 'if)) ",
        expect,
    );
}
