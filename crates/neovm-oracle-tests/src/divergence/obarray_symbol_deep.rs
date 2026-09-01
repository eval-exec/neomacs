//! Divergence tests: obarray, intern, mapatoms, symbol deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_intern_soft() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (car nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (intern-soft "car")
  (intern-soft "nonexistent-symbol-xyz-123")
  (symbolp (intern-soft "list"))
  (null (intern-soft "nonexistent-symbol-xyz-456"))) "#,
        expect,
    );
}

#[test]
fn divergence_intern_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t test-intern-symbol-unique-999)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((sym (intern "test-intern-symbol-unique-999")))
  (list (symbolp sym)
        (eq sym (intern "test-intern-symbol-unique-999"))
        (intern-soft "test-intern-symbol-unique-999"))) "#,
        expect,
    );
}

#[test]
fn divergence_mapatoms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((count 0))
  (mapatoms (lambda (s) (setq count (1+ count))))
  (list (> count 0)
        (integerp count)))"#,
        expect,
    );
}

#[test]
fn divergence_mapatoms_find_specific() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((found nil))
  (mapatoms (lambda (s)
              (when (eq s 'car)
                (setq found t))))
  (list found (not (null found)))) "#,
        expect,
    );
}

#[test]
fn divergence_obarray_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ob (make-obarray 50)))
  (list (obarrayp ob)
        (intern-soft "hello" ob)
        (intern "hello" ob)
        (intern-soft "hello" ob))) "#,
        expect,
    );
}

#[test]
fn divergence_symbol_function_boundp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil #<subr car> t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'obarray)
  (fboundp 'car)
  (fboundp 'nonexistent-fn-xyz)
  (symbol-function 'car)
  (null (fboundp 'nonexistent-fn-xyz))) "#,
        expect,
    );
}

#[test]
fn divergence_symbol_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (test-val (test-prop test-val) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((sym (make-symbol "test-sym")))
  (put sym 'test-prop 'test-val)
  (list (get sym 'test-prop)
        (symbol-plist sym)
        (get sym 'nonexistent))) "#,
        expect,
    );
}

#[test]
fn divergence_symbol_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"car\" \"cdr\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (symbol-name 'car)
  (symbol-name 'cdr)
  (stringp (symbol-name 'list))
  (equal (symbol-name 'foo) "foo"))"#,
        expect,
    );
}

#[test]
fn divergence_make_symbol_uninterned() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"uninterned\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((sym (make-symbol "uninterned")))
  (list (symbolp sym)
        (symbol-name sym)
        (null (intern-soft "uninterned"))
        (not (eq sym (intern "uninterned"))))) "#,
        expect,
    );
}

#[test]
fn divergence_keyword_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t \":hello\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (keywordp :hello)
  (keywordp 'hello)
  (symbolp :hello)
  (symbol-name :hello)
  (equal (symbol-name :hello) "hello"))"#,
        expect,
    );
}
