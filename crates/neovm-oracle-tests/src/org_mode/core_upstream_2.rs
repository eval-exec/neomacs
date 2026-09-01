//! Ported upstream ERT tests from org-mode's test-org.el (9.7.11) - batch 2.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ── Editing: org-edit-headline ───────────────────────────────────────

#[test]
fn upstream_org_edit_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* B\" \"* \" \"* A\" \"* TODO B\" \"* [#A] B\" \"* TODO [#A] B\" \"* B :tag:\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Basic edit.
     (with-temp-buffer (org-mode) (insert "* A")
       (goto-char (point-min))
       (org-edit-headline "B") (buffer-string))
     ;; Empty heading.
     (with-temp-buffer (org-mode) (insert "* A")
       (goto-char (point-min))
       (org-edit-headline "") (buffer-string))
     ;; From empty.
     (with-temp-buffer (org-mode) (insert "* ")
       (goto-char (point-min))
       (org-edit-headline "A") (buffer-string))
     ;; With TODO.
     (with-temp-buffer (org-mode) (insert "* TODO A")
       (goto-char (point-min))
       (org-edit-headline "B") (buffer-string))
     ;; With priority.
     (with-temp-buffer (org-mode) (insert "* [#A] A")
       (goto-char (point-min))
       (org-edit-headline "B") (buffer-string))
     ;; With TODO and priority.
     (with-temp-buffer (org-mode) (insert "* TODO [#A] A")
       (goto-char (point-min))
       (org-edit-headline "B") (buffer-string))
     ;; With tags.
     (with-temp-buffer (org-mode) (insert "* A :tag:")
       (goto-char (point-min))
       (let ((org-tags-column 4)) (org-edit-headline "B")) (buffer-string)))))"##,
        expect,
    );
}

// ── Editing: org-insert-heading ──────────────────────────────────────

#[test]
fn upstream_org_insert_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* \" \"* P\" \"* \\n* H\" \"** H\\nP\\n** \" \"\\n* \\n\\n* H1\" \"* \\n* \")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Empty buffer.
     (with-temp-buffer (org-mode)
       (org-insert-heading) (buffer-string))
     ;; At beginning of line.
     (with-temp-buffer (org-mode) (insert "P")
       (goto-char (point-min))
       (org-insert-heading) (buffer-string))
     ;; At beginning of headline: create above.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (org-insert-heading) (buffer-string))
     ;; New headline level depends on above.
     (with-temp-buffer (org-mode) (insert "** H\nP")
       (goto-char (point-max))
       (org-insert-heading) (buffer-string))
     ;; With blank-before-new-entry.
     (with-temp-buffer (org-mode) (insert "* H1")
       (goto-char (point-min))
       (let ((org-blank-before-new-entry '((heading . t))))
         (org-insert-heading)) (buffer-string))
     ;; Corner case: empty headline.
     (with-temp-buffer (org-mode) (insert "* ")
       (goto-char (point-min))
       (org-insert-heading) (buffer-string)))))"##,
        expect,
    );
}

// ── Editing: org-kill-line ───────────────────────────────────────────

#[test]
fn upstream_org_kill_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"ab\" \"\\n123\" \"* A :tag:\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; At beginning: kill whole line.
     (with-temp-buffer (org-mode) (insert "abc")
       (goto-char (point-min))
       (org-kill-line) (buffer-string))
     ;; In middle: kill until end.
     (with-temp-buffer (org-mode) (insert "abc")
       (goto-char (+ 2 (point-min)))
       (org-kill-line) (buffer-string))
     ;; Do not kill newline.
     (with-temp-buffer (org-mode) (insert "abc\n123")
       (goto-char (point-min))
       (org-kill-line) (buffer-string))
     ;; Special ctrl-k on headline.
     (with-temp-buffer (org-mode) (insert "* AB :tag:")
       (goto-char (point-min))
       (forward-char 3)
       (let ((org-special-ctrl-k t) (org-tags-column 0))
         (org-kill-line)) (buffer-string)))))"##,
        expect,
    );
}

// ── Editing: org-sort-entries ────────────────────────────────────────

