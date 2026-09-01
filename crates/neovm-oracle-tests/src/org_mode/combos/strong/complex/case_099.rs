use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo99_org_cycle_visibility_all_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:overview-heads 4) (:showall-heads 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\n** C\n* D\n") (let ((r '()))
  (goto-char (point-min)) (org-overview)
  (push (list :overview-heads (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (org-show-all) (push (list :showall-heads (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo99_org_latex_preview_fragment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:latex-preview-fbound t :toggle-fbound t :fragment-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :latex-preview-fbound (fboundp 'org-latex-preview) :toggle-fbound (fboundp 'org-toggle-latex-fragment)
 :fragment-fbound (fboundp 'org-preview-latex-fragment)))"##,
        expect,
    );
}
#[test]
fn combo99_org_babel_header_arg_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:aliases-fbound t :results-aliases nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :aliases-fbound (boundp 'org-babel-header-arg-names) :results-aliases (when (boundp 'org-babel-header-arg-names)
  (assq :results org-babel-header-arg-names))))"##,
        expect,
    );
}
#[test]
fn combo99_org_element_normalize_contents_min_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((paragraph nil \"line1\\n  line2\\nline3\") (paragraph nil \"line1\\n\\n  line2\\n\\nline3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-element)
 (list (org-element-normalize-contents '(paragraph nil "  line1\n    line2\n  line3"))
  (org-element-normalize-contents '(paragraph nil "  line1\n\n    line2\n\n  line3"))))"##,
        expect,
    );
}
#[test]
fn combo99_org_export_footnote_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:footnote-sep-fbound nil :footnote-sep nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox) (list
 :footnote-sep-fbound (boundp 'org-export-footnote-separator) :footnote-sep (when (boundp 'org-export-footnote-separator) org-export-footnote-separator)))"##,
        expect,
    );
}
#[test]
fn combo99_org_timestamp_ia_dates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :ia-range (let ((ts (org-timestamp-from-string "[2024-03-01 Fri]--[2024-03-05 Tue]")))
  (list (org-element-property :type ts) (org-element-property :year-start ts) (org-element-property :year-end ts)))
 :ia-single (let ((ts (org-timestamp-from-string "[2024-07-04 Thu]")))
  (list (org-element-property :type ts) (org-element-property :year-start ts)))))"##,
        expect,
    );
}
#[test]
fn combo99_org_table_copy_down_increment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :copy-error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| 1 |\n|   |\n|   |\n") (goto-char (point-min)) (forward-line 1)
 (condition-case nil (progn (org-table-copy-down 2) (list :after (org-table-to-lisp))) (error :copy-error)))"##,
        expect,
    );
}
#[test]
fn combo99_org_id_get_with_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-id)
 (insert "* H\n") (goto-char (point-min)) (let ((org-id-prefix "test-"))
  (let ((id (org-id-get-create))) (list :has-prefix (string-prefix-p "test-" id)))))"##,
        expect,
    );
}
#[test]
fn combo99_org_agenda_current_time_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:time-fbound nil :current-time-fbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :time-fbound (fboundp 'org-agenda-format-time) :current-time-fbound (fboundp 'org-agenda-current-time-string)))"##,
        expect,
    );
}
#[test]
fn combo99_org_babel_resolve_var_table_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"b\" 2 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+name: x\n| a | b |\n| 1 | 2 |\n| 3 | 4 |\n\n")
  (insert "#+begin_src emacs-lisp :results value :var d=x[,1]\nd\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp")
   (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
