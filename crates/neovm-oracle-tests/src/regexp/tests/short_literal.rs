//! GNU-refreshed bounded folded literal search parity.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

const MODES: &[&[(&str, &str)]] = &[
    &[("NEOVM_REGEX_SHORT_LITERAL", "off")],
    &[("NEOVM_REGEX_SHORT_LITERAL", "on")],
];

#[test]
fn oracle_cl2_short_literal_short_forward_matches_and_failures() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (insert "Жq жQ жжЖQ жж")
  (let ((case-fold-search t) (out nil))
    (dolist (pat '("жq" "ЖQ" "жz" "жж"))
      (goto-char 1)
      (let (hits)
        (while (re-search-forward pat nil t)
          (push (list (point) (match-beginning 0) (match-end 0)) hits))
        (push (list pat (nreverse hits) (point)) out)))
    (nreverse out)))"####;
    let expected = expect_test::expect![[
        r#""OK ((\"жq\" ((3 1 3) (6 4 6) (11 9 11)) 11) (\"ЖQ\" ((3 1 3) (6 4 6) (11 9 11)) 11) (\"жz\" nil 1) (\"жж\" ((9 7 9) (14 12 14)) 14))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_standard_sigma_mu_and_width_equivalents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (insert "Σq σQ ςq µq μQ Μq ıq İq ſq Kq")
  (let ((case-fold-search t))
    (mapcar (lambda (pat)
              (goto-char 1)
              (let (hits)
                (while (re-search-forward pat nil t)
                  (push (list (match-beginning 0) (match-end 0)) hits))
                (list pat (nreverse hits))))
            '("σq" "µq" "μq" "iq" "sq" "kq"))))"####;
    let expected = expect_test::expect![[
        r#""OK ((\"σq\" ((1 3) (4 6) (7 9))) (\"µq\" ((10 12) (13 15) (16 18))) (\"μq\" ((10 12) (13 15) (16 18))) (\"iq\" nil) (\"sq\" nil) (\"kq\" nil))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_bounds_markers_narrowing_and_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (insert "aЖq bжQ cЖq")
  (let ((case-fold-search t) (bound (copy-marker 7)) (out nil))
    (goto-char 1)
    (push (list (re-search-forward "жq" bound t) (match-beginning 0) (match-end 0)) out)
    (push (list (re-search-forward "жq" bound t) (point)) out)
    (narrow-to-region 5 9)
    (goto-char (point-min))
    (push (list (re-search-forward "жq" nil t) (point)
                (match-beginning 0) (match-end 0)) out)
    (push (list (re-search-forward "жq" nil t) (point)) out)
    (nreverse out)))"####;
    let expected = expect_test::expect![[r#""OK ((4 2 4) (nil 4) (8 8 6 8) (nil 8))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_forward_and_backward_count_stop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (insert "Жq жQ Жq")
  (let ((case-fold-search t) (out nil))
    (goto-char 1)
    (push (list (re-search-forward "жq" nil t 2) (match-beginning 0) (match-end 0)) out)
    (goto-char (point-max))
    (push (list (re-search-backward "жq" nil t 2) (match-beginning 0) (match-end 0)) out)
    (goto-char 1)
    (push (list (re-search-backward "жq" nil t -2) (point)) out)
    (goto-char (point-max))
    (push (list (re-search-forward "жq" nil t -2) (point)) out)
    (goto-char 2)
    (push (list (re-search-backward "жq" nil t) (point)) out)
    (nreverse out)))"####;
    let expected = expect_test::expect![[r#""OK ((6 4 6) (4 4 6) (6 6) (4 4) (nil 2))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_noerror_variants_and_zero_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (insert "жжж")
  (let ((case-fold-search t) (out nil))
    (dolist (noerror '(t move nil))
      (goto-char 2)
      (push (list noerror (condition-case err (re-search-forward "жq" nil noerror)
                           (error (car err))) (point)) out))
    (goto-char 2)
    (push (list (re-search-forward "жq" nil t 0) (point)
                (match-beginning 0) (match-end 0)) out)
    (push (condition-case err (re-search-forward 7 nil t 0)
            (error (list (car err) (cdr err)))) out)
    (nreverse out)))"####;
    let expected = expect_test::expect![[
        r#""OK ((t nil 2) (move nil 4) (nil search-failed 2) (2 2 2 2) (wrong-type-argument (stringp 7)))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_inhibit_match_data_and_register_padding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (insert "aЖq bжQ")
  (let ((case-fold-search t))
    (goto-char 1)
    (re-search-forward "\\(a\\)\\(Жq\\)")
    (let ((saved (match-data t)))
      (goto-char 5)
      (let* ((inhibit-changing-match-data t)
             (found (re-search-forward "жq" nil t)))
        (list found (equal saved (match-data t))
              (match-beginning 0) (match-end 0)
              (match-beginning 1) (match-end 1))))))"####;
    let expected = expect_test::expect![[r#""OK (8 t 1 4 1 2)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_plain_and_posix_trivial_literals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (insert "жЖq жQ")
  (let ((case-fold-search t))
    (goto-char 1)
    (let ((plain (list (re-search-forward "жq" nil t) (match-beginning 0) (match-end 0))))
      (goto-char 1)
      (list plain (list (posix-search-forward "жq" nil t)
                        (match-beginning 0) (match-end 0))))))"####;
    let expected = expect_test::expect![[r#""OK ((4 2 4) (4 2 4))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_escaped_literals_and_unsupported_shapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (insert "Ж.q ж*q Жq жжQ")
  (let ((case-fold-search t))
    (mapcar (lambda (pat)
              (goto-char 1)
              (list pat (re-search-forward pat nil t)
                    (match-beginning 0) (match-end 0)))
            '("ж\\.q" "ж\\*q" "\\(ж\\)q" "ж\\|q" "ж+q" "ж[qQ]" "^жq"))))"####;
    let expected = expect_test::expect![[
        r#""OK ((\"ж\\\\.q\" 4 1 4) (\"ж\\\\*q\" 8 5 8) (\"\\\\(ж\\\\)q\" 11 9 11) (\"ж\\\\|q\" 2 1 2) (\"ж+q\" 11 9 11) (\"ж[qQ]\" 11 9 11) (\"^жq\" nil 9 11))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_raw_bytes_multibyte_pattern_and_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (let* ((raw (string-make-multibyte (unibyte-string #xe9)))
         (pat (concat raw "q")) (case-fold-search t))
    (insert raw "Q " raw "q éq")
    (goto-char 1)
    (let (hits)
      (while (re-search-forward pat nil t)
        (push (list (match-beginning 0) (match-end 0)) hits))
      (list (nreverse hits) (point)))))"####;
    let expected = expect_test::expect![[r#""OK (((1 3) (4 6)) 6)""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_unibyte_pattern_multibyte_buffer_corner() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (let* ((u (unibyte-string #xe9 ?q))
         (r (string-make-multibyte u))
         (case-fold-search t))
    (insert r " " u " éq ÉQ")
    (mapcar (lambda (pat)
              (goto-char 1)
              (let (hits)
                (while (re-search-forward pat nil t)
                  (push (list (match-beginning 0) (match-end 0)) hits))
                (nreverse hits))) (list u r "éq"))))"####;
    let expected = expect_test::expect![[r#""OK (((1 3) (4 6)) ((1 3) (4 6)) ((7 9) (10 12)))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_unibyte_buffer_pattern_mixes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string #xe9 ?Q ?\s #xe9 ?q ?\s ?a ?Q))
  (let ((case-fold-search t))
    (mapcar (lambda (pat)
              (goto-char 1)
              (let (hits)
                (while (re-search-forward pat nil t)
                  (push (list (match-beginning 0) (match-end 0)) hits))
                (nreverse hits)))
            (list (unibyte-string #xe9 ?q)
                  (string-make-multibyte (unibyte-string #xe9 ?q)) "éq" "aQ"))))"####;
    let expected =
        expect_test::expect![[r#""OK (((1 3) (4 6)) ((1 3) (4 6)) ((1 3) (4 6)) ((7 9)))""#]];
    // The baseline does not convert a Unicode pattern to the equivalent unibyte byte. The shortcut excludes unibyte targets.
    crate::common::assert_oracle_divergence_expect(form, expected);
}

#[test]
fn oracle_cl2_short_literal_mutable_pattern_between_searches() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (insert "Жq Жr")
  (let ((pat (copy-sequence "жq")) (case-fold-search t))
    (goto-char 1)
    (let ((before (list (re-search-forward pat nil t)
                        (match-beginning 0) (match-end 0))))
      (aset pat 1 ?r)
      (goto-char 1)
      (list before (list (re-search-forward pat nil t)
                         (match-beginning 0) (match-end 0))))))"####;
    let expected = expect_test::expect![[r#""OK ((3 1 3) (6 4 6))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_search_spaces_regexp_fallback() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (insert "Ж q Ж   Q")
  (let ((case-fold-search t) (search-spaces-regexp "[ ]+"))
    (goto-char 1)
    (let (hits)
      (while (re-search-forward "ж q" nil t)
        (push (list (match-beginning 0) (match-end 0)) hits))
      (nreverse hits))))"####;
    let expected = expect_test::expect![[r#""OK ((1 4) (5 10))""#]];
    // The baseline ignores search-spaces-regexp. Expanding it in the frontend is a separate compatibility fix.
    crate::common::assert_oracle_divergence_expect(form, expected);
}

#[test]
fn oracle_cl2_short_literal_custom_case_table_cross_width_equivalence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(let ((table (copy-case-table (standard-case-table))))
  (set-case-syntax-pair #x212a ?k table)
  (set-case-syntax-pair ?ж ?a table)
  (with-temp-buffer
    (insert "Kq kQ Жq жQ aq")
    (with-case-table table
      (let ((case-fold-search t))
        (mapcar (lambda (pat)
                  (goto-char 1)
                  (let (hits)
                    (while (re-search-forward pat nil t)
                      (push (list (match-beginning 0) (match-end 0)) hits))
                    (nreverse hits))) '("kq" "Kq" "aq" "жq"))))))"####;
    let expected = expect_test::expect![[
        r#""OK (((1 3) (4 6)) ((1 3) (4 6)) ((7 9) (10 12) (13 15)) ((7 9) (10 12) (13 15)))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_custom_case_table_non_ascii_canonical_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Both custom equivalents occupy two bytes and canonicalize to Cyrillic
    // ж, so the sealed whole literal qualifies for the short-search path.
    let form = r####"(let ((table (copy-case-table (standard-case-table))))
  (set-case-syntax-pair ?Γ ?ж table)
  (with-temp-buffer
    (insert "Γq жQ Γr жr")
    (with-case-table table
      (let ((case-fold-search t))
        (mapcar (lambda (pat)
                  (goto-char 1)
                  (let (hits)
                    (while (re-search-forward pat nil t)
                      (push (list (match-beginning 0) (match-end 0)) hits))
                    (nreverse hits))) '("жq" "Γq" "жr" "Γr"))))))"####;
    let expected = expect_test::expect![[
        r#""OK (((1 3) (4 6)) ((1 3) (4 6)) ((7 9) (10 12)) ((7 9) (10 12)))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}

#[test]
fn oracle_cl2_short_literal_case_table_replacement_between_searches() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r####"(with-temp-buffer
  (insert "Kq kQ")
  (let ((case-fold-search t) (table (copy-case-table (standard-case-table))))
    (goto-char 1)
    (let ((before (list (re-search-forward "kq" nil t) (match-beginning 0) (match-end 0))))
      (set-case-syntax-pair #x212a ?k table)
      (set-case-table table)
      (goto-char 1)
      (list before (list (re-search-forward "kq" nil t)
                         (match-beginning 0) (match-end 0))))))"####;
    let expected = expect_test::expect![[r#""OK ((6 4 6) (3 1 3))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, MODES, expected);
}