#[test]
fn upstream_org_sort_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\n* abc\\n* def\\n* xyz\\n\" \"\\n* xyz\\n* def\\n* abc\\n\" \"\\n* 1\\n* 2\\n* 10\\n\" \"\\n* [#A] h2\\n* [#B] h3\\n* [#C] h1\\n\" \"\\n* [#C] h1\\n* [#B] h3\\n* [#A] h2\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Sort alphabetically ascending.
     (with-temp-buffer (org-mode)
       (insert "\n* def\n* xyz\n* abc\n")
       (goto-char (point-min))
       (org-sort-entries nil ?a) (buffer-string))
     ;; Sort alphabetically descending.
     (with-temp-buffer (org-mode)
       (insert "\n* def\n* xyz\n* abc\n")
       (goto-char (point-min))
       (org-sort-entries nil ?A) (buffer-string))
     ;; Sort numerically.
     (with-temp-buffer (org-mode)
       (insert "\n* 10\n* 1\n* 2\n")
       (goto-char (point-min))
       (org-sort-entries nil ?n) (buffer-string))
     ;; Sort by priority.
     (with-temp-buffer (org-mode)
       (insert "\n* [#C] h1\n* [#A] h2\n* [#B] h3\n")
       (goto-char (point-min))
       (org-sort-entries nil ?p) (buffer-string))
     ;; Sort by priority descending.
     (with-temp-buffer (org-mode)
       (insert "\n* [#C] h1\n* [#A] h2\n* [#B] h3\n")
       (goto-char (point-min))
       (org-sort-entries nil ?P) (buffer-string)))))"##,
        expect,
    );
}

// ── Editing: org-toggle-heading ──────────────────────────────────────

#[test]
fn upstream_org_toggle_heading_extended() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* Item\" \"Heading\" \"* Item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Toggle on.
     (with-temp-buffer (org-mode) (insert "Item")
       (goto-char (point-min))
       (org-toggle-heading) (buffer-string))
     ;; Toggle off.
     (with-temp-buffer (org-mode) (insert "* Heading")
       (goto-char (point-min))
       (org-toggle-heading) (buffer-string))
     ;; Toggle on numbered.
     (with-temp-buffer (org-mode) (insert "Item")
       (goto-char (point-min))
       (org-toggle-heading 1) (buffer-string)))))"##,
        expect,
    );
}

// ── Properties: org-set-property ─────────────────────────────────────

#[test]
fn upstream_org_set_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\":PROPERTIES:\\n:TEST: t\\n:END:\\n\" \"* H\\n:PROPERTIES:\\n:TEST: t\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Set property on empty buffer.
     (with-temp-buffer (org-mode)
       (let ((org-property-format "%s %s"))
         (org-set-property "TEST" "t"))
       (buffer-string))
     ;; Set property on headline.
     (with-temp-buffer (org-mode)
       (insert "* H")
       (goto-char (point-min))
       (let ((org-adapt-indentation nil) (org-property-format "%s %s"))
         (org-set-property "TEST" "t"))
       (buffer-string)))))"##,
        expect,
    );
}

// ── Properties: org-delete-property ──────────────────────────────────

#[test]
fn upstream_org_delete_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\" \":PROPERTIES:\\n:TEST1: t\\n:END:\" \"* H\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Delete from drawer.
     (with-temp-buffer (org-mode)
       (insert ":PROPERTIES:\n:TEST: t\n:END:")
       (goto-char (point-min))
       (org-delete-property "TEST") (buffer-string))
     ;; Delete one of two.
     (with-temp-buffer (org-mode)
       (insert ":PROPERTIES:\n:TEST1: t\n:TEST2: t\n:END:")
       (goto-char (point-min))
       (org-delete-property "TEST2") (buffer-string))
     ;; Delete from headline.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:TEST: t\n:END:")
       (goto-char (point-min))
       (org-delete-property "TEST") (buffer-string)))))"##,
        expect,
    );
}

// ── Properties: org-entry-get ────────────────────────────────────────

