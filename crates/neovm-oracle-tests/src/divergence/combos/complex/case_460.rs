/// Batch 460: input-method, quail, charset, category, case-table deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx460_input_method_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function current-input-method)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (current-input-method)
      (input-method-name)
      (input-method-after-insert-chunk-hook))"##,
        expect,
    );
}

#[test]
fn div_cx460_quail_define_package() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'quail)
  (list (fboundp 'quail-define-package)
        (fboundp 'quail-define-rules)))"##,
        expect,
    );
}

#[test]
fn div_cx460_charset_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (ascii ascii)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (list (charset-after 1) (charset-after 2)))"##,
        expect,
    );
}

#[test]
fn div_cx460_category_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments char-category-set 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ct (copy-category-table)))
  (define-category ?x "test" ct)
  (modify-category-entry ?a ?x ct)
  (list (char-category-set ?a ct)
        (char-category-set ?b ct)))"##,
        expect,
    );
}

#[test]
fn div_cx460_case_table_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ct (copy-case-table)))
  (list (case-table-p ct)
        (char-table-p ct)))"##,
        expect,
    );
}

#[test]
fn div_cx460_case_table_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ct (copy-case-table)))
  (list (aref ct ?a) (aref ct ?A)))"##,
        expect,
    );
}

#[test]
fn div_cx460_char_table_prototype() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-table-prototype)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ct (make-char-table 'syntax-table ?w)))
  (list (char-table-prototype ct)
        (aref ct 0)))"##,
        expect,
    );
}

#[test]
fn div_cx460_char_table_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#^[119 nil syntax-table 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119] 120)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((parent (make-char-table 'syntax-table ?w))
      (child (make-char-table 'syntax-table ?x)))
  (set-char-table-parent child parent)
  (list (char-table-parent child)
        (aref child ?a)))"##,
        expect,
    );
}

#[test]
fn div_cx460_syntax_table_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((st (make-syntax-table (syntax-table))))
  (list (char-table-p st)
        (syntax-table-p st)))"##,
        expect,
    );
}

#[test]
fn div_cx460_string_to_syntax_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((14) (14 . 98))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-to-syntax "!" ) (string-to-syntax "!b"))"##,
        expect,
    );
}

#[test]
fn div_cx460_syntax_class_to_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 32""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (syntax-class-to-char 0)
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx460_unibyte_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"ABC\" \"��\" 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (unibyte-string 65 66 67)
      (unibyte-string 200 201)
      (length (unibyte-string 128 129)))"##,
        expect,
    );
}

#[test]
fn div_cx460_multibyte_string_p_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s1 "abc")
      (s2 "cafe")
      (s3 (unibyte-string 65 66)))
  (list (multibyte-string-p s1)
        (multibyte-string-p s2)
        (multibyte-string-p s3)))"##,
        expect,
    );
}

#[test]
fn div_cx460_string_bytes_vs_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "cafe世界"))
  (list (string-bytes s) (length s)))"##,
        expect,
    );
}

#[test]
fn div_cx460_warehouse_string_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"aaaaa\" \"hello\" \"world\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (make-string 5 ?a)
      (string ?h ?e ?l ?l ?o)
      (concat (string ?w ?o) (string ?r ?l ?d)))"##,
        expect,
    );
}
