//! Strong combo-complex-51 oracle tests — deep multi-step divergence-prone
//! workflows: clone subtree mutations, narrow/edit/widen/reparse chains,
//! table formula error recovery, babel result type changes, property
//! special-char cycles, fold/edit/unfold content verification, multi-
//! heading mutation remap, export select+exclude tag combos, and
//! element adopt/extract/adopt-back consistency.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Clone subtree → modify clone → verify isolation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo51_clone_mutate_verify_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Original\n:PROPERTIES:\n:ID: orig-1\n:END:\nBody.\n")
  (let ((r '()))
    ;; clone
    (goto-char (point-min))
    (let ((clone-pos (org-clone-subtree-with-time-shift 1 nil)))
      (push (list :clone-created (and clone-pos (numberp clone-pos))) r))
    ;; modify original
    (goto-char (point-min))
    (search-forward "* Original") (beginning-of-line)
    (org-entry-put nil "MODIFIED" "yes")
    (push (list :orig-modified (org-entry-get nil "MODIFIED")) r)
    (push (list :orig-id (org-entry-get nil "ID")) r)
    ;; check clone is separate
    (goto-char (point-min))
    (search-forward "* Original")  ;; first occurrence
    (search-forward "* Original")  ;; second occurrence = clone
    (beginning-of-line)
    (push (list :clone-modified (org-entry-get nil "MODIFIED")) r)
    (push (list :clone-id (org-entry-get nil "ID")) r)
    ;; counts
    (push (list :headline-count (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Narrow → edit → widen → reparse → compare with pre-narrow parse
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo51_narrow_edit_widen_reparse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:pre-narrow-headlines 5) (:pre-narrow-sections 3) (:narrow-headlines 3) (:after-edit-narrow-headlines 4) (:after-widen-headlines 6) (:after-widen-raw (\"A\" \"B\" \"C\" \"B1\" \"D\" \"E\")) (:d-at headline))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\nBody B.\n** C\nBody C.\n* D\n** E\nBody E.\n")
  (let ((r '()))
    ;; full parse before narrowing
    (push (list :pre-narrow-headlines
                (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (push (list :pre-narrow-sections
                (length (org-element-map (org-element-parse-buffer) 'section #'identity))) r)
    ;; narrow to A + children
    (goto-char (point-min))
    (org-narrow-to-subtree)
    (push (list :narrow-headlines
                (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; edit inside narrowed region: add a heading
    (goto-char (point-max))
    (insert "\n*** B1\nNarrowed body.\n")
    (push (list :after-edit-narrow-headlines
                (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; widen
    (widen)
    ;; reparse full buffer
    (push (list :after-widen-headlines
                (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (push (list :after-widen-raw
                (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                        (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; at-point on D after widening
    (goto-char (point-min))
    (search-forward "* D") (beginning-of-line)
    (push (list :d-at (org-element-type (org-element-at-point))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Table formula: build → recalc → introduce error → fix → recalc
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo51_table_formula_error_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"| a |\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |\n|---+---+---|\n| 10 | 2 |   |\n| 20 | 5 |   |\n| 30 | 0 |   |\n")
  (insert "#+TBLFM: $3=$1+$2\n")
  (let ((r '()))
    ;; initial recalc
    (goto-char (point-min))
    (push (list :init (buffer-string)) r)
    (org-table-recalculate t)
    (org-table-align)
    (push (list :after-add (buffer-string)) r)
    ;; add division formula (row 3: 30/0 would error)
    (goto-char (point-max))
    (search-backward "#+TBLFM:")
    (kill-line)
    (insert "#+TBLFM: $3=$1/$2\n")
    (condition-case err
        (progn (goto-char (point-min)) (org-table-recalculate t) (org-table-align)
               (push (list :after-div (buffer-string)) r))
      (error (push (list :div-error (error-message-string err)) r)))
    ;; fix: change data so no division by zero, update formula
    (goto-char (point-min))
    (search-forward "| 30 | 0 |")
    (beginning-of-line)
    (kill-line)
    (insert "| 30 | 3 |   |\n")
    ;; fix formula to do both add and multiply
    (goto-char (point-min))
    (search-forward "#+TBLFM:")
    (kill-line)
    (insert "#+TBLFM: $3=$1+$2::$4=$1*$2\n")
    ;; need column 4 now: insert column
    (goto-char (point-min))
    (search-forward "| a |") (end-of-line)
    ;; add column header
    (goto-char (point-min))
    (search-forward "| a | b | c |") (end-of-line)
    ;; insert new header
    (insert " d |")
    ;; insert column in data rows - recalc handles column insertion
    (org-table-recalculate t)
    (org-table-align)
    (push (list :after-fix (buffer-string)) r)
    ;; to-lisp
    (goto-char (point-min))
    (push (list :to-lisp (org-table-to-lisp)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Babel: execute → change result type → re-execute → compare
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo51_babel_result_type_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (:after-value \"* Babel\\n#+begin_src emacs-lisp :results value\\n'(1 2 3 4 5)\\n#+end_src\\n\\n#+RESULTS:\\n| 1 | 2 | 3 | 4 | 5 |\\n\") \"0123456789\" (:after-output \"* Babel\\n#+begin_src emacs-lisp :results output\\n(princ \\\"0123456789\\\")\\n#+end_src\\n\\n#+RESULTS:\\n: 0123456789\\n\") (:result-count 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "* Babel\n")
    (insert "#+begin_src emacs-lisp :results value\n'(1 2 3 4 5)\n#+end_src\n")
    (let ((r '()))
      ;; execute with :results value (default)
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (push (list :after-value (buffer-substring-no-properties (point-min) (point-max))) r)
      ;; change to :results output
      (goto-char (point-min))
      (search-forward ":results value")
      (replace-match ":results output")
      ;; change code to print
      (search-forward "'(1 2 3 4 5)")
      (replace-match "(princ \"0123456789\")")
      ;; execute as output
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (push (list :after-output (buffer-substring-no-properties (point-min) (point-max))) r)
      ;; count result blocks
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      ;; result content
      (let ((res (car (org-element-map (org-element-parse-buffer) 'result #'identity))))
        (when res
          (push (list :result-type (org-element-type res)) r)
          (push (list :result-begin (org-element-property :begin res)) r)))
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Property: set with special chars → get → modify → get → delete → get
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo51_property_special_char_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:set1 \"key=value;path=/usr/local:8080\") (:set2 \"key=value;path=/usr/local:8080;extra=true\") (:set3 \"https://user:pass@host.com:8443/path?q=1\") (:set4 \"http://simple.com\") (:deleted nil) (:url-still-there \"http://simple.com\") (:all-keys (\"CATEGORY\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n")
  (let ((r '()))
    ;; set with special chars
    (org-entry-put nil "CONFIG" "key=value;path=/usr/local:8080")
    (push (list :set1 (org-entry-get nil "CONFIG")) r)
    ;; modify: append
    (org-entry-put nil "CONFIG" "key=value;path=/usr/local:8080;extra=true")
    (push (list :set2 (org-entry-get nil "CONFIG")) r)
    ;; set another with colons
    (org-entry-put nil "URL" "https://user:pass@host.com:8443/path?q=1")
    (push (list :set3 (org-entry-get nil "URL")) r)
    ;; modify URL
    (org-entry-put nil "URL" "http://simple.com")
    (push (list :set4 (org-entry-get nil "URL")) r)
    ;; delete CONFIG
    (org-entry-delete nil "CONFIG")
    (push (list :deleted (org-entry-get nil "CONFIG")) r)
    ;; URL still there
    (push (list :url-still-there (org-entry-get nil "URL")) r)
    ;; get all properties
    (push (list :all-keys (sort (mapcar #'car (org-entry-properties nil t)) #'string-lessp)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Visibility: fold → edit folded content → unfold → verify content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo51_fold_edit_unfold_verify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:before-paras 4) (:after-unfold-headlines 5) (:after-unfold-raw (\"A\" \"A1\" \"A2\" \"A3\" \"B\")) (:after-unfold-paras 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\nBody A.\n** A1\nBody A1.\n** A2\nBody A2.\n* B\nBody B.\n")
  (let ((r '()))
    ;; full parse before
    (push (list :before-paras (length (org-element-map (org-element-parse-buffer) 'paragraph #'identity))) r)
    ;; fold A's subtree
    (goto-char (point-min))
    (org-fold-hide-subtree)
    ;; edit inside A (even though invisible): add heading
    (goto-char (point-min))
    (forward-line)  ;; after heading, before A1 (still inside A subtree)
    ;; need to go to end of A subtree and insert before the end
    (goto-char (point-min))
    (let ((subtree-end (save-excursion (org-end-of-subtree) (point))))
      (goto-char (- subtree-end 1))
      (insert "\n** A3\nHidden body.\n"))
    ;; unfold A
    (goto-char (point-min))
    (org-fold-show-subtree)
    ;; after unfold: verify new heading visible
    (push (list :after-unfold-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (push (list :after-unfold-raw
                (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                        (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; parse full buffer after all
    (push (list :after-unfold-paras (length (org-element-map (org-element-parse-buffer) 'paragraph #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-heading mutation: change 5 headings todo/priority/tags → remap
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo51_multi_heading_mutation_remap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (user-error \"State ‘WAIT’ not valid in this file\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A :work:\n** TODO B :work:\n** TODO C :home:\n* TODO D :home:\n* TODO E :work:\n")
  (let ((r '()))
    ;; initial map
    (push (list :init (org-map-entries
                       (lambda () (list (org-get-heading t t t t)
                                        (org-get-todo-state)
                                        (org-get-tags))))) r)
    ;; change every TODO to DONE, change top-level tags
    (goto-char (point-min))
    ;; A: DONE, priority A, tag change
    (org-todo "DONE") (org-priority ?A) (org-set-tags '("work" "urgent"))
    (forward-line 1)
    ;; B: WAIT, priority B
    (org-todo "WAIT") (org-priority ?B)
    (forward-line 1)
    ;; C: DONE
    (org-todo "DONE")
    (forward-line 1)
    ;; D: CANCELED
    (org-todo "CANCELED")
    (forward-line 1)
    ;; E: DONE, set tags
    (org-todo "DONE") (org-set-tags '("done"))
    ;; final map
    (push (list :after-mutate (org-map-entries
                               (lambda () (list (org-get-heading t t t t)
                                                (org-get-todo-state)
                                                (org-get-tags)
                                                (org-get-priority (point)))))) r)
    ;; map only DONEs
    (push (list :dones (org-map-entries (lambda () (org-get-heading t t t t)) "TODO=\"DONE\"")) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: add/remove select+exclude tags → re-export → compare
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo51_export_select_exclude_tag_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:has-A nil) (:has-B nil) (:has-C 80) (:has-D nil) (:after-has-B nil) (:after-has-D 170) (:after-has-A nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox)
  (require 'ox-ascii)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-ascii-text-width 72)
        (org-export-exclude-tags '("exclude"))
        (org-export-select-tags '("include")))
    (insert "* A\nBody A.\n* B :exclude:\nBody B.\n* C :include:\nBody C.\n* D\nBody D.\n* E :include:exclude:\nBody E.\n")
    (let ((r '()))
      ;; export with both select and exclude
      (let ((out (org-export-as 'ascii nil nil t)))
        (push (list :has-A (and out (string-match-p "1. A" out))) r)
        (push (list :has-B (and out (string-match-p "Body B" out))) r)
        (push (list :has-C (and out (string-match-p "Body C" out))) r)
        (push (list :has-D (and out (string-match-p "Body D" out))) r))
      ;; remove exclude tag from B
      (goto-char (point-min))
      (search-forward "* B :exclude:") (beginning-of-line)
      (org-set-tags '())
      ;; add include tag to D
      (search-forward "* D") (beginning-of-line)
      (org-set-tags '("include"))
      ;; re-export
      (let ((out (org-export-as 'ascii nil nil t)))
        (push (list :after-has-B (and out (string-match-p "Body B" out))) r)
        (push (list :after-has-D (and out (string-match-p "Body D" out))) r)
        (push (list :after-has-A (and out (string-match-p "Body A" out))) r))
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Element: parse → adopt child → extract → adopt-back → check consistency
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo51_element_adopt_extract_adopt_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-adopt-element)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (insert "* P\n** C1\nBody C1.\n** C2\nBody C2.\n* Q\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (headlines (org-element-map tree 'headline #'identity))
           (p (nth 0 headlines))
           (c2 (nth 1 (org-element-map p 'headline #'identity)))
           (q (nth 3 headlines)))
      ;; initial state
      (push (list :p-children (length (org-element-map p 'headline #'identity))) r)
      (push (list :q-children (length (org-element-map q 'headline #'identity))) r)
      ;; extract C2 from P
      (when c2
        (let ((extracted c2))
          (push (list :extracted-name (substring-no-properties (org-element-property :raw-value extracted))) r)
          (org-element-extract-element extracted)
          (push (list :p-after-extract (length (org-element-map p 'headline #'identity))) r))
        ;; adopt under Q
        (org-element-adopt-element q c2)
        (push (list :q-after-adopt (length (org-element-map q 'headline #'identity))) r)
        (push (list :q-after-adopt-names (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                                 (org-element-map q 'headline #'identity))) r)))
    ;; tree is still interpretable
    (push (list :interpretable (> (length (substring-no-properties (org-element-interpret-data tree))) 0)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-clock: clock-in multiple headings → clock-sum all → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo51_multi_clock_sum_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:t1-minutes 0) (:t2-minutes 0) (:t3-minutes 0) (:total-clocks 3) (:total-logbooks 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-clock)
  (let ((org-clock-persist nil)
        (org-clock-out-remove-zero-clock-sum t))
    (insert "* Task 1\n* Task 2\n* Task 3\n")
    (let ((r '()))
      ;; clock + out on Task 1
      (goto-char (point-min))
      (org-clock-in nil) (org-clock-out nil nil)
      (push (list :t1-minutes (org-clock-sum-current-item)) r)
      ;; clock + out on Task 2
      (search-forward "* Task 2") (beginning-of-line)
      (org-clock-in nil) (org-clock-out nil nil)
      (push (list :t2-minutes (org-clock-sum-current-item)) r)
      ;; clock + out on Task 3
      (search-forward "* Task 3") (beginning-of-line)
      (org-clock-in nil) (org-clock-out nil nil)
      (push (list :t3-minutes (org-clock-sum-current-item)) r)
      ;; total clock entries across all
      (goto-char (point-min))
      (push (list :total-clocks (length (org-element-map (org-element-parse-buffer) 'clock #'identity))) r)
      ;; count logbooks
      (push (list :total-logbooks (length (org-element-map (org-element-parse-buffer) 'drawer
                                            (lambda (d) (when (equal "LOGBOOK" (org-element-property :drawer-name d)) d))))) r)
      (nreverse r))))"##,
        expect,
    );
}