#[test]
fn upstream_org_entry_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1\" \"1\" \"1\" \"1 2 3\" \"\" nil \"1\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Regular get.
     (with-temp-buffer (org-mode)
       (insert ":PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min))
       (org-entry-get (point) "A"))
     ;; From headline.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min))
       (org-entry-get (point) "A"))
     ;; Ignore case.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min))
       (org-entry-get (point) "a"))
     ;; Extended values.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:A+: 2\n:A: 1\n:A+: 3\n:END:")
       (goto-char (point-min))
       (org-entry-get (point) "A"))
     ;; Empty value.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:A:\n:END:")
       (goto-char (point-min))
       (org-entry-get (point) "A"))
     ;; nil value.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:A: nil\n:END:")
       (goto-char (point-min))
       (org-entry-get (point) "A"))
     ;; Inheritance.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:A: 1\n:END:\n** H2")
       (goto-char (point-max))
       (org-entry-get (point) "A" t))
     ;; Not found.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min))
       (org-entry-get (point) "B")))))"##,
        expect,
    );
}

// ── Properties: org-entry-put ────────────────────────────────────────

#[test]
fn upstream_org_entry_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"* TODO H\" 0 8 (org-todo-head \"TODO\")) #(\"* H\" 0 3 (org-todo-head nil)) \"* [#A] H\" \"* H\\n:PROPERTIES:\\n:A:        2\\n:END:\" \"* H\\n:PROPERTIES:\\n:A:        1\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Set TODO property.
     (with-temp-buffer (org-mode)
       (insert "* H")
       (goto-char (point-min))
       (org-entry-put (point) "TODO" "TODO")
       (buffer-string))
     ;; Remove TODO.
     (with-temp-buffer (org-mode)
       (insert "* TODO H")
       (goto-char (point-min))
       (org-entry-put (point) "TODO" nil)
       (buffer-string))
     ;; Set priority.
     (with-temp-buffer (org-mode)
       (insert "* [#B] H")
       (goto-char (point-min))
       (org-entry-put (point) "PRIORITY" "A")
       (buffer-string))
     ;; Set regular property.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min))
       (org-entry-put (point) "A" "2")
       (buffer-string))
     ;; Set property without drawer.
     (with-temp-buffer (org-mode)
       (insert "* H")
       (goto-char (point-min))
       (org-entry-put (point) "A" "1")
       (buffer-string)))))"##,
        expect,
    );
}

// ── Timestamps: org-parse-time-string ────────────────────────────────

#[test]
fn upstream_org_timestamp_from_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2023 2023)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Active timestamp.
     (with-temp-buffer (org-mode)
       (insert "<2023-10-13 Fri>")
       (goto-char (point-min))
       (let ((ts (org-timestamp-from-string "<2023-10-13 Fri>")))
         (and ts (org-element-property :year-start ts))))
     ;; Inactive timestamp.
     (with-temp-buffer (org-mode)
       (insert "[2023-10-13 Fri]")
       (goto-char (point-min))
       (let ((ts (org-timestamp-from-string "[2023-10-13 Fri]")))
         (and ts (org-element-property :year-start ts)))))))"##,
        expect,
    );
}

#[test]
fn upstream_org_timestamp_to_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"-001-11-30\" \"-001-11-30 00:00\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   ;; Convert timestamp string to time.
   (format-time-string "%Y-%m-%d"
     (org-timestamp-to-time "<2023-10-13 Fri>"))
   ;; With time.
   (format-time-string "%Y-%m-%d %H:%M"
     (org-timestamp-to-time "<2023-10-13 Fri 14:30>"))))"##,
        expect,
    );
}

// ── Planning: org-deadline ───────────────────────────────────────────

#[test]
fn upstream_org_deadline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\nDEADLINE: <2012-03-29 Thu>\" \"* H\\nDEADLINE: <2014-03-04 Tue>\" \"* H\\nDEADLINE: <2012-03-29 Thu +2y>\" \"* H\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-adapt-indentation nil))
    (list
     ;; Insert new deadline.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (org-deadline nil "<2012-03-29>")
       (buffer-string))
     ;; Replace existing.
     (with-temp-buffer (org-mode) (insert "* H\nDEADLINE: <2012-03-29>")
       (goto-char (point-min))
       (org-deadline nil "<2014-03-04>")
       (buffer-string))
     ;; With repeater.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (org-deadline nil "<2012-03-29 +2y>")
       (buffer-string))
     ;; Remove with C-u.
     (with-temp-buffer (org-mode) (insert "* H\nDEADLINE: <2012-03-29>")
       (goto-char (point-min))
       (org-deadline '(4))
       (buffer-string)))))"##,
        expect,
    );
}

