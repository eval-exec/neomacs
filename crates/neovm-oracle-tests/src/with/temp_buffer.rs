//! Oracle parity tests for `with-temp-buffer` and buffer manipulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_with_temp_buffer_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "hello world")
                    (buffer-string))"####;
    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""hello world""#, &o, &n);
}

#[test]
fn oracle_prop_with_temp_buffer_point_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "hello world")
                    (goto-char (point-min))
                    (forward-char 5)
                    (point))"####;
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("6", &o, &n);
}

#[test]
fn oracle_prop_with_temp_buffer_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "hello world")
                    (goto-char 6)
                    (delete-region 6 12)
                    (insert "emacs")
                    (buffer-string))"####;
    let expect = expect_test::expect![[r#""OK \"helloemacs\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""helloemacs""#, &o, &n);
}

#[test]
fn oracle_prop_with_temp_buffer_multiple_inserts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "a")
                    (insert "b")
                    (insert "c")
                    (buffer-string))"####;
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""abc""#, &o, &n);
}

#[test]
fn oracle_prop_with_temp_buffer_point_min_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "12345")
                    (list (point-min) (point-max)))"####;
    let expect = expect_test::expect![[r#""OK (1 6)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(1 6)", &o, &n);
}

#[test]
fn oracle_prop_with_temp_buffer_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "hello world")
                    (buffer-substring 1 6))"####;
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""hello""#, &o, &n);
}

#[test]
fn oracle_prop_with_temp_buffer_returns_last_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "ignored")
                    42)"####;
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_prop_with_temp_buffer_re_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "foo bar baz")
                    (goto-char (point-min))
                    (if (re-search-forward "bar" nil t)
                        (match-beginning 0)
                      nil))"####;
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_buffer_line_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(with-temp-buffer
                    (insert "line1\nline2\nline3")
                    (goto-char (point-min))
                    (forward-line 1)
                    (beginning-of-line)
                    (let ((start (point)))
                      (end-of-line)
                      (buffer-substring start (point))))"####;
    let expect = expect_test::expect![[r#""OK \"line2\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""line2""#, &o, &n);
}
