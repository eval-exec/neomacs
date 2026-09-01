//! Strong combo-complex-48 oracle tests — ultra-deep multi-step workflows
//! combining parse → structure edit → export → babel → clock → reparse.
//!
//! Every test chains 6-10 operations and captures deep mutable state.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build → set all heading properties → promote/demote → tag → verify all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo48_full_heading_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task")
  (let ((r '()))
    ;; step 1: add TODO and priority
    (goto-char (point-min)) (org-todo "TODO") (org-priority ?A)
    (push (list :step1 (list (org-get-todo-state) (org-get-priority (point)))) r)
    ;; step 2: add tags
    (org-set-tags '("work" "urgent"))
    (push (list :step2 (org-get-tags)) r)
    ;; step 3: add property
    (org-entry-put nil "OWNER" "alice")
    (push (list :step3 (org-entry-get nil "OWNER")) r)
    ;; step 4: schedule + deadline
    (org-schedule nil "<2025-01-10 Fri>")
    (org-deadline nil "<2025-01-20 Mon>")
    (push (list :step4 (org-element-map (org-element-parse-buffer) 'planning
                         (lambda (p) (list
                                      (when (org-element-property :scheduled p) "S")
                                      (when (org-element-property :deadline p) "D"))))) r)
    ;; step 5: create child headings
    (goto-char (point-max))
    (insert "\n** Subtask A\n** Subtask B")
    (goto-char (point-min))
    (push (list :step5 (mapcar (lambda (h) (list (org-element-property :level h)
                                                 (substring-no-properties
                                                  (org-element-property :raw-value h))))
                               (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; step 6: promote parent (demote children relatively)
    (goto-char (point-min))
    (org-promote-subtree)
    ;; step 7: toggle child A to DONE
    (search-forward "Subtask A") (beginning-of-line) (org-todo "DONE")
    (push (list :step7 (org-element-map (org-element-parse-buffer) 'headline
                         (lambda (h) (list (substring-no-properties
                                            (org-element-property :raw-value h))
                                           (org-element-property :todo-keyword h))))) r)
    ;; step 8: final buffer
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build multi-table → cross-ref formulas → insert columns/rows → recalc → dump
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo48_table_multi_edit_cascade() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range [nil 0 1] 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+name: src\n|  a |  b |\n|----+----|\n|  1 |  2 |\n|  3 |  4 |\n\n")
  (insert "#+name: dst\n| sum | prod | diff |\n|-----+------+------|\n|     |      |      |\n")
  (insert "#+TBLFM: @2$1=vsum(remote(src,@2$1..@3$1))::@2$2=vprod(remote(src,@2$2..@3$2))::@2$3=@2$1-@2$2\n")
  (let ((r '()))
    ;; initial
    (push (list :init-src (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; recalc dst
    (goto-char (point-min))
    (search-forward "dst")
    (forward-line) (forward-line)
    (org-table-recalculate t)
    (org-table-align)
    (push (list :after-calc (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; add column to dst
    (org-table-insert-column)
    (insert " min ")
    (org-table-align)
    (push (list :after-add-col (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; add row to dst
    (org-table-insert-row)
    (insert "   |   |   |     ")
    (org-table-align)
    (insert "\n#+TBLFM: @2$1=vsum(remote(src,@2$1..@3$1))::@2$2=vprod(remote(src,@2$2..@3$2))::@2$3=@2$1-@2$2::@2$4=vmin(remote(src,@2$1..@3$1))\n")
    (org-table-recalculate t)
    (push (list :after-add-row (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; element counts
    (push (list :tables (length (org-element-map (org-element-parse-buffer) 'table #'identity))) r)
    (push (list :cells (length (org-element-map (org-element-parse-buffer) 'table-cell #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Babel multi-lang pipeline with header args
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo48_babel_multilang_header_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ob-sh\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (require 'ob-sh)
  (let ((org-confirm-babel-evaluate nil))
    (insert "* Babel Pipeline\n")
    ;; Step 1: generate data with emacs-lisp
    (insert "#+name: data\n")
    (insert "#+begin_src emacs-lisp :results value list\n")
    (insert "'(:a 10 :b 20 :c 30)\n")
    (insert "#+end_src\n\n")
    ;; Step 2: process with emacs-lisp using data
    (insert "#+name: processed\n")
    (insert "#+begin_src emacs-lisp :results value :var x=data\n")
    (insert "(list :sum (+ (plist-get x :a) (plist-get x :b) (plist-get x :c))\n")
    (insert "      :prod (* (plist-get x :a) (plist-get x :b) (plist-get x :c))\n")
    (insert "      :avg (/ (+ (plist-get x :a) (plist-get x :b) (plist-get x :c)) 3.0))\n")
    (insert "#+end_src\n\n")
    ;; Step 3: format with shell
    (insert "#+begin_src sh :results output :var p=processed\n")
    (insert "echo \"sum=${p[sum]} prod=${p[prod]} avg=${p[avg]}\"\n")
    (insert "#+end_src\n")
    (let ((r '()))
      ;; execute data block
      (goto-char (point-min))
      (search-forward "#+name: data")
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; execute processed block
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; execute sh block
      (search-forward "#+begin_src sh")
      (push (org-babel-execute-src-block) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → fold/visibility cycle → parse visible only → unfold → reparse
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo48_visibility_parse_reparse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init-headlines (\"A\" \"A1\" \"A2\" \"B\" \"B1\" \"B2\" \"C\")) (:init-sections 5) (:overview-headlines (\"A\" \"A1\" \"A2\" \"B\" \"B1\" \"B2\" \"C\")) (:after-cycle1-headlines (\"A\" \"A1\" \"A2\" \"B\" \"B1\" \"B2\" \"C\")) (:after-cycle2-headlines (\"A\" \"A1\" \"A2\" \"B\" \"B1\" \"B2\" \"C\")) (:after-showall-headlines (\"A\" \"A1\" \"A2\" \"B\" \"B1\" \"B2\" \"C\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** A1\nBody A1.\n** A2\nBody A2.\n* B\n** B1\nBody B1.\n** B2\nBody B2.\n* C\nBody C.\n")
  (let ((r '()))
    ;; initial parse: all visible
    (push (list :init-headlines (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                        (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (push (list :init-sections (length (org-element-map (org-element-parse-buffer) 'section #'identity))) r)
    ;; overview: only top level
    (goto-char (point-min))
    (org-overview)
    (push (list :overview-headlines (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                            (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; cycle through visibility levels
    (goto-char (point-min))
    (org-cycle)
    (push (list :after-cycle1-headlines (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                                (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (org-cycle)
    (push (list :after-cycle2-headlines (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                                (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; show all
    (org-show-all)
    (push (list :after-showall-headlines (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                                 (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → insert macros → expand → export → compare expanded state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo48_macro_expand_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:raw-value \"{{{name}}} Report\") (:exported \"1 Project NeoMACS Report\\n========================\\n\\n  Version: 0.1.0.  Hello from oracle-host\\n\\n\\n1.1 Sub-section\\n~~~~~~~~~~~~~~~\\n\\n  More about Project NeoMACS 0.1.0.\\n\") (:macro-count 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-ascii-text-width 72))
    (insert "#+MACRO: name Project NeoMACS\n")
    (insert "#+MACRO: version 0.1.0\n")
    (insert "#+MACRO: greeting (eval (concat \"Hello from \" (system-name)))\n")
    (insert "\n* {{{name}}} Report\n")
    (insert "Version: {{{version}}}.\n")
    (insert "{{{greeting}}}\n\n")
    (insert "** Sub-section\n")
    (insert "More about {{{name}}} {{{version}}}.\n")
    (let ((r '()))
      ;; parse before export
      (push (list :raw-value (substring-no-properties
                              (org-element-property :raw-value
                               (car (org-element-map (org-element-parse-buffer) 'headline #'identity))))) r)
      ;; export to ascii
      (push (list :exported (org-export-as 'ascii nil nil t)) r)
      ;; count macros
      (push (list :macro-count
                  (length (org-element-map (org-element-parse-buffer) 'keyword
                            (lambda (k) (when (equal "MACRO" (org-element-property :key k)) k))))) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-map-entries with deep mutation and re-mapping
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo48_map_entries_deep_mutate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (user-error \"State ‘WAIT’ not valid in this file\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Project A\n** TODO Task 1 :work:\n** DONE Task 2 :work:\n")
  (insert "* TODO Project B\n** TODO Task 3 :home:\n** TODO Task 4 :home:\n")
  (insert "* DONE Project C\n** DONE Task 5 :done:\n")
  (let ((r '()))
    ;; initial map: all TODO items with :work: tag
    (push (list :init-work
                (org-map-entries
                 (lambda () (list (org-get-heading t t t t)
                                  (org-get-todo-state)
                                  (org-get-tags)))
                 "TODO=\"TODO\"+work")) r)
    ;; initial map: all TODO items
    (push (list :init-all-todo
                (org-map-entries
                 (lambda () (org-get-heading t t t t))
                 "TODO=\"TODO\"")) r)
    ;; mutate: change top-level Project B to WAIT
    (goto-char (point-min))
    (search-forward "* TODO Project B")
    (beginning-of-line)
    (org-todo "WAIT")
    (push (list :after-project-b-wait
                (org-map-entries (lambda () (list (org-get-heading t t t t)
                                                  (org-get-todo-state))))) r)
    ;; sub-map under Project B only
    (goto-char (point-min))
    (search-forward "* WAIT Project B")
    (let ((project-b-headline (org-element-at-point))
          (sub-items '()))
      (when (eq (org-element-type project-b-headline) 'headline)
        (org-element-map project-b-headline 'headline
          (lambda (h) (push (list (substring-no-properties (org-element-property :raw-value h))
                                  (org-element-property :todo-keyword h))
                            sub-items)))
        (push (list :sub-items (nreverse sub-items)) r)))
    ;; final buffer
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export → modify buffer → reexport → structural comparison
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo48_export_struct_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:initial-struct ((1 \"Org Document\" 1) (2 \"Section A\" 16) (2 \"Section B\" 59))) (:html1 t) (:after-insert-struct ((1 \"Org Document\") (2 \"Section A\") (2 \"Section A2\") (2 \"Section B\"))) (:html2 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-html)
  (let ((org-export-show-temporary-export-buffer nil))
    (insert "* Org Document\n")
    (insert "** Section A\n   Text in A with *emphasis*.\n")
    (insert "** Section B\n   Text in B.\n")
    (let ((r '()))
      ;; structural info before
      (push (list :initial-struct
                  (org-element-map (org-element-parse-buffer) 'headline
                    (lambda (h) (list (org-element-property :level h)
                                      (substring-no-properties (org-element-property :raw-value h))
                                      (org-element-property :begin h))))) r)
      ;; export
      (push (list :html1 (let ((e (org-export-as 'html nil nil t)))
                           (when e (> (length e) 0)))) r)
      ;; modify: insert new heading between A and B
      (goto-char (point-min))
      (search-forward "** Section B")
      (beginning-of-line)
      (insert "** Section A2\n   Inserted heading.\n\n")
      ;; structural info after
      (push (list :after-insert-struct
                  (org-element-map (org-element-parse-buffer) 'headline
                    (lambda (h) (list (org-element-property :level h)
                                      (substring-no-properties (org-element-property :raw-value h)))))) r)
      ;; reexport
      (push (list :html2 (let ((e (org-export-as 'html nil nil t)))
                           (when e (> (length e) 0)))) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Complex list: mixed types → indent/outdent → sort → checkbox → stats
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo48_mixed_list_reshape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] apple\n- [ ] banana\n- [ ] cherry\n- [X] date\n")
  (insert "1. first :desc:\n2. second :desc:\n3. third :desc:\n")
  (let ((r '()))
    ;; initial element types
    (push (list :init-types (delete-dups
                              (mapcar #'org-element-type
                                      (org-element-map (org-element-parse-buffer) t #'identity)))) r)
    ;; initial items
    (push (list :init-items (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
    ;; toggle checkboxes
    (goto-char (point-min))
    (search-forward "banana") (beginning-of-line) (org-toggle-checkbox)
    (search-forward "cherry") (beginning-of-line) (org-toggle-checkbox)
    ;; update statistics
    (org-update-statistics-cookies)
    (push (list :after-toggle (mapcar (lambda (i) (org-element-property :checkbox i))
                                      (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
    ;; sort alphabetical
    (goto-char (point-min))
    (org-sort-list t ?a)
    (push (list :after-sort (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; indent ordered list under a bullet
    (goto-char (point-min))
    (search-forward "1. ") (beginning-of-line)
    ;; skip what we can't indent; just check element integrity after sort
    (push (list :final-items (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
    (push (list :final-plain-lists (length (org-element-map (org-element-parse-buffer) 'plain-list #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Src block → execute → modify → re-execute → compare results
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo48_src_block_mutate_reexecute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (12 (:modified-src-value \"(+ 100 200 300)\\n\") 600 (:result-count 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "* Code\n")
    (insert "#+begin_src emacs-lisp :results value\n(+ 5 7)\n#+end_src\n")
    (let ((r '()))
      ;; execute
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; modify the block
      (goto-char (point-min))
      (search-forward "(+ 5 7)")
      (replace-match "(+ 100 200 300)")
      (push (list :modified-src-value
                  (org-element-property :value
                   (car (org-element-map (org-element-parse-buffer) 'src-block #'identity)))) r)
      ;; re-execute
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; verify multiple result blocks
      (push (list :result-count
                  (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Drawers: insert → populate → extract contents → modify → delete → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo48_drawer_insert_populate_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:props (\"abc-123\" \"h1\")) (:drawers 1) (:prop-drawers 0) (:note-contents \":ID:       abc-123\\n:CUSTOM_ID: h1\\n:NOTES:\\nNote line 1.\\nNote line 2.\\n\\n\") (:buffer \"* H1\\n:PROPERTIES:\\n:ID:       abc-123\\n:CUSTOM_ID: h1\\n:NOTES:\\nNote line 1.\\nNote line 2.\\n\\n:END:\\n:END:\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n")
  (let ((r '()))
    ;; insert property drawer and populate
    (goto-char (point-min))
    (org-insert-property-drawer)
    (org-entry-put nil "ID" "abc-123")
    (org-entry-put nil "CUSTOM_ID" "h1")
    (push (list :props (list (org-entry-get nil "ID")
                             (org-entry-get nil "CUSTOM_ID"))) r)
    ;; insert regular drawer
    (goto-char (point-min))
    (forward-line 4)  ;; after property drawer
    (org-insert-drawer nil "NOTES")
    (insert "Note line 1.\nNote line 2.\n")
    ;; verify drawer count
    (push (list :drawers (length (org-element-map (org-element-parse-buffer) 'drawer #'identity))) r)
    (push (list :prop-drawers (length (org-element-map (org-element-parse-buffer) 'property-drawer #'identity))) r)
    ;; get drawer contents
    (push (list :note-contents
                (let ((d (car (org-element-map (org-element-parse-buffer) 'drawer #'identity))))
                  (when d
                    (substring-no-properties
                     (buffer-substring-no-properties
                      (org-element-property :contents-begin d)
                      (org-element-property :contents-end d)))))) r)
    ;; final buffer
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}
