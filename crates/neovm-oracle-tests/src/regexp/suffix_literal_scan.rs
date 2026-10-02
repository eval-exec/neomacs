//! GNU-refreshed parity of folded non-ASCII + ASCII literal searches.
//! Run the same forms with the compile-time suffix knob off and on.

use crate::common::{
    assert_oracle_parity_under_envs_expect, return_if_neovm_enable_oracle_proptest_not_set,
};

const SUFFIX_MODES: &[&[(&str, &str)]] = &[
    &[("NEOVM_REGEX_SUFFIX_LITERAL", "off")],
    &[("NEOVM_REGEX_SUFFIX_LITERAL", "on")],
];

#[test]
fn oracle_suffix_literal_standard_case_corners() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((case-fold-search t) out)
  (dolist (case '(("жq" "ЖQ") ("σq" "ςQ") ("ςq" "ΣQ")
                  ("µq" "μQ") ("μq" "µQ") ("Μq" "µQ")
                  ("iq" "ıQ") ("ıq" "iQ") ("ıq" "ıQ")
                  ("вq" "ᲀQ") ("ßq" "ẞQ")))
    (with-temp-buffer
      (insert (make-string 300 ?ж) (cadr case) (make-string 300 ?q))
      (goto-char (point-min))
      (let ((forward (re-search-forward (car case) nil t)))
        (push (list case forward
                    (and forward (list (match-beginning 0) (match-end 0) (match-string 0)))
                    (progn (goto-char (point-max)) (re-search-backward (car case) nil t))) out))))
  (nreverse out))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"жq\" \"ЖQ\") 303 (301 303 \"ЖQ\") 301) ((\"σq\" \"ςQ\") 303 (301 303 \"ςQ\") 301) ((\"ςq\" \"ΣQ\") 303 (301 303 \"ΣQ\") 301) ((\"µq\" \"μQ\") 303 (301 303 \"μQ\") 301) ((\"μq\" \"µQ\") 303 (301 303 \"µQ\") 301) ((\"Μq\" \"µQ\") 303 (301 303 \"µQ\") 301) ((\"iq\" \"ıQ\") nil nil nil) ((\"ıq\" \"iQ\") nil nil nil) ((\"ıq\" \"ıQ\") 303 (301 303 \"ıQ\") 301) ((\"вq\" \"ᲀQ\") 303 (301 303 \"ᲀQ\") 301) ((\"ßq\" \"ẞQ\") 303 (301 303 \"ẞQ\") 301))""#
    ]];
    assert_oracle_parity_under_envs_expect(form, SUFFIX_MODES, expect);
}

#[test]
fn oracle_suffix_literal_bounds_posix_and_fallback_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert (make-string 300 ?ж) "ЖQ qq ΣQ жq " (make-string 300 ?q))
  (let ((case-fold-search t) out)
    (dolist (case '(("жq" 1 nil nil) ("жq" 1 302 nil) ("жq" 1 303 nil)
                    ("жq" 302 nil nil) ("жq" 1 nil t) ("σq" 1 nil t)
                    ("\\(ж\\)q" 1 nil nil) ("ж[qQ]" 1 nil nil)
                    ("жq\\|σq" 1 nil nil) ("жжq" 1 nil nil)
                    ("^жq" 1 nil nil)))
      (goto-char (nth 1 case))
      (let ((found (if (nth 3 case)
                       (posix-search-forward (car case) (nth 2 case) t)
                     (re-search-forward (car case) (nth 2 case) t))))
        (push (list case found (point)
                    (and found (list (match-beginning 0) (match-end 0)
                                     (match-beginning 1) (match-end 1)))) out)))
    (nreverse out)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"жq\" 1 nil nil) 303 303 (301 303 nil nil)) ((\"жq\" 1 302 nil) nil 1 nil) ((\"жq\" 1 303 nil) 303 303 (301 303 nil nil)) ((\"жq\" 302 nil nil) 312 312 (310 312 nil nil)) ((\"жq\" 1 nil t) 303 303 (301 303 nil nil)) ((\"σq\" 1 nil t) 309 309 (307 309 nil nil)) ((\"\\\\(ж\\\\)q\" 1 nil nil) 303 303 (301 303 301 302)) ((\"ж[qQ]\" 1 nil nil) 303 303 (301 303 nil nil)) ((\"жq\\\\|σq\" 1 nil nil) 303 303 (301 303 nil nil)) ((\"жжq\" 1 nil nil) 303 303 (300 303 nil nil)) ((\"^жq\" 1 nil nil) nil 1 nil))""#
    ]];
    assert_oracle_parity_under_envs_expect(form, SUFFIX_MODES, expect);
}

