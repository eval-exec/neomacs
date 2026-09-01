//! Strong combo-complex-4 oracle tests — deep multi-step workflows.
//!
//! Every test chains multiple operations capturing deep mutable state.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build doc → modify → export → verify export reflects changes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo4_modify_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init-html 154) (:after-html 167) (:after-level ((1 \"* TODO H1\" \"TODO\") (2 \"H2\" nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody1\n* H2\nBody2")
  (let ((r '()))
    ;; initial export
    (push (list :init-html (string-match-p "H1" (org-export-string-as (buffer-string) 'html t))) r)
    ;; modify
    (goto-char (point-min))
    (search-forward "H1")
    (beginning-of-line)
    (org-todo)
    (insert "* TODO ")
    (forward-line 2)
    (org-metaright)
    ;; export after modification
    (push (list :after-html (string-match-p "TODO" (org-export-string-as (buffer-string) 'html t))) r)
    (push (list :after-level (org-element-map (org-element-parse-buffer) 'headline
                                (lambda (h) (list (org-element-property :level h)
                                                  (org-element-property :raw-value h)
                                                  (org-element-property :todo-keyword h))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex table + formula → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo4_table_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a table\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| x | y |\n| 1 |   |\n| 2 |   |\n| 3 |   |\n#+TBLFM: $2=$1*10")
  (let ((r '()))
    ;; initial
    (push (list :init (buffer-string)) r)
    ;; evaluate
    (org-table-iterate)
    (push (list :after-iter (buffer-string)) r)
    ;; verify cells
    (goto-char (point-min))
    (forward-line 1)
    (push (list :cell1 (org-table-get "1" "2")) r)
    (forward-line)
    (push (list :cell2 (org-table-get "2" "2")) r)
    (forward-line)
    (push (list :cell3 (org-table-get "3" "2")) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex heading + tags + todo + properties → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo4_full_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:todo \"TODO\") (:priority \"A\") (:tags \":tag1:tag2:\") (:custom-id \"myid\") (:effort \"2h\") (:el-level 1) (:el-todo \"TODO\") (:el-priority 65) (:el-tags (\"tag1\" \"tag2\")) (:el-raw \"Heading\") (:after-todo \"DONE\") (:after-tag (\"tag1\" \"tag2\" \"newtag\")) (:after-prop \"3h\") (:content #(\"* DONE [#A] Heading                                        :tag1:tag2:newtag:\\n:PROPERTIES:\\n:CUSTOM_ID: myid\\n:EFFORT:   3h\\n:END:\\nBody text\" 0 77 (org-todo-head \"TODO\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Heading :tag1:tag2:\n:PROPERTIES:\n:CUSTOM_ID: myid\n:EFFORT: 2h\n:END:\nBody text")
  (let ((r '()))
    ;; heading properties
    (goto-char (point-min))
    (push (list :todo (org-entry-get nil "TODO")) r)
    (push (list :priority (org-entry-get nil "PRIORITY")) r)
    (push (list :tags (org-entry-get nil "TAGS")) r)
    (push (list :custom-id (org-entry-get nil "CUSTOM_ID")) r)
    (push (list :effort (org-entry-get nil "EFFORT")) r)
    ;; element properties
    (let ((el (org-element-at-point)))
      (push (list :el-level (org-element-property :level el)) r)
      (push (list :el-todo (org-element-property :todo-keyword el)) r)
      (push (list :el-priority (org-element-property :priority el)) r)
      (push (list :el-tags (org-element-property :tags el)) r)
      (push (list :el-raw (org-element-property :raw-value el)) r))
    ;; modify todo
    (org-todo 'done)
    (push (list :after-todo (org-entry-get nil "TODO")) r)
    ;; modify tags
    (org-toggle-tag "newtag")
    (push (list :after-tag (org-get-tags)) r)
    ;; modify property
    (org-entry-put nil "EFFORT" "3h")
    (push (list :after-prop (org-entry-get nil "EFFORT")) r)
    ;; verify buffer
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex list + checkbox → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo4_list_checkbox() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init ((off \"A\") (off \"B\") (off \"C\"))) (:after-a (on off off)) (:after-b (on on off)) (:stats \"* T [2/3]\") (:after-indent ((nil on) (nil on) (nil off))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T [0/3]\n- [ ] A\n- [ ] B\n- [ ] C")
  (let ((r '()))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'item
                        (lambda (i) (list (org-element-property :checkbox i)
                                          (org-trim (buffer-substring-no-properties
                                                      (org-element-property :contents-begin i)
                                                      (org-element-property :contents-end i))))))) r)
    ;; check A
    (goto-char (point-min))
    (forward-line 1)
    (org-toggle-checkbox)
    (push (list :after-a (org-element-map (org-element-parse-buffer) 'item
                            (lambda (i) (org-element-property :checkbox i)))) r)
    ;; check B
    (forward-line)
    (org-toggle-checkbox)
    (push (list :after-b (org-element-map (org-element-parse-buffer) 'item
                            (lambda (i) (org-element-property :checkbox i)))) r)
    ;; update stats
    (goto-char (point-min))
    (org-update-statistics-cookies t)
    (push (list :stats (buffer-substring-no-properties (line-beginning-position) (line-end-position))) r)
    ;; indent B under A
    (goto-char (point-min))
    (forward-line 2)
    (org-metaright)
    (push (list :after-indent (org-element-map (org-element-parse-buffer) 'item
                                (lambda (i) (list (org-element-property :level i)
                                                  (org-element-property :checkbox i))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex src block + results → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo4_src_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp :results value\n(+ 1 2)\n#+END_SRC")
  (let ((r '()))
    ;; initial
    (push (list :init (buffer-string)) r)
    ;; parse src block
    (push (list :lang (org-element-map (org-element-parse-buffer) 'src-block
                        (lambda (s) (org-element-property :language s)))) r)
    (push (list :params (org-element-map (org-element-parse-buffer) 'src-block
                          (lambda (s) (org-element-property :parameters s)))) r)
    (push (list :value (org-element-map (org-element-parse-buffer) 'src-block
                          (lambda (s) (org-element-property :value s)))) r)
    ;; execute
    (goto-char (point-min))
    (org-babel-execute-src-block)
    ;; verify results
    (push (list :after-exec (buffer-string)) r)
    (push (list :results (org-element-map (org-element-parse-buffer) 'fixed-width
                           (lambda (fw) (org-element-property :value fw)))) r)
    (nreverse r)))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex navigation → verify positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo4_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 2) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n*** C\n** D\n* E\n** F\n*** G")
  (let ((r '()))
    ;; forward same level
    (goto-char (point-min))
    (org-forward-heading-same-level 1)
    (push (list :fwd1 (org-element-property :raw-value (org-element-at-point))) r)
    (org-forward-heading-same-level 1)
    (push (list :fwd2 (org-element-property :raw-value (org-element-at-point))) r)
    ;; up
    (org-up-heading)
    (push (list :up1 (org-element-property :raw-value (org-element-at-point))) r)
    ;; backward same level
    (org-backward-heading-same-level 1)
    (push (list :back1 (org-element-property :raw-value (org-element-at-point))) r)
    ;; next visible
    (goto-char (point-min))
    (org-next-visible-heading 2)
    (push (list :next2 (org-element-property :raw-value (org-element-at-point))) r)
    ;; previous visible
    (org-previous-visible-heading 1)
    (push (list :prev1 (org-element-property :raw-value (org-element-at-point))) r)
    ;; end of subtree
    (goto-char (point-min))
    (org-end-of-subtree)
    (push (list :end (point)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex visibility → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo4_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:overview \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\\nSub\") (:content \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\\nSub\") (:all \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\\nSub\") (:narrowed \"*** H3\\nBody\") (:context \"*** H3\\nBody\") (:widened \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\\nSub\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b\n** H2b\nSub")
  (let ((r '()))
    ;; overview
    (org-overview)
    (push (list :overview (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; content
    (org-content)
    (push (list :content (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; all
    (org-show-all)
    (push (list :all (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; narrow to H2
    (goto-char (point-min))
    (search-forward "H2\n")
    (beginning-of-line)
    (org-narrow-to-subtree)
    (push (list :narrowed (buffer-string)) r)
    ;; show context
    (org-show-context 'agenda)
    (push (list :context (buffer-string)) r)
    ;; widen
    (widen)
    (push (list :widened (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex clock + planning → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo4_clock_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid date: \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\n:END:\nBody")
  (let ((r '()))
    ;; planning
    (push (list :planning (org-element-map (org-element-parse-buffer) 'planning
                            (lambda (p) (list (when (org-element-property :scheduled p) "S")
                                              (when (org-element-property :deadline p) "D"))))) r)
    ;; clock
    (push (list :clocks (org-element-map (org-element-parse-buffer) 'clock
                          (lambda (c) (list (org-element-property :status c)
                                            (org-element-property :duration c))))) r)
    ;; entry properties
    (goto-char (point-min))
    (push (list :todo (org-entry-get nil "TODO")) r)
    (push (list :sched (org-entry-get nil "SCHEDULED")) r)
    (push (list :dead (org-entry-get nil "DEADLINE")) r)
    ;; clock sum
    (org-clock-sum)
    (push (list :clock-sum org-clock-file-total-minutes) r)
    ;; clock string
    (push (list :clock-string (org-clock-get-clock-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex element cache → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo4_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-cache-status)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (let ((r '()))
    ;; initial cache
    (let ((s (org-element-cache-status)))
      (push (list :init-size (plist-get s :size)) r))
    ;; cache active
    (push (list :active (org-element-cache-active-p)) r)
    ;; modify buffer
    (insert "\nNew line")
    (let ((s (org-element-cache-status)))
      (push (list :after-mod (plist-get s :size)) r))
    ;; parse after modification
    (push (list :types (org-element-map (org-element-parse-buffer) 'element 'org-element-type)) r)
    ;; reset cache
    (org-element-cache-reset)
    (let ((s (org-element-cache-status)))
      (push (list :after-reset (plist-get s :size)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex indent → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo4_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub\n*** H3\nDeep")
  (let ((r ''))
    ;; initial
    (push (list :init (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; indent mode
    (org-indent-mode 1)
    (let ((indents '()))
      (goto-char (point-min))
      (while (not (eobp))
        (let ((indent (get-char-property (point) 'line-prefix)))
          (when indent (push (list (line-number-at-pos) indent) indents)))
        (forward-line))
      (push (list :indents (nreverse indents)) r))
    ;; indent buffer
    (org-indent-indent-buffer)
    (let ((indents '()))
      (goto-char (point-min))
      (while (not (eobp))
        (let ((indent (get-char-property (point) 'line-prefix)))
          (when indent (push (list (line-number-at-pos) indent) indents)))
        (forward-line))
      (push (list :buffer-indents (nreverse indents)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex lint → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo4_lint() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nSCHEDULED: <invalid>\nBody [[broken]]")
  (let ((r ''))
    ;; lint
    (push (list :lint-count (length (org-lint))) r)
    ;; verify buffer unchanged
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}
