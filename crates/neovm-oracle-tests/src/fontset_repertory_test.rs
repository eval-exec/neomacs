//! Fontset query metadata remains separate from native rendering coverage.

use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_fontset_family_only_registry_preserves_query_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((font-encoding-alist '(("-iso10646-1\\'" . unicode)))
      (family "Issue514 Family Only"))
  (set-fontset-font t '(#x80 . #x10ffff) nil)
  (set-fontset-font t nil nil)
  (let ((absent (font-spec :family family))
        (explicit (font-spec :family family :registry "iso10646-1")))
    (set-fontset-font t '(#x4e00 . #x9fff) absent)
    (let ((family-only (list (font-get absent :family)
                             (font-get absent :registry)
                             (fontset-font t #x4e2d)
                             (fontset-font t #x4e2d t))))
      (set-fontset-font t '(#x4e00 . #x9fff) explicit)
      (list family-only
            (list (font-get explicit :family)
                  (font-get explicit :registry)
                  (fontset-font t #x4e2d)
                  (fontset-font t #x4e2d t))))))
"#;
    let expected = expect_test::expect![[
        r#""OK ((Issue514\\ Family\\ Only nil nil nil) (Issue514\\ Family\\ Only iso10646-1 (\"Issue514 Family Only\" . \"iso10646-1\") nil))""#
    ]];
    assert_oracle_parity_expect(form, expected);
}

#[test]
fn oracle_fontset_encoding_and_repertory_have_distinct_query_roles() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((family "Issue514 Repertory")
      (repertory (make-char-table nil nil))
      (results nil))
  (aset repertory #x4e2d t)
  (set-fontset-font t '(#x80 . #x10ffff) nil)
  (set-fontset-font t nil nil)
  (dolist (encoding (list 'unicode
                         (cons 'ascii 'unicode)
                         (cons 'unicode 'ascii)
                         (cons 'unicode repertory)
                         (cons 'ascii repertory)))
    (let ((font-encoding-alist
           (list (cons "\\`Issue514 Repertory-\\'" encoding))))
      (set-fontset-font t '(#x4e00 . #x9fff) (font-spec :family family))
      (setq results
            (cons (list (fontset-font t #x4e2d)
                        (fontset-font t #x65e5)
                        (fontset-font t #x4e2d t))
                  results))))
  (nreverse results))
"#;
    let expected = expect_test::expect![[
        r#""OK (((\"Issue514 Repertory\") (\"Issue514 Repertory\") nil) (nil nil nil) ((\"Issue514 Repertory\") (\"Issue514 Repertory\") nil) ((\"Issue514 Repertory\") (\"Issue514 Repertory\") nil) (nil nil nil))""#
    ]];
    assert_oracle_parity_expect(form, expected);
}

#[test]
fn oracle_fontset_range_script_and_charset_targets_preserve_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((font-encoding-alist '(("-iso10646-1\\'" . unicode)))
      (results nil))
  (dolist (target (list '(#x4e00 . #x4eff) 'han 'japanese-jisx0208))
    (set-fontset-font t '(#x80 . #x10ffff) nil)
    (set-fontset-font t nil nil)
    (set-fontset-font t target
                      (font-spec :family "Issue514 Target" :registry "iso10646-1"))
    (setq results
          (cons (list target
                      (fontset-font t #x4e2d)
                      (fontset-font t #x65e5)
                      (fontset-font t #x1f600))
                results)))
  (nreverse results))
"#;
    let expected = expect_test::expect![[
        r#""OK (((19968 . 20223) (\"Issue514 Target\" . \"iso10646-1\") nil nil) (han (\"Issue514 Target\" . \"iso10646-1\") (\"Issue514 Target\" . \"iso10646-1\") nil) (japanese-jisx0208 (\"Issue514 Target\" . \"iso10646-1\") (\"Issue514 Target\" . \"iso10646-1\") nil))""#
    ]];
    assert_oracle_parity_expect(form, expected);
}
