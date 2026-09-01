//! Source-audit divergences: casefiddle / casetab / category / syntax / width.
//!
//! From a direct GNU src vs neovm-core Rust audit: case operations ignore the
//! installed case table (hardcoded Rust mapping), word boundaries use
//! is_alphanumeric() not the syntax table, char-width table mutations are
//! ignored, and several special-case mappings differ (ß, İ, Greek final sigma).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_ac_case_table_ignored_for_downcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    // Buffer-local case table mapping A→x; GNU reads it, Neomacs ignores it.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (copy-case-table)))
  (set-char-table-range ct ?A ?x)
  (set-case-table ct)
  (downcase "A"))
"##,
        expect,
    );
}

#[test]
fn div_ac_case_table_ignored_for_upcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (copy-case-table)))
  (set-char-table-range ct ?a ?X)
  (set-case-table ct)
  (upcase "a"))
"##,
        expect,
    );
}

#[test]
fn div_ac_upcase_sharp_s_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7838""#]];
    // char-upcase of ß: GNU returns ß unchanged (SS is multi-char),
    // Neomacs maps ß→ẞ (7838).
    crate::common::assert_oracle_parity_expect(r##"(upcase ?ß)"##, expect);
}

#[test]
fn div_ac_downcase_dotted_I_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 304""#]];
    // downcase of İ (U+0130): GNU unchanged (one-to-many), Neomacs → i (105).
    crate::common::assert_oracle_parity_expect(r##"(downcase ?İ)"##, expect);
}

#[test]
fn div_ac_greek_final_sigma_downcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ας\"""#]];
    // Σ at end of word → ς (final sigma) in GNU; Neomacs → σ always.
    crate::common::assert_oracle_parity_expect(r##"(downcase "ΑΣ")"##, expect);
}

#[test]
fn div_ac_upcase_strasse_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"STRASSE\"""#]];
    crate::common::assert_oracle_parity_expect(r##"(upcase "straße")"##, expect);
}

#[test]
fn div_ac_with_case_table_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-number-of-arguments""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-case-table (let ((ct (copy-case-table)))
                       (set-char-table-range ct ?a ?B) ct)
      (downcase "a"))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_ac_case_symbols_as_words() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Foo_bar Baz\"""#]];
    // GNU: _ is word-constituent with case-symbols-as-words -> foo_bar one word.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-symbols-as-words t))
  (capitalize "foo_bar baz"))
"##,
        expect,
    );
}

#[test]
fn div_ac_forward_sexp_syntax_text_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    // Override syntax of "(" to word-constituent via text property.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ab(cd)ef")
  (put-text-property 3 4 'syntax-table (string-to-syntax "w"))
  (goto-char 1)
  (let ((parse-sexp-lookup-properties t))
    (forward-sexp 1)
    (point)))
"##,
        expect,
    );
}

#[test]
fn div_ac_char_width_table_mutation_ignored() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-width-table)""#]];
    // GNU consults the (mutable) char-width-table; Neomacs hardcodes width.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((cw (char-width ?\x300)))
  (set-char-table-range (char-width-table) ?\x300 1)
  (list cw (char-width ?\x300)))
"##,
        expect,
    );
}

#[test]
fn div_ac_display_table_string_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument arrayp nil)""#]];
    // string-width should honor buffer-display-table glyph replacement.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (setq buffer-display-table (make-display-table))
  (aset (char-table-extra-slot buffer-display-table 0) ?a (vector ?X ?Y))
  (string-width "a"))
"##,
        expect,
    );
}

#[test]
fn div_ac_standard_category_docstring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Latin\" \"ASCII\\nASCII graphic characters 32-126 (ISO646 IRV:1983[4/0])\" \"Roman\\nJapanese roman\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (category-docstring ?l (standard-category-table))
      (category-docstring ?a (standard-category-table))
      (category-docstring ?r (standard-category-table)))
"##,
        expect,
    );
}

#[test]
fn div_ac_make_category_set_uppercase_letter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    // Category letter "A" -> bit position; uppercase letters map to bits 27-52.
    crate::common::assert_oracle_parity_expect(r##"(aref (make-category-set "A") 28)"##, expect);
}

#[test]
fn div_ac_char_width_display_property_in_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1)""#]];
    // char-width text property / display glyph affecting column accounting.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "x")
  (put-text-property 1 2 'display (vector ?a ?b ?c))
  (list (current-column) (string-width (buffer-substring 1 2))))
"##,
        expect,
    );
}
