//! Text-property stickiness/inheritance (insert-and-inherit, rear-nonsticky,
//! get-char-property, propertize, field ops) and time edge cases (pre-epoch,
//! year boundary, far future) parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn insert_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert (propertize "AB" 'face 'bold 'rear-nonsticky nil))
  (goto-char (point-max))
  (insert-and-inherit "CD")
  (list (get-text-property 3 'face) (get-text-property 1 'face)))"##,
        expect,
    );
}

#[test]
fn prop_field_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 4 #(\"aaa\" 0 3 (field f1)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "aaabbbccc")
  (put-text-property 1 4 'field 'f1)
  (put-text-property 4 7 'field 'f2)
  (goto-char 2)
  (list (field-beginning) (field-end) (field-string)))"##,
        expect,
    );
}

#[test]
fn propertize_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold 1 (face bold help-echo \"hi\" x 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (propertize "hello" 'face 'bold 'help-echo "hi" 'x 1)))
  (list (get-text-property 0 'face s) (get-text-property 0 'x s)
        (text-properties-at 0 s)))"##,
        expect,
    );
}

#[test]
fn stickiness_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (put-text-property 1 2 'face 'bold)
  (put-text-property 1 2 'rear-nonsticky t)
  (list (get-text-property 1 'face) (get-char-property 1 'face)))"##,
        expect,
    );
}

#[test]
fn time_far_future() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"2100-01-01\" \"001\" 2100)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%Y-%m-%d" 4102444800 t)
        (format-time-string "%j" 4102444800 t)
        (nth 5 (decode-time 4102444800 t)))"##,
        expect,
    );
}

#[test]
fn time_pre_epoch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"1969-12-31 05:47:44\" \"1969\" \"1969-12-31\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%Y-%m-%d %H:%M:%S" '(-1 0) t)
        (format-time-string "%Y" '(-100 0) t)
        (format-time-string "%Y-%m-%d" -86400 t))"##,
        expect,
    );
}

#[test]
fn time_year_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"1970-01-01 19:20:20\" \"1970-01-01\" (0 0 0 1 1 1970 4 nil 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%Y-%m-%d %H:%M:%S" '(1 4084) t)
        (format-time-string "%Y-%m-%d" 0 t)
        (decode-time 0 t))"##,
        expect,
    );
}
