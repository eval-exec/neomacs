//! Strong combo-complex-47 oracle tests — deep multi-step workflows
//! exercising plan→clock→modify→reparse→export chains.
//!
//! Every test chains multiple operations and captures intermediate state.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build doc → set properties → plan → clock → modify → reparse → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo47_plan_clock_replan_reparse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init (\"Task\")) (:after-schedule (\"s\")) (:after-deadline ((\"S\" \"D\"))) (:after-props (\"2:30\" \"dev\")) (:clocking-p t) (:clocking-p-after nil) (:clock-entries 1) (:after-child ((\"S\" \"D\") (\"S\" nil))) (:buffer \"* Task\\nDEADLINE: <2024-03-15 Fri> SCHEDULED: <2024-03-01 Fri>\\n:PROPERTIES:\\n:EFFORT:   2:30\\n:CATEGORY: dev\\n:END:\\n:LOGBOOK:\\nCLOCK: [2026-06-15 Mon 12:00]--[2026-06-15 Mon 12:00] =>  0:00\\n:END:\\n\\n** Sub-task\\nSCHEDULED: <2024-03-10 Sun>\\n:PROPERTIES:\\n:EFFORT:   1:00\\n:END:\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task\n")
  (let ((r '()))
    ;; state: initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h)))) r)
    ;; schedule
    (goto-char (point-min))
    (org-schedule nil "<2024-03-01 Fri>")
    (push (list :after-schedule
                (org-element-map (org-element-parse-buffer) 'planning
                  (lambda (p) (when (org-element-property :scheduled p) "s")))) r)
    ;; deadline
    (org-deadline nil "<2024-03-15 Fri>")
    (push (list :after-deadline
                (org-element-map (org-element-parse-buffer) 'planning
                  (lambda (p) (list (when (org-element-property :scheduled p) "S")
                                    (when (org-element-property :deadline p) "D"))))) r)
    ;; add properties
    (org-entry-put nil "EFFORT" "2:30")
    (org-entry-put nil "CATEGORY" "dev")
    (push (list :after-props (list (org-entry-get nil "EFFORT")
                                   (org-entry-get nil "CATEGORY"))) r)
    ;; clock in → clock out
    (org-clock-in nil)
    (push (list :clocking-p (org-clocking-p)) r)
    (org-clock-out nil nil)
    (push (list :clocking-p-after (org-clocking-p)) r)
    ;; verify clock entry
    (push (list :clock-entries (length (org-element-map (org-element-parse-buffer) 'clock #'identity))) r)
    ;; add child headline
    (goto-char (point-max))
    (insert "\n** Sub-task")
    (goto-char (point-max))
    (org-entry-put nil "EFFORT" "1:00")
    (org-schedule nil "<2024-03-10 Sun>")
    (push (list :after-child
                (org-element-map (org-element-parse-buffer) 'planning
                  (lambda (p) (list
                               (when (org-element-property :scheduled p) "S")
                               (when (org-element-property :deadline p) "D"))))) r)
    ;; final buffer
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build multi-table doc → remote ref formulas → recalc → add row → recalc
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo47_multitable_cascade_recalc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+name: input\n| a | b |\n| 10 | 20 |\n| 30 | 40 |\n\n")
  (insert "#+name: output\n| sum | prod |\n|     |      |\n")
  (insert "#+TBLFM: @2$1=vsum(remote(input,@2$1..@3$1))::@2$2=vprod(remote(input,@2$2..@3$2))\n")
  (let ((r '()))
    ;; initial state
    (push (list :init (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; recalc output table
    (goto-char (point-min))
    (search-forward "output")
    (forward-line) (forward-line)  ;; on first data row of output
    (org-table-recalculate t)
    (org-table-align)
    (push (list :after-calc (buffer-substring-no-properties
                              (point-min) (point-max))) r)
    ;; add row to input table
    (goto-char (point-min))
    (search-forward "input")
    (forward-line) (forward-line) (forward-line) ;; last row
    (org-table-insert-row)
    (insert "50 | 60")
    (org-table-align)
    ;; update formula range in output
    (goto-char (point-min))
    (search-forward "#+TBLFM:")
    (kill-line)
    (insert "#+TBLFM: @2$1=vsum(remote(input,@2$1..@4$1))::@2$2=vprod(remote(input,@2$2..@4$2))\n")
    ;; recalc again
    (goto-char (point-min))
    (search-forward "output")
    (forward-line) (forward-line)
    (org-table-recalculate t)
    (push (list :after-insert-row (buffer-substring-no-properties
                                    (point-min) (point-max))) r)
    ;; get computed values
    (push (list :sum-val (org-table-get "" "sum")) r)
    (push (list :prod-val (org-table-get "" "prod")) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build complex doc → parse → edit inline → export each step → compare
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo47_edit_export_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:export1 \"1 Report\\n========\\n\\n  This is a *bold* statement about /emphasized/ topics.\\n\") (:export2 \"1 Report\\n========\\n\\n  This is a *bold* statement about /emphasized/ topics.\\n\\n\\n1.1 Details\\n~~~~~~~~~~~\\n\\n  More text with `code' and `verbatim'.\\n   Col1  Col2 \\n  ------------\\n   A     B    \\n\") (:export3 \"1 Report\\n========\\n\\n  This is a **important** statement about /emphasized/ topics.\\n\\n\\n1.1 Details\\n~~~~~~~~~~~\\n\\n  More text with `code' and `verbatim'.\\n   Col1  Col2 \\n  ------------\\n   A     B    \\n\") (:export4 \"1 Report\\n========\\n\\n  This is a **important** statement about /emphasized/ topics.\\n\\n\\n1.1 Details\\n~~~~~~~~~~~\\n\\n  More text with `code' and `verbatim'.\\n   Col1  Col2 \\n  ------------\\n   A     B    \\n\\n  ,----\\n  | (+ 1 2)\\n  `----\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-ascii-text-width 72))
    (insert "* Report\n")
    (insert "This is a *bold* statement about /emphasized/ topics.\n")
    (let ((r '()))
      ;; initial export
      (push (list :export1 (org-export-as 'ascii nil nil t)) r)
      ;; insert more content
      (goto-char (point-max))
      (insert "\n** Details\n")
      (insert "More text with =code= and ~verbatim~.\n")
      (insert "| Col1 | Col2 |\n|------+------|\n|  A   |  B   |\n")
      (push (list :export2 (org-export-as 'ascii nil nil t)) r)
      ;; modify the bold text
      (goto-char (point-min))
      (search-forward "*bold*")
      (replace-match "**important**")
      (push (list :export3 (org-export-as 'ascii nil nil t)) r)
      ;; add a source block
      (goto-char (point-max))
      (insert "\n#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
      (push (list :export4 (org-export-as 'ascii nil nil t)) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc with list → indent/dedent → add checkbox → sort → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo47_list_reshape_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- zebra\n- apple\n- mango\n- banana\n")
  (let ((r '()))
    ;; initial items
    (push (list :init (mapcar (lambda (i)
                                (substring-no-properties
                                 (org-element-property :raw-value i)))
                              (org-element-map (org-element-parse-buffer) 'item #'identity)))
          r)
    ;; sort
    (goto-char (point-min))
    (org-sort-list nil ?a)
    (push (list :after-sort (mapcar (lambda (i)
                                      (substring-no-properties
                                       (org-element-property :raw-value i)))
                                    (org-element-map (org-element-parse-buffer) 'item #'identity)))
          r)
    ;; indent two items
    (goto-char (point-min))
    (forward-line 1)  ;; first fruit (banana after sort)
    (org-metaright)
    (push (list :after-indent (mapcar (lambda (i)
                                        (list (org-element-property :level i)
                                              (substring-no-properties
                                               (org-element-property :raw-value i))))
                                      (org-element-map (org-element-parse-buffer) 'item #'identity)))
          r)
    ;; add checkboxes to all
    (goto-char (point-min))
    (let ((cnt 0))
      (org-element-map (org-element-parse-buffer) 'item
        (lambda (i)
          (goto-char (org-element-property :begin i))
          (org-toggle-checkbox (setq cnt (1+ cnt)))
          cnt)))
    (push (list :after-checkbox (mapcar (lambda (i)
                                         (org-element-property :checkbox i))
                                       (org-element-map (org-element-parse-buffer) 'item #'identity)))
          r)
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Babel session: execute multiple blocks, capture state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo47_babel_session_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"ob-emacs-lisp backend does not support sessions\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "* Session\n")
    (insert "#+begin_src emacs-lisp :results value :session test\n")
    (insert "(setq myvar 42)\n")
    (insert "#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :results value :session test\n")
    (insert "(+ myvar 58)\n")
    (insert "#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :results value :session test\n")
    (insert "(list myvar (+ myvar 58) (* myvar 2))\n")
    (insert "#+end_src\n")
    (let ((r '()))
      ;; execute block 1
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; execute block 2 (uses myvar from session)
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; execute block 3
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; check buffer state
      (push (list :num-src-blocks
                  (length (org-element-map (org-element-parse-buffer) 'src-block #'identity))) r)
      (push (list :num-results
                  (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → insert footnotes → renumber → sort → delete → verify refs
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo47_footnote_renumber_sort_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-footnote-renumber-fn-n)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Footnotes\n")
  (insert "First reference[fn:alpha] and second[fn:beta].\n")
  (insert "Third one[fn:gamma].\n\n")
  (insert "[fn:alpha] Definition A.\n")
  (insert "[fn:beta] Definition B.\n")
  (insert "[fn:gamma] Definition C.\n")
  (let ((r '()))
    ;; initial state
    (push (list :init-refs
                (mapcar (lambda (f) (org-element-property :label f))
                        (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity)))
          r)
    (push (list :init-defs
                (mapcar (lambda (f) (org-element-property :label f))
                        (org-element-map (org-element-parse-buffer) 'footnote-definition #'identity)))
          r)
    ;; renumber
    (goto-char (point-min))
    (org-footnote-renumber-fn-n)
    (push (list :after-renumber
                (mapcar (lambda (f) (org-element-property :label f))
                        (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity)))
          r)
    ;; normalize
    (goto-char (point-min))
    (org-footnote-normalize 'sort)
    (push (list :after-normalize
                (mapcar (lambda (f) (list (org-element-property :label f)
                                          (substring-no-properties
                                           (buffer-substring-no-properties
                                            (org-element-property :begin f)
                                            (org-element-property :end f)))))
                        (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity)))
          r)
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → org-map-entries → modify inside map → re-map → compare
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo47_map_entries_mutate_remap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init ((\"A\" \"TODO\") (\"B\" \"DONE\") (\"C\" \"TODO\") (\"D\" \"TODO\") (\"E\" \"DONE\"))) (:init-todo-only (\"A\" \"C\" \"D\")) (:after-mutate ((\"A\" \"TODO\") (\"B\" \"DONE\") (#(\"C\" 0 1 (org-todo-head \"TODO\")) #(\"DONE\" 0 4 (org-todo-head \"TODO\"))) (\"D\" \"TODO\") (\"E\" \"DONE\"))) (:after-todo-only (\"A\" \"D\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n** DONE B\n** TODO C\n* TODO D\n* DONE E\n")
  (let ((r '()))
    ;; initial map: all headings with TODO state
    (push (list :init (org-map-entries
                       (lambda () (list (org-get-heading t t t t)
                                        (org-get-todo-state))))) r)
    ;; initial map: TODO only
    (push (list :init-todo-only (org-map-entries
                                 (lambda () (org-get-heading t t t t))
                                 "TODO=\"TODO\"")) r)
    ;; change C to DONE
    (goto-char (point-min))
    (search-forward "** TODO C")
    (beginning-of-line)
    (org-todo "DONE")
    ;; map again
    (push (list :after-mutate (org-map-entries
                               (lambda () (list (org-get-heading t t t t)
                                                (org-get-todo-state))))) r)
    (push (list :after-todo-only (org-map-entries
                                  (lambda () (org-get-heading t t t t))
                                  "TODO=\"TODO\"")) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Full document: parse → deep inspect → adpot-extract → reparse → compare
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo47_parse_adopt_extract_reparse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init ((1 \"H1\") (2 \"H2\") (1 \"H3\") (2 \"H4\"))) (:extracted-h2 \"H1\") (:h1-children-after-extract 2) (:final ((1 \"H3\") (2 \"H4\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (insert "* H1\nPara A.\n** H2\nPara B.\n* H3\nPara C.\n** H4\nPara D.\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (h1 (car (org-element-map tree 'headline #'identity)))
           (h3 (nth 1 (org-element-map tree 'headline #'identity))))
      ;; initial headline order and levels
      (push (list :init (mapcar (lambda (h) (list (org-element-property :level h)
                                                  (substring-no-properties
                                                   (org-element-property :raw-value h))))
                                (org-element-map tree 'headline #'identity))) r)
      ;; extract H2 from H1
      (let ((h2 (car (org-element-map h1 'headline #'identity))))
        (push (list :extracted-h2 (substring-no-properties (org-element-property :raw-value h2))) r)
        (org-element-extract-element h2)
        ;; after extract: H1 has no children
        (push (list :h1-children-after-extract
                    (length (org-element-map h1 'headline #'identity))) r))
      ;; adopt H2 to H3
      (let* ((h2 (car (org-element-map tree 'headline
                       (lambda (h) (when (equal "H2" (org-element-property :raw-value h)) h))))))
        (when h2
          (org-element-adopt-element h3 h2)
          (push (list :h3-children-after-adopt
                      (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                              (org-element-map h3 'headline #'identity))) r)))
      ;; final state
      (push (list :final (mapcar (lambda (h) (list (org-element-property :level h)
                                                   (substring-no-properties
                                                    (org-element-property :raw-value h))))
                                 (org-element-map tree 'headline #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build full document → multi-export backend compare (ASCII/HTML/latex)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo47_multi_export_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:ascii-nonempty t) (:html-nonempty t) (:latex-nonempty t) (:ascii-has-heading 2) (:html-has-bold 375) (:html-has-table 463) (:latex-has-section 0) (:latex-has-tabular 148))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (require 'ox-html)
  (require 'ox-latex)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-ascii-text-width 72))
    (insert "* Document\n")
    (insert "Some *bold* and /italic/ text with [[https://example.com][a link]].\n\n")
    (insert "| X | Y |\n|---+---|\n| 1 | 2 |\n")
    (let ((r '()))
      (let ((ascii-out (org-export-as 'ascii nil nil t))
            (html-out  (org-export-as 'html nil nil t))
            (latex-out (org-export-as 'latex nil nil t)))
        ;; only compare deterministic properties, not full bodies with random IDs
        (push (list :ascii-nonempty (and ascii-out (> (length ascii-out) 0))) r)
        (push (list :html-nonempty (and html-out (> (length html-out) 0))) r)
        (push (list :latex-nonempty (and latex-out (> (length latex-out) 0))) r)
        (push (list :ascii-has-heading (and ascii-out (string-match-p "Document" ascii-out))) r)
        (push (list :html-has-bold (and html-out (string-match-p "<b>bold</b>" html-out))) r)
        (push (list :html-has-table (and html-out (string-match-p "<table" html-out))) r)
        (push (list :latex-has-section (and latex-out (string-match-p "\\\\section" latex-out))) r)
        (push (list :latex-has-tabular (and latex-out (string-match-p "tabular" latex-out))) r))
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Property inheritance chain: set → inherit → override → clear → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo47_property_inherit_override_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:child-color \"blue\") (:child-size \"large\") (:child-color-after \"red\") (:child-size-after \"large\") (:gc-color \"red\") (:gc-size \"large\") (:other-color nil) (:other-size nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Parent\n** Child\n*** Grandchild\n* Other\n")
  (let ((r '()))
    ;; set properties on parent
    (goto-char (point-min))
    (org-entry-put nil "COLOR" "blue")
    (org-entry-put nil "SIZE" "large")
    ;; child inherits
    (push (list :child-color (progn (forward-line 1) (org-entry-get nil "COLOR" t))) r)
    (push (list :child-size (org-entry-get nil "SIZE" t)) r)
    ;; override COLOR on child
    (org-entry-put nil "COLOR" "red")
    (push (list :child-color-after (org-entry-get nil "COLOR" t)) r)
    (push (list :child-size-after (org-entry-get nil "SIZE" t)) r)
    ;; grandchild inherits overridden COLOR
    (forward-line 1)  ;; now at Grandchild
    (push (list :gc-color (org-entry-get nil "COLOR" t)) r)
    (push (list :gc-size (org-entry-get nil "SIZE" t)) r)
    ;; Other does not inherit
    (goto-char (point-min))
    (search-forward "* Other")
    (push (list :other-color (org-entry-get nil "COLOR" t)) r)
    (push (list :other-size (org-entry-get nil "SIZE" t)) r)
    (nreverse r)))"##,
        expect,
    );
}
