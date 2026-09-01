//! UTF-8 / multibyte *syntax, movement, columns & char-fold* divergence probes.
//!
//! Probes `char-syntax` over multibyte, `forward-word`/`skip-syntax-forward`
//! word boundaries around non-ASCII, `current-column` display accounting with
//! wide chars, word-boundary regex (`\<`/`\>`), and `char-fold-to-regexp`
//! (Unicode folding tables, separate from the general-category tables).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- char-syntax over multibyte ---------------------------------------------

#[test]
fn div_utf8_char_syntax_non_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 119 119 119 119 119 32 62 95 119)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (char-syntax ?a) (char-syntax ?A) (char-syntax ?1)
      (char-syntax ?é) (char-syntax ?\x3042) (char-syntax ?\x4e2d)
      (char-syntax ?\s) (char-syntax ?\n) (char-syntax ?-)
      (char-syntax ?ß))
"#,
        expect,
    );
}

// --- word movement around multibyte -----------------------------------------

#[test]
fn div_utf8_forward_word_multibyte_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 8 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "café 世界 hello")
  (goto-char 1)
  (list (progn (forward-word 1) (point))
        (progn (forward-word 1) (point))
        (progn (forward-word 1) (point))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_skip_syntax_forward_word_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "café123")
  (skip-syntax-forward "w")
  (point))
"#,
        expect,
    );
}

#[test]
fn div_utf8_backward_word_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "hello 世界 café")
  (goto-char (point-max))
  (list (progn (backward-word 1) (point))
        (progn (backward-word 1) (point))))
"#,
        expect,
    );
}

// --- display column accounting ----------------------------------------------

#[test]
fn div_utf8_current_column_wide_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "a世界b😀")
  (current-column))
"#,
        expect,
    );
}

#[test]
fn div_utf8_indent_to_and_column_with_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 10 \"café\t  \")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "café")
  (end-of-line)
  (list (current-column)
        (progn (move-to-column 10 t) (current-column))
        (buffer-substring (point-min) (point-max))))
"#,
        expect,
    );
}

// --- word-boundary regex ----------------------------------------------------

#[test]
fn div_utf8_word_boundary_regex_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(progn
  (string-match "\\<café\\>" "le café here and cafébar")
  (list (match-beginning 0) (match-end 0)))
"#,
        expect,
    );
}

// --- char-fold (Unicode folding tables) -------------------------------------

#[test]
fn div_utf8_char_fold_to_regexp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 97)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (char-fold-to-regexp ?a)
      (char-fold-to-regexp ?A)
      (length (char-fold-to-regexp ?é))
      (length (char-fold-to-regexp ?ß))
      (length (char-fold-to-regexp ?\x3042)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_char_fold_search_accent_insensitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 101)""#]];
    // char-fold should match café even when searching for cafe (base+combining
    // equivalence) depending on search defaults.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((search-default-mode #'char-fold-to-regexp))
  (list (string-match (char-fold-to-regexp ?e) "café")
        (string-match (char-fold-to-regexp ?é) "cafe")))
"#,
        expect,
    );
}

// --- regex repeat & group over multibyte ------------------------------------

#[test]
fn div_utf8_regex_multibyte_group_and_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer *scratch*> 6 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(progn
  (string-match "\\(世界\\)+"
                "hello 世界 世界 done")
  (list (match-beginning 0) (match-end 0) (match-string 0)
        (match-beginning 1) (match-end 1)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_regex_char_alternation_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (1 2) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (string-match "[éèêë]" "cafe")
      (progn (string-match "[éèêë]" "cëfe")
             (list (match-beginning 0) (match-end 0)))
      (string-match "[一-龥]" "中文字"))
"#,
        expect,
    );
}
