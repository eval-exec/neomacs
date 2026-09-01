//! Strong combo-complex-66 oracle tests — babel :file header
//! with image output, agenda redo structure, element cache
//! after massive mutation, org-babel with :session :results
//! output combo, org-export with :footnotes and :todo-keywords
//! toggling, org-element-parse-buffer after org-cycle,
//! org-table-sort-lines, and org-src-lang-modes integration.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo66_babel_file_image_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:file-error void-variable))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil)
        (imgfile (make-temp-file "org-babel-image-" nil ".svg")))
    (insert (format "#+begin_src emacs-lisp :results file :file %s\n" imgfile))
    (insert "(with-temp-file imgfile (insert \"<svg></svg>\"))\nimgfile\n")
    (insert "#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp")
      (condition-case e
          (progn (push (org-babel-execute-src-block) r)
                 (push (list :file-exists (file-exists-p imgfile)) r))
        (error (push (list :file-error (car e)) r)))
      (condition-case nil (ignore-errors (delete-file imgfile)) (error nil))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo66_agenda_redo_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:redo-fbound t) (:todo-list-fbound t) (:todos-error t) (:tags-fbound nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-agenda)
  (insert "* TODO A :work:\n* TODO B :home:\n* DONE C :work:\n")
  (let ((r '()))
    (push (list :redo-fbound (fboundp 'org-agenda-redo)) r)
    (push (list :todo-list-fbound (fboundp 'org-agenda)) r)
    ;; org-agenda-get-todos
    (condition-case nil
        (let* ((todos (when (fboundp 'org-agenda-get-todos)
                        (org-agenda-get-todos))))
          (push (list :todos-fbound (fboundp 'org-agenda-get-todos)) r))
      (error (push (list :todos-error t) r)))
    ;; org-agenda-get-tags
    (condition-case nil
        (let* ((tags (when (fboundp 'org-agenda-get-tags)
                       (org-agenda-get-tags))))
          (push (list :tags-fbound (fboundp 'org-agenda-get-tags)) r))
      (error nil))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo66_element_cache_massive_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:init 3) (:after-add 13) (:after-delete 7))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* 1\n* 2\n* 3\n")
  (let ((r '()))
    ;; parse initial
    (push (list :init (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; massive mutation: add 10 headings
    (dotimes (i 10)
      (goto-char (point-max))
      (insert (format "\n* I%d\n" i)))
    ;; cache reset and reparse
    (condition-case nil (org-element-cache-reset) (error nil))
    (push (list :after-add (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; delete half
    (goto-char (point-min))
    (dotimes (i 6)
      (goto-char (point-min))
      (when (re-search-forward "^\\* I[0-9]+" nil t)
        (beginning-of-line)
        (kill-line)
        (kill-line)))
    (condition-case nil (org-element-cache-reset) (error nil))
    (push (list :after-delete (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo66_babel_session_output_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"ob-emacs-lisp backend does not support sessions\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results output :session out-session\n(princ \"A\")(princ \"B\")(princ \"C\")\n#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :results output :session out-session\n(princ \"D\")(princ \"E\")\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp :results output :session")
      (push (org-babel-execute-src-block) r)
      (search-forward "#+begin_src emacs-lisp :results output :session")
      (push (org-babel-execute-src-block) r)
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo66_export_footnotes_and_todo_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:has-footnote 67) (:has-todo 2) (:no-footnote t) (:no-todo t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-export-with-footnotes t)
        (org-export-with-todo-keywords t)
        (org-ascii-text-width 72))
    (insert "* TODO Test[fn:1]\nBody.\n[fn:1] A footnote.\n")
    (let ((r '()))
      (let ((out (org-export-as 'ascii nil nil t)))
        (push (list :has-footnote (and out (string-match-p "A footnote" out))) r)
        (push (list :has-todo (and out (string-match-p "TODO" out))) r))
      ;; with footnotes off
      (let ((org-export-with-footnotes nil)
            (org-export-with-todo-keywords nil))
        (let ((out (org-export-as 'ascii nil nil t)))
          (push (list :no-footnote (and out (not (string-match-p "A footnote" out)))) r)
          (push (list :no-todo (and out (not (string-match-p "TODO" out)))) r)))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo66_element_parse_after_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:folded-count 3) (:unfolded-count 3) (:folded-equals-unfolded t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\nBody B.\n* C\nBody C.\n")
  (let ((r '()))
    ;; fold all
    (goto-char (point-min))
    (org-overview)
    ;; parse (should still find all headlines even when invisible)
    (push (list :folded-count (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; show all
    (org-show-all)
    (push (list :unfolded-count (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; verify counts match
    (push (list :folded-equals-unfolded
                (= (plist-get (car (last r)) :folded-count)
                   (plist-get (car r) :unfolded-count))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo66_table_sort_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:after-sort #(\"| Name | Score |\\n|-------+-------|\\n| Alice |    95 |\\n| Bob   |    82 |\\n| Carol |    99 |\\n\" 0 16 (face org-table) 16 17 (face org-table-row) 17 34 (face org-table) 34 35 (face org-table-row) 35 52 (face org-table) 52 53 (face org-table-row) 53 70 (face org-table) 70 71 (face org-table-row) 71 88 (face org-table) 88 89 (face org-table-row))) (:to-lisp ((#(\"Name\" 0 4 (face org-table)) #(\"Score\" 0 5 (face org-table))) hline (#(\"Alice\" 0 5 (face org-table)) #(\"95\" 0 2 (face org-table))) (#(\"Bob\" 0 3 (face org-table)) #(\"82\" 0 2 (face org-table))) (#(\"Carol\" 0 5 (face org-table)) #(\"99\" 0 2 (face org-table))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| Name | Score |\n|-------+-------|\n| Alice |    95 |\n| Bob   |    82 |\n| Carol |    99 |\n")
  (let ((r '()))
    (goto-char (point-min))
    (forward-line 2)  ;; on Alice row
    ;; sort numerically on column 2 ascending
    (condition-case nil
        (progn (org-table-sort-lines nil ?n 2)
               (push (list :after-sort (buffer-string)) r)
               (goto-char (point-min))
               (push (list :to-lisp (org-table-to-lisp)) r))
      (error (push (list :sort-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo66_src_lang_modes_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:lang-modes-fbound t) (:src-mode-fbound t) (:edit-fbound t) (:exit-fbound t) (:fontify-fbound nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-src)
  (list
   ;; org-src-lang-modes
   (list :lang-modes-fbound (boundp 'org-src-lang-modes))
   ;; org-src-mode is available
   (list :src-mode-fbound (fboundp 'org-src-mode))
   ;; org-edit-src-code
   (list :edit-fbound (fboundp 'org-edit-src-code))
   ;; org-edit-src-exit
   (list :exit-fbound (fboundp 'org-edit-src-exit))
   ;; org-src-fontify-natively
   (list :fontify-fbound (boundp 'org-src-fontify-natively))
   ))"##,
        expect,
    );
}

#[test]
fn combo66_indent_mode_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:indent-fbound t) (:add-prop-fbound t) (:headlines-after 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-indent)
  (insert "* A\n** B\nBody B.\n*** C\nBody C.\n")
  (let ((r '()))
    ;; org-indent-mode
    (push (list :indent-fbound (fboundp 'org-indent-mode)) r)
    ;; org-indent-add-properties
    (push (list :add-prop-fbound (fboundp 'org-indent-add-properties)) r)
    ;; after enabling, parse should still work
    (condition-case nil
        (progn (when (fboundp 'org-indent-mode) (org-indent-mode 1))
               (push (list :headlines-after (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r))
      (error (push (list :indent-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo66_org_export_before_processing_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:hook-bound t) (:prune-ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox)
  (insert "* Test\nContent.\n")
  (let ((org-export-before-processing-hook
         '((lambda (backend)
             (goto-char (point-min))
             (while (re-search-forward "Content" nil t)
               (replace-match "MODIFIED"))))))
    (let* ((info (org-export-get-environment))
           (tree (org-element-parse-buffer))
           (r '()))
      ;; hook should modify buffer during processing
      (push (list :hook-bound (boundp 'org-export-before-processing-hook)) r)
      (condition-case nil
          (let ((modified-info (when (fboundp 'org-export--prune-tree)
                                 (org-export--prune-tree tree info))))
            (push (list :prune-ok t) r))
        (error (push (list :prune-error t) r)))
      (nreverse r))))"##,
        expect,
    );
}
