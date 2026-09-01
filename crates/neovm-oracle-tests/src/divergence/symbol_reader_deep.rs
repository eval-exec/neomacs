//! Divergence tests: symbol property, obarray, and Lisp reader edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_symbol_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((a 1 b 2 c 3 d 4) 1 2 nil 4 4 (a 1 b 2 c 3 d 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setplist 'my-sym-plist '(a 1 b 2 c 3))
  (list (symbol-plist 'my-sym-plist)
        (get 'my-sym-plist 'a)
        (get 'my-sym-plist 'b)
        (get 'my-sym-plist 'z)
        (put 'my-sym-plist 'd 4)
        (get 'my-sym-plist 'd)
        (symbol-plist 'my-sym-plist)))"#,
        expect,
    );
}

#[test]
fn divergence_symbol_function_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((closure (t) (x) x) \"docstring\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-sym-test-fn (x) "docstring" x)
  (list (symbol-function 'my-sym-test-fn)
        (documentation 'my-sym-test-fn)
        (function-get 'my-sym-test-fn 'defalias-for)))"#,
        expect,
    );
}

#[test]
fn divergence_obarray_iter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ob (make-obarray 127)))
  (intern "alpha" ob)
  (intern "beta" ob)
  (intern "gamma" ob)
  (list (intern-soft "alpha" ob)
        (intern-soft "beta" ob)
        (intern-soft "delta" ob)
        (unintern "beta" ob)
        (intern-soft "beta" ob)))"#,
        expect,
    );
}

#[test]
fn divergence_mapatoms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ob (make-obarray 127))
        syms)
  (intern "foo" ob)
  (intern "bar" ob)
  (intern "baz" ob)
  (mapatoms (lambda (s) (push (symbol-name s) syms)) ob)
  (sort syms #'string<))"#,
        expect,
    );
}

#[test]
fn divergence_read_special_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (read-from-string "?\\C-a")
  (read-from-string "?\\M-a")
  (read-from-string "?\\C-M-a")
  (read-from-string "?\\\\")
  (read-from-string "?\\n"))"#,
        expect,
    );
}

#[test]
fn divergence_read_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"hello world\" . 13) (\"hello \\\"world\\\"\" . 17) (\"a\\nb\" . 6) (\"\t\" . 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (read-from-string "\"hello world\"")
  (read-from-string "\"hello \\\"world\\\"\"")
  (read-from-string "\"a\\nb\"")
  (read-from-string "\"\\t\""))"#,
        expect,
    );
}

#[test]
fn divergence_read_list_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((a b c) . 7) ((a . b) . 7) ((a b . c) . 9) (nil . 3) (nil . 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (read-from-string "(a b c)")
  (read-from-string "(a . b)")
  (read-from-string "(a b . c)")
  (read-from-string "nil")
  (read-from-string "()"))"#,
        expect,
    );
}

#[test]
fn divergence_read_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (([1 2 3] . 7) ([a \\?b c] . 9) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (read-from-string "[1 2 3]")
  (read-from-string "[a \\?b c]")
  (vectorp (read-from-string "[1 2 3]")))"#,
        expect,
    );
}

#[test]
fn divergence_read_char_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 1 2 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ct (make-char-table 'foo)))
  (aset ct ?a 1)
  (aset ct ?b 2)
  (list (char-table-p ct)
        (aref ct ?a)
        (aref ct ?b)
        (aref ct ?z)))"#,
        expect,
    );
}

#[test]
fn divergence_read_bool_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-count-matches)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((bv (make-bool-vector 10 t)))
  (aset bv 3 nil)
  (list (bool-vector-p bv)
        (aref bv 0)
        (aref bv 3)
        (bool-vector-count-matches bv t)
        (bool-vector-count-matches bv nil)))"#,
        expect,
    );
}
