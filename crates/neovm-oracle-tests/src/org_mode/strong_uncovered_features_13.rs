//! Strong uncovered-features-13 oracle tests — test features not yet tested.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-timer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_timer_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- 0:00:00 :: * T\\n:LOGBOOK:\\n:END:\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\n:END:")
  (goto-char (point-min))
  (org-timer-start)
  (org-timer-item)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-todo-last-state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_todo_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#(\"DONE\" 0 4 (org-todo-head \"TODO\")) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T")
  (goto-char (point-min))
  (org-todo 'done)
  (let ((s1 (org-get-todo-state)))
    (org-todo 'none)
    (let ((s2 (org-get-todo-state)))
      (list s1 s2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-priority
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_priority() {
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
// org-toggle-tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"existing\" \"new\") (\"new\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T :existing:")
  (goto-char (point-min))
  (org-toggle-tag "new")
  (let ((t1 (org-get-tags)))
    (org-toggle-tag "existing")
    (let ((t2 (org-get-tags)))
      (list t1 t2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-set-property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"3\" \"2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-set-property "A" "1")
  (org-set-property "B" "2")
  (org-set-property "A" "3")
  (list (org-entry-get nil "A") (org-entry-get nil "B")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-columns
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_columns_view() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-get-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY\n* TODO [#A] H1\n* DONE [#B] H2")
  (let ((fmt (org-columns-get-format)))
    (mapcar (lambda (spec) (list (nth 1 spec) (nth 2 spec))) fmt)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-skip
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_agenda_skip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"B\" \"D\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n* DONE D")
  (goto-char (point-min))
  (let ((r '()))
    (while (re-search-forward org-heading-regexp nil t)
      (when (org-entry-is-done-p)
        (push (org-get-heading t t t t) r)))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-property-inherited
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_inherited() {
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
// org-at-property-p in drawer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_at_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:before nil) (:at t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:")
  (let ((r '()))
    (push (list :before (org-at-property-p)) r)
    (goto-char (point-min))
    (search-forward ":A:")
    (push (list :at (org-at-property-p)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-get-multivalued-property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"v1\" \"v2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: v1\n:A+: v2\n:END:")
  (goto-char (point-min))
  (org-entry-get-multivalued-property nil "A"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-entry-put-multivalued-property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_multi_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"v1 v2 v3\" (\"v1\" \"v2\" \"v3\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (org-entry-put-multivalued-property nil "A" "v1" "v2" "v3")
  (list (org-entry-get nil "A")
        (org-entry-get-multivalued-property nil "A")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timestamp-to-time
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_ts_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-timestamp-to-time)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t (org-timestamp-to-time (org-timestamp-from-string "<2026-01-15 Wed>"))))
  (list (nth 0 t) (nth 1 t)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timestamp-from-string
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_ts_from() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-timestamp-from-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ts (org-timestamp-from-string "<2026-01-15 Wed 10:30>")))
  (list (org-element-property :year-start ts)
        (org-element-property :month-start ts)
        (org-element-property :day-start ts)
        (org-element-property :hour-start ts)
        (org-element-property :minute-start ts)
        (org-element-property :dayofweek ts)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timestamp-format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_ts_fmt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-timestamp-from-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ts (org-timestamp-from-string "<2026-01-15 Wed 10:30>")))
  (org-timestamp-format ts "%Y-%m-%d %H:%M"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timestamp-up/down
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_ts_ud() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15 Wed>")
  (goto-char (point-min))
  (search-forward "<2026")
  (backward-char 2)
  (let ((d1 (org-element-property :day-start (org-element-context))))
    (org-timestamp-up-day)
    (let ((d2 (org-element-property :day-start (org-element-context))))
      (org-timestamp-down-day)
      (let ((d3 (org-element-property :day-start (org-element-context))))
        (list d1 d2 d3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-to-appt
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_appt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"No event to add\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15 Wed 10:00>\n* U\nDEADLINE: <2026-01-16 Thu 14:00>")
  (let ((appt-current-buffer (current-buffer)))
    (org-agenda-to-appt t)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-lint
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_lint() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <invalid>\nBody [[broken]]")
  (length (org-lint)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-parse-secondary-string
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_secondary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-map)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-element-map (org-element-parse-secondary-string "*bold* /italic/ \\usepackage{a}" (org-element-restriction 'paragraph))
  'object
  (lambda (o) (org-element-type o)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-restriction
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_restriction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-restriction)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-element-restriction 'paragraph)
        (org-element-restriction 'headline)
        (org-element-restriction 'item))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-property :robust-begin :robust-end
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_robust() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|---+---|\n| 1 | 2 |")
  (let ((table (org-element-at-point)))
    (list (org-element-property :robust-begin table)
          (org-element-property :robust-end table))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-parent-element
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_parent_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"bold\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* text")
  (search-forward "bold")
  (let* ((bold (org-element-context))
         (para (org-element-property :parent bold))
         (headline (org-element-property :parent para)))
    (list (org-element-type bold)
          (org-element-type para)
          (org-element-type headline))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-post-affiliated
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_affiliated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+NAME: tbl\n#+CAPTION: My Table\n| a |")
  (let ((el (org-element-at-point)))
    (list (org-element-property :name el)
          (org-element-property :caption el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-begin/end/post-blank
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 10 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n")
  (let ((el (org-element-at-point)))
    (list (org-element-property :begin el)
          (org-element-property :end el)
          (org-element-property :post-blank el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-greater-element-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_greater() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-greater-element-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n- item\n:drawer:\n:END:")
  (list (org-element-greater-element-p (org-element-at-point))
        (progn (goto-char (point-min)) (search-forward "item")
               (org-element-greater-element-p (org-element-at-point)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-contents-begin/end
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 21)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody 1\n** H2\nSub")
  (goto-char (point-min))
  (let ((h1 (org-element-at-point)))
    (list (org-element-property :contents-begin h1)
          (org-element-property :contents-end h1))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-put/get/delete-property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_el_pgd() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-delete-property)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (let ((h (org-element-at-point)))
    (org-element-put-property h :CUSTOM_ID "myid")
    (let ((v (org-element-property :CUSTOM_ID h)))
      (org-element-delete-property h :CUSTOM_ID)
      (list v (org-element-property :CUSTOM_ID h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-lineage (get full lineage)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_lineage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"italic\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara /italic/ text")
  (search-forward "italic")
  (let* ((obj (org-element-context))
         (lineage (org-element-lineage obj '(headline paragraph italic) t)))
    (mapcar 'org-element-type lineage)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-set-element
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp section)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (let ((h (org-element-at-point)))
    (org-element-set-element h 'section)
    (org-element-type h)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-swap-A-B
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf13_swap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"B\" \"C\") (\"B\" \"A\" \"C\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n* B\n* C")
  (goto-char (point-min))
  (let ((d1 (org-element-map (org-element-parse-buffer) 'headline
              (lambda (h) (org-element-property :raw-value h)))))
    (org-element-swap-A-B (org-element-at-point) (progn (forward-line) (org-element-at-point)))
    (let ((d2 (org-element-map (org-element-parse-buffer) 'headline
                (lambda (h) (org-element-property :raw-value h)))))
      (list d1 d2))))"##,
        expect,
    );
}
