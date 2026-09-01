use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo102_org_table_formula_debug_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:debugger-fbound t :edit-formula-fbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :debugger-fbound (fboundp 'org-table-toggle-formula-debugger) :edit-formula-fbound (fboundp 'org-table-edit-formula)))"##,
        expect,
    );
}
#[test]
fn combo102_org_babel_remove_blank_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:chomp-fbound t :trim-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :chomp-fbound (fboundp 'org-babel-chomp) :trim-fbound (fboundp 'org-babel-trim)))"##,
        expect,
    );
}
#[test]
fn combo102_org_export_reference_target() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox)
 (insert "<<target>>\n") (let* ((t (org-element-parse-buffer)) (tgts (org-element-map t 'target #'identity))
  (r '())) (push (list :count (length tgts)) r)
  (when (car tgts) (push (list :value (org-element-property :value (car tgts))) r)) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo102_org_latex_fragment_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "$x^2 + y^2 = z^2$\n") (let* ((t (org-element-parse-buffer))
  (lfs (org-element-map t 'latex-fragment #'identity)) (r '()))
  (push (list :count (length lfs)) r) (when (car lfs) (push (list :value (org-element-property :value (car lfs))) r))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo102_org_latex_environment_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "\\begin{equation}\nx = 1\n\\end{equation}\n") (let* ((t (org-element-parse-buffer))
  (les (org-element-map t 'latex-environment #'identity)) (r '()))
  (push (list :count (length les)) r) (when (car les) (push (list :value (org-element-property :value (car les))) r))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo102_org_babel_call_with_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+name: square\n#+begin_src emacs-lisp :results value :var n=0\n(* n n)\n#+end_src\n\n")
  (insert "#+call: square[:results raw](n=11)\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src") (push (org-babel-execute-src-block) r)
   (goto-char (point-min)) (search-forward "#+call:") (condition-case e (push (org-babel-lob-execute-maybe) r)
    (error (push (list :err (car e)) r))) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo102_org_heading_levels_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:count 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* L1\n** L2\n*** L3\n**** L4\n***** L5\n") (let ((r '()))
  (while (re-search-forward org-heading-regexp nil t)
   (push (list :stars (length (match-string 1)) :level (org-outline-level)) r))
  (push (list :count (length r)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo102_org_comment_block_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+BEGIN_COMMENT\nHidden text *bold* here.\n#+END_COMMENT\nVisible.\n")
 (let* ((t (org-element-parse-buffer)) (cbs (org-element-map t 'comment-block #'identity))
  (ps (org-element-map t 'paragraph #'identity)) (r '()))
  (push (list :cb-count (length cbs)) r) (push (list :p-count (length ps)) r)
  (when (car cbs) (push (list :cb-value (org-element-property :value (car cbs))) r)) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo102_org_timestamp_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:has-schedule nil :planning-count 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* H\nSCHEDULED: <2024-01-15 Mon>\n")
 (goto-char (point-min)) (org-schedule '(4))
 (list :has-schedule (org-entry-get nil "SCHEDULED") :planning-count
  (length (org-element-map (org-element-parse-buffer) 'planning #'identity))))"##,
        expect,
    );
}
#[test]
fn combo102_org_babel_chomp_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:chomp-empty \"\" :chomp-spaces \"  hello  \" :trim-empty \"\" :trim-spaces \"hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :chomp-empty (org-babel-chomp "" "[\n\r]") :chomp-spaces (org-babel-chomp "  hello  " "[\n\r]")
 :trim-empty (org-babel-trim "") :trim-spaces (org-babel-trim "  hello  ")))"##,
        expect,
    );
}
