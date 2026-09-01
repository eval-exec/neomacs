//! Strong uncovered-features-19 oracle tests — complex state capture.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-paste-subtree at different levels
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_paste_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error #(\"The kill is not a (set of) tree(s).  Use ‘C-y’ to yank anyway\" 42 45 (font-lock-face help-key-binding face help-key-binding)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Dest")
  (org-kill-new "Paste\n** Sub")
  (goto-char (point-min))
  (org-paste-subtree 2)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-meta-return in empty buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_meta_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"First\" \"Second\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (org-meta-return)
  (insert "First")
  (org-meta-return)
  (insert "Second")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-metaright on heading multiple times
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_meta_right_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (goto-char (point-min))
  (let ((r '()))
    (dotimes (_ 3) (org-metaright) (push (org-element-property :level (org-element-at-point)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-metaleft on deep heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_meta_left_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error \"Cannot promote to level 0.  UNDO to recover if necessary\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "*** Deep")
  (goto-char (point-min))
  (let ((r '()))
    (dotimes (_ 3) (org-metaleft) (push (org-element-property :level (org-element-at-point)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-metaup on heading with children
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_meta_up_children() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1 \"A\") (2 \"A1\") (1 \"B\") (2 \"B1\") (1 \"C\")) ((1 \"B\") (2 \"B1\") (1 \"A\") (2 \"A1\") (1 \"C\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** A1\n* B\n** B1\n* C")
  (goto-char (point-min))
  (forward-line 2)
  (let ((o1 (org-element-map (org-element-parse-buffer) 'headline
              (lambda (h) (list (org-element-property :level h)
                                (org-element-property :raw-value h))))))
    (org-metaup)
    (let ((o2 (org-element-map (org-element-parse-buffer) 'headline
                (lambda (h) (list (org-element-property :level h)
                                  (org-element-property :raw-value h))))))
      (list o1 o2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-metadown on heading with children
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_meta_down_children() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1 \"A\") (2 \"A1\") (1 \"B\") (2 \"B1\") (1 \"C\")) ((1 \"B\") (2 \"B1\") (1 \"A\") (2 \"A1\") (1 \"C\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** A1\n* B\n** B1\n* C")
  (goto-char (point-min))
  (let ((o1 (org-element-map (org-element-parse-buffer) 'headline
              (lambda (h) (list (org-element-property :level h)
                                (org-element-property :raw-value h))))))
    (org-metadown)
    (let ((o2 (org-element-map (org-element-parse-buffer) 'headline
                (lambda (h) (list (org-element-property :level h)
                                  (org-element-property :raw-value h))))))
      (list o1 o2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-shiftmetaright/left on heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_shiftmeta() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n* B\n* C")
  (goto-char (point-min))
  (forward-line 1)
  (let ((r '()))
    (org-shiftmetaright)
    (push (org-element-property :level (org-element-at-point)) r)
    (org-shiftmetaright)
    (push (org-element-property :level (org-element-at-point)) r)
    (org-shiftmetaleft)
    (push (org-element-property :level (org-element-at-point)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-shiftmetaup/down on list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_shiftmeta_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"B\" \"C\") (\"A\" \"C\" \"B\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (goto-char (point-min))
  (forward-line 1)
  (let ((d1 (org-element-map (org-element-parse-buffer) 'item
              (lambda (i) (org-trim (buffer-substring-no-properties
                                      (org-element-property :contents-begin i)
                                      (org-element-property :contents-end i)))))))
    (org-shiftmetadown)
    (let ((d2 (org-element-map (org-element-parse-buffer) 'item
                (lambda (i) (org-trim (buffer-substring-no-properties
                                        (org-element-property :contents-begin i)
                                        (org-element-property :contents-end i)))))))
      (list d1 d2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-return in list with auto-insert
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_return_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item")
  (goto-char (point-max))
  (org-return)
  (insert "new")
  (org-return)
  (org-element-map (org-element-parse-buffer) 'item
    (lambda (i) (org-trim (buffer-substring-no-properties
                            (org-element-property :contents-begin i)
                            (org-element-property :contents-end i))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-return in table
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_return_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | b |\\nc\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |")
  (goto-char (point-max))
  (org-return)
  (insert "c")
  (org-return)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-open-at-point on link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_open_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"Link\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n[[http://example.com][Link]]")
  (search-forward "Link")
  (list (org-element-property :type (org-element-context))
        (org-element-property :path (org-element-context))
        (org-element-property :raw-link (org-element-context))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_insert_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[[http://example.com][Example]]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (org-insert-link nil "http://example.com" "Example")
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-all-links
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_insert_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (org-kill-new "http://a.com\nhttp://b.com")
  (org-insert-all-links t)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-store-link on various elements
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_store() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n** H3")
  (let ((r '()))
    (goto-char (point-min))
    (org-store-link nil)
    (push (car org-stored-links) r)
    (forward-line)
    (org-store-link nil)
    (push (car org-stored-links) r)
    (forward-line)
    (org-store-link nil)
    (push (car org-stored-links) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-todo cycling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_todo_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"TODO\" 0 4 (org-todo-head \"TODO\")) #(\"DONE\" 0 4 (org-todo-head \"TODO\")) nil #(\"TODO\" 0 4 (org-todo-head \"TODO\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (goto-char (point-min))
  (let ((r '()))
    (dotimes (_ 4) (org-todo) (push (org-get-todo-state) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-todo with done state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_todo_done() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:state #(\"DONE\" 0 4 (org-todo-head \"TODO\")) :log nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO H")
  (goto-char (point-min))
  (let ((r '()))
    (org-todo 'done)
    (push (list :state (org-get-todo-state) :log (org-element-map (org-element-parse-buffer) 'clock 'identity)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-schedule
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_schedule() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\\nSCHEDULED: <2026-01-15 Thu>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-schedule nil "<2026-01-15>")
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-deadline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_deadline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\\nDEADLINE: <2026-01-20 Tue>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-deadline nil "<2026-01-20>")
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-time-stamp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_ts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-max))
  (org-time-stamp nil)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-time-stamp-inactive
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_ts_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-max))
  (org-time-stamp-inactive nil)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-set-tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_set_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"tag1\" \"tag2\") \"* T                                                               :tag1:tag2:\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-set-tags '("tag1" "tag2"))
  (list (org-get-tags) (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_toggle_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:add (\"existing\" \"new\")) (:remove (\"new\")) (:toggle nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T :existing:")
  (goto-char (point-min))
  (let ((r '()))
    (org-toggle-tag "new")
    (push (list :add (org-get-tags)) r)
    (org-toggle-tag "existing")
    (push (list :remove (org-get-tags)) r)
    (org-toggle-tag "new")
    (push (list :toggle (org-get-tags)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-priority cycling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_prio_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65 nil 65)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* [#B] T")
  (goto-char (point-min))
  (let ((r '()))
    (org-priority-up)
    (push (org-element-property :priority (org-element-at-point)) r)
    (org-priority-up)
    (push (org-element-property :priority (org-element-at-point)) r)
    (org-priority-down)
    (push (org-element-property :priority (org-element-at-point)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-set-property multiple
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_set_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"1\" \"2\" \"3\" \"* T\\n:PROPERTIES:\\n:A:        1\\n:B:        2\\n:C:        3\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-set-property "A" "1")
  (org-set-property "B" "2")
  (org-set-property "C" "3")
  (list (org-entry-get nil "A") (org-entry-get nil "B") (org-entry-get nil "C")
        (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-put with id
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_entry_put_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"test-id\" \"myid\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-id\n:END:")
  (goto-char (point-min))
  (org-entry-put nil "CUSTOM_ID" "myid")
  (list (org-entry-get nil "ID") (org-entry-get nil "CUSTOM_ID"))) "##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-in/out
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-clock-in-switch-to-state)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (let ((org-clock-out-switch-to-state nil)
        (org-clock-in-switch-to-state nil))
    (org-clock-in)
    (org-clock-out)
    (org-element-map (org-element-parse-buffer) 'clock
      (lambda (c) (org-element-property :status c)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clock-in/out with LOGBOOK
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf19_clock_logbook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-clock-in-switch-to-state)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\n:END:")
  (goto-char (point-min))
  (let ((org-clock-out-switch-to-state nil)
        (org-clock-in-switch-to-state nil)
        (org-log-into-drawer t))
    (org-clock-in)
    (org-clock-out)
    (let ((r (list (org-element-map (org-element-parse-buffer) 'clock 'identity)
                   (buffer-string))))
      r)))"##,
        expect,
    );
}
