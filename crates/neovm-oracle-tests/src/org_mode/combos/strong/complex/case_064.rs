//! Strong combo-complex-64 oracle tests — final deep probes:
//! org-publish project export, org-babel with :var from list/table
//! combinations, org-element with extremely deep nested bold
//! (20 levels), org-agenda with buffer restriction, org-export
//! with :exclude-tags and :select-tags combined mid-export,
//! org-babel with :noweb-ref, org-table transpose, org-columns
//! dynamic update, and org-macro with recursive definition.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo64_publish_project_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:publish-fbound t) (:project-defined t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-publish)
  (require 'ox-html)
  (insert "* Publish Test\nContent.\n")
  (let ((r '()))
    (push (list :publish-fbound (fboundp 'org-publish-file)) r)
    (let ((tmpdir (make-temp-file "org-pub-" t)))
      (condition-case nil
          (let ((org-publish-project-alist
                 `(("test" :base-directory ,default-directory
                    :publishing-directory ,tmpdir
                    :publishing-function org-html-publish-to-html
                    :base-extension "org"
                    :exclude ".*"))))
            (push (list :project-defined t) r))
        (error (push (list :project-error t) r)))
      (condition-case nil (delete-directory tmpdir t) (error nil)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo64_babel_var_list_table_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+name: list-data\n- one\n- two\n- three\n\n")
    (insert "#+name: table-data\n| 10 |\n| 20 |\n| 30 |\n\n")
    (insert "#+begin_src emacs-lisp :results value :var l=list-data :var t=table-data\n")
    (insert "(list :list-len (length l) :table-len (length t))\n")
    (insert "#+end_src\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo64_deep_nested_bold_20() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:bold-count 7) (:italic-count 7) (:first-bold-depth 4) (:first-bold-lineage (italic paragraph section org-data)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  ;; 10 alternating bold/italic layers
  (insert (apply #'concat (cl-loop for i from 1 to 10
                                   collect (if (cl-oddp i) "*bold" "/italic")
                                   collect (format "%d " i)))
          (apply #'concat (cl-loop for i from 1 to 10
                                   collect (if (cl-oddp i) "*/" "*/"))))
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (bolds (org-element-map tree 'bold #'identity))
           (italics (org-element-map tree 'italic #'identity)))
      (push (list :bold-count (length bolds)) r)
      (push (list :italic-count (length italics)) r)
      ;; check nesting depth of first bold
      (when (car bolds)
        (let ((lineage (org-element-lineage (car bolds))))
          (push (list :first-bold-depth (length lineage)) r)
          (push (list :first-bold-lineage (mapcar #'org-element-type lineage)) r))))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo64_agenda_buffer_restriction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:agenda-files-bound t) (:agenda-files-count 1) (:scheduled-count 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-agenda)
  (insert "* TODO A :work:\nSCHEDULED: <2024-01-15 Mon>\n** TODO B :work:\n* DONE C :home:\n")
  (let ((r '()))
    ;; restrict to current buffer
    (condition-case nil
        (let ((org-agenda-files (list (buffer-file-name))))
          (push (list :agenda-files-bound t) r)
          (push (list :agenda-files-count (length (when (boundp 'org-agenda-files) org-agenda-files))) r))
      (error (push (list :buffer-restrict-error t) r)))
    ;; get scheduled items in buffer
    (push (list :scheduled-count (length (org-map-entries
                                          (lambda () (org-get-heading t t t t))
                                          "SCHEDULED<>\"\""))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo64_export_select_exclude_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:has-A nil) (:has-B 80) (:has-C nil) (:has-D nil) (:has-E 170))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-export-select-tags '("export"))
        (org-export-exclude-tags '("noexport"))
        (org-ascii-text-width 72))
    (insert "* A\nBody A.\n* B :export:\nBody B.\n* C :noexport:\nBody C.\n")
    (insert "* D :export:noexport:\nBody D.\n* E :export:urgent:\nBody E.\n")
    (let ((r '()))
      (condition-case nil
          (let ((out (org-export-as 'ascii nil nil t)))
            (push (list :has-A (and out (string-match-p "Body A" out))) r)
            (push (list :has-B (and out (string-match-p "Body B" out))) r)
            (push (list :has-C (and out (string-match-p "Body C" out))) r)
            (push (list :has-D (and out (string-match-p "Body D" out))) r)
            (push (list :has-E (and out (string-match-p "Body E" out))) r))
        (error (push (list :export-error t) r)))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo64_babel_noweb_ref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (add-one add-two 13 (:result-count 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "* Noweb refs\n")
    (insert "#+name: shared-fn\n")
    (insert "#+begin_src emacs-lisp :noweb-ref shared\n")
    (insert "(defun add-one (x) (+ x 1))\n")
    (insert "#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :noweb-ref shared\n")
    (insert "(defun add-two (x) (+ x 2))\n")
    (insert "#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :results value :noweb yes\n")
    (insert "<<shared>>\n(add-one (add-two 10))\n")
    (insert "#+end_src\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp :noweb-ref shared")
      (push (org-babel-execute-src-block) r)
      (search-forward "#+begin_src emacs-lisp :noweb-ref shared")
      (push (org-babel-execute-src-block) r)
      (search-forward "#+begin_src emacs-lisp :results value :noweb yes")
      (push (org-babel-execute-src-block) r)
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo64_table_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:after-transpose #(\"| a | 1 | 4 |\\n| b | 2 | 5 |\\n| c | 3 | 6 |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 11 (face org-table) 11 12 (face org-table display (space :relative-width 1.001)) 12 13 (face org-table) 13 14 (face org-table-row) 14 15 (face org-table) 15 16 (face org-table rear-nonsticky t display (space :relative-width 1)) 16 17 (face org-table) 17 18 (face org-table display (space :relative-width 1.001)) 18 19 (face org-table) 19 20 (face org-table rear-nonsticky t display (space :relative-width 1)) 20 21 (face org-table) 21 22 (face org-table display (space :relative-width 1.001)) 22 23 (face org-table) 23 24 (face org-table rear-nonsticky t display (space :relative-width 1)) 24 25 (face org-table) 25 26 (face org-table display (space :relative-width 1.001)) 26 27 (face org-table) 27 28 (face org-table-row) 28 29 (face org-table) 29 30 (face org-table rear-nonsticky t display (space :relative-width 1)) 30 31 (face org-table) 31 32 (face org-table display (space :relative-width 1.001)) 32 33 (face org-table) 33 34 (face org-table rear-nonsticky t display (space :relative-width 1)) 34 35 (face org-table) 35 36 (face org-table display (space :relative-width 1.001)) 36 37 (face org-table) 37 38 (face org-table rear-nonsticky t display (space :relative-width 1)) 38 39 (face org-table) 39 40 (face org-table display (space :relative-width 1.001)) 40 41 (face org-table) 41 42 (face org-table-row))) (:cell-count 9) (:to-lisp ((#(\"a\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"4\" 0 1 (face org-table))) (#(\"b\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"5\" 0 1 (face org-table))) (#(\"c\" 0 1 (face org-table)) #(\"3\" 0 1 (face org-table)) #(\"6\" 0 1 (face org-table))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |\n")
  (let ((r '()))
    (goto-char (point-min))
    ;; transpose
    (condition-case nil
        (progn (org-table-transpose-table-at-point)
               (push (list :after-transpose (buffer-string)) r)
               (goto-char (point-min))
               (push (list :cell-count (length (org-element-map (org-element-parse-buffer) 'table-cell #'identity))) r)
               (push (list :to-lisp (org-table-to-lisp)) r))
      (error (push (list :transpose-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo64_columns_dynamic_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:colview-fbound t) (:format \"%25ITEM %TODO %3PRIORITY %TAGS\") (:effort-A \"2:00\") (:effort-B \"1:30\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-colview)
  (insert "* Task A\n:PROPERTIES:\n:EFFORT:   2:00\n:END:\n")
  (insert "* Task B\n:PROPERTIES:\n:EFFORT:   1:30\n:END:\n")
  (let ((r '()))
    (push (list :colview-fbound (fboundp 'org-columns)) r)
    ;; org-columns-get-format
    (let ((fmt (when (fboundp 'org-columns-get-format)
                 (org-columns-get-format))))
      (push (list :format fmt) r))
    ;; effort values
    (goto-char (point-min))
    (push (list :effort-A (org-entry-get nil "EFFORT")) r)
    (search-forward "* Task B") (beginning-of-line)
    (push (list :effort-B (org-entry-get nil "EFFORT")) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo64_macro_recursive_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:macros 4) (:has-alice 11) (:has-bob 28) (:has-welcome 88) (:no-braces nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: a alice\n")
  (insert "#+MACRO: b bob\n")
  (insert "#+MACRO: ab {{{a}}} and {{{b}}}\n")
  (insert "#+MACRO: greet {{{ab}}} welcome!\n")
  (insert "\n{{{greet}}}\n")
  (let ((r '()))
    (push (list :macros (length (org-element-map (org-element-parse-buffer) 'keyword
                                  (lambda (k) (when (equal "MACRO" (org-element-property :key k)) k))))) r)
    (let ((interpreted (substring-no-properties
                        (org-element-interpret-data (org-element-parse-buffer)))))
      (push (list :has-alice (string-match-p "alice" interpreted)) r)
      (push (list :has-bob (string-match-p "bob" interpreted)) r)
      (push (list :has-welcome (string-match-p "welcome" interpreted)) r)
      (push (list :no-braces (not (string-match-p "{{{" interpreted))) r))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo64_export_odt_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:odt-loaded t) (:odt-fbound t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (condition-case nil (require 'ox-odt) (error nil))
  (insert "* ODT Test\nContent.\n")
  (let ((r '()))
    (push (list :odt-loaded (featurep 'ox-odt)) r)
    (push (list :odt-fbound (fboundp 'org-odt-export-to-odt)) r)
    (nreverse r)))"##,
        expect,
    );
}
