//! Strong uncovered-features-20 oracle tests — complex state capture.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-at-heading-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_at_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:bol t) (:mid t) (:body nil) (:h2 t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n** H2")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :bol (org-at-heading-p)) r)
    (push (list :mid (progn (forward-char 2) (org-at-heading-p))) r)
    (forward-line)
    (push (list :body (org-at-heading-p)) r)
    (forward-line)
    (push (list :h2 (org-at-heading-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-table-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_at_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:before nil) (:in t) (:in2 t) (:after nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Before\n| a |\n| b |\nAfter")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :before (org-at-table-p)) r)
    (forward-line)
    (push (list :in (org-at-table-p)) r)
    (forward-line)
    (push (list :in2 (org-at-table-p)) r)
    (forward-line)
    (push (list :after (org-at-table-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-item-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_at_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:before nil) (:item t) (:cont nil) (:after nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Before\n- item\n  continued\nAfter")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :before (org-at-item-p)) r)
    (forward-line)
    (push (list :item (org-at-item-p)) r)
    (forward-line)
    (push (list :cont (org-at-item-p)) r)
    (forward-line)
    (push (list :after (org-at-item-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-planning-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_at_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:heading nil) (:plan t) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15>\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :heading (org-at-planning-p)) r)
    (forward-line)
    (push (list :plan (org-at-planning-p)) r)
    (forward-line)
    (push (list :body (org-at-planning-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-comment-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_at_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:c1 t) (:norm nil) (:c2 t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# comment\nNormal\n# another")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :c1 (org-at-comment-p)) r)
    (forward-line)
    (push (list :norm (org-at-comment-p)) r)
    (forward-line)
    (push (list :c2 (org-at-comment-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-block-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_at_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:begin t) (:inside nil) (:end nil) (:normal nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\nNormal")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :begin (org-at-block-p)) r)
    (forward-line)
    (push (list :inside (org-at-block-p)) r)
    (forward-line)
    (push (list :end (org-at-block-p)) r)
    (forward-line)
    (push (list :normal (org-at-block-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-timestamp-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_at_ts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:before nil) (:active year) (:inactive nil) (:after nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text <2026-01-15> more [2026-01-20] end")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :before (org-at-timestamp-p)) r)
    (search-forward "<2026")
    (push (list :active (org-at-timestamp-p)) r)
    (search-forward "[2026")
    (push (list :inactive (org-at-timestamp-p)) r)
    (goto-char (point-max))
    (push (list :after (org-at-timestamp-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-drawer-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_at_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:drawer t) (:prop nil) (:end t) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (forward-line)
    (push (list :drawer (org-at-drawer-p)) r)
    (forward-line)
    (push (list :prop (org-at-drawer-p)) r)
    (forward-line)
    (push (list :end (org-at-drawer-p)) r)
    (forward-line)
    (push (list :body (org-at-drawer-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-property-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_at_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:heading nil) (:prop_a t) (:prop_b t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:B: 2\n:END:")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :heading (org-at-property-p)) r)
    (search-forward ":A:")
    (push (list :prop_a (org-at-property-p)) r)
    (search-forward ":B:")
    (push (list :prop_b (org-at-property-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-clock-log-p at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_at_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:heading nil) (:clock t) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :heading (org-at-clock-log-p)) r)
    (forward-line)
    (push (list :clock (org-at-clock-log-p)) r)
    (forward-line)
    (push (list :body (org-at-clock-log-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-at-heading-or-item-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_at_hi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:heading t) (:item t) (:body nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n- item\nBody")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :heading (org-at-heading-or-item-p)) r)
    (forward-line)
    (push (list :item (org-at-heading-or-item-p)) r)
    (forward-line)
    (push (list :body (org-at-heading-or-item-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-narrow-to-subtree + org-show-context
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_narrow_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"** H2\\n*** H3\\nBody\" \"** H2\\n*** H3\\nBody\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b\n** H2b")
  (goto-char (point-min))
  (search-forward "H2")
  (beginning-of-line)
  (org-narrow-to-subtree)
  (let ((narrowed (buffer-string)))
    (org-show-context 'agenda)
    (let ((after (buffer-string)))
      (widen)
      (list narrowed after))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-heading on empty line
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_toggle_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((paragraph nil) (paragraph nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Before\n\nAfter")
  (goto-char (point-min))
  (forward-line)
  (org-toggle-heading)
  (org-element-map (org-element-parse-buffer) '(headline paragraph)
    (lambda (e) (list (org-element-type e)
                      (org-element-property :raw-value e)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-heading on plain text
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_toggle_plain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Plain text line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Plain text line")
  (goto-char (point-min))
  (org-toggle-heading)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-toggle-heading on numbered list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_toggle_numbered() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (headline plain-list item item)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "1. First\n2. Second\n3. Third")
  (goto-char (point-min))
  (org-toggle-heading)
  (org-element-map (org-element-parse-buffer) '(headline plain-list item)
    (lambda (e) (org-element-type e))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-sort-entries with different sort types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_sort_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO C\n* DONE A\n* TODO B")
  (org-sort-entries nil ?o)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :raw-value h)
                      (org-element-property :todo-keyword h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-sort-entries by priority
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_sort_prio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* [#C] C\n* [#A] A\n* [#B] B")
  (org-sort-entries nil ?p)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :raw-value h)
                      (org-element-property :priority h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-sort-entries by tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_sort_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* C :z:\n* A :a:\n* B :m:")
  (org-sort-entries nil ?t)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :raw-value h)
                      (org-element-property :tags h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-sort-entries by scheduled
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_sort_sched() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* C\nSCHEDULED: <2026-03-01>\n* A\nSCHEDULED: <2026-01-01>\n* B\nSCHEDULED: <2026-02-01>")
  (org-sort-entries nil ?s)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-clone-subtree with n=3
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_clone3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clone-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n** Sub")
  (goto-char (point-min))
  (org-clone-subtree 3)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-mark-subtree then kill/yank
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_mark_kill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H2\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n* H2")
  (goto-char (point-min))
  (org-mark-subtree)
  (kill-region (region-beginning) (region-end))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-end-of-subtree from different levels
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_end_sub() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:h1 34) (:h2 23) (:h3 23))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n** H2b\nSub\n* H1b\n** H2c")
  (goto-char (point-min))
  (let ((r '()))
    (org-end-of-subtree)
    (push (list :h1 (point)) r)
    (goto-char (point-min))
    (search-forward "H2\n")
    (beginning-of-line)
    (org-end-of-subtree)
    (push (list :h2 (point)) r)
    (goto-char (point-min))
    (search-forward "H3")
    (beginning-of-line)
    (org-end-of-subtree)
    (push (list :h3 (point)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-up-heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_up() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"Body\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
  (search-forward "Body")
  (beginning-of-line)
  (let ((r '()))
    (org-up-heading)
    (push (org-element-property :raw-value (org-element-at-point)) r)
    (org-up-heading)
    (push (org-element-property :raw-value (org-element-at-point)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-backward-heading-same-level
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_back_same() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n** C\n** D\n* E")
  (goto-char (point-max))
  (let ((r '()))
    (org-backward-heading-same-level 1)
    (push (org-element-property :raw-value (org-element-at-point)) r)
    (org-backward-heading-same-level 1)
    (push (org-element-property :raw-value (org-element-at-point)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-forward-heading-same-level
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_fwd_same() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"E\" \"E\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n** C\n** D\n* E")
  (goto-char (point-min))
  (let ((r '()))
    (org-forward-heading-same-level 1)
    (push (org-element-property :raw-value (org-element-at-point)) r)
    (org-forward-heading-same-level 1)
    (push (org-element-property :raw-value (org-element-at-point)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-next-visible-heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_next_vis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"B\" \"C\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n*** C\n* D")
  (goto-char (point-min))
  (let ((r '()))
    (org-next-visible-heading 1)
    (push (org-element-property :raw-value (org-element-at-point)) r)
    (org-next-visible-heading 1)
    (push (org-element-property :raw-value (org-element-at-point)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-previous-visible-heading
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_prev_vis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"C\" \"B\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n*** C\n* D")
  (goto-char (point-max))
  (let ((r '()))
    (org-previous-visible-heading 1)
    (push (org-element-property :raw-value (org-element-at-point)) r)
    (org-previous-visible-heading 1)
    (push (org-element-property :raw-value (org-element-at-point)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-next-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_next_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK (\"#+BEGIN_SRC emacs-lisp\" \"#+BEGIN_QUOTE\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Before\n#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\nBetween\n#+BEGIN_QUOTE\nQ\n#+END_QUOTE\nAfter")
  (goto-char (point-min))
  (let ((r '()))
    (org-next-block 1)
    (push (buffer-substring-no-properties (line-beginning-position) (line-end-position)) r)
    (org-next-block 1)
    (push (buffer-substring-no-properties (line-beginning-position) (line-end-position)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-previous-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf20_prev_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK (\"#+BEGIN_QUOTE\" \"#+BEGIN_SRC emacs-lisp\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Before\n#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\nBetween\n#+BEGIN_QUOTE\nQ\n#+END_QUOTE\nAfter")
  (goto-char (point-max))
  (let ((r '()))
    (org-previous-block 1)
    (push (buffer-substring-no-properties (line-beginning-position) (line-end-position)) r)
    (org-previous-block 1)
    (push (buffer-substring-no-properties (line-beginning-position) (line-end-position)) r)
    (nreverse r)))"##,
        expect,
    );
}