// ── Planning: org-schedule ───────────────────────────────────────────

#[test]
fn upstream_org_schedule() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\nSCHEDULED: <2012-03-29 Thu>\" \"* H\\nSCHEDULED: <2014-03-04 Tue>\" \"* H\\nSCHEDULED: <2012-03-29 Thu +2y>\" \"* H\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-adapt-indentation nil))
    (list
     ;; Insert new schedule.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (org-schedule nil "<2012-03-29>")
       (buffer-string))
     ;; Replace existing.
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2012-03-29>")
       (goto-char (point-min))
       (org-schedule nil "<2014-03-04>")
       (buffer-string))
     ;; With repeater.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (org-schedule nil "<2012-03-29 +2y>")
       (buffer-string))
     ;; Remove with C-u.
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2012-03-29>")
       (goto-char (point-min))
       (org-schedule '(4))
       (buffer-string)))))"##,
        expect,
    );
}

// ── Navigation: org-mark-element ─────────────────────────────────────

#[test]
fn upstream_org_mark_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((t t) (t t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Mark paragraph.
     (with-temp-buffer (org-mode) (insert "Paragraph")
       (goto-char (point-min))
       (org-mark-element)
       (list (bobp) (= (mark) (point-max))))
     ;; Mark in middle of two paragraphs.
     (with-temp-buffer (org-mode) (insert "P1\n\nParagraph\n\nP2")
       (goto-char (point-min))
       (forward-line 2)
       (org-mark-element)
       (list (looking-at "Paragraph")
             (org-with-point-at (mark) (looking-at "P2")))))))"##,
        expect,
    );
}

// ── Navigation: org-mark-subtree ─────────────────────────────────────

#[test]
fn upstream_org_mark_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((12 32) (1 32))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Mark current subtree.
     (with-temp-buffer (org-mode)
       (insert "* Headline\n** Sub-headline\nBody")
       (goto-char (point-min))
       (forward-line 2)
       (org-mark-subtree)
       (list (region-beginning) (region-end)))
     ;; With argument: move up.
     (with-temp-buffer (org-mode)
       (insert "* Headline\n** Sub-headline\nBody")
       (goto-char (point-min))
       (forward-line 2)
       (org-mark-subtree 1)
       (list (region-beginning) (region-end))))))"##,
        expect,
    );
}

// ── Navigation: org-collect-keywords ─────────────────────────────────

#[test]
fn upstream_org_collect_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (((\"TITLE\" \"My Title\") (\"AUTHOR\" \"Me\")) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Basic collection.
     (with-temp-buffer (org-mode)
       (insert "#+TITLE: My Title\n#+AUTHOR: Me\nBody")
       (goto-char (point-min))
       (org-collect-keywords '("TITLE" "AUTHOR")))
     ;; Inside example block: not collected.
     (with-temp-buffer (org-mode)
       (insert "#+begin_example\n#+foo: bar\n#+end_example")
       (goto-char (point-min))
       (org-collect-keywords '("FOO"))))))"##,
        expect,
    );
}

// ── Refile: org-refile-get-targets ───────────────────────────────────

#[test]
fn upstream_org_refile_get_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"H2\" \"H4\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let ((org-mode-hook nil)
        (org-refile-targets '((nil :maxlevel . 2))))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n*** H3\n* H4")
      (goto-char (point-min))
      (mapcar (lambda (r) (car r))
              (org-refile-get-targets)))))"##,
        expect,
    );
}

// ── TODO: org-todo ───────────────────────────────────────────────────

#[test]
fn upstream_org_todo_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"* TODO Heading\" 0 14 (org-todo-head \"TODO\")) #(\"* DONE Heading\" 0 14 (org-todo-head \"TODO\")) #(\"* Heading\" 0 9 (org-todo-head \"TODO\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-todo-keywords '((sequence "TODO" "DONE"))))
    (list
     ;; Cycle to TODO.
     (with-temp-buffer (org-mode) (insert "* Heading")
       (goto-char (point-min))
       (org-todo 'todo) (buffer-string))
     ;; Cycle to DONE.
     (with-temp-buffer (org-mode) (insert "* TODO Heading")
       (goto-char (point-min))
       (org-todo 'done) (buffer-string))
     ;; Cycle DONE -> empty.
     (with-temp-buffer (org-mode) (insert "* DONE Heading")
       (goto-char (point-min))
       (org-todo nil) (buffer-string)))))"##,
        expect,
    );
}

// ── Tags: org-set-tags-command ───────────────────────────────────────

#[test]
fn upstream_org_set_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* Heading                                                              :tag1:\" \"* Heading                                                               :new:\" \"* Heading                                                               :a:b:\" \"* Heading\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Set tag.
     (with-temp-buffer (org-mode) (insert "* Heading")
       (goto-char (point-min))
       (org-set-tags '("tag1"))
       (buffer-string))
     ;; Replace tag.
     (with-temp-buffer (org-mode) (insert "* Heading :old:")
       (goto-char (point-min))
       (org-set-tags '("new"))
       (buffer-string))
     ;; Multiple tags.
     (with-temp-buffer (org-mode) (insert "* Heading")
       (goto-char (point-min))
       (org-set-tags '("a" "b"))
       (buffer-string))
     ;; Remove tags.
     (with-temp-buffer (org-mode) (insert "* Heading :tag:")
       (goto-char (point-min))
       (org-set-tags nil)
       (buffer-string)))))"##,
        expect,
    );
}

