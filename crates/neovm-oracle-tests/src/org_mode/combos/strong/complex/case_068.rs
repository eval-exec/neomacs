//! Strong combo-complex-68 oracle tests — final batch:
//! org-capture finalize, org-table named formulas cross-ref,
//! org-babel with :results html, org-export with body-only
//! for all backends, org-element with org-element-parse-secondary-
//! string, org-entities with replace, org-insert-heading with
//! force-heading, and org-babel-get-src-block-info edge cases.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo68_capture_finalize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:finalize-fbound t :capture-get-fbound t :capture-put-fbound t :capture-kill-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-capture)
  (list
   :finalize-fbound (fboundp 'org-capture-finalize)
   :capture-get-fbound (fboundp 'org-capture-get)
   :capture-put-fbound (fboundp 'org-capture-put)
   :capture-kill-fbound (fboundp 'org-capture-kill)
   ))"##,
        expect,
    );
}

#[test]
fn combo68_table_named_formula_cross_ref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((:after-recalc \"#+name: rates\\n| item | price |\\n|------+-------|\\n| A    |    10 |\\n| B    |    25 |\\n\\n#+name: orders\\n| item | qty | total |\\n|------+-----+-------|\\n| A    |   3 |     0 |\\n| B    |   2 |     0 |\\n#+TBLFM: $3=$2*remote(rates,@@#$3)\\n\") (:table-count 2))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+name: rates\n| item | price |\n|------+-------|\n| A    |    10 |\n| B    |    25 |\n\n")
  (insert "#+name: orders\n| item | qty | total |\n|------+-----+-------|\n| A    |   3 |       |\n| B    |   2 |       |\n")
  (insert "#+TBLFM: $3=$2*remote(rates,@@#$3)\n")
  (let ((r '()))
    (goto-char (point-min))
    (search-forward "orders") (forward-line) (forward-line)
    (condition-case nil
        (progn (org-table-recalculate t) (org-table-align)
               (push (list :after-recalc (buffer-substring-no-properties (point-min) (point-max))) r))
      (error (push (list :recalc-error t) r)))
    (goto-char (point-min))
    (push (list :table-count (length (org-element-map (org-element-parse-buffer) 'table #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo68_babel_results_html() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"<b>bold</b>\" (:result-count 0) (:buffer \"#+begin_src emacs-lisp :results html\\n\\\"<b>bold</b>\\\"\\n#+end_src\\n\\n#+RESULTS:\\n#+begin_export html\\n<b>bold</b>\\n#+end_export\\n\"))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results html\n\"<b>bold</b>\"\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo68_export_body_only_all_backends() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:error t) (:error t) (:error t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (require 'ox-html)
  (require 'ox-latex)
  (let ((org-export-show-temporary-export-buffer nil))
    (insert "* H\nBody.\n")
    (let ((r '()))
      (dolist (backend '(ascii html latex))
        (condition-case nil
            (let ((out (org-export-as backend nil nil t t)))  ;; body-only
              (push (list :ok (and out (> (length out) 0))) r))
          (error (push (list :error t) r))))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo68_element_parse_secondary_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:error t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  ;; test org-element-parse-secondary-string
  (condition-case nil
      (let ((result (org-element-parse-secondary-string
                     "*bold* /italic/" 'bold)))
        (list :result-type (when result (type-of result))))
    (error (list :error t))))"##,
        expect,
    );
}

#[test]
fn combo68_entities_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 14 38)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-entities)
  (list
   ;; org-entity-get replacement
   (let ((ent (org-entity-get "alpha")))
     (list :full (nth 6 ent)
           :ascii (nth 1 ent)
           :latex (nth 2 ent)
           :html (nth 3 ent)))
   ;; org-entity-get with math
   (let ((ent (org-entity-get "sum")))
     (list :sum-ascii (nth 1 ent)
           :sum-latex (nth 2 ent))))))"##,
        expect,
    );
}

#[test]
fn combo68_insert_heading_force_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:after-insert \"- item 1\\n- item 2\\n* New heading\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item 1\n- item 2\n")
  (let ((r '()))
    ;; org-insert-heading-respect-content
    (goto-char (point-min))
    (search-forward "item 1") (end-of-line)
    (condition-case nil
        (progn (org-insert-heading-respect-content t)
               (insert "New heading")
               (push (list :after-insert (buffer-string)) r))
      (error (push (list :insert-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo68_babel_get_src_block_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:info-not-nil t) (:language \"emacs-lisp\") (:body \"(+ x 10)\") (:var-param (x . 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-core)
  (insert "#+begin_src emacs-lisp :results value :var x=5\n(+ x 10)\n#+end_src\n")
  (let ((r '()))
    (goto-char (point-min))
    (search-forward "#+begin_src emacs-lisp")
    (beginning-of-line)
    (let ((info (org-babel-get-src-block-info)))
      (push (list :info-not-nil (and info (listp info))) r)
      (when info
        (push (list :language (car info)) r)
        (push (list :body (nth 1 info)) r)
        (let ((params (nth 2 info)))
          (push (list :var-param (cdr (assoc :var params))) r))))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo68_agenda_file_skip_archive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:skip-archived-trees-fbound t :skip-comment-trees-fbound t :skip-function-fbound t :skip-scheduled-if-done t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-agenda)
  (list
   :skip-archived-trees-fbound (boundp 'org-agenda-skip-archived-trees)
   :skip-comment-trees-fbound (boundp 'org-agenda-skip-comment-trees)
   :skip-function-fbound (boundp 'org-agenda-skip-function)
   :skip-scheduled-if-done (boundp 'org-agenda-skip-scheduled-if-done)
   ))"##,
        expect,
    );
}

#[test]
fn combo68_org_table_goto_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:goto-line-fbound t) (:current-line-fbound t) (:goto-col-fbound t) (:current-cell \"1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |\n|---+---+---|\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |\n")
  (let ((r '()))
    (goto-char (point-min))
    (forward-line 2)  ;; on first data row
    (forward-char 2)  ;; inside cell
    ;; org-table-goto-line
    (push (list :goto-line-fbound (fboundp 'org-table-goto-line)) r)
    ;; org-table-current-line
    (push (list :current-line-fbound (fboundp 'org-table-current-line)) r)
    ;; org-table-goto-column
    (push (list :goto-col-fbound (fboundp 'org-table-goto-column)) r)
    ;; get current cell
    (push (list :current-cell (org-table-get nil nil)) r)
    (nreverse r)))"##,
        expect,
    );
}
