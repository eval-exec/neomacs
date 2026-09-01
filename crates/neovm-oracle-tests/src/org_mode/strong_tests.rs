//! Strong org-mode oracle tests — every test returns concrete data.
//!
//! Each test calls `assert_oracle_parity` with Elisp that produces a
//! specific, structured result (lists, strings, numbers). If Neomacs
//! and GNU Emacs diverge on any value, the test fails with a colored
//! diff showing exactly where.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Parser structure: every element type must parse identically
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_parse_all_element_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((headline . 1) (section . 2) (paragraph . 6) (bold . 1) (italic . 1) (underline . 0) (verbatim . 0) (code . 1) (strike-through . 0) (link . 1) (citation . 1) (citation-reference . 1) (footnote-reference . 1) (footnote-definition . 1) (quote-block . 1) (src-block . 0) (center-block . 0) (table . 1) (table-row . 3) (table-cell . 4) (plain-list . 1) (item . 2) (planning . 1) (clock . 1) (property-drawer . 1) (drawer . 1) (keyword . 1) (entity . 1) (latex-fragment . 1) (macro . 1) (statistics-cookie . 0) (target . 1) (radio-target . 1) (timestamp . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: T
* TODO [#A] H1 :tag:
SCHEDULED: <2024-01-15 Mon>
:PROPERTIES:
:CUSTOM_ID: id1
:END:
:LOGBOOK:
CLOCK: [2024-01-15 Mon 09:00]--[2024-01-15 Mon 10:00] =>  1:00
:END:
Para *bold* /italic/ ~code~.
[[https://example.org][link]] [cite:@k1] [fn:1]
| a | b |
|---+---|
| 1 | 2 |
#+BEGIN_QUOTE
quoted
#+END_QUOTE
- [X] item1
- [ ] item2
<<target>>
<<<radio>>>
{{{macro}}}
$E=mc^2$
\\alpha
<2024-01-20 Sat 14:00>
[fn:1] Footnote def.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (counts (mapcar (lambda (type)
                               (cons type (length (org-element-map tree type #'identity))))
                             '(headline section paragraph bold italic underline
                               verbatim code strike-through link citation
                               citation-reference footnote-reference footnote-definition
                               quote-block src-block center-block table table-row table-cell
                               plain-list item planning clock property-drawer drawer
                               keyword entity latex-fragment macro statistics-cookie
                               target radio-target timestamp))))
        counts))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Property access: every property must return the same value
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_headline_property_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"TODO\" 65 (\"work\" \"urgent\") \"Project\" 1 t t t t t org-data)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] Project :work:urgent:\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (hl (car (org-element-map tree 'headline #'identity))))
        (list (org-element-property :todo-keyword hl)
              (org-element-property :priority hl)
              (org-element-property :tags hl)
              (substring-no-properties (org-element-property :raw-value hl))
              (org-element-property :level hl)
              (numberp (org-element-property :begin hl))
              (numberp (org-element-property :end hl))
              (numberp (org-element-property :contents-begin hl))
              (numberp (org-element-property :contents-end hl))
              (numberp (org-element-property :post-blank hl))
              (org-element-type (org-element-property :parent hl)))))))"##,
        expect,
    );
}

#[test]
fn strong_planning_timestamp_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (2024 1 15 nil nil cumulate 1 week all 3 day 2024 1 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H\nDEADLINE: <2024-01-15 Mon +1w -3d> SCHEDULED: <2024-01-14 Sun>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (planning (car (org-element-map tree 'planning #'identity)))
             (dl (org-element-property :deadline planning))
             (sc (org-element-property :scheduled planning)))
        (list
         ;; Deadline components.
         (org-element-property :year-start dl)
         (org-element-property :month-start dl)
         (org-element-property :day-start dl)
         (org-element-property :hour-start dl)
         (org-element-property :minute-start dl)
         ;; Repeater.
         (org-element-property :repeater-type dl)
         (org-element-property :repeater-value dl)
         (org-element-property :repeater-unit dl)
         ;; Warning.
         (org-element-property :warning-type dl)
         (org-element-property :warning-value dl)
         (org-element-property :warning-unit dl)
         ;; Scheduled.
         (org-element-property :year-start sc)
         (org-element-property :month-start sc)
         (org-element-property :day-start sc))))))"##,
        expect,
    );
}

#[test]
fn strong_link_property_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"https\" \"//orgmode.org\" has-desc) (\"file\" \"path.org\" has-desc) (\"id\" \"uuid\" has-desc))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "[[https://orgmode.org][Org mode]] and [[file:path.org::*heading][file]] and [[id:uuid][id]].")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (links (org-element-map tree 'link #'identity)))
        (mapcar (lambda (l)
                  (list (org-element-property :type l)
                        (org-element-property :path l)
                        (if (org-element-contents l) 'has-desc 'no-desc)))
                links)))))"##,
        expect,
    );
}

#[test]
fn strong_src_block_property_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"emacs-lisp\" \"-n -r\" \":results output :exports code\" \"(+ 1 2)\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_SRC emacs-lisp -n -r :results output :exports code\n(+ 1 2)\n#+END_SRC")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (src (car (org-element-map tree 'src-block #'identity))))
        (list (org-element-property :language src)
              (org-element-property :switches src)
              (org-element-property :parameters src)
              (substring-no-properties (org-element-property :value src)))))))"##,
        expect,
    );
}

