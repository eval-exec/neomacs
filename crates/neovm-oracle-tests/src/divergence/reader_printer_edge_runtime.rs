//! Reader edge (char escapes, unicode names, uninterned/empty symbols,
//! #s records, dotted/improper lists, radix edges, +/-/.N float forms) and
//! printer edge (escape controls/nonascii, gensym/quoted, bool-vector,
//! record, special floats, nested vectors); plus the backslash-space
//! string-continuation divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn read_char_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 134217825 9 127 27 0 32 10 92 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "?\\C-a") (read "?\\M-a") (read "?\\^I") (read "?\\d") (read "?\\e")
        (read "?\\0") (read "?\\s") (read "?\\n") (read "?\\\\") (read "?\\t"))"##,
        expect,
    );
}

#[test]
fn read_dotted_circular() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((1 . 2) (1 2 . 3) invalid-read-syntax (a b c . d))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "(1 . 2)") (read "(1 2 . 3)")
        (condition-case e (read "(1 . 2 . 3)") (error (car e)))
        (read "(a b c . d)"))"##,
        expect,
    );
}

#[test]
fn read_float_forms2() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 0.5 -0.5 5.0 1500.0 50.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "+5") (read "+.5") (read "-.5") (read "5e0") (read "1.5E3") (read ".5e2"))"##,
        expect,
    );
}

#[test]
fn read_radix_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (-31 -5 -15 255 15 1500000000000000000000)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "#x-1F") (read "#b-101") (read "#o-17")
        (read "#16rFF") (read "#2r1111") (read "1500000000000000000000"))"##,
        expect,
    );
}

#[test]
fn read_record_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t eq)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((r (read "#s(hash-table size 5 test eq)")))
  (list (hash-table-p r) (hash-table-test r)))"##,
        expect,
    );
}

#[test]
fn read_unicode_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (97 128512 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "?\\N{LATIN SMALL LETTER A}") (read "?\\N{U+1F600}")
        (length (read "\"\\N{BULLET}\"")))"##,
        expect,
    );
}

#[test]
fn read_uninterned_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t bar t \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((hh (make-string 2 ?#)))
  (list (symbolp (read "#:foo")) (intern-soft (symbol-name (read "#:bar")))
        (eq (read hh) (read hh)) (symbol-name (read hh))))"##,
        expect,
    );
}

#[test]
fn print_bool_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""OK (\"#&5\\\"\u{15}\\\"\" #&5\"\u{15}\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((bv (make-bool-vector 5 nil)))
  (aset bv 0 t) (aset bv 2 t) (aset bv 4 t)
  (list (prin1-to-string bv) (read (prin1-to-string bv))))"##,
        expect,
    );
}

#[test]
fn print_char_table_record() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""OK (\"#s(foo 1 \\\"two\\\" (3))\" t)""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (prin1-to-string (record 'foo 1 "two" '(3)))
        (recordp (read (prin1-to-string (record 'bar 9)))))"##,
        expect,
    );
}

#[test]
fn print_escape_controls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"\\\"a\\\\nb\\\\11c\\\"\" \"\\\"x\\\\0y\\\"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((print-escape-newlines t) (print-escape-control-characters t))
  (list (prin1-to-string "a\nb\tc") (prin1-to-string "x\0y")))"##,
        expect,
    );
}

#[test]
fn print_escape_nonascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"\\\"café\\\"\" \"\\\"λ\\\"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((print-escape-nonascii t))
  (list (prin1-to-string "café") (prin1-to-string "λ")))"##,
        expect,
    );
}

#[test]
fn print_gensym_quoted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""OK (\"#:g\" \"'(a b)\" \"#'fn\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((print-gensym t) (print-quoted t))
  (list (prin1-to-string (make-symbol "g")) (prin1-to-string ''(a b))
        (prin1-to-string '#'fn)))"##,
        expect,
    );
}

#[test]
fn print_special_floats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"1.0e+INF\" \"-1.0e+INF\" \"0.0e+NaN\" \"-0.0\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (prin1-to-string 1.0e+INF) (prin1-to-string -1.0e+INF)
        (prin1-to-string 0.0e+NaN) (prin1-to-string -0.0))"##,
        expect,
    );
}

#[test]
fn print_vector_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Invalid escape char syntax: \\\\x not followed by hex digit\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(prin1-to-string (vector 1 "two" '(3 . 4) [5 6] ?\x))"##,
        expect,
    );
}

#[test]
fn divergence_read_string_backslash_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ab\" 2 \"xy\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "\"a\\ b\"") (length (read "\"a\\ b\"")) (read "\"x\\ \\ y\""))"##,
        expect,
    );
}
