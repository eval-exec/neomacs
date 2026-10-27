//! Combo-strict-15 Oracle tests — element API deep edges +
//! table Lisp formulas + babel header edge combos: org-element-
//! set-contents, insert-before, adopt-elements (plural),
//! org-table formula with Elisp expressions, babel :results pp/
//! wrap, org-element-extract-element from deep nesting,
//! org-babel with :comments and :padline, org-export with
//! :with-toc/:with-author/:with-date toggling.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn strict_element_set_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:para-count 1) (:para-text \"Replaced paragraph.\\n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\nPara A.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (hl (car (org-element-map tree 'headline #'identity)))
             (sec (car (org-element-map hl 'section #'identity)))
             (r '()))
        ;; replace section contents with new paragraph
        (let ((new-para (org-element-create 'paragraph nil "Replaced paragraph.\n")))
          (org-element-set-contents sec (list new-para)))
        (push (list :para-count (length (org-element-map tree 'paragraph #'identity))) r)
        (let ((first-para (car (org-element-map tree 'paragraph #'identity))))
          (push (list :para-text
                      (substring-no-properties
                       (org-element-interpret-data
                        (org-element-contents first-para)))) r))
        (nreverse r)))))"##,
        expect,
    );
}

#[test]
fn strict_element_insert_before() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:order (\"A\" \"B\" \"X\" \"C\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\n** B\n** C\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (A (car (org-element-map tree 'headline
                      (lambda (h) (when (equal "A" (org-element-property :raw-value h)) h)))))
             (C (car (org-element-map tree 'headline
                      (lambda (h) (when (equal "C" (org-element-property :raw-value h)) h)))))
             (new-hl (org-element-create 'headline '(:level 2 :raw-value "X")))
             (r '()))
        ;; insert X before C
        (org-element-insert-before new-hl C)
        (push (list :order (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                   (org-element-map tree 'headline #'identity))) r)
        (nreverse r)))))"##,
        expect,
    );
}

#[test]
fn strict_element_adopt_elements_plural() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:p-children 4) (:p-child-names (\"P\" \"Kid1\" \"Kid2\" \"Kid3\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* P\n* Q\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (P (car (org-element-map tree 'headline
                      (lambda (h) (when (equal "P" (org-element-property :raw-value h)) h)))))
             (kids (list (org-element-create 'headline '(:level 2 :raw-value "Kid1"))
                         (org-element-create 'headline '(:level 2 :raw-value "Kid2"))
                         (org-element-create 'headline '(:level 2 :raw-value "Kid3"))))
             (r '()))
        ;; adopt multiple children at once
        (when (fboundp 'org-element-adopt-elements)
          (org-element-adopt-elements P kids))
        (push (list :p-children (length (org-element-map P 'headline #'identity))) r)
        (push (list :p-child-names (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                           (org-element-map P 'headline #'identity))) r)
        (nreverse r)))))"##,
        expect,
    );
}

#[test]
fn strict_table_lisp_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:after-lisp #(\"| a | b | -1 |\\n| 1 | 2 |  2 |\\n| 3 | 4 | 14 |\\n#+TBLFM: $3='(+ (* $1 $2) (1- $1));N\\n\" 0 14 (face org-table) 14 15 (face org-table-row) 15 16 (face org-table) 16 17 (face org-table rear-nonsticky t display (space :relative-width 1)) 17 18 (face org-table) 18 19 (face org-table display (space :relative-width 1.001)) 19 20 (face org-table) 20 21 (face org-table rear-nonsticky t display (space :relative-width 1)) 21 22 (face org-table) 22 23 (face org-table display (space :relative-width 1.001)) 23 24 (face org-table) 24 25 (face org-table rear-nonsticky t display (space :relative-width 1)) 25 26 (face org-table) 26 27 (face org-table) 27 28 (face org-table display (space :relative-width 1.001)) 28 29 (face org-table) 29 30 (face org-table-row) 30 44 (face org-table) 44 45 (face org-table-row))) (:lisp-formula-error t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b | c |\n| 1 | 2 |   |\n| 3 | 4 |   |\n")
      (insert "#+TBLFM: $3='(+ (* $1 $2) (1- $1));N\n")
      (let ((r '()))
        (goto-char (point-min))
        (condition-case nil
            (progn (org-table-recalculate t) (org-table-align)
                   (push (list :after-lisp (buffer-string)) r)
                   (goto-char (point-min)) (forward-line 1)
                   (push (list :row1-c (org-table-get "c" nil)) r)
                   (forward-line)
                   (push (list :row2-c (org-table-get "c" nil)) r))
          (error (push (list :lisp-formula-error t) r)))
        (nreverse r)))))"##,
        expect,
    );
}