// ── Clock: org-clock-table ───────────────────────────────────────────

#[test]
fn upstream_org_clock_table_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #<killed buffer>""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Task\n:LOGBOOK:\nCLOCK: [2023-10-13 Fri 10:00]--[2023-10-13 Fri 11:30] =>  1:30\n:END:")
      (goto-char (point-min))
      (let ((table (org-clock-get-table-data (current-buffer) '(:maxlevel 2))))
        (car table)))))"##,
        expect,
    );
}

// ── Footnote: org-footnote-action ────────────────────────────────────

#[test]
fn upstream_org_footnote_action() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"Text[fn:1]\\n\\n* Footnotes\\n\\n[fn:1] \\n\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Create footnote.
     (with-temp-buffer (org-mode) (insert "Text")
       (goto-char (point-max))
       (org-footnote-action)
       (buffer-string))
     ;; Go to definition.
     (with-temp-buffer (org-mode)
       (insert "Text[fn:1]\n\n[fn:1] Definition.")
       (goto-char (point-min))
       (search-forward "[fn:1]")
       (org-footnote-action)
       (looking-at "Definition.")))))"##,
        expect,
    );
}

// ── List: org-list-struct ────────────────────────────────────────────

#[test]
fn upstream_org_list_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- item1\n- item2\n  - sub1\n  - sub2\n- item3")
      (goto-char (point-min))
      (let ((struct (org-list-struct)))
        (length struct)))))"##,
        expect,
    );
}

// ── List: org-toggle-checkbox ────────────────────────────────────────

#[test]
fn upstream_org_toggle_checkbox() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"- item\" \"- [ ] item\" \"- [ ] item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Toggle on.
     (with-temp-buffer (org-mode) (insert "- item")
       (goto-char (point-min))
       (org-toggle-checkbox) (buffer-string))
     ;; Toggle off.
     (with-temp-buffer (org-mode) (insert "- [X] item")
       (goto-char (point-min))
       (org-toggle-checkbox) (buffer-string))
     ;; Toggle to intermediate.
     (with-temp-buffer (org-mode) (insert "- [X] item")
       (goto-char (point-min))
       (org-toggle-checkbox 'checkbox) (buffer-string)))))"##,
        expect,
    );
}

// ── List: org-cycle-list-bullet ──────────────────────────────────────

#[test]
fn upstream_org_cycle_list_bullet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"+ item\" \"1. item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Cycle from dash.
     (with-temp-buffer (org-mode) (insert "- item")
       (goto-char (point-min))
       (org-cycle-list-bullet)
       (buffer-string))
     ;; Cycle from plus.
     (with-temp-buffer (org-mode) (insert "+ item")
       (goto-char (point-min))
       (org-cycle-list-bullet)
       (buffer-string)))))"##,
        expect,
    );
}
