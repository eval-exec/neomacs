use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo95_full_document_parse_interpret_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+TITLE: Test\n#+AUTHOR: X\n* TODO H :tag:\nSCHEDULED: <2024-01-01>\nBody *bold* /italic/.\n")
 (let ((r '())) (let* ((t (org-element-parse-buffer)) (i (substring-no-properties (org-element-interpret-data t)))
   (t2 (with-temp-buffer (org-mode) (insert i) (goto-char (point-min)) (org-element-parse-buffer))))
  (push (list :orig-headlines (length (org-element-map t 'headline #'identity))) r)
  (push (list :re-headlines (length (org-element-map t2 'headline #'identity))) r)
  (push (list :orig-bold (length (org-element-map t 'bold #'identity))) r)
  (push (list :re-bold (length (org-element-map t2 'bold #'identity))) r)
  (push (list :stable (and (= (length (org-element-map t 'headline #'identity))
   (length (org-element-map t2 'headline #'identity)))
   (= (length (org-element-map t 'bold #'identity))
   (length (org-element-map t2 'bold #'identity)))))) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo95_babel_multiple_blocks_dependency_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (30)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+name: a\n#+begin_src emacs-lisp :results value\n10\n#+end_src\n\n")
  (insert "#+name: b\n#+begin_src emacs-lisp :results value :var x=a\n(+ x 5)\n#+end_src\n\n")
  (insert "#+name: c\n#+begin_src emacs-lisp :results value :var y=b\n(* y 2)\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp :results value")
   (org-babel-execute-src-block) (search-forward "#+begin_src emacs-lisp :results value :var x=a")
   (org-babel-execute-src-block) (search-forward "#+begin_src emacs-lisp :results value :var y=b")
   (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo95_export_compare_html_latex_summary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((html t) (latex t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-html) (require 'ox-latex)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "* H\n*bold* /italic/.\n| a |\n| 1 |\n")
  (let ((r '())) (dolist (b '(html latex)) (let ((out (org-export-as b nil nil t)))
   (push (list b (and out (> (length out) 0))) r))) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo95_table_eval_lisp_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| a | b |\n| 3 | 4 |\n") (insert "#+TBLFM: $2='(1+ $1);N\n")
 (let ((r '())) (goto-char (point-min))
  (condition-case nil (progn (org-table-recalculate t) (push (list :ok t) r)) (error (push :err r)))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo95_org_edit_with_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:A \"1\") (:B \"2\") (:C \"3\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* H\n:PROPERTIES:\n:A: 1\n:B: 2\n:END:\n")
 (let ((r '())) (goto-char (point-min))
  (org-set-property "C" "3") (push (list :A (org-entry-get nil "A")) r)
  (push (list :B (org-entry-get nil "B")) r) (push (list :C (org-entry-get nil "C")) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo95_cycle_global_visibility_states() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init-invis nil) (:1-invis nil) (:2-invis nil) (:show-invis nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\n** C\n* D\n") (let ((r '()))
  (goto-char (point-min)) (push (list :init-invis (get-char-property (point) 'invisible)) r)
  (org-shifttab 1) (push (list :1-invis (get-char-property (point) 'invisible)) r)
  (org-shifttab 1) (push (list :2-invis (get-char-property (point) 'invisible)) r)
  (org-show-all) (push (list :show-invis (get-char-property (point) 'invisible)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo95_multi_block_src_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+begin_src emacs-lisp\n1\n#+end_src\n\n#+begin_src sh\necho 2\n#+end_src\n\n#+begin_example\n3\n#+end_example\n")
 (let ((r '())) (let* ((t (org-element-parse-buffer)))
  (push (list :src-count (length (org-element-map t 'src-block #'identity))) r)
  (push (list :ex-count (length (org-element-map t 'example-block #'identity))) r)
  (push (list :src-langs (mapcar (lambda (s) (org-element-property :language s))
   (org-element-map t 'src-block #'identity))) r)) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo95_org_babel_parse_params_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:parse-fbound t :merge-fbound t :process-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :parse-fbound (fboundp 'org-babel-parse-header-arguments)
 :merge-fbound (fboundp 'org-babel-merge-params)
 :process-fbound (fboundp 'org-babel-process-params)))"##,
        expect,
    );
}
#[test]
fn combo95_export_raw_snippet_backends() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "@@html:<b>H</b>@@ @@latex:\\textbf{L}@@ @@ascii:*A*@@\n")
 (let ((r '())) (let* ((t (org-element-parse-buffer)) (es (org-element-map t 'export-snippet #'identity)))
  (push (list :count (length es)) r) (push (list :backends (mapcar (lambda (s) (org-element-property :back-end s)) es)) r))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo95_org_timestamp_with_repeater_warning_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (let ((ts (org-timestamp-from-string "<2024-01-01 Mon +1m -3d>")))
  (list :type (org-element-property :type ts) :repeater-type (org-element-property :repeater-type ts)
   :repeater-value (org-element-property :repeater-value ts) :repeater-unit (org-element-property :repeater-unit ts)
   :warning-type (org-element-property :warning-type ts) :warning-value (org-element-property :warning-value ts)
   :warning-unit (org-element-property :warning-unit ts))))"##,
        expect,
    );
}
