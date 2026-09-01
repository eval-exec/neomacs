//! Oracle parity for buffer/text deep edge cases.
//! GNU src/editfns.c, src/buffer.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- erase-buffer ---

#[test]
fn oracle_erase_buffer_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*eb1*")) (erase-buffer))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

// --- buffer-string on empty buffer ---

#[test]
fn oracle_buffer_string_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*bs-e*")) (erase-buffer) (buffer-string))"#,
        expect,
    );
    assert_ok_eq("\"\"", &o, &n);
}

// --- insert integers as chars ---

#[test]
fn oracle_insert_integers_as_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ABC\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*ins-ints*")) (erase-buffer) (insert 65 66 67) (buffer-string))"#,
        expect,
    );
    assert_ok_eq("\"ABC\"", &o, &n);
}

// --- buffer-substring-no-properties ---

#[test]
fn oracle_buffer_substring_no_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"bcd\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*bsnp*")) (erase-buffer) (insert "abcdef") (buffer-substring-no-properties 2 5))"#,
        expect,
    );
    assert_ok_eq("\"bcd\"", &o, &n);
}

// --- buffer-size ---

#[test]
fn oracle_buffer_size_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*bsize*")) (erase-buffer) (buffer-size))"#,
        expect,
    );
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_buffer_size_after_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*bsize2*")) (erase-buffer) (insert "hello") (buffer-size))"#,
        expect,
    );
    assert_ok_eq("5", &o, &n);
}

// --- point-min / point-max ---

#[test]
fn oracle_point_min_max_empty_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*pmm*")) (erase-buffer) (list (point-min) (point-max)))"#,
        expect,
    );
    assert_ok_eq("(1 1)", &o, &n);
}

// --- goto-char bounds ---

#[test]
fn oracle_goto_char_beyond_max_goes_to_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    // goto-char clamps to valid range
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*gc*")) (erase-buffer) (insert "abc") (goto-char 100) (point))"#,
        expect,
    );
    assert_ok_eq("4", &o, &n);
}

// --- narrowing ---

#[test]
fn oracle_narrow_to_region_and_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 6 9)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (set-buffer (get-buffer-create "*narrow*")) (erase-buffer) (insert "abcdefgh") (narrow-to-region 3 6) (list (point-min) (point-max) (progn (widen) (point-max))))"#,
        expect,
    );
    assert_ok_eq("(3 6 9)", &o, &n);
}
