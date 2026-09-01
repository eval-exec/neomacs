use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo80_babel_ob_awk_sed_langs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 79)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (list
 :ob-awk-e (condition-case nil (require 'ob-awk) (error nil))
 :ob-sed-e (condition-case nil (require 'ob-sed) (error nil))
 :ob-sql-e (condition-case nil (require 'ob-sql) (error (featurep 'ob-sql))))))"##,
        expect,
    );
}
#[test]
fn combo80_element_cache_state_after_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:pre 2) (:post 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-element)
 (insert "* A\n** B\n")
 (let ((r '())) (let ((t1 (org-element-parse-buffer))) (push (list :pre (length (org-element-map t1 'headline #'identity))) r))
  (goto-char (point-max)) (insert "\n** C\n") (org-element-cache-reset)
  (let ((t2 (org-element-parse-buffer))) (push (list :post (length (org-element-map t2 'headline #'identity))) r))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo80_hierarchy_sort_children() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:sorted (\"A\" \"B\" \"C\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\n** C\n")
 (let ((r '())) (goto-char (point-min)) (org-sort-entries nil ?a)
  (push (list :sorted (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
    (org-element-map (org-element-parse-buffer) 'headline #'identity))) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo80_footnote_at_definition_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:on-def1 (\"1\" 12 31 \"Definition.\")) (:on-def2 (\"2\" 31 45 \"Def 2.\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "Ref[fn:1]\n\n[fn:1] Definition.\n[fn:2] Def 2.\n")
 (let ((r '())) (goto-char (point-max)) (search-backward "[fn:1]")
  (push (list :on-def1 (org-footnote-at-definition-p)) r)
  (search-forward "[fn:2]") (push (list :on-def2 (org-footnote-at-definition-p)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo80_babel_call_with_end_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+name: square\n#+begin_src emacs-lisp :results value :var x=0\n(* x x)\n#+end_src\n\n")
  (insert "#+call: square[:results raw](x=9)\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src") (org-babel-execute-src-block)
   (goto-char (point-min)) (search-forward "#+call:")
   (condition-case e (push (org-babel-lob-execute-maybe) r) (error (push (list :error (car e)) r)))
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo80_export_handle_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:num nil) (:toc nil) (:author nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox)
 (insert "#+OPTIONS: num:nil toc:nil \\n:t author:nil\n* H\n")
 (let* ((info (org-export-get-environment)) (r '()))
  (push (list :num (plist-get info :with-numbers)) r) (push (list :toc (plist-get info :with-toc)) r)
  (push (list :author (plist-get info :with-author)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo80_agenda_custom_commands_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:pre-hook-fbound t :finalize-hook-fbound t :mode-hook-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :pre-hook-fbound (boundp 'org-agenda-before-write-hook)
 :finalize-hook-fbound (boundp 'org-agenda-finalize-hook)
 :mode-hook-fbound (boundp 'org-agenda-mode-hook)))"##,
        expect,
    );
}
#[test]
fn combo80_org_update_parent_checkbox() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:parent-cb nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "- [-] Parent\n  - [ ] Child 1\n  - [ ] Child 2\n")
 (let ((r '())) (goto-char (point-min)) (org-toggle-checkbox 1)
  (push (list :parent-cb (org-element-property :checkbox (org-element-at-point))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo80_org_dblock_columnview() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:dblock-columnview-fbound t :dblock-insert-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-colview) (list
 :dblock-columnview-fbound (fboundp 'org-dblock-write:columnview)
 :dblock-insert-fbound (fboundp 'org-columns-dblock-write-default)))"##,
        expect,
    );
}
#[test]
fn combo80_org_narrow_to_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:narrow-heads 2) (:wide-heads 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\nBody B.\n*** C\n* D\n")
 (let ((r '())) (goto-char (point-min)) (search-forward "** B") (beginning-of-line)
  (org-narrow-to-element) (push (list :narrow-heads (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (widen) (push (list :wide-heads (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
