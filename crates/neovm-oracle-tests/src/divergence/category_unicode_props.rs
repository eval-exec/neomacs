//! Divergence tests: category table, category set operations deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_category_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'category-table)
  (fboundp 'category-table-p)
  (fboundp 'make-category-table)
  (fboundp 'set-category-table)
  (category-table-p (category-table))) "#,
        expect,
    );
}

#[test]
fn divergence_category_define() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'define-category)
  (fboundp 'category-docstring)
  (fboundp 'category-set-mnemonics)
  (fboundp 'modify-category-entry))"#,
        expect,
    );
}

#[test]
fn divergence_char_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'char-category-set)
  (fboundp 'category-set-mnemonics)
  (fboundp 'modify-category-entry)) "#,
        expect,
    );
}

#[test]
fn divergence_category_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'copy-category-table)
  (fboundp 'category-table-parent)
  (fboundp 'set-category-table-parent)
  (fboundp 'merge-category-table)) "#,
        expect,
    );
}

#[test]
fn divergence_category_table_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'standard-category-table)
  (category-table-p (standard-category-table))
  (eq (category-table) (standard-category-table))) "#,
        expect,
    );
}

#[test]
fn divergence_unicode_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (Lu Ll Nd Zs)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (get-char-code-property ?A 'general-category)
  (get-char-code-property ?a 'general-category)
  (get-char-code-property ?0 'general-category)
  (get-char-code-property ?  'general-category)) "#,
        expect,
    );
}

#[test]
fn divergence_char_script() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'char-script-table)
  (fboundp 'script-representative-chars)
  (fboundp 'char-symbols)) "#,
        expect,
    );
}

#[test]
fn divergence_char_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'get-char-code-property)
  (fboundp 'char-code-property-description)
  (stringp (char-code-property-description ?A 'name))) "#,
        expect,
    );
}

#[test]
fn divergence_unicode_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function char-code-property-alist)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'char-code-property-alist)
  (listp (char-code-property-alist))
  (member 'name (char-code-property-alist))
  (member 'general-category (char-code-property-alist))) "#,
        expect,
    );
}

#[test]
fn divergence_unicode_combining() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (get-char-code-property ?A 'canonical-combining-class)
  (get-char-code-property ?a 'canonical-combining-class)
  (fboundp 'unicode-property-table-internal)) "#,
        expect,
    );
}
