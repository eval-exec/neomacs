use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo81_babel_ob_ref_special_references() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (((10 30) (10 20)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp) (require 'ob-ref)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+name: data\n| 10 | 20 |\n| 30 | 40 |\n\n")
  (insert "#+begin_src emacs-lisp :results value :var d=data[,0] :var e=data[0,]\n(list d e)\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src")
   (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo81_agenda_search_view() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:search-view-fbound t :occur-fbound t :match-sparse-tree-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :search-view-fbound (fboundp 'org-search-view) :occur-fbound (fboundp 'org-occur)
 :match-sparse-tree-fbound (fboundp 'org-match-sparse-tree)))"##,
        expect,
    );
}
#[test]
fn combo81_org_todo_depend_triggers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:depend-fbound nil :blocker-fbound t :checkbox-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :depend-fbound (fboundp 'org-todo-trigger-todo-changes)
 :blocker-fbound (boundp 'org-enforce-todo-dependencies)
 :checkbox-fbound (boundp 'org-enforce-todo-checkbox-dependencies)))"##,
        expect,
    );
}
#[test]
fn combo81_org_table_wrap_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:wrap-region-fbound t :create-fbound t :convert-region-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :wrap-region-fbound (fboundp 'org-table-wrap-region) :create-fbound (fboundp 'org-table-create)
 :convert-region-fbound (fboundp 'org-table-convert-region)))"##,
        expect,
    );
}
#[test]
fn combo81_export_with_custom_id_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "* A\n:PROPERTIES:\n:CUSTOM_ID: target\n:END:\n")
  (insert "Link to [[#target]]\n")
  (let ((r '())) (condition-case nil (let ((out (org-export-as 'ascii nil nil t)))
    (push (list :ok (> (length out) 0)) r)) (error (push (list :error t) r)))
  (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo81_clock_report_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock)
 (let ((org-clock-persist nil)) (insert "* A\n** B\n") (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
  (search-forward "** B") (beginning-of-line) (org-clock-in nil) (org-clock-out nil nil)
  (goto-char (point-min)) (insert "#+BEGIN: clocktable :maxlevel 3 :scope file :block thisyear\n#+END:\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+BEGIN:") (beginning-of-line) (org-dblock-update)
   (push (list :ok (> (length (buffer-string)) 0)) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo81_babel_org_babel_execute_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:execute-buffer-fbound t :execute-subtree-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :execute-buffer-fbound (fboundp 'org-babel-execute-buffer)
 :execute-subtree-fbound (fboundp 'org-babel-execute-subtree)))"##,
        expect,
    );
}
#[test]
fn combo81_org_mark_ring_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:outline-previous-fbound t) (:outline-next-fbound t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\n* C\n") (let ((r '())) (goto-char (point-min))
  (push (list :outline-previous-fbound (fboundp 'org-mark-ring-goto)) r)
  (push (list :outline-next-fbound (fboundp 'org-mark-ring-push)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo81_element_affiliated_multiple_keywords_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:caption (((#(\"Fig 1\" 0 5 (:parent (#(\"Fig 1\" 0 5 (:parent #7))))))))) (:name \"my-fig\") (:attr-html (\":class fig\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+CAPTION: Fig 1\n#+NAME: my-fig\n#+ATTR_HTML: :class fig\n| a |\n| 1 |\n")
 (let ((r '())) (let* ((tree (org-element-parse-buffer)) (table (car (org-element-map tree 'table #'identity))))
  (when table (push (list :caption (org-element-property :caption table)) r)
   (push (list :name (org-element-property :name table)) r)
   (push (list :attr-html (org-element-property :attr_html table)) r)))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo81_export_latex_classes_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:default-class-fbound t :classes-fbound t :packages-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox-latex) (list
 :default-class-fbound (boundp 'org-latex-default-class)
 :classes-fbound (boundp 'org-latex-classes)
 :packages-fbound (boundp 'org-latex-packages-alist)))"##,
        expect,
    );
}
