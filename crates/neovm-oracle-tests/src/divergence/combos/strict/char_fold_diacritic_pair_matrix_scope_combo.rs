//! Strict combo oracle probes, batch 328: char-fold diacritic pair matrix —
//! scopes divergence #5 exactly. Tests char-fold search match for each
//! base-letter → precomposed-diacritic pair (a→ä, e→é, i→ï, o→ö, u→ü, y→ÿ,
//! c→ç, n→ñ, plus uppercase variants). The resulting match/no-match vector
//! pinpoints which folds Neomacs's char-fold table is missing.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_char_fold_diacritic_lowercase_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'char-fold)
(let ((pairs '((?a . "ä") (?e . "é") (?i . "ï") (?o . "ö") (?u . "ü")
               (?y . "ÿ") (?c . "ç") (?n . "ñ") (?s . "š") (?z . "ž"))))
  (mapcar (lambda (p)
            (let* ((base (char-to-string (car p)))
                   (accented (cdr p))
                   (re (char-fold-to-regexp base)))
              (with-temp-buffer
                (insert accented)
                (goto-char (point-min))
                (if (re-search-forward re nil t) 'match 'no-match))))
          pairs))
"##;
    let expect = expect_test::expect![[
        r#""OK (match match match match match match match match match match)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_fold_diacritic_uppercase_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'char-fold)
(let ((pairs '((?A . "Ä") (?E . "É") (?I . "Ï") (?O . "Ö") (?U . "Ü")
               (?Y . "Ÿ") (?C . "Ç") (?N . "Ñ"))))
  (mapcar (lambda (p)
            (let* ((base (char-to-string (car p)))
                   (accented (cdr p))
                   (re (char-fold-to-regexp base)))
              (with-temp-buffer
                (insert accented)
                (goto-char (point-min))
                (if (re-search-forward re nil t) 'match 'no-match))))
          pairs))
"##;
    let expect =
        expect_test::expect![[r#""OK (match match match match match match match match)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_fold_combining_mark_decompose_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'char-fold)
(let ((combining-pairs '((?a . "ä") (?e . "é") (?o . "ö")
                         (?u . "ü") (?n . "ñ") (?c . "ç"))))
  (mapcar (lambda (p)
            (let* ((base (char-to-string (car p)))
                   (re (char-fold-to-regexp base)))
              (with-temp-buffer
                (insert (cdr p))
                (goto-char (point-min))
                (if (re-search-forward re nil t) 'match 'no-match))))
          combining-pairs))
"##;
    let expect = expect_test::expect![[r#""OK (match match match match match match)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