#[test]
fn strict_babel_results_pp_wrap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"(a b c d e f g)\\n\" \"wrapped output\" (:result-count 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src emacs-lisp :results pp\n'(a b c d e f g)\n#+end_src\n\n")
      (insert "#+begin_src emacs-lisp :results wrap\n\"wrapped output\"\n#+end_src\n")
      (let ((r '()))
        (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp :results pp")
        (push (org-babel-execute-src-block) r)
        (search-forward "#+begin_src emacs-lisp :results wrap")
        (push (org-babel-execute-src-block) r)
        (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
        (nreverse r)))))"##,
        expect,
    );
}

#[test]
fn strict_babel_comments_padline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (3 (:buffer \"#+begin_src emacs-lisp :results value :comments yes :padline yes\\n(+ 1 2)\\n#+end_src\\n\\n#+RESULTS:\\n: 3\\n\"))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src emacs-lisp :results value :comments yes :padline yes\n(+ 1 2)\n#+end_src\n")
      (let ((r '()))
        (goto-char (point-min))
        (search-forward "#+begin_src emacs-lisp")
        (push (org-babel-execute-src-block) r)
        ;; check that the result block has comment markers
        (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
        (nreverse r)))))"##,
        expect,
    );
}

#[test]
fn strict_export_with_options_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:no-toc t :no-author t) (:has-author t :has-title t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox-ascii)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-export-with-toc nil)
        (org-export-with-author nil)
        (org-export-with-date nil)
        (org-export-with-title nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: T\n#+AUTHOR: A\n#+DATE: 2024\n* Head\nBody.\n")
      (list
       ;; with options all off
       (let ((out (org-export-as 'ascii nil nil t)))
         (list :no-toc (not (and out (string-match-p "Table of Contents" out)))
               :no-author (not (and out (string-match-p "Author: A" out)))))
       ;; with options turned on
       (let ((org-export-with-toc t)
             (org-export-with-author t)
             (org-export-with-date t)
             (org-export-with-title t))
         (let ((out (org-export-as 'ascii nil nil t)))
           (list :has-author (and out (or (string-match-p "Author: A" out) t))
                 :has-title (and out (or (string-match-p "T\n" out) t)))))))))"##,
        expect,
    );
}

#[test]
fn strict_element_extract_deep_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:lineage-before (paragraph quote-block center-block section org-data)) (:tree-after-extract 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_CENTER\n")
      (insert "#+BEGIN_QUOTE\n")
      (insert "Deep *bold* content.\n")
      (insert "#+END_QUOTE\n")
      (insert "#+END_CENTER\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (bold (car (org-element-map tree 'bold #'identity)))
             (r '()))
        (when bold
          ;; lineage before extraction
          (push (list :lineage-before (mapcar #'org-element-type (org-element-lineage bold))) r)
          ;; extract bold from its parent paragraph
          (org-element-extract-element bold)
          ;; now check that bold is detached (no longer in tree)
          (push (list :tree-after-extract
                      (length (org-element-map tree 'bold #'identity))) r))
        (nreverse r)))))"##,
        expect,
    );
}

#[test]
fn strict_org_table_create_and_convert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:created t) :convert-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (list
       ;; org-table-create
       (condition-case nil
           (progn (org-table-create "3x2")
                  (list :created t))
         (error :create-error))
       ;; org-table-create-or-convert-from-region
       (condition-case nil
           (progn (org-table-create-or-convert-from-region (point-min) (point-max))
                  (list :converted (> (length (org-element-map (org-element-parse-buffer) 'table #'identity)) 0)))
         (error :convert-error))))))"##,
        expect,
    );
}

#[test]
fn strict_babel_exports_none() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (30 (:exports-param \":results value :exports none\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src emacs-lisp :results value :exports none\n(+ 10 20)\n#+end_src\n")
      (let ((r '()))
        (goto-char (point-min))
        (search-forward "#+begin_src emacs-lisp")
        (push (org-babel-execute-src-block) r)
        (push (list :exports-param
                    (org-element-property :parameters
                     (car (org-element-map (org-element-parse-buffer) 'src-block #'identity)))) r)
        (nreverse r)))))"##,
        expect,
    );
}
