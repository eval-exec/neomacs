//! Yotta-strict combo tests for org-mode extreme edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-fill-element combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_fill_element_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (#(\"| a |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table-row)) \"some \\\\\\\\\\nlong text\" \"A B\" \"- A B\" \"  # A B\" \"#+BEGIN_COMMENT\\nSome text\\n#+END_COMMENT\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Table alignment.
     (with-temp-buffer (org-mode) (insert "|a|")
       (goto-char (point-min)) (org-fill-element) (buffer-string))
     ;; Paragraph with line break.
     (with-temp-buffer (org-mode) (insert "some \\\\\nlong\ntext")
       (goto-char (point-min)) (let ((fill-column 20)) (org-fill-element)) (buffer-string))
     ;; Fill at end of paragraph.
     (with-temp-buffer (org-mode) (insert "A\nB")
       (goto-char (point-max)) (let ((fill-column 20)) (org-fill-element)) (buffer-string))
     ;; Item fill.
     (with-temp-buffer (org-mode) (insert "- A\n  B")
       (goto-char (point-min)) (let ((fill-column 20)) (org-fill-element)) (buffer-string))
     ;; Comment fill.
     (with-temp-buffer (org-mode) (insert "  # A\n  # B")
       (goto-char (point-min)) (let ((fill-column 20)) (org-fill-element)) (buffer-string))
     ;; Comment block fill.
     (with-temp-buffer (org-mode) (insert "#+BEGIN_COMMENT\nSome\ntext\n#+END_COMMENT")
       (goto-char (point-min)) (forward-line) (let ((fill-column 20)) (org-fill-element)) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-indent-line combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_indent_line_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (0 0 2 0 t \"* H\\n:PROPERTIES:\\n:key:      value\\n:END:\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; No indent for headline.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-indent-line) (org-get-indentation))
     ;; No indent before first headline.
     (with-temp-buffer (org-mode) (insert "")
       (goto-char (point-min)) (org-indent-line) (org-get-indentation))
     ;; Indent according to level.
     (with-temp-buffer (org-mode) (insert "* H\nA")
       (goto-char (point-max)) (let ((org-adapt-indentation t)) (org-indent-line)) (org-get-indentation))
     ;; No indent when org-adapt-indentation is nil.
     (with-temp-buffer (org-mode) (insert "* H\nA")
       (goto-char (point-max)) (let ((org-adapt-indentation nil)) (org-indent-line)) (org-get-indentation))
     ;; Preserve point position.
     (with-temp-buffer (org-mode) (insert "* H\nAB")
       (goto-char (point-min)) (forward-line) (forward-char)
       (let ((org-adapt-indentation t)) (org-indent-line)) (looking-at "B"))
     ;; Property alignment.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:key: value\n:END:")
       (goto-char (point-min)) (forward-line 2)
       (let ((org-property-format "%-10s %s")) (org-indent-line)) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-return combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_return_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Para\\n graph\" \"  Para\\n  graph\" t \"* H :tag:\\n\" \"* TODO H :tag:\\n\" \"\\n* h\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Regular.
     (with-temp-buffer (org-mode) (insert "Para graph")
       (goto-char (+ 4 (point-min))) (org-return) (buffer-string))
     ;; With indent.
     (with-temp-buffer (org-mode) (insert "  Para graph")
       (goto-char (+ 6 (point-min))) (org-return t) (buffer-string))
     ;; On table.
     (with-temp-buffer (org-mode) (insert "| a |\n| b |")
       (goto-char (point-min)) (forward-char 2) (org-return) (looking-at "b"))
     ;; On tags: add newline below.
     (with-temp-buffer (org-mode) (insert "* H :tag:")
       (goto-char (point-min)) (search-forward ":tag") (org-return) (buffer-string))
     ;; Before headline text.
     (with-temp-buffer (org-mode) (insert "* TODO H :tag:")
       (goto-char (point-min)) (forward-char 2) (org-return) (buffer-string))
     ;; At bol of headline.
     (with-temp-buffer (org-mode) (insert "* h")
       (goto-char (point-min)) (org-return) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-meta-return combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_meta_return_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* a\" \"- \\n- a\" #(\"|   |\\n| a |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table-row) 6 7 (face org-table) 7 8 (face org-table rear-nonsticky t display (space :relative-width 1)) 8 9 (face org-table) 9 10 (face org-table display (space :relative-width 1.001)) 10 11 (face org-table) 11 12 (face org-table-row)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; In paragraph: turn into header.
     (with-temp-buffer (org-mode) (insert "a")
       (goto-char (point-min)) (org-meta-return) (buffer-string))
     ;; In item: insert item above.
     (with-temp-buffer (org-mode) (insert "- a")
       (goto-char (point-min)) (org-meta-return) (buffer-string))
     ;; In table: insert row above.
     (with-temp-buffer (org-mode) (insert "| a |")
       (goto-char (point-min)) (forward-char 2) (org-meta-return) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-entry-blocked-p combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_entry_blocked_p_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-enforce-todo-dependencies t)
        (org-blocker-hook '(org-block-todo-from-children-or-siblings-or-parent)))
    (list
     ;; Blocked: children not all DONE.
     (with-temp-buffer (org-mode) (insert "* TODO Blocked\n** DONE one\n** TODO two")
       (goto-char (point-min)) (org-entry-blocked-p))
     ;; Not blocked: all children DONE.
     (with-temp-buffer (org-mode) (insert "* TODO Blocked\n** DONE one\n** DONE two")
       (goto-char (point-min)) (org-entry-blocked-p))
     ;; Not blocked: no TODO keyword.
     (with-temp-buffer (org-mode) (insert "* Blocked\n** TODO one")
       (goto-char (point-min)) (org-entry-blocked-p))
     ;; Not blocked: DONE keyword.
     (with-temp-buffer (org-mode) (insert "* DONE Blocked\n** TODO one")
       (goto-char (point-min)) (org-entry-blocked-p))
     ;; Ordered: blocked.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:ORDERED: t\n:END:\n** TODO one\n** TODO two")
       (goto-char (point-min)) (org-entry-blocked-p))
     ;; Ordered: not blocked.
     (with-temp-buffer (org-mode)
       (insert "* H\n:PROPERTIES:\n:ORDERED: t\n:END:\n** TODO one\n** DONE two")
       (goto-char (point-min)) (org-entry-blocked-p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-find-olp combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_find_olp_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer>)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\n* Headline\n** COMMENT headline2\n** TODO headline3\n*** [#A] headline4 :tags:\n** [#A]headline5\n** [0%] headline6\n** headline7 [100%]\n** headline8 [1/5] :some:more:tags:\n* Test")
      (goto-char (point-min))
      (list
       (org-find-olp '("Headline") t)
       (org-find-olp '("Headline" "headline2") t)
       (org-find-olp '("Headline" "headline3") t)
       (org-find-olp '("Headline" "headline3" "headline4") t)
       (org-find-olp '("Headline" "headline6") t)
       (org-find-olp '("Headline" "headline7") t)
       (org-find-olp '("Headline" "headline8") t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-map-entries combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_map_entries_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 11) (1) (6) (11) (1) (1) (23) (1 12) (22))""#]];
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
     ;; Multiple criteria (and).
     (with-temp-buffer (org-mode) (insert "* H1 :no:\n** H2 :yes:\n* H3 :yes:")
       (goto-char (point-min))
       (let (org-odd-levels-only (org-use-tag-inheritance nil))
         (org-map-entries #'point "yes+LEVEL=1")))
     ;; Or criteria.
     (with-temp-buffer (org-mode) (insert "* H1 :yes:\n* H2 :no:\n* H3 :maybe:")
       (goto-char (point-min)) (let (org-odd-levels-only) (org-map-entries #'point "yes|no")))
     ;; And criteria.
     (with-temp-buffer (org-mode) (insert "* H1 :yes:\n* H2 :no:\n* H3 :yes:no:")
       (goto-char (point-min)) (let (org-odd-levels-only) (org-map-entries #'point "yes&no"))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-edit-headline combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_edit_headline_combinations() {
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
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     ;; Empty heading.
     (with-temp-buffer (org-mode) (insert "* A")
       (goto-char (point-min)) (org-edit-headline "") (buffer-string))
     ;; From empty.
     (with-temp-buffer (org-mode) (insert "* ")
       (goto-char (point-min)) (org-edit-headline "A") (buffer-string))
     ;; With TODO.
     (with-temp-buffer (org-mode) (insert "* TODO A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     ;; With priority.
     (with-temp-buffer (org-mode) (insert "* [#A] A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     ;; With TODO and priority.
     (with-temp-buffer (org-mode) (insert "* TODO [#A] A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     ;; With tags.
     (with-temp-buffer (org-mode) (insert "* A :tag:")
       (goto-char (point-min)) (let ((org-tags-column 4)) (org-edit-headline "B")) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-insert-heading combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_insert_heading_combinations() {
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
     (with-temp-buffer (org-mode) (org-insert-heading) (buffer-string))
     ;; At beginning of line.
     (with-temp-buffer (org-mode) (insert "P")
       (goto-char (point-min)) (org-insert-heading) (buffer-string))
     ;; At beginning of headline: create above.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-insert-heading) (buffer-string))
     ;; New headline level depends on above.
     (with-temp-buffer (org-mode) (insert "** H\nP")
       (goto-char (point-max)) (org-insert-heading) (buffer-string))
     ;; With blank-before-new-entry.
     (with-temp-buffer (org-mode) (insert "* H1")
       (goto-char (point-min))
       (let ((org-blank-before-new-entry '((heading . t)))) (org-insert-heading)) (buffer-string))
     ;; Corner case: empty headline.
     (with-temp-buffer (org-mode) (insert "* ")
       (goto-char (point-min)) (org-insert-heading) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-kill-line combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_kill_line_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"ab\" \"\\n123\" \"* A :tag:\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; At beginning: kill whole line.
     (with-temp-buffer (org-mode) (insert "abc")
       (goto-char (point-min)) (org-kill-line) (buffer-string))
     ;; In middle: kill until end.
     (with-temp-buffer (org-mode) (insert "abc")
       (goto-char (+ 2 (point-min))) (org-kill-line) (buffer-string))
     ;; Do not kill newline.
     (with-temp-buffer (org-mode) (insert "abc\n123")
       (goto-char (point-min)) (org-kill-line) (buffer-string))
     ;; Special ctrl-k on headline.
     (with-temp-buffer (org-mode) (insert "* AB :tag:")
       (goto-char (point-min)) (forward-char 3)
       (let ((org-special-ctrl-k t) (org-tags-column 0)) (org-kill-line)) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-sort-entries combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_sort_entries_combinations() {
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
     (with-temp-buffer (org-mode) (insert "\n* def\n* xyz\n* abc\n")
       (goto-char (point-min)) (org-sort-entries nil ?a) (buffer-string))
     ;; Sort alphabetically descending.
     (with-temp-buffer (org-mode) (insert "\n* def\n* xyz\n* abc\n")
       (goto-char (point-min)) (org-sort-entries nil ?A) (buffer-string))
     ;; Sort numerically.
     (with-temp-buffer (org-mode) (insert "\n* 10\n* 1\n* 2\n")
       (goto-char (point-min)) (org-sort-entries nil ?n) (buffer-string))
     ;; Sort by priority.
     (with-temp-buffer (org-mode) (insert "\n* [#C] h1\n* [#A] h2\n* [#B] h3\n")
       (goto-char (point-min)) (org-sort-entries nil ?p) (buffer-string))
     ;; Sort by priority descending.
     (with-temp-buffer (org-mode) (insert "\n* [#C] h1\n* [#A] h2\n* [#B] h3\n")
       (goto-char (point-min)) (org-sort-entries nil ?P) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-mark-element combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_mark_element_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((t t) (t t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Mark paragraph.
     (with-temp-buffer (org-mode) (insert "Paragraph")
       (goto-char (point-min)) (org-mark-element) (list (bobp) (= (mark) (point-max))))
     ;; Mark in middle of two paragraphs.
     (with-temp-buffer (org-mode) (insert "P1\n\nParagraph\n\nP2")
       (goto-char (point-min)) (forward-line 2) (org-mark-element)
       (list (looking-at "Paragraph") (org-with-point-at (mark) (looking-at "P2")))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-mark-subtree combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_mark_subtree_combinations() {
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
       (goto-char (point-min)) (forward-line 2) (org-mark-subtree)
       (list (region-beginning) (region-end)))
     ;; With argument: move up.
     (with-temp-buffer (org-mode)
       (insert "* Headline\n** Sub-headline\nBody")
       (goto-char (point-min)) (forward-line 2) (org-mark-subtree 1)
       (list (region-beginning) (region-end))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-collect-keywords combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_collect_keywords_combinations() {
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
       (goto-char (point-min)) (org-collect-keywords '("TITLE" "AUTHOR")))
     ;; Inside example block: not collected.
     (with-temp-buffer (org-mode)
       (insert "#+begin_example\n#+foo: bar\n#+end_example")
       (goto-char (point-min)) (org-collect-keywords '("FOO"))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-shiftright-heading combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_shiftright_heading_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"* TODO a1\\n** a2\\n* DONE b1\\n\" 0 9 (org-todo-head \"TODO\")) #(\"* TODO a1\\n** a2\\n* b1\\n\" 0 9 (org-todo-head \"TODO\") 16 20 (org-todo-head nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-todo-keywords '((sequence "TODO" "DONE"))))
    (list
     ;; Shift to TODO.
     (with-temp-buffer (org-mode) (insert "* a1\n** a2\n* DONE b1\n")
       (goto-char (point-min)) (org-shiftright) (buffer-string))
     ;; Shift with region.
     (with-temp-buffer (org-mode) (insert "* a1\n** a2\n* DONE b1\n")
       (goto-char (point-min))
       (let ((org-loop-over-headlines-in-active-region 'start-level))
         (transient-mark-mode 1) (push-mark (point) t t)
         (search-forward "* DONE b1") (org-shiftright))
       (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-beginning-of-line combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_beginning_of_line_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Standard.
     (with-temp-buffer (org-mode) (insert "Some text\nSome other text")
       (goto-char (point-max)) (org-beginning-of-line) (bolp))
     ;; At headline with special movement.
     (with-temp-buffer (org-mode) (insert "* TODO Headline")
       (goto-char (point-max))
       (let ((org-special-ctrl-a/e t))
         (list (progn (org-beginning-of-line) (looking-at-p "Headline"))
               (progn (org-beginning-of-line) (bolp))
               (progn (org-beginning-of-line) (looking-at-p "Headline")))))
     ;; At item with special movement.
     (with-temp-buffer (org-mode) (insert "- [ ] Item")
       (goto-char (point-max))
       (let ((org-special-ctrl-a/e t))
         (list (progn (org-beginning-of-line) (looking-at-p "Item"))
               (progn (org-beginning-of-line) (bolp)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-end-of-line combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_end_of_line_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Standard.
     (with-temp-buffer (org-mode) (insert "Some text\nSome other text")
       (goto-char (point-min)) (org-end-of-line) (eolp))
     ;; At headline with special movement.
     (with-temp-buffer (org-mode) (insert "* TODO Headline :tag:")
       (goto-char (point-min))
       (let ((org-special-ctrl-a/e t))
         (list (progn (org-end-of-line) (looking-back "Headline" nil))
               (progn (org-end-of-line) (eolp)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-at-property-p combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_at_property_p_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:PROP: t\n:END:")
       (goto-char (point-min)) (forward-line 2) (org-at-property-p))
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:PROP: t\n:END:")
       (goto-char (point-min)) (forward-line 1) (org-at-property-p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-at-property-drawer-p combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_at_property_drawer_p_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; On PROPERTIES line.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:PROP: t\n:END:")
       (goto-char (point-min)) (forward-line 1) (org-at-property-drawer-p))
     ;; Not inside drawer.
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:PROP: t\n:END:")
       (goto-char (point-min)) (forward-line 1) (org-at-property-drawer-p))
     ;; Incomplete drawer.
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:PROP: t")
       (goto-char (point-min)) (forward-line 1) (org-at-property-drawer-p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-get-property-block combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_get_property_block_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((14 . 14) (14 . 23) \"* H\\n:PROPERTIES:\\n:END:\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Empty drawer.
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:END:")
       (goto-char (point-min)) (org-get-property-block))
     ;; With content.
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:KEY: V:\n:END:")
       (goto-char (point-min)) (org-get-property-block))
     ;; Force create.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (let ((org-adapt-indentation nil)) (org-get-property-block nil 'force))
       (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Yotta: org-element with all org-insert-property-drawer combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn yotta_all_insert_property_drawer_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\":PROPERTIES:\\n:END:\\n\" \"* H\\n:PROPERTIES:\\n:END:\\nParagraph\" \"* H\\nDEADLINE: <2014-03-04 tue.>\\n:PROPERTIES:\\n:END:\\nParagraph\" \"* H\\n  :PROPERTIES:\\n  :END:\\nParagraph\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Empty buffer.
     (with-temp-buffer (org-mode)
       (let ((org-adapt-indentation nil)) (org-insert-property-drawer)) (buffer-string))
     ;; After headline.
     (with-temp-buffer (org-mode) (insert "* H\nParagraph")
       (goto-char (point-min))
       (let ((org-adapt-indentation nil)) (org-insert-property-drawer)) (buffer-string))
     ;; After planning.
     (with-temp-buffer (org-mode)
       (insert "* H\nDEADLINE: <2014-03-04 tue.>\nParagraph")
       (goto-char (point-min))
       (let ((org-adapt-indentation nil)) (org-insert-property-drawer)) (buffer-string))
     ;; With indentation.
     (with-temp-buffer (org-mode) (insert "* H\nParagraph")
       (goto-char (point-min))
       (let ((org-adapt-indentation t)) (org-insert-property-drawer)) (buffer-string)))))"##,
        expect,
    );
}