#[test]
fn strong_timestamp_all_types_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((active 2024 1 15 nil nil 2024 1 15 nil nil) (inactive 2024 1 15 nil nil 2024 1 15 nil nil) (active 2024 1 15 14 30 2024 1 15 14 30) (active-range 2024 1 15 nil nil 2024 1 16 nil nil) (active-range 2024 1 15 14 30 2024 1 15 15 30) (diary nil nil nil nil nil nil nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2024-01-15 Mon>\n[2024-01-15 Mon]\n<2024-01-15 Mon 14:30>\n<2024-01-15 Mon>--<2024-01-16 Tue>\n<2024-01-15 Mon 14:30-15:30>\n<%%(diary-float t 4 2)>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (timestamps (org-element-map tree 'timestamp #'identity)))
        (mapcar (lambda (ts)
                  (list (org-element-property :type ts)
                        (org-element-property :year-start ts)
                        (org-element-property :month-start ts)
                        (org-element-property :day-start ts)
                        (org-element-property :hour-start ts)
                        (org-element-property :minute-start ts)
                        (org-element-property :year-end ts)
                        (org-element-property :month-end ts)
                        (org-element-property :day-end ts)
                        (org-element-property :hour-end ts)
                        (org-element-property :minute-end ts)))
                timestamps)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edit operations: buffer content after operations must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_edit_headline_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"* B\" \"* TODO B\" \"* [#A] B\" \"* B :tag:\" \"* A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* TODO A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* [#A] A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* A :tag:")
       (goto-char (point-min)) (let ((org-tags-column 4)) (org-edit-headline "B")) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* ")
       (goto-char (point-min)) (org-edit-headline "A") (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn strong_insert_heading_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* \" \"* \\n* H\" \"** H\\nP\\n** \" \"\\n* \\n\\n* H1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Empty buffer.
     (with-temp-buffer (org-mode) (org-insert-heading) (buffer-string))
     ;; At beginning of headline.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-insert-heading) (buffer-string))
     ;; New headline level depends on above.
     (with-temp-buffer (org-mode) (insert "** H\nP")
       (goto-char (point-max)) (org-insert-heading) (buffer-string))
     ;; With blank-before-new-entry.
     (with-temp-buffer (org-mode) (insert "* H1")
       (goto-char (point-min))
       (let ((org-blank-before-new-entry '((heading . t)))) (org-insert-heading))
       (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn strong_toggle_heading_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* Item\" \"Heading\" \"* Item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "Item")
       (goto-char (point-min)) (org-toggle-heading) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* Heading")
       (goto-char (point-min)) (org-toggle-heading) (buffer-string))
     (with-temp-buffer (org-mode) (insert "Item")
       (goto-char (point-min)) (org-toggle-heading 1) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Navigation: point position after navigation must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_navigation_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n*** H3\n** H4\n* H5")
      (goto-char (point-min))
      (list
       ;; next-visible-heading
       (progn (org-next-visible-heading 1) (looking-at-p "\\* H1"))
       (progn (org-next-visible-heading 1) (looking-at-p "\\*\\* H2"))
       ;; forward-heading-same-level
       (progn (org-forward-heading-same-level 1) (looking-at-p "\\*\\* H4"))
       ;; up-heading-safe
       (progn (org-up-heading-safe) (looking-at-p "\\* H1"))
       ;; back to heading
       (progn (forward-line 3) (org-back-to-heading) (bobp))))))"##,
        expect,
    );
}

#[test]
fn strong_navigation_end_of_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\n* H\n** S1\n** S2\nasd\n* H2")
      (goto-char (point-min)) (forward-line 1) (org-end-of-subtree)
      (forward-char)
      (looking-at-p "^\\* H2"))))"##,
        expect,
    );
}

