//! Strong uncovered-feature oracle tests — features not yet tested.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-at-heading-p / org-at-item-p / org-at-table-p / org-at-block-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_at_heading_item_table_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:heading t nil nil) (:item nil t nil) (:table nil nil t) (:block nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\n- item\n| table |\n#+BEGIN_SRC\n(+ 1)\n#+END_SRC\n: fixed")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :heading (org-at-heading-p) (org-at-item-p) (org-at-table-p)) r)
    (forward-line 1)
    (push (list :item (org-at-heading-p) (org-at-item-p) (org-at-table-p)) r)
    (forward-line 1)
    (push (list :table (org-at-heading-p) (org-at-item-p) (org-at-table-p)) r)
    (forward-line 1)
    (push (list :block (org-at-heading-p) (org-at-item-p) (org-at-table-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-cycle-hide-drawers with different states
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_cycle_hide_drawers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n:PROPERTIES:\n:A: 1\n:END:\nBody\n* H2")
  (goto-char (point-min))
  (org-cycle-hide-drawers 'all)
  (let ((hidden1 (get-char-property (search-forward "A") 'invisible)))
    (goto-char (point-max))
    (org-cycle-hide-drawers nil)
    (let ((hidden2 (get-char-property (search-forward "A") 'invisible)))
      (list hidden1 hidden2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-heading on plain text
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_toggle_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* Plain text\" \"Existing heading\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Plain text\n* Existing heading")
  (goto-char (point-min))
  (org-toggle-heading)
  (let ((s1 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    (forward-line)
    (org-toggle-heading)
    (let ((s2 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
      (list s1 s2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-move-item-up / org-move-item-down
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_move_item_up_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"A\" \"B\" \"C\") (\"B\" \"A\" \"C\") (\"A\" \"B\" \"C\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (goto-char (point-min))
  (let ((d1 (org-element-map (org-element-parse-buffer) 'item
              (lambda (i) (org-trim (buffer-substring-no-properties
                                      (org-element-property :contents-begin i)
                                      (org-element-property :contents-end i)))))))
    (org-move-item-down)
    (let ((d2 (org-element-map (org-element-parse-buffer) 'item
                (lambda (i) (org-trim (buffer-substring-no-properties
                                        (org-element-property :contents-begin i)
                                        (org-element-property :contents-end i)))))))
      (org-move-item-up)
      (let ((d3 (org-element-map (org-element-parse-buffer) 'item
                  (lambda (i) (org-trim (buffer-substring-no-properties
                                          (org-element-property :contents-begin i)
                                          (org-element-property :contents-end i)))))))
        (list d1 d2 d3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-todo-heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_insert_todo_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Existing\" nil) (\"New task\" \"TODO\") (\"Right task\" \"TODO\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Existing")
  (goto-char (point-max))
  (org-insert-todo-heading nil)
  (insert "New task")
  (org-insert-todo-heading 'right)
  (insert "Right task")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :raw-value h)
                      (org-element-property :todo-keyword h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-sort-entries with different keys
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sort_entries_alpha() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Zebra\n* Apple\n* Mango")
  (org-sort-entries nil ?a)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-tag with argument
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_toggle_tag_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"a\") (\"a\" \"b\") (\"b\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H :a:")
  (goto-char (point-min))
  (let ((t1 (org-get-tags nil t)))
    (org-toggle-tag "b" 'on)
    (let ((t2 (org-get-tags nil t)))
      (org-toggle-tag "a" 'off)
      (let ((t3 (org-get-tags nil t)))
        (list t1 t2 t3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-priority with specific value
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_priority_specific() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] H1\n* TODO H2\n* TODO [#C] H3")
  (goto-char (point-min))
  (let ((p1 (org-get-priority (char-after))))
    (org-priority ?B)
    (forward-line)
    (org-priority ?A)
    (forward-line)
    (org-priority 'up)
    (list p1
          (progn (goto-char (point-min)) (org-get-priority (char-after)))
          (progn (forward-line) (org-get-priority (char-after)))
          (progn (forward-line) (org-get-priority (char-after))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-deadline with remove
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_deadline_set_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO H")
  (goto-char (point-min))
  (org-deadline nil "2026-01-20")
  (let ((dl1 (org-entry-get nil "DEADLINE")))
    (org-schedule nil "2026-01-15")
    (let ((dl2 (org-entry-get nil "DEADLINE"))
          (sc (org-entry-get nil "SCHEDULED")))
      (org-deadline nil nil)
      (let ((dl3 (org-entry-get nil "DEADLINE")))
        (list dl1 dl2 sc dl3)))))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clone-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clone_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clone-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task\n** Sub1\n** Sub2")
  (goto-char (point-min))
  (org-clone-subtree 2)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-copy / org-cut / org-paste subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_copy_paste_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"H1\") (2 \"Sub1\") (1 \"H2\") (2 \"Sub2\") (1 \"H1\") (2 \"Sub1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** Sub1\n* H2\n** Sub2")
  (goto-char (point-min))
  (org-copy-subtree)
  (goto-char (point-max))
  (org-paste-subtree 1)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-sparse-tree with tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sparse_tree_tag_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"B\" \"C\" \"D\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A :work:\n* B :personal:\n* C :work:urgent:\n* D")
  (goto-char (point-min))
  (org-tags-sparse-tree nil "work")
  (let ((vis '()) (hid '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((h (org-get-heading t t t t)))
        (when h
          (if (get-char-property (point) 'invisible)
              (push h hid) (push h vis))))
      (forward-line))
    (list (nreverse vis) (nreverse hid))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-map-entries with match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_map_entries_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n* WAITING D")
  (list (org-map-entries (lambda () (org-get-heading t t t t)) "TODO" 'file)
        (org-map-entries (lambda () (org-get-heading t t t t)) "DONE" 'file)
        (org-map-entries (lambda () (org-get-heading t t t t)) "-DONE" 'file)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-get-repeat
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_get_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"+1w\" \"+1m\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO W\nSCHEDULED: <2026-01-15 +1w>\n* TODO M\nDEADLINE: <2026-01-20 +1m>\n* TODO N")
  (goto-char (point-min))
  (let ((r1 (org-get-repeat)))
    (forward-line 2)
    (let ((r2 (org-get-repeat)))
      (forward-line 2)
      (let ((r3 (org-get-repeat)))
        (list r1 r2 r3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-timestamp-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_at_timestamp_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"<\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text <2026-01-15> and [2026-01-20] here")
  (let ((r '()))
    (goto-char (point-min))
    (forward-char 6)
    (push (list :before (org-at-timestamp-p 'lax)) r)
    (search-forward "<")
    (push (list :active (org-at-timestamp-p 'lax)) r)
    (search-forward "[")
    (push (list :inactive (org-at-timestamp-p 'lax)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_insert_link_stored() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\nBody")
  (goto-char (point-min))
  (org-store-link nil)
  (goto-char (point-max))
  (let ((stored (car org-stored-links)))
    (org-insert-link nil stored "click here")
    (buffer-substring-no-properties (point-min) (point-max))))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-footnote-action
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_footnote_action_new() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function footnote-add-footnote)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text")
  (goto-char (point-max))
  (footnote-add-footnote)
  (let ((count (count-matches "\\[fn:" (point-min) (point-max)))
        (has-def (search-backward "[fn:" nil t)))
    (list count has-def)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-sum with scope
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clock_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 210""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task A\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00\nCLOCK: [2026-01-11 14:00]--[2026-01-11 15:30] =>  1:30\n:END:\n* Task B\n:LOGBOOK:\nCLOCK: [2026-01-12 09:00]--[2026-01-12 10:00] =>  1:00\n:END:")
  (let ((total (org-clock-sum)))
    total))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timer with item
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_timer_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- 0:00:00 ::\\n- 0:00:00 :: \"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-timer)
  (org-timer-start)
  (sleep-for 0.05)
  (org-timer-item)
  (sleep-for 0.05)
  (org-timer-item)
  (buffer-substring-no-properties (point-min) (point-max)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-columns with dynamic columns
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_columns_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-get-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY %TAGS %EFFORT\n* TODO [#A] Task 1 :work:\n:PROPERTIES:\n:EFFORT: 2h\n:END:\n* DONE [#B] Task 2 :home:\n:PROPERTIES:\n:EFFORT: 30m\n:END:")
  (goto-char (point-min))
  (org-columns-get-format))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-pcomplete-todo
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_pcomplete_todo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"TODO\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TO")
  (goto-char (point-max))
  (let ((completions (all-completions "TO" '("TODO" "DONE" "WAITING"))))
    completions))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-refile with target level
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_refile_target_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"P1\" (nil \"^\\\\(\\\\*+\\\\)\\\\(?: +\\\\(DONE\\\\|TODO\\\\)\\\\)?\\\\(?: +\\\\(\\\\[#\\\\(?:[A-Z]\\\\|[0-9]\\\\|[1-5][0-9]\\\\|6[0-4]\\\\)\\\\]\\\\)\\\\)?\\\\(?: +\\\\(?:COMMENT +\\\\)?\\\\(?:\\\\[[0-9%/]+\\\\] *\\\\)*\\\\(P1\\\\)\\\\(?: *\\\\[[0-9%/]+\\\\]\\\\)*\\\\)\\\\(?:[ \t]+\\\\(:\\\\([[:alnum:]_@#%:]+\\\\):\\\\)\\\\)?[ \t]*$\" 1)) (\"P2\" (nil \"^\\\\(\\\\*+\\\\)\\\\(?: +\\\\(DONE\\\\|TODO\\\\)\\\\)?\\\\(?: +\\\\(\\\\[#\\\\(?:[A-Z]\\\\|[0-9]\\\\|[1-5][0-9]\\\\|6[0-4]\\\\)\\\\]\\\\)\\\\)?\\\\(?: +\\\\(?:COMMENT +\\\\)?\\\\(?:\\\\[[0-9%/]+\\\\] *\\\\)*\\\\(P2\\\\)\\\\(?: *\\\\[[0-9%/]+\\\\]\\\\)*\\\\)\\\\(?:[ \t]+\\\\(:\\\\([[:alnum:]_@#%:]+\\\\):\\\\)\\\\)?[ \t]*$\" 19)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* P1\n** T1\n*** S1\n* P2\n** T2")
  (let ((targets (org-refile-get-targets nil)))
    (mapcar (lambda (t) (list (car t) (cdr t))) targets)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-todo-list with custom keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_agenda_custom_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq org-todo-keywords '((sequence "IDEA" "WORKING" "DONE")))
  (insert "* IDEA Feature 1\n* WORKING Feature 2\n* DONE Feature 3")
  (org-map-entries
    (lambda ()
      (list (org-get-heading t t t t)
            (org-get-todo-state)))
    nil 'file))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-property with various types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_property_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Title\" \"TODO\" 65 (\"tag1\" \"tag2\") 1 1 76 31 76 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Title :tag1:tag2:\n:PROPERTIES:\n:VAR: val\n:END:\nBody text\n** Sub")
  (let* ((tree (org-element-parse-buffer))
         (h (car (org-element-map tree 'headline (lambda (h) h)))))
    (list (org-element-property :raw-value h)
          (org-element-property :todo-keyword h)
          (org-element-property :priority h)
          (org-element-property :tags h)
          (org-element-property :level h)
          (org-element-property :begin h)
          (org-element-property :end h)
          (org-element-property :contents-begin h)
          (org-element-property :contents-end h)
          (org-element-property :post-blank h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-contents with various parent types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_contents_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (section)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n- item\n| tbl |\n#+BEGIN_SRC\n(+ 1)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (h (car (org-element-map tree 'headline (lambda (h) h))))
         (children (org-element-contents h)))
    (mapcar 'org-element-type children)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with info
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_map_with_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) (h info) (list (org-element--property :raw-value h nil nil) (plist-get info :first-match))) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n** H2b\n*** H3")
  (let* ((tree (org-element-parse-buffer))
         (result (org-element-map tree 'headline
                   (lambda (h info)
                     (list (org-element-property :raw-value h)
                           (plist-get info :first-match)))
                   nil 'first-match)))
    result))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map recursive control
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_map_no_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"H2a\" \"H3\" \"H2b\") (\"H2a\" \"H3\" \"H2b\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n*** H3\n** H2b")
  (let* ((tree (org-element-parse-buffer))
         (h1 (car (org-element-map tree 'headline (lambda (h) h))))
         (direct-only (org-element-map (org-element-contents h1) 'headline
                        (lambda (h) (org-element-property :raw-value h))
                        nil nil nil t))
         (recursive (org-element-map (org-element-contents h1) 'headline
                      (lambda (h) (org-element-property :raw-value h)))))
    (list direct-only recursive)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-get-environment with buffer keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_env_full_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Full Test\" 0 9 (:parent (#(\"Full Test\" 0 9 (:parent #4)))))) (#(\"Author Name\" 0 11 (:parent (#(\"Author Name\" 0 11 (:parent #4)))))) \"test@example.com\" (#(\"2026-01-15\" 0 10 (:parent (#(\"2026-01-15\" 0 10 (:parent #4)))))) nil nil \"en\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Full Test\n#+AUTHOR: Author Name\n#+EMAIL: test@example.com\n#+DATE: 2026-01-15\n#+DESCRIPTION: A test\n#+KEYWORDS: org test\n#+LANGUAGE: en\n#+SELECT_TAGS: export\n#+EXCLUDE_TAGS: noexport\n#+OPTIONS: toc:2 num:t ^:{} \\n:t")
  (let* ((info (org-export-get-environment nil)))
    (list (plist-get info :title)
          (plist-get info :author)
          (plist-get info :email)
          (plist-get info :date)
          (plist-get info :description)
          (plist-get info :keywords)
          (plist-get info :language))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-backend with multiple backends
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_backend_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (html ((bold . org-html-bold) (center-block . org-html-center-block) (clock . org-html-clock) (code . org-html-code) (drawer . org-html-drawer) (dynamic-block . org-html-dynamic-block) (entity . org-html-entity) (example-block . org-html-example-block) (export-block . org-html-export-block) (export-snippet . org-html-export-snippet) (fixed-width . org-html-fixed-width) (footnote-reference . org-html-footnote-reference) (headline . org-html-headline) (horizontal-rule . org-html-horizontal-rule) (inline-src-block . org-html-inline-src-block) (inlinetask . org-html-inlinetask) (inner-template . org-html-inner-template) (italic . org-html-italic) (item . org-html-item) (keyword . org-html-keyword) (latex-environment . org-html-latex-environment) (latex-fragment . org-html-latex-fragment) (line-break . org-html-line-break) (link . org-html-link) (node-property . org-html-node-property) (paragraph . org-html-paragraph) (plain-list . org-html-plain-list) (plain-text . org-html-plain-text) (planning . org-html-planning) (property-drawer . org-html-property-drawer) (quote-block . org-html-quote-block) (radio-target . org-html-radio-target) (section . org-html-section) (special-block . org-html-special-block) (src-block . org-html-src-block) (statistics-cookie . org-html-statistics-cookie) (strike-through . org-html-strike-through) (subscript . org-html-subscript) (superscript . org-html-superscript) (table . org-html-table) (table-cell . org-html-table-cell) (table-row . org-html-table-row) (target . org-html-target) (template . org-html-template) (timestamp . org-html-timestamp) (underline . org-html-underline) (verbatim . org-html-verbatim) (verse-block . org-html-verse-block)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((be (org-export-get-backend 'html)))
    (list (org-export-backend-name be)
          (org-export-backend-transcoders be))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute-src-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_babel_src_block_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"emacs-lisp\" \"my-block\" \":results value :exports both\" nil \"(+ x 1)\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+NAME: my-block\n#+HEADER: :var x=1\n#+BEGIN_SRC emacs-lisp :results value :exports both\n(+ x 1)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (block (car (org-element-map tree 'src-block (lambda (b) b)))))
    (list (org-element-property :language block)
          (org-element-property :name block)
          (org-element-property :parameters block)
          (org-element-property :switches block)
          (org-element-property :value block))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-id-get-create
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_id_get_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"‘org-id-get’ expects a file-visiting buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading")
  (goto-char (point-min))
  (let ((id1 (org-id-get nil 'create))
        (id2 (org-id-get nil 'create)))
    (list (stringp id1) (stringp id2) (string= id1 id2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-get with various properties
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_entry_get_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"TODO\" \"A\" \":tag:\" \"<2026-01-15>\" nil nil nil nil \"Heading\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Heading :tag:\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\nCLOSED: [2026-01-10]\n:PROPERTIES:\n:CUSTOM_ID: myid\n:EFFORT: 2h\n:END:")
  (goto-char (point-min))
  (list (org-entry-get nil "TODO")
        (org-entry-get nil "PRIORITY")
        (org-entry-get nil "TAGS")
        (org-entry-get nil "SCHEDULED")
        (org-entry-get nil "DEADLINE")
        (org-entry-get nil "CLOSED")
        (org-entry-get nil "CUSTOM_ID")
        (org-entry-get nil "EFFORT")
        (org-entry-get nil "ITEM")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-parent / org-element-lineage
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_parent_lineage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (paragraph section headline headline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n** Sub\nBody text")
  (goto-char (point-min))
  (search-forward "Body")
  (let* ((para (org-element-at-point))
         (h (org-element-property :parent para))
         (h2 (org-element-property :parent h))
         (tree (org-element-property :parent h2)))
    (list (org-element-type para)
          (org-element-type h)
          (org-element-type h2)
          (org-element-type tree))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-post-affiliated
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_post_affiliated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (paragraph (((#(\"My cap\" 0 6 (:parent (#(\"My cap\" 0 6 (:parent #6)))))))) \"my-fig\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CAPTION: My cap\n#+NAME: my-fig\n[[file:test.png]]")
  (let* ((tree (org-element-parse-buffer))
         (link (car (org-element-map tree 'link (lambda (l) l))))
         (parent (org-element-property :parent link))
         (pa (org-element-property :post-affiliated parent)))
    (list (org-element-type parent)
          (org-element-property :caption parent)
          (org-element-property :name parent)
          (numberp pa))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-greater-elements
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_type_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n- item\n| tbl |")
  (let* ((tree (org-element-parse-buffer))
         (h (car (org-element-map tree 'headline (lambda (h) h))))
         (p (car (org-element-map tree 'paragraph (lambda (p) p))))
         (pl (car (org-element-map tree 'plain-list (lambda (l) l)))))
    (list (org-element-type-p h 'headline)
          (org-element-type-p h 'paragraph)
          (org-element-type-p p 'paragraph)
          (org-element-type-p p 'headline)
          (org-element-type-p pl 'plain-list))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-property-inherited
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_property_inherited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+FILETAGS: :global:\n* Parent :local:\n** Child")
  (goto-char (point-min))
  (search-forward "Child")
  (let* ((tree (org-element-parse-buffer))
         (child (car (org-element-map tree 'headline
                       (lambda (h) (when (string= (org-element-property :raw-value h) "Child") h))))))
    (list (org-element-property :tags child)
          (org-element-property :parent-type child))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-operations (deferred modifications)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_deferred_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:todo \"TODO\" :pri 65 :tags (\"tag\") :var \"val\" :title \"Original\") (:todo \"DONE\" :pri 66 :tags (\"newtag\") :var \"newval\" :title \"Changed\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Original :tag:\n:PROPERTIES:\n:VAR: val\n:END:\nBody")
  (goto-char (point-min))
  ;; Read initial state
  (let* ((el (org-element-at-point))
         (p1 (list :todo (org-element-property :todo-keyword el)
                   :pri (org-element-property :priority el)
                   :tags (org-element-property :tags el)
                   :var (org-entry-get nil "VAR")
                   :title (org-element-property :raw-value el))))
    ;; Chain 5 operations
    (org-todo 'right)
    (org-priority 'down)
    (org-set-tags '("newtag"))
    (org-entry-put nil "VAR" "newval")
    (org-edit-headline "Changed")
    ;; Read back
    (let* ((el2 (org-element-at-point))
           (p2 (list :todo (org-element-property :todo-keyword el2)
                     :pri (org-element-property :priority el2)
                     :tags (org-element-property :tags el2)
                     :var (org-entry-get nil "VAR")
                     :title (org-element-property :raw-value el2))))
      (list p1 p2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-planning-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_at_planning_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:heading nil) (:planning t) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nSCHEDULED: <2026-01-15>\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :heading (org-at-planning-p)) r)
    (forward-line 1)
    (push (list :planning (org-at-planning-p)) r)
    (forward-line 1)
    (push (list :body (org-at-planning-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-end-of-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_end_of_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (31 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n*** H3\nBody\n** H2b\n* H1b")
  (goto-char (point-min))
  (let ((p1 (progn (org-end-of-subtree) (point))))
    (goto-char (point-min))
    (search-forward "H2a")
    (beginning-of-line)
    (let ((p2 (progn (org-end-of-subtree) (point))))
      (list p1 p2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-mark-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_mark_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 1 21)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub\n* H1b")
  (goto-char (point-min))
  (org-mark-subtree)
  (let ((m (mark))
        (p (point)))
    (list (< p m) p m)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-narrow-to-subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_narrow_to_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* H1\\nBody 1\\n** H2\\nSub\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody 1\n** H2\nSub\n* H2b\nBody 2")
  (goto-char (point-min))
  (org-narrow-to-subtree)
  (let ((narrowed (buffer-string))
        (w (window-point (selected-window))))
    (widen)
    (list narrowed)))"##,
        expect,
    );
}
