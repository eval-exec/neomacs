//! Strong combo-complex-69 oracle tests — babel with
//! :results code and org-table-orig, org-babel with
//! :prologue/:epilogue on emacs-lisp, element with org-element-
//! set-element for deep replacement, export with custom backend
//! transcoders for all element types, org-agenda with org-agenda-
//! column-view, and org-capture with template annotation expansion.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo69_babel_results_code_org() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"42\\n\" (:buffer-after \"#+begin_src emacs-lisp :results code\\n(setq x 42)\\n#+end_src\\n\\n#+RESULTS:\\n#+begin_src emacs-lisp\\n42\\n#+end_src\\n\"))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results code\n(setq x 42)\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (push (list :buffer-after (buffer-substring-no-properties (point-min) (point-max))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo69_element_set_element_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable tree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (insert "* A\n** B\n** C\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (headlines (org-element-map tree 'headline #'identity))
           (B (nth 1 headlines))  ;; ** B
           (new-hl (org-element-create 'headline '(:level 2 :raw-value "REPLACED" :todo-keyword "DONE" :priority ?A))))
      ;; set-element: replace B with new-hl
      (org-element-set-element B new-hl)
      (push (list :after-replace (mapcar (lambda (h) (list (org-element-property :level h)
                                                           (substring-no-properties (org-element-property :raw-value h))
                                                           (org-element-property :todo-keyword h)))
                                         (org-element-map tree 'headline #'identity))) r))
    ;; interpretable
    (push (list :interpret-ok (> (length (substring-no-properties (org-element-interpret-data tree))) 0)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo69_export_custom_backend_all_transcoders() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Unknown \\\"nil\\\" backend: Aborting export\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox)
  (let* ((test-b (org-export-create-backend
                  :parent 'ascii
                  :name 'custom-all
                  :transcoders
                  '((bold . (lambda (o c i) (concat "<b>" c "</b>")))
                    (italic . (lambda (o c i) (concat "<i>" c "</i>")))
                    (verbatim . (lambda (o c i) (concat "<code>" c "</code>")))
                    (paragraph . (lambda (p c i) (concat "<p>" c "</p>\n"))))))
         (exported (org-export-string-as "*bold* /italic/." 'custom-all t))
         (r '()))
    (push (list :has-b-tag (string-match-p "<b>" exported)) r)
    (push (list :has-i-tag (string-match-p "<i>" exported)) r)
    (push (list :has-p-tag (string-match-p "<p>" exported)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo69_agenda_column_view() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:column-view-fbound t :colview-fbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-agenda)
  (list
   :column-view-fbound (fboundp 'org-agenda-columns)
   :colview-fbound (fboundp 'org-agenda-colview-compute)
   ))"##,
        expect,
    );
}

#[test]
fn combo69_capture_annotation_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:annotation-fbound t :store-fbound t :template-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-capture)
  (list
   :annotation-fbound (boundp 'org-capture-link-is-already-stored)
   :store-fbound (fboundp 'org-capture-put-target-region-and-position)
   :template-fbound (fboundp 'org-capture-fill-template)
   ))"##,
        expect,
    );
}

#[test]
fn combo69_babel_prologue_epilogue_lisp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable test69-var)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results value :prologue \"(setq-local test69-var 42)\" :epilogue \"(message \\\"done\\\")\"\n")
    (insert "(+ test69-var 58)\n")
    (insert "#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo69_org_timestamp_up_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:up-fbound t) (:down-fbound t) (:after-up \"[2024-06-15 Sat]\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<2024-06-15 Sat>\n")
  (let ((r '()))
    (goto-char (point-min))
    ;; org-timestamp-up
    (push (list :up-fbound (fboundp 'org-timestamp-up)) r)
    ;; org-timestamp-down
    (push (list :down-fbound (fboundp 'org-timestamp-down)) r)
    ;; try up
    (condition-case nil
        (progn (org-timestamp-up 1)
               (push (list :after-up (buffer-string)) r))
      (error (push (list :up-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo69_org_agenda_bulk_action() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:bulk-mark-fbound t :bulk-unmark-fbound t :bulk-action-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-agenda)
  (list
   :bulk-mark-fbound (fboundp 'org-agenda-bulk-mark)
   :bulk-unmark-fbound (fboundp 'org-agenda-bulk-unmark)
   :bulk-action-fbound (fboundp 'org-agenda-bulk-action)
   ))"##,
        expect,
    );
}

#[test]
fn combo69_babel_with_results_verbatim() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"\\\"literal output\\\"\" (:buffer \"#+begin_src emacs-lisp :results verbatim\\n\\\"literal output\\\"\\n#+end_src\\n\\n#+RESULTS:\\n: \\\"literal output\\\"\\n\"))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results verbatim\n\"literal output\"\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min))(search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo69_element_with_empty_post_blank() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:post-blanks (0 0 0)) (:count 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n* C\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (headlines (org-element-map tree 'headline #'identity)))
      (push (list :post-blanks (mapcar (lambda (h) (org-element-property :post-blank h)) headlines)) r)
      (push (list :count (length headlines)) r))
    (nreverse r)))"##,
        expect,
    );
}