#[test]
fn strong_navigation_forward_backward_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "First.\n\n\nSecond.")
      (goto-char (point-min))
      (org-forward-element)
      (looking-at "Second."))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Operations: specific results, not booleans
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_deadline_insert_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\nDEADLINE: <2012-03-29 Thu>\" \"* H\\nDEADLINE: <2014-03-04 Tue>\" \"* H\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-adapt-indentation nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-deadline nil "<2012-03-29>") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H\nDEADLINE: <2012-03-29>")
       (goto-char (point-min)) (org-deadline nil "<2014-03-04>") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H\nDEADLINE: <2012-03-29>")
       (goto-char (point-min)) (org-deadline '(4)) (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn strong_schedule_insert_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\nSCHEDULED: <2012-03-29 Thu>\" \"* H\\nSCHEDULED: <2014-03-04 Tue>\" \"* H\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-adapt-indentation nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-schedule nil "<2012-03-29>") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2012-03-29>")
       (goto-char (point-min)) (org-schedule nil "<2014-03-04>") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2012-03-29>")
       (goto-char (point-min)) (org-schedule '(4)) (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn strong_property_operations_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"1\" \"1\" \"1 2\" \"1\" \"* H\\n:PROPERTIES:\\n:A:        1\\n:END:\\n\" \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; entry-get.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     ;; entry-get ignore case.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "a"))
     ;; entry-get extended.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A+: 2\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     ;; entry-get inheritance.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:\n** H2")
       (goto-char (point-max)) (org-entry-get (point) "A" t))
     ;; entry-put result.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-entry-put (point) "A" "1") (buffer-string))
     ;; delete-property result.
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:TEST: t\n:END:")
       (goto-char (point-min)) (org-delete-property "TEST") (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn strong_tag_operations_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H                                                                    :test:\" \"* H\" \"* H                                                                     :a:b:\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-toggle-tag "test") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H :test:")
       (goto-char (point-min)) (org-toggle-tag "test") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-set-tags '("a" "b")) (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn strong_todo_cycle_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"* TODO H\" 0 8 (org-todo-head \"TODO\")) #(\"* DONE H\" 0 8 (org-todo-head \"TODO\")) #(\"* H\" 0 3 (org-todo-head \"TODO\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-todo-keywords '((sequence "TODO" "DONE"))))
    (list
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-todo 'todo) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* TODO H")
       (goto-char (point-min)) (org-todo 'done) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* DONE H")
       (goto-char (point-min)) (org-todo nil) (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn strong_sort_entries_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\n* abc\\n* def\\n* xyz\\n\" \"\\n* 1\\n* 2\\n* 10\\n\" \"\\n* [#A] h2\\n* [#B] h3\\n* [#C] h1\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "\n* def\n* xyz\n* abc\n")
       (goto-char (point-min)) (org-sort-entries nil ?a) (buffer-string))
     (with-temp-buffer (org-mode) (insert "\n* 10\n* 1\n* 2\n")
       (goto-char (point-min)) (org-sort-entries nil ?n) (buffer-string))
     (with-temp-buffer (org-mode) (insert "\n* [#C] h1\n* [#A] h2\n* [#B] h3\n")
       (goto-char (point-min)) (org-sort-entries nil ?p) (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn strong_move_subtree_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-move-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* A\nBody A\n* B\nBody B\n* C\nBody C")
       (goto-char (point-min)) (org-move-subtree 1)
       (buffer-substring-no-properties (point-min) (point-max)))
     (with-temp-buffer (org-mode) (insert "* A\nBody A\n* B\nBody B\n* C\nBody C")
       (goto-char (point-min)) (forward-line 2) (org-move-subtree -1)
       (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn strong_promote_demote_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\" \"** H\" \"* H1\\n** S1\\n** S2\" \"** H1\\n*** S1\\n*** S2\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "** H")
       (goto-char (point-min)) (org-promote) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-demote) (buffer-string))
     (with-temp-buffer (org-mode) (insert "** H1\n*** S1\n*** S2")
       (goto-char (point-min)) (org-promote-subtree) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H1\n** S1\n** S2")
       (goto-char (point-min)) (org-demote-subtree) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Table operations: buffer content must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_operations_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Format specifier doesn’t match argument type\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Align.
     (with-temp-buffer (org-mode) (insert "|a|b|\n|c|d|")
       (goto-char (point-min)) (org-table-align)
       (buffer-substring-no-properties (point-min) (point-max)))
     ;; Insert column.
     (with-temp-buffer (org-mode) (insert "| a | b |\n| c | d |")
       (goto-char (point-min)) (org-table-insert-column)
       (buffer-substring-no-properties (point-min) (point-max)))
     ;; Delete column.
     (with-temp-buffer (org-mode) (insert "| a | b | c |\n| d | e | f |")
       (goto-char (point-min)) (forward-char 4) (org-table-delete-column)
       (buffer-substring-no-properties (point-min) (point-max)))
     ;; Insert row.
     (with-temp-buffer (org-mode) (insert "| a | b |\n| c | d |")
       (goto-char (point-min)) (org-table-insert-row)
       (buffer-substring-no-properties (point-min) (point-max)))
     ;; Transpose.
     (with-temp-buffer (org-mode) (insert "| a | b |\n| c | d |\n| e | f |")
       (goto-char (point-min)) (org-table-transpose-table-at-point)
       (buffer-substring-no-properties (point-min) (point-max)))
     ;; Sort.
     (with-temp-buffer (org-mode) (insert "| c |\n| a |\n| b |")
       (goto-char (point-min)) (org-table-sort-lines ?a 'string)
       (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn strong_table_formula_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a #+TBLFM line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (list
     ;; Column sum.
     (with-temp-buffer (org-mode)
       (insert "| 2 |\n| 4 |\n| 8 |\n|   |\n#+TBLFM: @>$1=vsum(@<..@>>)")
       (goto-char (point-min)) (org-table-calc-current-TBLFM)
       (buffer-substring-no-properties (point-min) (point-max)))
     ;; Multiply.
     (with-temp-buffer (org-mode)
       (insert "| 3 | 4 |   |\n#+TBLFM: $3=$1*$2")
       (goto-char (point-min)) (org-table-calc-current-TBLFM)
       (buffer-substring-no-properties (point-min) (point-max)))
     ;; Remote reference.
     (with-temp-buffer (org-mode)
       (insert "#+NAME: tbl\n| 1 | 2 |\n| 3 | 4 |\n\n|   |   |\n#+TBLFM: $1=remote(tbl,@2$1)::$2=remote(tbl,@2$2)")
       (goto-char (point-min)) (org-table-calc-current-TBLFM)
       (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn table_ref_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"A2\" \"A1 = $0\" \"C& = remote(FOO, @@#B&)\" \"@2$1\" \"@1$1 = $0\" \"$3 = remote(FOO, @@#$2)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (list
   (org-table-convert-refs-to-an "@2$1")
   (org-table-convert-refs-to-an "@1$1 = $0")
   (org-table-convert-refs-to-an "$3 = remote(FOO, @@#$2)")
   (org-table-convert-refs-to-rc "A2")
   (org-table-convert-refs-to-rc "A1 = $0")
   (org-table-convert-refs-to-rc "C& = remote(FOO, @@#B&)")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// List operations: buffer content must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_list_checkbox_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"- item\" \"- [ ] item\" \"- [ ] item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "- item")
       (goto-char (point-min)) (org-toggle-checkbox) (buffer-string))
     (with-temp-buffer (org-mode) (insert "- [X] item")
       (goto-char (point-min)) (org-toggle-checkbox) (buffer-string))
     (with-temp-buffer (org-mode) (insert "- [X] item")
       (goto-char (point-min)) (org-toggle-checkbox 'checkbox) (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn strong_cycle_list_bullet_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"  + item\" \"1. item\" \"- item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-plain-list-ordered-item-terminator t))
    (list
     (with-temp-buffer (org-mode) (insert "  - item")
       (goto-char (point-min)) (org-cycle-list-bullet) (buffer-string))
     (with-temp-buffer (org-mode) (insert "- item")
       (goto-char (point-min)) (org-cycle-list-bullet "1.") (buffer-string))
     (with-temp-buffer (org-mode) (insert "+ item")
       (goto-char (point-min)) (org-cycle-list-bullet 'previous) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Footnote operations: buffer content must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_footnote_new_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Text[fn:1]\\n\\n[fn:1] \\n\" \"Text[fn::]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-footnote-auto-label t)
        (org-footnote-section nil))
    (list
     (with-temp-buffer (org-mode) (insert "Text")
       (goto-char (point-max)) (org-footnote-new) (buffer-string))
     (with-temp-buffer (org-mode) (insert "Text")
       (goto-char (point-max))
       (let ((org-footnote-auto-label 'anonymous))
         (org-footnote-new)) (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn strong_footnote_delete_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Don’t know which footnote to remove\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-footnote-section nil))
    (list
     (with-temp-buffer (org-mode)
       (insert "Text[fn:1]\n\n[fn:1] Def")
       (goto-char (point-min)) (search-forward "[fn:1]")
       (org-footnote-delete) (org-trim (buffer-string)))
     (with-temp-buffer (org-mode)
       (insert "Para[fn::def]")
       (goto-char (point-min)) (search-forward "[fn::")
       (org-footnote-delete) (org-trim (buffer-string))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Fill operations: buffer content must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_fill_element_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"| a |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table-row)) \"A B\" \"- A B\" \"  # A B\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Table alignment.
     (with-temp-buffer (org-mode) (insert "|a|")
       (goto-char (point-min)) (org-fill-element) (buffer-string))
     ;; Paragraph fill.
     (with-temp-buffer (org-mode) (insert "A\nB")
       (goto-char (point-max)) (let ((fill-column 20)) (org-fill-element)) (buffer-string))
     ;; Item fill.
     (with-temp-buffer (org-mode) (insert "- A\n  B")
       (goto-char (point-min)) (let ((fill-column 20)) (org-fill-element)) (buffer-string))
     ;; Comment fill.
     (with-temp-buffer (org-mode) (insert "  # A\n  # B")
       (goto-char (point-min)) (let ((fill-column 20)) (org-fill-element)) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Fold operations: property on buffer must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_fold_operations_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-fold)
  (let ((org-mode-hook nil))
    (list
     ;; Hide drawer.
     (with-temp-buffer (org-mode) (insert ":drawer:\ncontents\n:end:")
       (goto-char (point-min)) (org-fold-show-all)
       (org-fold-hide-drawer-toggle)
       (get-char-property (line-end-position) 'invisible))
     ;; Show drawer.
     (with-temp-buffer (org-mode) (insert ":drawer:\ncontents\n:end:")
       (goto-char (point-min))
       (org-fold-hide-drawer-toggle)
       (org-fold-hide-drawer-toggle 'off)
       (get-char-property (line-end-position) 'invisible))
     ;; Hide block.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_CENTER\ncontents\n#+END_CENTER")
       (goto-char (point-min))
       (org-fold-hide-block-toggle)
       (get-char-property (line-end-position) 'invisible))
     ;; Show block.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_CENTER\ncontents\n#+END_CENTER")
       (goto-char (point-min))
       (org-fold-hide-block-toggle)
       (org-fold-hide-block-toggle 'off)
       (get-char-property (line-end-position) 'invisible)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Indent operations: buffer content must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_indent_line_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (0 2 0 \"* H\\n:PROPERTIES:\\n:key:      value\\n:END:\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Headline: no indent.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-indent-line) (org-get-indentation))
     ;; Body: indent according to level.
     (with-temp-buffer (org-mode) (insert "* H\nA")
       (goto-char (point-max)) (let ((org-adapt-indentation t)) (org-indent-line)) (org-get-indentation))
     ;; Body: no indent when disabled.
     (with-temp-buffer (org-mode) (insert "* H\nA")
       (goto-char (point-max)) (let ((org-adapt-indentation nil)) (org-indent-line)) (org-get-indentation))
     ;; Property alignment.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:key: value\n:END:")
       (goto-char (point-min)) (forward-line 2)
       (let ((org-property-format "%-10s %s")) (org-indent-line))
       (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Timer/duration: return values must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_timer_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"0:00:30\" \"0:02:10\" \"1:01:30\" \"-1:01:30\" 30 130 3690 \"1:02:03\" \"0:02:03\" \"0:00:03\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-timer)
  (list
   (org-timer-secs-to-hms 30)
   (org-timer-secs-to-hms 130)
   (org-timer-secs-to-hms 3690)
   (org-timer-secs-to-hms -3690)
   (org-timer-hms-to-secs "0:00:30")
   (org-timer-hms-to-secs "0:02:10")
   (org-timer-hms-to-secs "1:01:30")
   (org-timer-fix-incomplete "1:02:03")
   (org-timer-fix-incomplete "02:03")
   (org-timer-fix-incomplete "03")))"##,
        expect,
    );
}

#[test]
fn strong_duration_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (61.0 80.5 130.0 1502.0 150.0 2.0 0.0 \"1:00\" \"1:01:30\" \"1h\" \"0h 50min\" 0 0 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-duration)
  (list
   (org-duration-to-minutes "1:01")
   (org-duration-to-minutes "1:20:30")
   (org-duration-to-minutes "2h 10min")
   (org-duration-to-minutes "1d 1:02")
   (org-duration-to-minutes "2.5h")
   (org-duration-to-minutes "2")
   (org-duration-to-minutes "")
   (let ((org-duration-format 'h:mm)) (org-duration-from-minutes 60))
   (let ((org-duration-format 'h:mm:ss)) (org-duration-from-minutes 61.5))
   (let ((org-duration-format '(("h" . nil) ("min" . nil)))) (org-duration-from-minutes 60))
   (let ((org-duration-format '(("h" . t) ("min" . t)))) (org-duration-from-minutes 50))
   (org-duration-p "3:12")
   (org-duration-p "3d 3h 4min")
   (org-duration-p "3::12")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Colview: format compilation must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_colview_formats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"ITEM\" \"ITEM\" nil nil nil)) ((\"ITEM\" \"ITEM\" nil nil nil) (\"TODO\" \"TODO\" nil nil nil)) ((\"ITEM\" \"ITEM\" 10 nil nil)) ((\"ITEM\" \"some title\" nil nil nil)) ((\"ITEM\" \"ITEM\" nil \"+\" nil)) ((\"ITEM\" \"ITEM\" nil \"+\" \"%.1f\")) \"%ITEM\" \"%10ITEM\" \"%ITEM(some title)\" \"%ITEM{+}\" \"%ITEM{+;%.1f}\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-colview)
  (list
   (org-columns-compile-format "%ITEM")
   (org-columns-compile-format "%ITEM %TODO")
   (org-columns-compile-format "%10ITEM")
   (org-columns-compile-format "%ITEM(some title)")
   (org-columns-compile-format "%ITEM{+}")
   (org-columns-compile-format "%ITEM{+;%.1f}")
   (org-columns-uncompile-format '(("ITEM" "ITEM" nil nil nil)))
   (org-columns-uncompile-format '(("ITEM" "ITEM" 10 nil nil)))
   (org-columns-uncompile-format '(("ITEM" "some title" nil nil nil)))
   (org-columns-uncompile-format '(("ITEM" "ITEM" nil "+" nil)))
   (org-columns-uncompile-format '(("ITEM" "ITEM" nil "+" "%.1f")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Macros: buffer content after expansion must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_macro_expansion_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-macro)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode)
       (insert "#+MACRO: A B\n1 {{{A}}} 3")
       (goto-char (point-min)) (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates) (buffer-string))
     (with-temp-buffer (org-mode)
       (insert "#+MACRO: macro $1 $2\n{{{macro(some,text)}}}")
       (goto-char (point-min)) (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates) (buffer-string))
     (with-temp-buffer (org-mode)
       (insert "#+MACRO: in inner\n#+MACRO: out {{{in}}} outer\n{{{out}}}")
       (goto-char (point-min)) (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Archive: buffer content after archive must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_archive_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"No file associated to buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-archive)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Top\n** DONE One\n** TODO Two")
      (goto-char (point-min)) (forward-line 1) (org-archive-subtree)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Datetree: buffer content after create must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_datetree_create_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* 2012\\n\\n** 2012-03 March\\n\\n*** 2012-03-29 Thursday\" \"* 2012\\n\\n** 2012-03 March\\n\\n*** 2012-03-29 Thursday\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-datetree)
  (let ((org-mode-hook nil)
        (org-datetree-add-timestamp nil)
        (org-blank-before-new-entry '((heading . t))))
    (list
     (with-temp-buffer (org-mode)
       (org-datetree-find-date-create '(3 29 2012))
       (org-trim (buffer-string)))
     (with-temp-buffer (org-mode) (insert "* 2012\n")
       (org-datetree-find-date-create '(3 29 2012))
       (org-trim (buffer-string))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Protocol: parsed parameters must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_protocol_parse_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"abc\" \"def\") (\"abc\" \"def\") (\"abc\" \"def\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-protocol)
  (list
   (let ((data (org-protocol-parse-parameters '(:url "abc" :title "def") nil)))
     (list (plist-get data :url) (plist-get data :title)))
   (let ((data (org-protocol-parse-parameters "url=abc&title=def" t)))
     (list (plist-get data :url) (plist-get data :title)))
   (let ((data (org-protocol-parse-parameters "abc/def" nil '(:url :title))))
     (list (plist-get data :url) (plist-get data :title)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Capture: template expansion must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_capture_template_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"success!\\n\" \"2026\\n\" \"\" \"%i\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-capture)
  (let ((org-store-link-plist nil))
    (list
     (org-capture-fill-template "%(concat \"success\" \"!\")")
     (org-capture-fill-template "%<%Y>")
     (org-capture-fill-template "%i" "success!")
     (org-capture-fill-template "\\%i" "success!"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Outline path: return values must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_outline_path_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil (\"H\") (\"H\") (\"H\") (\"H\") #(\"one/two/three\" 0 3 (face org-level-1) 4 7 (face org-level-2) 8 13 (face org-level-3)) \"\" \">>\" #(\">>|one|two|three\" 3 6 (face org-level-1) 7 10 (face org-level-2) 11 16 (face org-level-3)) #(\"one/two/..\" 0 3 (face org-level-1) 4 7 (face org-level-2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H") (goto-char (point-min)) (org-get-outline-path))
     (with-temp-buffer (org-mode) (insert "* H\n** S") (goto-char (point-max)) (org-get-outline-path))
     (with-temp-buffer (org-mode) (insert "* H\n** S\nText") (goto-char (point-max)) (org-get-outline-path))
     (with-temp-buffer (org-mode) (insert "* H") (goto-char (point-min)) (org-get-outline-path t))
     (with-temp-buffer (org-mode) (insert "* H\n** ") (goto-char (point-max)) (org-get-outline-path))
     (org-format-outline-path (list "one" "two" "three"))
     (org-format-outline-path '())
     (org-format-outline-path '() nil ">>")
     (org-format-outline-path (list "one" "two" "three") nil ">>" "|")
     (org-format-outline-path (list "one" "two" "three" "four") 10))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export: structured output must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_headline_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1) 1 t) ((1 1) 2 t) ((1 1 1) 3 t) ((1 1 2) 3 t) ((1 2) 2 t) ((2) 1 t) ((2 1) 2 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+OPTIONS: num:t H:3\n* Ch1\n** S1\n*** SS1\n*** SS2\n** S2\n* Ch2\n** S3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (mapcar (lambda (h) (list (org-export-get-headline-number h info)
                            (org-export-get-relative-level h info)
                            (org-export-numbered-headline-p h info)))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

#[test]
fn strong_export_footnote_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1] more[fn:2]\n\n[fn:1] Def 1\n[fn:2] Def 2")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (list
         (mapcar (lambda (ref) (org-export-get-footnote-number ref info))
                 (org-element-map tree 'footnote-reference #'identity))
         (mapcar (lambda (ref) (org-export-footnote-first-reference-p ref info))
                 (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

#[test]
fn strong_export_tags_and_categories() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+CATEGORY: work\n* H1 :tag1:\n** H2 :tag2:\n* H3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (list
         (mapcar (lambda (h) (org-export-get-tags h info))
                 (org-element-map tree 'headline #'identity))
         (mapcar (lambda (h) (org-export-get-category h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

#[test]
fn strong_export_backend_transcoders() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((parent) t ((lambda (h c i) (format \"C: %s\" (org-element-property :raw-value h))) (lambda (s c i) c)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let (org-export-registered-backends)
    (org-export-define-backend 'parent
      '((headline . (lambda (h c i) (format "P: %s" (org-element-property :raw-value h))))
        (section . (lambda (s c i) c))
        (paragraph . (lambda (p c i) c))
        (plain-text . (lambda (t i) t))))
    (org-export-define-derived-backend 'child 'parent
      :translate-alist '((headline . (lambda (h c i) (format "C: %s" (org-element-property :raw-value h))))))
    (list
     (org-export-derived-backend-p 'child 'parent)
     (org-export-derived-backend-p 'child 'child)
     (let ((all (org-export-get-all-transcoders 'child)))
       (list (cdr (assq 'headline all))
             (cdr (assq 'section all)))))))"##,
        expect,
    );
}

#[test]
fn strong_export_filter_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"210\" \"20\" \"0\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   (org-export-filter-apply-functions
    (list (lambda (value &rest _) (concat "1" value))
          (lambda (value &rest _) (concat "2" value)))
    "0" nil)
   (org-export-filter-apply-functions
    (list #'ignore (lambda (value &rest _) (concat "2" value)))
    "0" nil)
   (org-export-filter-apply-functions (list #'ignore) "0" nil)
   (org-export-filter-apply-functions
    (list (lambda (_value &rest _) "")
          (lambda (value &rest _) (concat "2" value)))
    "0" nil)))"##,
        expect,
    );
}

#[test]
fn strong_export_read_attribute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:a \"1\" :b \"2\") nil (:a nil :b nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer (org-mode) (insert "#+ATTR_HTML: :a 1 :b 2\nParagraph")
        (goto-char (point-min)) (org-element-at-point)))
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer (org-mode) (insert "Paragraph")
        (goto-char (point-min)) (org-element-at-point)))
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer (org-mode) (insert "#+ATTR_HTML: :a nil :b nil\nParagraph")
        (goto-char (point-min)) (org-element-at-point))))))"##,
        expect,
    );
}

#[test]
fn strong_export_caption() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"My caption\" 0 10 (:parent (#(\"My caption\" 0 10 (:parent #4)))))) ((#(\"long caption\" 0 12 (:parent (#(\"long caption\" 0 12 (:parent #5)))))) (#(\"short\" 0 5 (:parent (#(\"short\" 0 5 (:parent #5))))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode)
       (insert "#+CAPTION: My caption\n| a | b |")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (table (car (org-element-map tree 'table #'identity))))
         (org-export-get-caption table)))
     (with-temp-buffer (org-mode)
       (insert "#+CAPTION[short]: long caption\n| a | b |")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (table (car (org-element-map tree 'table #'identity))))
         (list (org-export-get-caption table)
               (org-export-get-caption table t)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element API: return values must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_type_api() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (plain-text nil nil dummy dummy nil anonymous anonymous nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (org-element-type "string")
   (org-element-type nil)
   (org-element-type 1)
   (org-element-type '(dummy))
   (org-element-type '(dummy nil 'foo))
   (org-element-type '((dummy)))
   (org-element-type '((dummy)) t)
   (org-element-type '("string") t)
   (org-element-type '(1 2) t)))"##,
        expect,
    );
}

#[test]
fn strong_element_type_p_api() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (foo) (foo bar) nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (org-element-type-p '(foo) 'foo)
   (org-element-type-p '(foo) '(foo))
   (org-element-type-p '(foo) '(foo bar))
   (org-element-type-p '(foo) 'bar)
   (org-element-type-p '(foo) '(bar baz))
   (org-element-type-p "string" 'plain-text)
   (org-element-type-p '((foo)) 'anonymous)))"##,
        expect,
    );
}

#[test]
fn strong_element_class_api() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (element object element object object element element object object object)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (org-element-class '(paragraph nil) nil)
   (org-element-class '(target nil) nil)
   (org-element-class '(org-data nil) nil)
   (org-element-class "text" nil)
   (org-element-class '("secondary " "string") nil)
   (org-element-class '(foo nil) nil)
   (org-element-class '(foo nil) '(center-block nil))
   (org-element-class '(foo nil) '(bold nil))
   (org-element-class '(foo nil) '(paragraph nil))
   (org-element-class '(foo nil) '("secondary"))))"##,
        expect,
    );
}

#[test]
fn strong_element_property_inherited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 (1 2 3) (\"p\") (\"c\") (\"gc\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((gc (org-element-create 'gc '(:shared 3 :own-gc "gc")))
         (c (org-element-create 'c '(:shared 2 :own-c "c") gc))
         (p (org-element-create 'p '(:shared 1 :own-p "p") c)))
    (list
     (org-element-property-inherited :shared gc)
     (org-element-property-inherited :shared gc 'with-self)
     (org-element-property-inherited :shared gc 'with-self 'accumulate)
     (org-element-property-inherited :own-p gc 'with-self 'accumulate)
     (org-element-property-inherited :own-c gc 'with-self 'accumulate)
     (org-element-property-inherited :own-gc gc 'with-self 'accumulate))))"##,
        expect,
    );
}

#[test]
fn strong_element_operations_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((doc (org-element-create 'org-data nil))
         (h1 (org-element-create 'headline '(:level 1 :raw-value "A")
              (org-element-create 'section nil (org-element-create 'paragraph nil "P1.\n"))))
         (h2 (org-element-create 'headline '(:level 1 :raw-value "B")
              (org-element-create 'section nil (org-element-create 'paragraph nil "P2.\n"))))
         (h3 (org-element-create 'headline '(:level 1 :raw-value "C")
              (org-element-create 'section nil (org-element-create 'paragraph nil "P3.\n")))))
    (org-element-adopt doc h1 h2 h3)
    (let ((after-adopt (substring-no-properties (org-element-interpret-data doc))))
      (org-element-extract h2)
      (let ((after-extract (substring-no-properties (org-element-interpret-data doc))))
        (org-element-swap-A-B h1 h3)
        (let ((after-swap (substring-no-properties (org-element-interpret-data doc))))
          (list after-adopt after-extract after-swap
                (org-element-property :parent h2))))))))"##,
        expect,
    );
}

#[test]
fn strong_deferred_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((bar nil) bar (bar bar) (bar [org-element-deferred (closure (t) (_) 'bar) nil nil] bar bar) (1 1) (1 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (let ((el (org-element-create 'd
              `(:deferred ,(org-element-deferred-create t
                            (lambda (el) (org-element-put-property el :foo 'bar) nil))))))
     (list (org-element-property :foo el) (org-element-property :foo2 el)))
   (let ((el (org-element-create 'd `(:foo ,(org-element-deferred-create nil (lambda (_) 'bar))))))
     (org-element-property :foo el))
   (let ((el (org-element-create 'd `(:foo ,(org-element-deferred-create t (lambda (_) 'bar))))))
     (list (org-element-property :foo el) (org-element-property-raw :foo el)))
   (let ((el (org-element-create 'd `(:foo ,(org-element-deferred-create nil (lambda (_) 'bar))))))
     (list (org-element-property :foo el)
           (org-element-property-raw :foo el)
           (org-element-property :foo el nil 'force)
           (org-element-property-raw :foo el)))
   (let ((el (org-element-create 'd `( :foo 1 :bar ,(org-element-deferred-create-alias :foo)))))
     (list (org-element-property :foo el) (org-element-property :bar el)))
   (let ((el (org-element-create 'd `(:foo ,(org-element-deferred-create-list
                              (list 1 2 (org-element-deferred-create nil (lambda (_) 3))))))))
     (org-element-property :foo el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-map-entries: concrete results must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_map_entries_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 11) (1) (6) (11) (1) (1) (23) (1 12))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Full match.
     (with-temp-buffer (org-mode) (insert "* Level 1\n** Level 2")
       (goto-char (point-min)) (org-map-entries #'point))
     ;; Level match.
     (with-temp-buffer (org-mode) (insert "* Level 1\n** Level 2")
       (goto-char (point-min)) (let (org-odd-levels-only) (org-map-entries #'point "LEVEL=1")))
     ;; TODO match.
     (with-temp-buffer (org-mode) (insert "* H1\n* TODO H2\n* DONE H3")
       (goto-char (point-min)) (org-map-entries #'point "TODO=\"TODO\""))
     ;; Tag match.
     (with-temp-buffer (org-mode) (insert "* H1 :no:\n* H2 :yes:")
       (goto-char (point-min)) (org-map-entries #'point "yes"))
     ;; Priority match.
     (with-temp-buffer (org-mode) (insert "* [#A] H1\n* [#B] H2")
       (goto-char (point-min)) (org-map-entries #'point "PRIORITY=\"A\""))
     ;; Property match.
     (with-temp-buffer (org-mode)
       (insert "* H1\n:PROPERTIES:\n:TEST: 1\n:END:\n* H2\n:PROPERTIES:\n:TEST: 2\n:END:")
       (goto-char (point-min)) (org-map-entries #'point "TEST=1"))
     ;; Multiple criteria.
     (with-temp-buffer (org-mode) (insert "* H1 :no:\n** H2 :yes:\n* H3 :yes:")
       (goto-char (point-min))
       (let (org-odd-levels-only (org-use-tag-inheritance nil))
         (org-map-entries #'point "yes+LEVEL=1")))
     ;; Or criteria.
     (with-temp-buffer (org-mode) (insert "* H1 :yes:\n* H2 :no:\n* H3 :maybe:")
       (goto-char (point-min))
       (let (org-odd-levels-only) (org-map-entries #'point "yes|no"))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Entry-blocked-p: concrete results must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_entry_blocked_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-enforce-todo-dependencies t)
        (org-blocker-hook '(org-block-todo-from-children-or-siblings-or-parent)))
    (list
     (with-temp-buffer (org-mode) (insert "* TODO Blocked\n** DONE one\n** TODO two")
       (goto-char (point-min)) (org-entry-blocked-p))
     (with-temp-buffer (org-mode) (insert "* TODO Blocked\n** DONE one\n** DONE two")
       (goto-char (point-min)) (org-entry-blocked-p))
     (with-temp-buffer (org-mode) (insert "* Blocked\n** TODO one")
       (goto-char (point-min)) (org-entry-blocked-p))
     (with-temp-buffer (org-mode) (insert "* DONE Blocked\n** TODO one")
       (goto-char (point-min)) (org-entry-blocked-p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Pcomplete: buffer content after completion must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_pcomplete_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-pcomplete)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "\\alp")
       (goto-char (point-max)) (pcomplete) (buffer-string))
     (with-temp-buffer (org-mode) (insert "\\frac1")
       (goto-char (point-max)) (pcomplete) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Sparse tree: visible headlines must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sparse_tree_visible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"B\" \"C\" \"D\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO A\n* DONE B\n* TODO C\n* DONE D")
      (goto-char (point-min))
      (org-match-sparse-tree nil "TODO")
      (let ((visible nil))
        (org-element-map (org-element-parse-buffer) 'headline
          (lambda (h) (let ((title (org-element-property :raw-value h)))
                   (when (org-element-property :begin h) (push title visible)))))
        (nreverse visible)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Clock table data: return value must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clock_table_data() {
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
      (car (org-clock-get-table-data (current-buffer) '(:maxlevel 2))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Refile targets: return value must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_refile_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"B\" \"C\" \"D\" \"E\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let ((org-mode-hook nil)
        (org-refile-targets '((nil :maxlevel . 3))))
    (with-temp-buffer (org-mode)
      (insert "* A\n** B\n*** C\n* D\n** E")
      (goto-char (point-min))
      (mapcar (lambda (r) (car r)) (org-refile-get-targets)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Num mode: overlay after-string must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_num_mode_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"1 \" 0 2 (face org-level-1)) #(\"1.1 \" 0 4 (face org-level-2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-num)
  (let ((org-mode-hook nil) (org-num-max-level 2))
    (with-temp-buffer (org-mode) (insert "* H1\n** H2\n*** H3")
      (goto-char (point-min))
      (org-num-mode 1)
      (sort (mapcar (lambda (o) (overlay-get o 'after-string))
                    (overlays-in (point-min) (point-max)))
            #'string-lessp))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Category: return value must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_get_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Work\" \"???\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "#+CATEGORY: Work\n* H")
       (goto-char (point-min)) (org-get-category))
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-get-category)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Repeat/timestamp: return values must match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_get_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"+1w\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2023-10-13 Fri +1w>")
       (goto-char (point-min)) (forward-line 1) (org-get-repeat))
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2023-10-13 Fri>")
       (goto-char (point-min)) (forward-line 1) (org-get-repeat)))))"##,
        expect,
    );
}

#[test]
fn strong_timestamp_has_time_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "<2023-10-13 Fri 14:30>")
       (goto-char (point-min)) (org-at-timestamp-p 'lax) (org-timestamp-has-time-p))
     (with-temp-buffer (org-mode) (insert "<2023-10-13 Fri>")
       (goto-char (point-min)) (org-at-timestamp-p 'lax) (org-timestamp-has-time-p)))))"##,
        expect,
    );
}
