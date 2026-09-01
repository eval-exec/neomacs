//! Category table (define-category, modify-category-entry, char-category-set,
//! category-set-mnemonics), translation tables (make-translation-table /
//! -from-alist + translate-region), and char-table parent chains + range
//! inheritance + subtype parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn category_define_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"my test category\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((tbl (copy-category-table)))
    (define-category ?z "my test category" tbl)
    (with-current-buffer (current-buffer)
      (set-category-table tbl)
      (modify-category-entry ?a ?z)
      (list (category-docstring ?z)
            (aref (char-category-set ?a) ?z)))))"##,
        expect,
    );
}

#[test]
fn category_set_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\".Lalr\" \".HLchj|\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (list (category-set-mnemonics (char-category-set ?A))
        (category-set-mnemonics (char-category-set ?あ))))"##,
        expect,
    );
}

#[test]
fn char_table_parent_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (grandparent parent nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((gp (make-char-table 'test)) (p (make-char-table 'test)) (c (make-char-table 'test)))
  (aset gp ?a 'grandparent)
  (set-char-table-parent p gp)
  (set-char-table-parent c p)
  (aset p ?b 'parent)
  (list (aref c ?a) (aref c ?b) (aref c ?c)))"##,
        expect,
    );
}

#[test]
fn char_table_range_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (lower def lower)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-char-table 'test 'def)) (c (make-char-table 'test)))
  (set-char-table-range p '(?a . ?z) 'lower)
  (set-char-table-parent c p)
  (list (aref c ?m) (aref c ?A) (char-table-range c ?m)))"##,
        expect,
    );
}

#[test]
fn make_char_table_subtype() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (case-table t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ct (make-char-table 'case-table)))
  (list (char-table-subtype ct) (char-table-p ct)))"##,
        expect,
    );
}

#[test]
fn standard_category_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function current-category-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (category-table-p (standard-category-table))
        (category-table-p (current-category-table))
        (char-table-p (make-category-table)))"##,
        expect,
    );
}

#[test]
fn translation_from_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"XYcXYc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((tt (make-translation-table-from-alist '((?a . ?X) (?b . ?Y)))))
  (with-temp-buffer
    (insert "abcabc")
    (translate-region (point-min) (point-max) tt)
    (buffer-string)))"##,
        expect,
    );
}

#[test]
fn translation_from_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp [65 66 67])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((tt (make-translation-table (vector ?A ?B ?C))))
  (with-temp-buffer
    (insert (string 0 1 2 3))
    (translate-region (point-min) (point-max) tt)
    (mapcar #'identity (buffer-string))))"##,
        expect,
    );
}
