//! Strict combo oracle probes, batch 149: char/syntax table-driven traversal.
//! skip-chars-forward / skip-chars-backward over char sets incl ranges and
//! negation, skip-syntax-forward over syntax classes, char-fold-to-regexp
//! folding (with JIS-like equivalence for ASCII), regexp char classes
//! [[:alpha:]] [[:alnum:]] [[:space:]] [[:punct:]], and char-category table
//! probes.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_skip_chars_forward_backward_ranges_negation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "abcXYZ123-_. \t\ndef")
  (goto-char 1)
  (let ((s1 (progn (skip-chars-forward "a-z") (point)))
        (s2 (progn (skip-chars-forward "A-Z") (point)))
        (s3 (progn (skip-chars-forward "0-9") (point)))
        (s4 (progn (skip-chars-forward "-_.") (point)))
        (s5 (progn (skip-chars-forward " \t") (point)))
        (back (progn (skip-chars-backward "^a-z") (point))))
    (list s1 s2 s3 s4 s5 back)))
"##;
    let expect = expect_test::expect![[r#""OK (4 7 10 13 15 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_skip_syntax_forward_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "symbols99 ;; a comment\n   \"string\" (parens)")
  (goto-char 1)
  (let ((w (progn (skip-syntax-forward "w_") (point)))
        (ws1 (progn (skip-syntax-forward " ") (point)))
        (cmt-start (point))
        (in-cmt (progn (skip-syntax-forward "<") (point)))
        (after-cmt (progn (skip-syntax-forward ".") (point))))
    (list w ws1 in-cmt after-cmt
          (save-excursion (goto-char 1) (skip-syntax-forward "^ ") (point)))))
"##;
    let expect = expect_test::expect![[r#""OK (10 11 11 13 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_regexp_char_classes_alpha_alnum_space_punct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((s "Hello, World! 123\tabc"))
  (list (string-match "[[:alpha:]]+" s)
        (match-string 0 s)
        (string-match "[[:alnum:]]+" s 7)
        (match-string 0 s)
        (string-match "[[:space:]]+" s)
        (match-string 0 s)
        (string-match "[[:punct:]]+" s)
        (match-string 0 s)
        (string-match "[^[:alpha:][:space:]]+" s)
        (match-string 0 s)))
"##;
    let expect =
        expect_test::expect![[r#""OK (0 \"Hello\" 7 \"World\" 6 \" \" 5 \",\" 5 \",\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_fold_to_regexp_ascii_equiv() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((s "café naïve résumé"))
  (list (char-fold-to-regexp ?a)
        (length (char-fold-to-regexp ?e))
        ;; char-fold search should match base letters ignoring diacritics
        (save-excursion
          (goto-char (point-min))
          (with-temp-buffer
            (insert s)
            (goto-char (point-min))
            (let ((re (char-fold-to-regexp "cafe")))
              (re-search-forward re nil t))))
        ;; nil-safe: char-fold-to-regexp of plain ASCII letter
        (regexp-quote (char-fold-to-regexp "naive"))))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 97)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_category_table_and_syntax_class_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((ct (standard-category-table)))
  (list (char-category-set ?a)
        (char-category-set ?1)
        (char-category-set ?  )
        (aref (category-table) ?a)
        (modify-category-entry ?X ?q)
        (aref (category-table) ?X)
        (char-syntax ?a)
        (char-syntax ? )
        (char-syntax ?()
        (char-syntax ?\")
        (char-syntax ?;)))
"##;
    let expect = expect_test::expect![[
        r#""OK (#&128\"\\0\\0\\0\\0\\0@\\0\\0\\0\u{10}\\0\\0\u{2}\u{10}\u{4}\\0\" #&128\"\\0\\0\\0\\0\\0@@\\0\\0\\0\\0\\0\u{2}\u{10}\u{4}\\0\" #&128\"\\0\\0\\0\\0\\0@\\0\\0\\0\\0\\0\\0\u{2}\u{10}\\0\\0\" #&128\"\\0\\0\\0\\0\\0@\\0\\0\\0\u{10}\\0\\0\u{2}\u{10}\u{4}\\0\" nil #&128\"\\0\\0\\0\\0\\0@\\0\\0\\0\u{10}\\0\\0\u{2}\u{10}\u{6}\\0\" 119 32 40 34 60)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
