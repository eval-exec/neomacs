//! Strong combo-complex-77/78 oracle tests — extreme org-agenda
//! + babel + element probes: org-agenda-todo with batch processing,
//! org-agenda-redo cycle, org-babel ob-emacs-lisp with :results
//! replace+raw combo, org-element create+interpret for babel-call,
//! org-timestamp from time and format roundtrip, org-list-make-
//! subtree, and org-export to file with body-only.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo77_agenda_todo_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable r)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-agenda)
 (insert "* TODO A\n** TODO B\n* DONE C\n* TODO D\n")
 (let ((r '())) (push (list :org-agenda-todo-fbound (fboundp 'org-todo-list)) r)
  (push (list :todo-count (length (org-map-entries (lambda () t) "TODO=\"TODO\""))) r)
  ;; mark all TODO as DONE via map
  (org-map-entries (lambda () (org-todo "DONE")) "TODO=\"TODO\"")
  (push (list :remaining-todo (length (org-map-entries (lambda () t) "TODO=\"TODO\""))) r)
  (push (list :now-done (length (org-map-entries (lambda () t) "TODO=\"DONE\""))) r))
 (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo77_agenda_redo_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:redo-fbound t :kill-fbound t :quit-fbound t :exit-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :redo-fbound (fboundp 'org-agenda-redo)
 :kill-fbound (fboundp 'org-agenda-kill)
 :quit-fbound (fboundp 'org-agenda-quit)
 :exit-fbound (fboundp 'org-agenda-exit)
 ))"##,
        expect,
    );
}
#[test]
fn combo77_babel_results_replace_raw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"**not org**\" (:buffer \"#+begin_src emacs-lisp :results replace raw\\n\\\"**not org**\\\"\\n#+end_src\\n\\n#+RESULTS:\\n**not org**\\n\"))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+begin_src emacs-lisp :results replace raw\n\"**not org**\"\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src")
   (push (org-babel-execute-src-block) r)
   (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo77_element_create_babel_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:type babel-call) (:call \"square\") (:inside-header '(:x . \"5\")) (:str-hash-call 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-element)
 (let* ((call (org-element-create 'babel-call '(:call "square" :inside-header '(:x . "5"))))
        (r '()))
  (push (list :type (org-element-type call)) r)
  (push (list :call (org-element-property :call call)) r)
  (push (list :inside-header (org-element-property :inside-header call)) r)
  (let ((str (substring-no-properties (org-element-interpret-data call))))
   (push (list :str-hash-call (string-match-p "#\\+CALL" str)) r))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo77_timestamp_from_string_to_format_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (let ((ts (org-timestamp-from-string "<2024-03-15 Fri 14:30>")))
 (list :day-start (org-element-property :day-start ts)
  :month-start (org-element-property :month-start ts)
  :year-start (org-element-property :year-start ts)
  :hour-start (org-element-property :hour-start ts)
  :minute-start (org-element-property :minute-start ts)
  :fmt-iso (org-timestamp-format ts "%Y-%m-%dT%H:%M:%S")
  :fmt-us (org-timestamp-format ts "%m/%d/%Y %I:%M %p")
  :fmt-eu (org-timestamp-format ts "%d.%m.%Y %H:%M"))))"##,
        expect,
    );
}
#[test]
fn combo77_org_list_make_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:make-subtree-fbound t) (:item-count-before 4) (:after \"* item 1\\n* item 2\\n** sub\\n* item 3\\n\") (:headline-count-after 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "- item 1\n- item 2\n  + sub\n- item 3\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :make-subtree-fbound (fboundp 'org-list-make-subtree)) r)
  (push (list :item-count-before (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
  (condition-case nil
   (progn (org-list-make-subtree) (push (list :after (buffer-string)) r))
   (error (push (list :error t) r)))
  (push (list :headline-count-after (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo77_export_to_file_body_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer (org-mode) (require 'ox-html)
 (insert "* Test\nBody.\n")
 (let ((tmpfile (make-temp-file "org-export-" nil ".html")))
  (let ((r '())) (condition-case nil
   (progn (org-html-export-to-html nil nil nil t) (push (list :body-only t) r))
   (error (push (list :error t) r)))
  (condition-case nil (delete-file tmpfile) (error nil))
  (nreverse r))))"##,
    );
}
#[test]
fn combo77_org_face_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:face-at-point-fbound t) (:face-prop nil) (:body-face nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* TODO Headline :tag:\nBody.\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :face-at-point-fbound (fboundp 'get-text-property)) r)
  (push (list :face-prop (get-text-property (point) 'face)) r)
  (search-forward "Body.")
  (push (list :body-face (get-text-property (point) 'face)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo77_org_set_effort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:set-effort-fbound t) (:effort \"2:30\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* Task\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :set-effort-fbound (fboundp 'org-set-effort)) r)
  (condition-case nil
   (progn (org-set-effort nil "2:30") (push (list :effort (org-entry-get nil "EFFORT")) r))
   (error (push (list :error t) r)))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo77_org_archive_to_archive_sibling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:archive-to-fbound t :archive-subtree-fbound t :archive-default-fbound t :archive-save-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-archive) (list
 :archive-to-fbound (fboundp 'org-archive-to-archive-sibling)
 :archive-subtree-fbound (fboundp 'org-archive-subtree)
 :archive-default-fbound (fboundp 'org-archive-subtree-default)
 :archive-save-fbound (boundp 'org-archive-save-context-info)
 ))"##,
        expect,
    );
}
