//! Divergence tests: case-table, translation-table, character-category deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_case_table_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (t 97 97 122)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((ct (current-case-table)))
  (list (char-table-p ct)
        (aref ct ?A)
        (aref ct ?a)
        (aref ct ?z)))"#, expect);
}

#[test]
fn divergence_case_table_set_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK t""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((ct (copy-case-table (current-case-table))))
  (set-case-table ct)
  (list (eq (downcase ?A) ?a)
        (eq (upcase ?a) ?A)
        (eq (capitalize "hello world") "Hello World"))
  (set-case-table (standard-case-table))
  (eq (downcase ?A) ?a))"#, expect);
}

#[test]
fn divergence_with_case_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\\\"\" 4 38)""##]];
crate::common::assert_oracle_parity_expect(
        r#"(with-case-table (standard-case-table)
  (list (downcase "HELLO")
        (upcase "hello")
        (capitalize "hello world")))#" ,
    );
}

#[test]
fn divergence_translation_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    assert_oracle_parity(
        r#"(list
  (fboundp 'make-translation-table)
  (fboundp 'make-translation-table-from-vector)
  (fboundp 'translate-region))"#, expect);
}

#[test]
fn divergence_char_category_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (t \".Lalr\" \".al\" \".6alr\")""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((ct (standard-category-table)))
  (list (category-table-p ct)
        (category-set-mnemonics (aref ct ?a))
        (category-set-mnemonics (aref ct ? ))
        (category-set-mnemonics (aref ct ?0))))"#, expect);
}

#[test]
fn divergence_modify_category_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (error \"Undefined category: x\")""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (modify-category-entry ?X ?x)
  (let ((cs (aref (standard-category-table) ?X)))
    (list (if cs (category-set-mnemonics cs) nil))))"#, expect);
}

#[test]
fn divergence_string_search_case_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (0 4 4 9)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((case-fold-search t))
  (list (string-match "hello" "HELLO WORLD")
        (string-match "hello" "Say HELLO")
        (match-beginning 0)
        (match-end 0)))"#, expect);
}

#[test]
fn divergence_string_search_no_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (nil 0 nil)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((case-fold-search nil))
  (list (string-match "hello" "HELLO WORLD")
        (string-match "hello" "hello world")
        (string-match "HELLO" "hello world")))"#, expect);
}

#[test]
fn divergence_char_inspect_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-function char-name)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (char-name ? )
  (char-name ?\n)
  (get-char-property ?A 'name)
  (stringp (char-name ?\x01)))"#, expect);
}

#[test]
fn divergence_char_general_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (Lu Ll Nd Zs Cc)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (get-char-code-property ?A 'general-category)
  (get-char-code-property ?a 'general-category)
  (get-char-code-property ?0 'general-category)
  (get-char-code-property ?  'general-category)
  (get-char-code-property ?\n 'general-category))"#, expect);
}
