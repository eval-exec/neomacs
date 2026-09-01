//! Combo-strict-17 oracle tests — edge probes for babel with
//! :results list/table, org-element with parse-secondary-string
//! edge, org-timestamp with from-time/formatter, org-plot with
//! data extraction, org-babel with :cache yes, org-macro with
//! argument-less expansion, and org-element with text that looks
//! like markup but isn't.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn strict_babel_results_list_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 14 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src emacs-lisp :results list\n'(a b c d e)\n#+end_src\n\n")
      (insert "#+begin_src emacs-lisp :results table\n'((1 2) (3 4))\n#+end_src\n")
      (let ((r '()))
        (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp :results list")
        (push (org-babel-execute-src-block) r)
        (search-forward "#+begin_src emacs-lisp :results table")
        (push (org-babel-execute-src-block) r)
        (push (list :table-count (length (org-element-map (org-element-parse-buffer) 'table #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_timestamp_from_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; org-timestamp-from-time
   (condition-case nil
       (let* ((ts (org-timestamp-from-time (float-time) nil))
              (props (list (org-element-property :type ts)
                           (org-element-property :year-start ts)
                           (numberp (org-element-property :year-start ts)))))
         (list :from-time-ok props))
     (error (list :from-time-error t)))
   ;; org-timestamp-from-string -> org-timestamp-format roundtrip
   (let ((ts (org-timestamp-from-string "<2024-03-15 Fri>")))
     (list :format-short (org-timestamp-format ts "%d/%m")
           :format-iso (org-timestamp-format ts "%Y-%m-%d")
           :format-long (org-timestamp-format ts "%B %d, %Y %A"))))))"##,
        expect,
    );
}

#[test]
fn strict_plot_data_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:plot-fbound t :table-plot-info-fbound nil :plot-presets-bound t :gnuplot-available :not-found)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-plot)
  (list
   :plot-fbound (fboundp 'org-plot/gnuplot)
   :table-plot-info-fbound (boundp 'org-plot/gnuplot-to-grid-data)
   :plot-presets-bound (boundp 'org-plot/preset-plot-types)
   :gnuplot-available (or (executable-find "gnuplot") :not-found)
   ))"##,
        expect,
    );
}

#[test]
fn strict_babel_cache_invalidation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 17 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src emacs-lisp :results value :cache yes\n(+ 10 20)\n#+end_src\n")
      (let ((r '()))
        ;; execute once
        (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp")
        (push (org-babel-execute-src-block) r)
        (push (list :after1 (buffer-string)) r)
        ;; execute again (should use cache)
        (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp")
        (push (org-babel-execute-src-block) r)
        (push (list :after2 (buffer-string)) r)
        (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_macro_no_args_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 13 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    ;; test macro with no arguments
    (with-temp-buffer (org-mode)
      (insert "#+MACRO: heart (eval \"\u2665\")\n")
      (insert "I {{{heart}}} org-mode.\n")
      (let* ((tree (org-element-parse-buffer))
             (interpreted (substring-no-properties (org-element-interpret-data tree)))
             (r '()))
        (push (list :has-heart (string-match-p "\u2665" interpreted)) r)
        (push (list :interpreted-length (> (length interpreted) 0)) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_element_text_looks_like_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 18 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      ;; content that looks like markup but shouldn't be parsed as such
      (insert "*stars* at start of word is NOT bold.\n")
      (insert "/italic/ with leading slash also not italic.\n")
      (insert "This=is=not=verbatim with multi-equals.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (bolds (org-element-map tree 'bold #'identity))
             (italics (org-element-map tree 'italic #'identity))
             (verbatims (org-element-map tree 'verbatim #'identity))
             (r '()))
        (push (list :bold-count (length bolds)) r)
        (push (list :italic-count (length italics)) r)
        (push (list :verbatim-count (length verbatims)) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_babel_session_value_persistence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"ob-emacs-lisp backend does not support sessions\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src emacs-lisp :results value :session strict17\n(setq strict17-x 99)\n#+end_src\n\n")
      (insert "#+begin_src emacs-lisp :results value :session strict17\n(+ strict17-x 1)\n#+end_src\n")
      (let ((r '()))
        (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp :results value :session strict17")
        (push (org-babel-execute-src-block) r)
        (search-forward "#+begin_src emacs-lisp :results value :session strict17")
        (push (org-babel-execute-src-block) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_element_parse_secondary_string_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 13 82)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (list
   ;; parse-secondary-string for plain text
   (let ((result (org-element-parse-secondary-string
                  "plain text" nil)))
     (list :type (type-of result)
           :length (length result)))
   ;; parse-secondary-string for bold
   (let ((result (org-element-parse-secondary-string
                  "*bold* text" '(bold italic))))
     (list :has-bold (> (length (org-element-map result 'bold #'identity)) 0))))))"##,
        expect,
    );
}

#[test]
fn strict_org_table_hline_movement() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 16 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b |\n|---+---|\n| 1 | 2 |\n| 3 | 4 |\n")
      (let ((r '()))
        (goto-char (point-min))
        ;; move hline
        (push (list :move-hline-fbound (fboundp 'org-table-move-row)) r)
        ;; org-table-kill-row
        (push (list :kill-row-fbound (fboundp 'org-table-kill-row)) r)
        ;; org-table-insert-hline
        (push (list :insert-hline-fbound (fboundp 'org-table-insert-hline)) r)
        ;; count rows
        (push (list :row-count (length (org-element-map (org-element-parse-buffer) 'table-row #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_org_agenda_custom_commands() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:custom-commands-bound t :sticky-fbound t :span-fbound t :start-day-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-agenda)
  (list
   :custom-commands-bound (boundp 'org-agenda-custom-commands)
   :sticky-fbound (boundp 'org-agenda-sticky)
   :span-fbound (boundp 'org-agenda-span)
   :start-day-fbound (boundp 'org-agenda-start-day)
   ))"##,
        expect,
    );
}
