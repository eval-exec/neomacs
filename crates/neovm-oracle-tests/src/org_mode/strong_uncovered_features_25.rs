//! Strong uncovered-features-25 oracle tests — org-agenda and org-capture.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{
    assert_oracle_parity, assert_oracle_parity_with_shared_tempdir,
    return_if_neovm_enable_oracle_proptest_not_set,
};

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_agenda_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r##"(let* ((file (expand-file-name "test.org" (getenv "NEOVM_ORACLE_TEST_TMPDIR")))
       (org-agenda-files (list file)))
  (with-temp-file file
    (insert "* TODO T\nSCHEDULED: <2026-01-15>\n* DONE D"))
  (condition-case nil
      (org-agenda-list)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-todo-list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_todo_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r##"(let* ((file (expand-file-name "test.org" (getenv "NEOVM_ORACLE_TEST_TMPDIR")))
       (org-agenda-files (list file)))
  (with-temp-file file
    (insert "* TODO T1\n* DONE D1\n* TODO T2"))
  (condition-case nil
      (org-todo-list)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-tags-view
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_tags_view() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r##"(let* ((file (expand-file-name "test.org" (getenv "NEOVM_ORACLE_TEST_TMPDIR")))
       (org-agenda-files (list file)))
  (with-temp-file file
    (insert "* T1 :work:\n* T2 :home:\n* T3 :work:"))
  (condition-case nil
      (org-tags-view nil "work")
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-search-view
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_search_view() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r##"(let* ((file (expand-file-name "test.org" (getenv "NEOVM_ORACLE_TEST_TMPDIR")))
       (org-agenda-files (list file)))
  (with-temp-file file
    (insert "* T1 keyword\n* T2 other\n* T3 keyword"))
  (condition-case nil
      (org-search-view nil "keyword")
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-filter
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_agenda_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:all 0) (:todo 0) (:done 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T1\n* DONE D1\n* TODO T2\n* DONE D2")
  (org-agenda-prepare-buffers (list (current-buffer)))
  (let ((r '()))
    (push (list :all (length (org-map-entries (lambda () t) nil 'file))) r)
    (push (list :todo (length (org-map-entries (lambda () t) "TODO" 'file))) r)
    (push (list :done (length (org-map-entries (lambda () t) "DONE" 'file))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-get-todos
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_agenda_todos() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"T1\" \"D1\" \"T2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T1\n* DONE D1\n* WAITING W1\n* TODO T2")
  (mapcar (lambda (x) (org-element-property :raw-value x))
          (org-element-map (org-element-parse-buffer) 'headline
            (lambda (h) (when (org-element-property :todo-keyword h) h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-get-deadlines
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_agenda_deadlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1\nDEADLINE: <2026-01-15>\n* T2\nDEADLINE: <2026-01-20>\n* T3")
  (org-element-map (org-element-parse-buffer) 'planning
    (lambda (p) (when (org-element-property :deadline p)
                  (org-element-property :raw-value
                    (org-element-property :parent p))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-get-scheduled
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_agenda_scheduled() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1\nSCHEDULED: <2026-01-15>\n* T2\nSCHEDULED: <2026-01-20>\n* T3")
  (org-element-map (org-element-parse-buffer) 'planning
    (lambda (p) (when (org-element-property :scheduled p)
                  (org-element-property :raw-value
                    (org-element-property :parent p))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-get-timestamps
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_agenda_timestamps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((active 2026 15) (inactive 2026 20) (active-range 2026 25))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1\n<2026-01-15>\n* T2\n[2026-01-20]\n* T3\n<2026-01-25>--<2026-01-30>")
  (org-element-map (org-element-parse-buffer) 'timestamp
    (lambda (ts) (list (org-element-property :type ts)
                      (org-element-property :year-start ts)
                      (org-element-property :day-start ts)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-get-blocks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_agenda_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((2026 15 2026 20))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1\n<2026-01-15>--<2026-01-20>\n* T2\n[2026-01-25]--[2026-01-30]")
  (org-element-map (org-element-parse-buffer) 'timestamp
    (lambda (ts) (when (eq (org-element-property :type ts) 'active-range)
                   (list (org-element-property :year-start ts)
                         (org-element-property :day-start ts)
                         (org-element-property :year-end ts)
                         (org-element-property :day-end ts))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-get-sexps
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_agenda_sexps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"%%(diary-anniversary 1 1 2000)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "%%(diary-anniversary 1 1 2000)")
  (org-element-map (org-element-parse-buffer) 'diary-sexp
    (lambda (d) (org-element-property :value d))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-to-appt
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_appt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"No event to add\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1\nSCHEDULED: <2026-01-15 10:00>\n* T2\nDEADLINE: <2026-01-16 14:00>")
  (condition-case nil
      (org-agenda-to-appt t)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-set-restriction-lock
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_restriction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (void-variable org-agenda-restrict-lock-current)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\n* H4")
  (goto-char (point-min))
  (search-forward "H2")
  (beginning-of-line)
  (condition-case nil
      (org-agenda-set-restriction-lock 'subtree)
    (error nil))
  (list org-agenda-restrict-lock-current))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-remove-restriction-lock
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_restriction_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (void-variable org-agenda-restrict-lock-current)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (condition-case nil
      (org-agenda-set-restriction-lock 'subtree)
    (error nil))
  (org-agenda-remove-restriction-lock)
  (list org-agenda-restrict-lock-current))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-prepare-buffers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_prepare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H\\nBody\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (org-agenda-prepare-buffers (list (current-buffer)))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-format-item
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T1 :work:")
  (goto-char (point-min))
  (condition-case nil
      (org-agenda-format-item nil "T1" 'todo nil nil nil)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-finalize
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_finalize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* TODO T1\\n* DONE D1\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T1\n* DONE D1")
  (condition-case nil
      (org-agenda-finalize)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-mark-filtered-text
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_filter_mark() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* TODO T1\\n* DONE D1\\n* TODO T2\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T1\n* DONE D1\n* TODO T2")
  (condition-case nil
      (org-agenda-mark-filtered-text)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-filter-apply
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_filter_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* TODO T1\\n* DONE D1\\n* TODO T2\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T1\n* DONE D1\n* TODO T2")
  (condition-case nil
      (org-agenda-filter-apply "+work")
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-redo
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_redo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* TODO T1\\n* DONE D1\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T1\n* DONE D1")
  (condition-case nil
      (org-agenda-redo)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-quit
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_quit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (condition-case nil
      (org-agenda-quit)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-agenda-exit
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf25_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (condition-case nil
      (org-agenda-exit)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}
