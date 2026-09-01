//! Strict combo oracle probes, batch 124: regex \\= (match-at-point),
//! \\_</\\_> symbol boundaries, print-number-table, format %s/%S of
//! records/structs, face :box/:underline variants, and combo search with
//! invisible+narrow+case-fold. Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_t8_regex_symbol_boundaries_match_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (insert "foo bar baz")
  (goto-char 5)
  (list (string-match-p "\\=" "bar" 0)
        (save-excursion
          (goto-char 5)
          (looking-at "\\=bar"))
        (string-match-p "\\_<bar\\_>" "foo bar baz")
        (string-match-p "\\_<foo\\_>" "foo bar")
        (and (string-match "\\_<\\(\\w+\\)\\_>" "foo bar baz")
             (match-string 1))))
"#;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 0 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t8_print_number_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((shared (list 42 3.14 "str"))
      (tree (list 1 2 3)))
  (list (let ((print-number-table t)) (prin1-to-string (list shared shared)))
        (let ((print-number-table t)) (prin1-to-string (list tree tree tree)))
        (let ((print-circle t) (print-number-table t))
          (prin1-to-string (list shared shared)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"((42 3.14 \\\"str\\\") (42 3.14 \\\"str\\\"))\" \"((1 2 3) (1 2 3) (1 2 3))\" \"(#1=(42 3.14 \\\"str\\\") #1#)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t8_format_s_S_records_structs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (cl-defstruct probe-fmt-rec a b)
  (let ((r (make-probe-fmt-rec :a 1 :b 2)))
    (list (format "%s" r)
          (format "%S" r)
          (string-match "probe-fmt-rec" (format "%s" r))
          (string-match "#s" (format "%S" r)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t8_face_box_underline_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((f1 (make-face 'probe-box-face))
      (f2 (make-face 'probe-underline-face)))
  (set-face-attribute 'probe-box-face nil :box '(:line-width 2 :color "red" :style released-button))
  (set-face-attribute 'probe-underline-face nil :underline '(:color "blue" :style wave :position -2))
  (list (face-attribute 'probe-box-face :box nil 'default)
        (face-attribute 'probe-underline-face :underline nil 'default)
        (face-attribute 'probe-box-face :box nil nil)
        (face-attribute 'probe-underline-face :underline nil nil)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((:line-width 2 :color \"red\" :style released-button) (:color \"blue\" :style wave :position -2) (:line-width 2 :color \"red\" :style released-button) (:color \"blue\" :style wave :position -2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_t8_combo_search_invisible_narrow_casefold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((log nil))
  (with-temp-buffer
    (insert "FINDME hidden SECRET visible FINDME end")
    (let ((o (make-overlay 8 21)))
      (overlay-put o 'invisible t))
    (narrow-to-region 1 45)
    (let ((case-fold-search t))
      (goto-char 1)
      (while (search-forward "findme" nil t)
        (push (match-beginning 0) log))
      (list (nreverse log)
            (buffer-string)
            (buffer-substring-no-properties 1 45)
            (point-min)
            (point-max)
            (length (overlays-in 1 45))
            (let ((case-fold-search nil))
              (goto-char 1)
              (re-search-forward "FINDME" nil t)
              (match-beginning 0)))))
    (widen)))
"#;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 45)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
