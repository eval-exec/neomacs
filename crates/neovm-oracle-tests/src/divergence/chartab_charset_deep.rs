//! Divergence tests: map-char-table, char-table-parent, set-char-table-parent.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_char_table_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (word #^[nil nil syntax-table #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil word nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] #^^[1 0 #^^[2 0 #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil word nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((parent (make-char-table 'syntax-table nil))
        (child (make-char-table 'syntax-table nil)))
  (set-char-table-parent child parent)
  (aset parent ?a 'word)
  (list (aref child ?a)
        (char-table-parent child)
        (eq (char-table-parent child) parent)))"#,
        expect,
    );
}

#[test]
fn divergence_map_char_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (setting-constant nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ct (make-char-table 'syntax-table nil))
        ranges nil)
  (aset ct ?A 'word)
  (aset ct ?B 'word)
  (aset ct ?C 'word)
  (map-char-table (lambda (range val)
                    (push (list range val) ranges)) ct)
  ranges)"#,
        expect,
    );
}

#[test]
fn divergence_char_table_extra_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range #^[nil nil syntax-table nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ct (make-char-table 'syntax-table)))
  (list (char-table-extra-slot ct 0)
        (char-table-extra-slot ct 1)
        (set-char-table-extra-slot ct 0 'test)
        (char-table-extra-slot ct 0)))"#,
        expect,
    );
}

#[test]
fn divergence_char_table_subtype() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function char-table-type)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ct (make-char-table 'category-table)))
  (list (char-table-p ct)
        (char-table-type ct)
        (eq (char-table-type ct) 'category-table)))"#,
        expect,
    );
}

#[test]
fn divergence_unify_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'unify-charset)
  (fboundp 'charset-info)
  (fboundp 'decode-char)
  (fboundp 'encode-char)
  (= (decode-char 'ascii 65) ?A))"#,
        expect,
    );
}

#[test]
fn divergence_charset_dimension() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function charset-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (charset-dimension 'ascii)
  (charset-dimension 'unicode)
  (charset-width 'ascii)
  (charset-width 'unicode))"#,
        expect,
    );
}

#[test]
fn divergence_charset_reverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (65 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((code (encode-char ?A 'ascii)))
  (list code
        (= code 65)
        (= (decode-char 'ascii code) ?A)))"#,
        expect,
    );
}

#[test]
fn divergence_modify_category_entry_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Undefined category: x\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ct (standard-category-table)))
    (modify-category-entry ?X ?x ct)
    (list (category-set-mnemonics (aref ct ?X))
          (category-table-p ct))))"#,
        expect,
    );
}

#[test]
fn divergence_category_table_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ct (standard-category-table)))
  (list (char-table-parent ct)
        (category-table-p ct)
        (null (char-table-parent ct))))"#,
        expect,
    );
}

#[test]
fn divergence_define_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'define-category)
  (fboundp 'category-docstring)
  (stringp (category-docstring ?a)))"#,
        expect,
    );
}
