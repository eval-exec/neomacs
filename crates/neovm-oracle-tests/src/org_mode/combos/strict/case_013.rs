//! Combo-strict-13 oracle tests — more edge paths and data-driven
//! operations: org-datetree insertion, org-babel with :shebang and
//! :prologue, org-table-copy-down, org-footnote-action, org-insert-
//! structure-template, org-entities-help, org-babel with :results
//! scalar vs vector, org-table with formula debugger toggle,
//! org-element-cache-reset, org-babel-ref-resolve edge cases,
//! org-timestamp with dayname variants.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn strict_datetree_insertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 15 45)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-datetree)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (goto-char (point-min))
      ;; org-datetree-find-date-create creates or finds a date heading
      (condition-case nil
          (let ((pos (org-datetree-find-date-create (org-today))))
            (list
             :datetree-created (and pos (numberp pos))
             :headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))
             :levels (mapcar (lambda (h) (org-element-property :level h))
                             (org-element-map (org-element-parse-buffer) 'headline #'identity))))
        (error (list :datetree-error t)))))))"##,
        expect,
    );
}

#[test]
fn strict_babel_shebang_prologue() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ob-sh\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-sh)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src sh :results output :shebang \"#!/usr/bin/env bash\"\necho test\n#+end_src\n\n")
      (insert "#+begin_src sh :results output :prologue \"export FOO=bar\"\necho $FOO\n#+end_src\n")
      (let ((r '()))
        ;; execute shebang block
        (goto-char (point-min)) (search-forward "#+begin_src sh")
        (condition-case e
            (push (org-babel-execute-src-block) r)
          (error (push (list :shebang-error (car e)) r)))
        ;; execute prologue block
        (search-forward "#+begin_src sh")
        (condition-case e
            (push (org-babel-execute-src-block) r)
          (error (push (list :prologue-error (car e)) r)))
        (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_table_copy_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 15 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b |\n| 1 | 2 |\n|   |   |\n")
      (let ((r '()))
        (goto-char (point-min)) (forward-line 2)  ;; last row
        ;; org-table-copy-down: copy from above
        (condition-case nil
            (progn (org-table-copy-down 1)
                   (push (list :after-copy (buffer-substring-no-properties (point-min) (point-max))) r))
          (error (push (list :copy-error t) r)))
        (goto-char (point-min))
        (push (list :to-lisp (org-table-to-lisp)) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_footnote_action_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-footnote-renumber-fn-n)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1]\n[fn:1] Definition.\n")
      (let ((r '()))
        ;; on footnote definition
        (goto-char (point-min))
        (search-forward "[fn:1] Definition")
        (beginning-of-line)
        (push (list :on-def (org-footnote-at-definition-p)) r)
        ;; org-footnote-action on reference - just check context type
        (goto-char (point-min))
        (search-forward "[fn:1]") (backward-char 1)
        (push (list :context-type (org-element-type (org-element-context))) r)
        ;; manually delete reference to avoid timeout in org-footnote-action
        (goto-char (point-min))
        (search-forward "[fn:1]") (backward-char 1)
        (let ((start (point)))
          (forward-char 6)
          (delete-region start (point)))
        ;; renumber remaining
        (org-footnote-renumber-fn-n)
        (push (list :remaining-refs (length (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_insert_structure_template() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 20 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-tempo)
  (let ((org-structure-template-alist
         '(("s" . "src")
           ("e" . "example")
           ("q" . "quote")
           ("v" . "verse")
           ("c" . "center")))
        (r '()))
    (push (list :template-keys (mapcar #'car org-structure-template-alist)) r)
    (push (list :template-types (mapcar #'cdr org-structure-template-alist)) r)
    (with-temp-buffer (org-mode)
      ;; insert <s and try completion
      (insert "<s")
      (condition-case nil
          (progn (org-try-structure-completion)
                 (push (list :template-s (buffer-string)) r))
        (error (push (list :template-s-error t) r))))
    (nreverse r))))"##,
        expect,
    );
}

#[test]
fn strict_entities_help_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp \"* Letters\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-entities)
  (list
   ;; org-entities-help should list entities
   (list :help-fbound (fboundp 'org-entities-help))
   ;; entity count
   (list :total-entities (length org-entities))
   ;; entity completeness check
   (list :alpha-complete (and (nth 0 (assoc "Alpha" org-entities))
                              (nth 0 (assoc "alpha" org-entities))))
   ;; entity types
   (list :entity-types (let ((types '()))
                         (dolist (ent org-entities)
                           (push (nth 1 ent) types))
                         (delete-dups types)))
   ;; user entity lookup
   (list (org-entity-get "dots"))
   (list (org-entity-get "hellip"))))"##,
        expect,
    );
}

#[test]
fn strict_babel_results_scalar_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 17 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer (org-mode)
      (insert "#+begin_src emacs-lisp :results value scalar\n42\n#+end_src\n\n")
      (insert "#+begin_src emacs-lisp :results value vector\n'(1 2 3)\n#+end_src\n\n")
      (insert "#+begin_src emacs-lisp :results value\n'(a b c)\n#+end_src\n")
      (let ((r '()))
        (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp :results value scalar")
        (push (org-babel-execute-src-block) r)
        (search-forward "#+begin_src emacs-lisp :results value vector")
        (push (org-babel-execute-src-block) r)
        (search-forward "#+begin_src emacs-lisp :results value")
        (push (org-babel-execute-src-block) r)
        (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_table_formula_debugger() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b | c |\n| 1 | 2 |   |\n")
      (insert "#+TBLFM: $3=$1+$2\n")
      (let ((r '()))
        ;; toggle formula debugger
        (push (list :debugger-fbound (fboundp 'org-table-toggle-formula-debugger)) r)
        ;; recalc normally
        (goto-char (point-min))
        (org-table-recalculate t) (org-table-align)
        (push (list :recalc-result (buffer-string)) r)
        ;; get cell
        (goto-char (point-min)) (forward-line 1)
        (push (list :cell-c (org-table-get "c" nil)) r)
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_element_cache_reset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 24 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\n** B\n* C\n")
      (let ((r '()))
        ;; parse once
        (let ((t1 (org-element-parse-buffer)))
          (push (list :before-headlines (length (org-element-map t1 'headline #'identity))) r))
        ;; add heading
        (goto-char (point-max))
        (insert "\n* D\n")
        ;; reset cache
        (condition-case nil
            (progn (org-element-cache-reset)
                   (push (list :cache-reset t) r))
          (error (push (list :cache-reset-error t) r)))
        ;; re-parse
        (let ((t2 (org-element-parse-buffer)))
          (push (list :after-headlines (length (org-element-map t2 'headline #'identity))) r)
          (push (list :after-raw (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                         (org-element-map t2 'headline #'identity))) r))
        (nreverse r))))))"##,
        expect,
    );
}

#[test]
fn strict_timestamp_dayname_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; with dayname
   (org-timestamp-format (org-timestamp-from-string "<2024-06-15 Sat>") "%Y-%m-%d %a")
   ;; without dayname in source
   (condition-case nil
       (org-timestamp-format (org-timestamp-from-string "<2024-06-15>") "%Y-%m-%d")
     (error :no-dayname-error))
   ;; with time
   (org-timestamp-format (org-timestamp-from-string "<2024-06-15 Sat 10:30>") "%H:%M")
   ;; active range with format
   (let ((ts (org-timestamp-from-string "<2024-01-01 Mon>--<2024-01-07 Sun>")))
     (list :range-type (org-element-property :type ts)
           :year-start (org-element-property :year-start ts)
           :year-end (org-element-property :year-end ts)))
   ;; inactive
   (let ((ts (org-timestamp-from-string "[2024-12-25 Wed]")))
     (list :inactive-type (org-element-property :type ts))))))"##,
        expect,
    );
}

#[test]
fn strict_babel_ref_resolve() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 11 37)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-ref)
  (with-temp-buffer (org-mode)
    (insert "#+name: alpha\n| 10 |\n| 20 |\n\n")
    ;; babel ref resolve
    (condition-case nil
        (let ((val (org-babel-ref-resolve "alpha")))
          (list :ref-resolved (and val (listp val))
                :ref-type (type-of val)))
      (error (list :ref-error t))))))"##,
        expect,
    );
}

#[test]
fn strict_org_table_export_to_csv() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 15 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b | c |\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |\n")
      (goto-char (point-min))
      (let ((r '()))
        ;; org-table-to-lisp gives the structured data
        (push (list :to-lisp (org-table-to-lisp)) r)
        ;; org-table-export may exist
        (push (list :export-fbound (fboundp 'org-table-export)) r)
        ;; cell count
        (push (list :cell-count (length (org-element-map (org-element-parse-buffer) 'table-cell #'identity))) r)
        (push (list :row-count (length (org-element-map (org-element-parse-buffer) 'table-row #'identity))) r)
        (nreverse r))))))"##,
        expect,
    );
}
