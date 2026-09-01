//! Strong uncovered-features-48 oracle tests — org-property complex, org-effort, org-priority.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-entry-get ITEM
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_entry_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Heading\" \"TODO\" \"A\" \":tag:\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Heading :tag:")
  (goto-char (point-min))
  (list (org-entry-get nil "ITEM")
        (org-entry-get nil "TODO")
        (org-entry-get nil "PRIORITY")
        (org-entry-get nil "TAGS")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-get planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_entry_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"<2026-01-15>\" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\nCLOSED: [2026-01-10]")
  (goto-char (point-min))
  (list (org-entry-get nil "SCHEDULED")
        (org-entry-get nil "DEADLINE")
        (org-entry-get nil "CLOSED")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-get custom
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_entry_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"myid\" \"2h\" \"test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:CUSTOM_ID: myid\n:EFFORT: 2h\n:VAR: test\n:END:")
  (goto-char (point-min))
  (list (org-entry-get nil "CUSTOM_ID")
        (org-entry-get nil "EFFORT")
        (org-entry-get nil "VAR")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-put-multiple
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_entry_put_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1\" \"2\" \"3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-entry-put nil "A" "1")
  (org-entry-put nil "B" "2")
  (org-entry-put nil "C" "3")
  (list (org-entry-get nil "A") (org-entry-get nil "B") (org-entry-get nil "C")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-delete-multiple
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_entry_delete_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"2\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:B: 2\n:C: 3\n:END:")
  (goto-char (point-min))
  (org-entry-delete nil "A")
  (org-entry-delete nil "C")
  (list (org-entry-get nil "A") (org-entry-get nil "B") (org-entry-get nil "C")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-put with id
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_entry_put_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"test-id\" \"myid\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:ID: test-id\n:END:")
  (goto-char (point-min))
  (org-entry-put nil "CUSTOM_ID" "myid")
  (list (org-entry-get nil "ID") (org-entry-get nil "CUSTOM_ID")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-priority
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65 67)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* [#B] T")
  (goto-char (point-min))
  (org-priority ?A)
  (let ((p1 (org-element-property :priority (org-element-at-point))))
    (org-priority ?C)
    (let ((p2 (org-element-property :priority (org-element-at-point))))
      (list p1 p2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-priority-up/down
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_priority_ud() {
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
// org-get-priority
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_get_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument stringp (headline (:standard-properties [1 1 nil nil 10 0 (:title) first-section element t nil nil nil 1 #<killed buffer> [org-element-deferred org-element--headline-deferred nil t] nil (org-data (:standard-properties [1 1 1 27 27 0 nil org-data nil t nil 3 27 nil #<killed buffer> [org-element-deferred org-element--get-global-node-properties nil t] nil nil] :pre-blank 0 :path nil))] :pre-blank 0 :raw-value [org-element-deferred org-element--headline-parse-title (t) t] :title [org-element-deferred org-element--headline-parse-title (t) t] :level [org-element-deferred org-element--headline-parse-title (t) t] :priority [org-element-deferred org-element--headline-parse-title (t) t] :tags [org-element-deferred org-element--headline-parse-title (t) t] :todo-keyword [org-element-deferred org-element--headline-parse-title (t) t] :todo-type [org-element-deferred org-element--headline-parse-title (t) t] :footnote-section-p [org-element-deferred org-element--headline-parse-title (t) t] :archivedp [org-element-deferred org-element--headline-parse-title (t) t] :commentedp [org-element-deferred org-element--headline-parse-title (t) t])))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* [#A] T\n* [#B] U\n* [#C] V")
  (goto-char (point-min))
  (list (org-get-priority (org-element-at-point))
        (progn (forward-line) (org-get-priority (org-element-at-point)))
        (progn (forward-line) (org-get-priority (org-element-at-point)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_toggle_tag() {
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
// org-set-tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_set_tags() {
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
// org-get-tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_get_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"t1\" \"t2\") (\"t3\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T :t1:t2:\n* U :t3:")
  (goto-char (point-min))
  (list (org-get-tags)
        (progn (forward-line) (org-get-tags))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-get-tags inherited
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_get_tags_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:h1 nil) (:h2 nil) (:h3 nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+FILETAGS: :t1:t2:\n* H1\n** H2\n*** H3")
  (let ((r '()))
    (goto-char (point-min))
    (search-forward "H1")
    (push (list :h1 (org-get-tags nil 'inherit)) r)
    (search-forward "H2")
    (push (list :h2 (org-get-tags nil 'inherit)) r)
    (search-forward "H3")
    (push (list :h3 (org-get-tags nil 'inherit)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-todo
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_todo() {
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
fn uf48_todo_done() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"DONE\" 0 4 (org-todo-head \"TODO\")) #(\"* DONE H\" 0 8 (org-todo-head \"TODO\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO H")
  (goto-char (point-min))
  (org-todo 'done)
  (list (org-get-todo-state) (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-get-todo-state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_todo_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"TODO\" \"DONE\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\n* DONE D\n* WAITING W")
  (goto-char (point-min))
  (list (org-get-todo-state)
        (progn (forward-line) (org-get-todo-state))
        (progn (forward-line) (org-get-todo-state))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-is-todo-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_is_todo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"TODO\") nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\n* DONE D\n* H")
  (goto-char (point-min))
  (list (org-entry-is-todo-p)
        (progn (forward-line) (org-entry-is-todo-p))
        (progn (forward-line) (org-entry-is-todo-p))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-is-done-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_is_done() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (\"DONE\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\n* DONE D\n* H")
  (goto-char (point-min))
  (list (org-entry-is-done-p)
        (progn (forward-line) (org-entry-is-done-p))
        (progn (forward-line) (org-entry-is-done-p))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-todo-list (match)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf48_todo_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C")
  (list (org-map-entries (lambda () (org-get-heading t t t t)) "TODO" 'file)
        (org-map-entries (lambda () (org-get-heading t t t t)) "DONE" 'file)))"##,
        expect,
    );
}
