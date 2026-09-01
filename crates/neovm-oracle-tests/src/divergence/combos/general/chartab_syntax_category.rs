//! Divergence tests: char-table + syntax + category table combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_char_table_basic_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function char-table-type)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ct (make-char-table 'syntax-table nil)))
    (aset ct ?A 'word)
    (aset ct ?B 'word)
    (aset ct ?+ 'symbol)
    (list (aref ct ?A)
          (eq (aref ct ?A) 'word)
          (aref ct ?B)
          (eq (aref ct ?B) 'word)
          (aref ct ?+)
          (eq (aref ct ?+) 'symbol)
          (aref ct ?Z)
          (null (aref ct ?Z))
          (char-table-p ct)
          (char-table-type ct)))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_table_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (119 t 32 t 40 t 41 t 34 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((st (syntax-table)))
    (list (char-syntax ?a)
          (eq (char-syntax ?a) ?w)
          (char-syntax ? )
          (eq (char-syntax ? ) ? )
          (char-syntax ?()
          (eq (char-syntax ?() ?\()
          (char-syntax ?))
          (eq (char-syntax ?)) ?\))
          (char-syntax ?\")
          (eq (char-syntax ?\") ?\")))) "#,
        expect,
    );
}

#[test]
fn divergence_modify_syntax_entry_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (39 t 95 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((st (copy-syntax-table (syntax-table))))
    (with-syntax-table st
      (modify-syntax-entry ?$ "'")
      (modify-syntax-entry ?% "_")
      (list (char-syntax ?$)
            (eq (char-syntax ?$) ?')
            (char-syntax ?%)
            (eq (char-syntax ?%) ?_))))) "#,
        expect,
    );
}

#[test]
fn divergence_category_table_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Category ‘1’ is already defined\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ct (category-table)))
    (define-category ?1 "test cat 1" ct)
    (define-category ?2 "test cat 2" ct)
    (modify-category-entry ?A ?1 ct)
    (modify-category-entry ?B ?1 ct)
    (modify-category-entry ?B ?2 ct)
    (list (aref (category-table) ?A)
          (memq ?1 (aref (category-table) ?A))
          (memq ?2 (aref (category-table) ?A))
          (null (memq ?2 (aref (category-table) ?A)))
          (aref (category-table) ?B)
          (memq ?1 (aref (category-table) ?B))
          (memq ?2 (aref (category-table) ?B))
          (category-docstring ?1 ct)
          (string= (category-docstring ?1 ct) "test cat 1")))) "#,
        expect,
    );
}

#[test]
fn divergence_char_table_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (wrong-number-of-arguments set-char-table-range 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ct (make-char-table 'test-ct-xxx nil)))
    (set-char-table-range ct ?A ?Z 'letter)
    (set-char-table-range ct ?0 ?9 'digit)
    (list (aref ct ?G)
          (eq (aref ct ?G) 'letter)
          (aref ct ?5)
          (eq (aref ct ?5) 'digit)
          (aref ct ?!)
          (null (aref ct ?!))
          (char-table-range ct ?A)
          (eq (char-table-range ct ?A) 'letter)
          (char-table-range ct ?0)
          (eq (char-table-range ct ?0) 'digit)))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_class_of_various_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (119 t 119 t 62 nil 32 t 95 t 95 nil 60 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (char-syntax ?a)
        (eq (char-syntax ?a) ?w)
        (char-syntax ?0)
        (eq (char-syntax ?0) ?w)
        (char-syntax ?\n)
        (eq (char-syntax ?\n) ? )
        (char-syntax ?\t)
        (eq (char-syntax ?\t) ? )
        (char-syntax ?_)
        (eq (char-syntax ?_) ?_)
        (char-syntax ?:)
        (eq (char-syntax ?:) ?.)
        (char-syntax ?\;)
        (eq (char-syntax ?\;) ?<))) "#,
        expect,
    );
}

#[test]
fn divergence_parse_partial_sexp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(foo (bar baz) quux)")
  (let ((p1 (scan-lists 1 1 0))
        (p2 (scan-lists 1 -1 0)))
    (list p1 p2
          (= p1 19)
          (= p2 1)
          (buffer-string)
          (scan-lists 6 1 0)
          (= (scan-lists 6 1 0) 14)))) "#,
        expect,
    );
}

#[test]
fn divergence_forward_comment_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 14 99 t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert ";; comment 1\ncode\n;; comment 2\nmore code")
  (goto-char 1)
  (let ((moved (forward-comment 1)))
    (let ((p1 (point))
          (c1 (char-after)))
      (forward-line 1)
      (let ((moved2 (forward-comment 1)))
        (list moved p1 c1 moved2
              (or (null c1) (/= c1 59))
              (= (point) 1)
              (>= (point) 1)))))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_text_property_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable p1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "abc-def ghi")
  (let ((st (copy-syntax-table)))
    (with-syntax-table st
      (modify-syntax-entry ?- "w" st)
      (goto-char 1)
      (forward-word 1)
      (let ((p1 (point))
            (w1 (buffer-substring 1 p1)))
        (list p1
              (string= w1 "abc-def")
              (>= p1 7)
              (buffer-substring 1 p1)))))) "#,
        expect,
    );
}

#[test]
fn divergence_char_table_parent_extra_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (from-parent t from-child t nil t #^[nil nil test-ct-parent-xxx #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil from-parent nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] #^^[1 0 #^^[2 0 #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil from-parent nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((parent (make-char-table 'test-ct-parent-xxx nil))
        (child (make-char-table 'test-ct-child-xxx nil)))
    (aset parent ?A 'from-parent)
    (set-char-table-parent child parent)
    (aset child ?B 'from-child)
    (list (aref child ?A)
          (eq (aref child ?A) 'from-parent)
          (aref child ?B)
          (eq (aref child ?B) 'from-child)
          (aref child ?Z)
          (null (aref child ?Z))
          (char-table-parent child)
          (eq (char-table-parent child) parent)))) "#,
        expect,
    );
}
