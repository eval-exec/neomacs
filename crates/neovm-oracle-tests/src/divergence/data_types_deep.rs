//! Divergence tests: lisp data types edge - bool-vector, char-table, record.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_bool_vector_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-count-matches)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((a (make-bool-vector 16 t))
        (b (make-bool-vector 16 nil)))
  (aset b 0 t)
  (aset b 5 t)
  (list (bool-vector-count-matches a t)
        (bool-vector-count-matches b t)
        (bool-vector-count-matches a nil)
        (bool-vector-count-matches b nil)
        (aref a 0)
        (aref b 0)
        (aref a 1)
        (aref b 1)))"#,
        expect,
    );
}

#[test]
fn divergence_bool_vector_union_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (#&8\"\u{1f}\" #&8\"\u{4}\" #&8\"\u{11}\" #&8\"\\352\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((a (make-bool-vector 8 nil))
        (b (make-bool-vector 8 nil)))
  (aset a 0 t) (aset a 2 t) (aset a 4 t)
  (aset b 1 t) (aset b 2 t) (aset b 3 t)
  (list (bool-vector-union a b)
        (bool-vector-intersection a b)
        (bool-vector-set-difference a b)
        (bool-vector-not a)))"#,
        expect,
    );
}

#[test]
fn divergence_char_table_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function char-table-default)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ct (make-char-table 'syntax-table 'default-val)))
  (list (char-table-default ct)
        (aref ct ?A)
        (aref ct ?a)
        (set-char-table-default ct 'new-default)
        (char-table-default ct)
        (aref ct ?z)))"#,
        expect,
    );
}

#[test]
fn divergence_record_vs_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil t 3 4 1 tag 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((v [1 2 3])
        (r (record 'tag 1 2 3)))
  (list (vectorp v)
        (vectorp r)
        (recordp v)
        (recordp r)
        (length v)
        (length r)
        (aref v 0)
        (aref r 0)
        (aref r 1)))"#,
        expect,
    );
}

#[test]
fn divergence_record_type_descriptor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'make-record-type)
  (fboundp 'record-type-name)
  (fboundp 'record-type-fields)
  (fboundp 'record-type-p))"#,
        expect,
    );
}

#[test]
fn divergence_string_byte_vs_char_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 6 72 233 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s "Héllo"))
  (list (length s)
        (string-bytes s)
        (aref s 0)
        (aref s 1)
        (= (length s) 5)
        (= (string-bytes s) 6)))"#,
        expect,
    );
}

#[test]
fn divergence_string_eq_vs_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s1 "hello")
        (s2 "hello"))
  (list (eq s1 s2)
        (equal s1 s2)
        (string= s1 s2)
        (string-equal s1 s2)))"#,
        expect,
    );
}

#[test]
fn divergence_multibyte_string_char_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 12 t t \"中文\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s "中文测试"))
  (list (length s)
        (string-bytes s)
        (= (aref s 0) ?中)
        (= (aref s 1) ?文)
        (substring s 0 2)))"#,
        expect,
    );
}

#[test]
fn divergence_unibyte_string_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 3 3 97)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((us (string ?a ?b ?c)))
  (list (multibyte-string-p us)
        (length us)
        (string-bytes us)
        (aref us 0)))"#,
        expect,
    );
}

#[test]
fn divergence_string_as_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil \"abc\" \"abc\" \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ms "abc"))
  (list (multibyte-string-p ms)
        (string-as-unibyte ms)
        (string-as-unibyte "abc")
        (string-to-multibyte "abc"))) "#,
        expect,
    );
}