#[test]
fn oracle_suffix_literal_custom_case_table_and_wide_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (let ((table (copy-sequence (current-case-table))) (case-fold-search t) out)
    (set-case-syntax-pair ?中 ?é table)
    (set-case-table table)
    (insert (make-string 300 ?ж) "中Q ЖQ" (make-string 300 ?q))
    (dolist (re '("éq" "\\(?:жq\\)"))
      (goto-char (point-min))
      (let ((found (re-search-forward re nil t)))
        (push (list re found (and found (match-string 0))) out)))
    (let ((canon (char-table-extra-slot (current-case-table) 1)))
      (aset canon ?中 ?q)
      (goto-char (point-max))
      (insert "Ж中")
      (goto-char (point-min))
      (let (matches)
        ;; Keep GNU on its regexp engine when editing only CANON: its
        ;; literal-search inverse table is intentionally not changed here.
        (while (re-search-forward "\\(?:жq\\)" nil t)
          (push (list (match-beginning 0) (match-end 0) (match-string 0)) matches))
        (push (nreverse matches) out)))
    (nreverse out)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"éq\" 303 \"中Q\") (\"\\\\(?:жq\\\\)\" 306 \"ЖQ\") ((300 302 \"ж中\") (304 306 \"ЖQ\") (606 608 \"Ж中\")))""#
    ]];
    assert_oracle_parity_under_envs_expect(form, SUFFIX_MODES, expect);
}

#[test]
fn oracle_suffix_literal_failed_scan_keeps_unused_ascii_translation_live() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (set-case-table (copy-sequence (current-case-table)))
  (insert (make-string 300 ?ж))
  (let ((case-fold-search t) (re "\\(?:жq\\)") out)
    (goto-char (point-min))
    (push (re-search-forward re nil t) out)
    (aset (char-table-extra-slot (current-case-table) 1) ?Q ?z)
    (goto-char (point-max))
    (insert "ЖQ" (make-string 300 ?q))
    (goto-char (point-min))
    (let ((found (re-search-forward re nil t)))
      (push (list found (and found (list (match-beginning 0) (match-end 0)))) out))
    (nreverse out)))"#;
    let expect = expect_test::expect![[r#""OK (nil (nil nil))""#]];
    assert_oracle_parity_under_envs_expect(form, SUFFIX_MODES, expect);
}

#[test]
fn oracle_suffix_literal_raw_bytes_and_unibyte_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((case-fold-search t) out)
  (dolist (multibyte '(t nil))
    (with-temp-buffer
      (set-buffer-multibyte multibyte)
      (insert (make-string 300 ?q)
              (string-as-unibyte (string 255)) "Q"
              (make-string 300 ?q))
      (dolist (pattern (list (concat (string-as-unibyte (string 255)) "q")
                            (string-as-multibyte (concat (string-as-unibyte (string 255)) "q"))))
        (goto-char (point-min))
        (let ((found (re-search-forward pattern nil t)))
          (push (list multibyte (multibyte-string-p pattern) found
                      (and found (list (match-beginning 0) (match-end 0)))) out)))))
  (nreverse out))"#;
    let expect = expect_test::expect![[
        r#""OK ((t nil 304 (301 304)) (t t nil nil) (nil nil 304 (301 304)) (nil t nil nil))""#
    ]];
    assert_oracle_parity_under_envs_expect(form, SUFFIX_MODES, expect);
}

#[test]
fn oracle_suffix_literal_suffix_density_and_long_prefix_fallback() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
  (insert (make-string 4000 ?q) "中" (make-string 4000 ?q) "ЖQ" (make-string 4000 ?q))
  (let ((case-fold-search t) out)
    (dolist (re (list "жq" "жжq" (concat (make-string 130 ?ж) "q") "qq"))
      (goto-char (point-min))
      (let ((found (re-search-forward re nil t)))
        (push (list (length re) found
                    (and found (list (match-beginning 0) (match-end 0)))) out)))
    (nreverse out)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 8004 (8002 8004)) (3 nil nil) (131 nil nil) (2 3 (1 3)))""#
    ]];
    assert_oracle_parity_under_envs_expect(form, SUFFIX_MODES, expect);
}

#[test]
fn oracle_suffix_literal_string_search_and_match_data_preservation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((case-fold-search t) out)
  (dolist (re '("жq" "σq" "µq"))
    (let* ((text (concat (make-string 300 ?ж) "ЖQ ςQ μQ" (make-string 300 ?q)))
           (found (string-match re text)))
      (push (list re found (and found (list (match-beginning 0) (match-end 0)))) out)))
  (with-temp-buffer
    (insert (make-string 300 ?ж) "ЖQ" (make-string 300 ?q))
    (string-match "\\(keep\\)" "keep")
    (let ((saved (match-data t)))
      (goto-char (point-min))
      (let ((inhibit-changing-match-data t))
        (push (list (re-search-forward "жq" nil t)
                    (equal saved (match-data t))) out))))
  (nreverse out))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"жq\" 300 (300 302)) (\"σq\" 303 (303 305)) (\"µq\" 306 (306 308)) (303 t))""#
    ]];
    assert_oracle_parity_under_envs_expect(form, SUFFIX_MODES, expect);
}
