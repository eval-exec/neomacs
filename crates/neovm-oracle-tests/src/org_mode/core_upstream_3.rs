//! Ported upstream ERT tests from org-mode's test-org.el (9.7.11) - batch 3.
//!
//! Covers: property, fill, indent, return, meta-return, entry-blocked,
//! find-olp, map-entries, coderef, custom-id, fuzzy-links,
//! beginning/end-of-line, shiftright.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ── Property: at-property-p ──────────────────────────────────────────

#[test]
fn upstream_org_at_property_p() {
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

// ── Property: at-property-drawer-p ───────────────────────────────────

#[test]
fn upstream_org_at_property_drawer_p() {
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

// ── Property: get-property-block ─────────────────────────────────────

#[test]
fn upstream_org_get_property_block() {
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

// ── Property: insert-property-drawer ─────────────────────────────────

#[test]
fn upstream_org_insert_property_drawer() {
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
       (let ((org-adapt-indentation nil)) (org-insert-property-drawer))
       (buffer-string))
     ;; After headline.
     (with-temp-buffer (org-mode) (insert "* H\nParagraph")
       (goto-char (point-min))
       (let ((org-adapt-indentation nil)) (org-insert-property-drawer))
       (buffer-string))
     ;; After planning.
     (with-temp-buffer (org-mode)
       (insert "* H\nDEADLINE: <2014-03-04 tue.>\nParagraph")
       (goto-char (point-min))
       (let ((org-adapt-indentation nil)) (org-insert-property-drawer))
       (buffer-string))
     ;; With indentation.
     (with-temp-buffer (org-mode) (insert "* H\nParagraph")
       (goto-char (point-min))
       (let ((org-adapt-indentation t)) (org-insert-property-drawer))
       (buffer-string)))))"##,
        expect,
    );
}

// ── Fill: org-fill-element ───────────────────────────────────────────

#[test]
fn upstream_org_fill_element() {
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
       (goto-char (point-min))
       (let ((fill-column 20)) (org-fill-element)) (buffer-string))
     ;; Fill at end of paragraph.
     (with-temp-buffer (org-mode) (insert "A\nB")
       (goto-char (point-max))
       (let ((fill-column 20)) (org-fill-element)) (buffer-string))
     ;; Item fill.
     (with-temp-buffer (org-mode) (insert "- A\n  B")
       (goto-char (point-min))
       (let ((fill-column 20)) (org-fill-element)) (buffer-string))
     ;; Comment fill.
     (with-temp-buffer (org-mode) (insert "  # A\n  # B")
       (goto-char (point-min))
       (let ((fill-column 20)) (org-fill-element)) (buffer-string))
     ;; Comment block fill.
     (with-temp-buffer (org-mode) (insert "#+BEGIN_COMMENT\nSome\ntext\n#+END_COMMENT")
       (goto-char (point-min)) (forward-line)
       (let ((fill-column 20)) (org-fill-element)) (buffer-string)))))"##,
        expect,
    );
}

// ── Indent: org-indent-line ──────────────────────────────────────────

#[test]
fn upstream_org_indent_line() {
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
       (goto-char (point-max))
       (let ((org-adapt-indentation t)) (org-indent-line)) (org-get-indentation))
     ;; No indent when org-adapt-indentation is nil.
     (with-temp-buffer (org-mode) (insert "* H\nA")
       (goto-char (point-max))
       (let ((org-adapt-indentation nil)) (org-indent-line)) (org-get-indentation))
     ;; Preserve point position.
     (with-temp-buffer (org-mode) (insert "* H\nAB")
       (goto-char (point-min)) (forward-line) (forward-char)
       (let ((org-adapt-indentation t)) (org-indent-line))
       (looking-at "B"))
     ;; Property alignment.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:key: value\n:END:")
       (goto-char (point-min)) (forward-line 2)
       (let ((org-property-format "%-10s %s")) (org-indent-line))
       (buffer-string)))))"##,
        expect,
    );
}

// ── Return: org-return ───────────────────────────────────────────────

#[test]
fn upstream_org_return() {
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
       (goto-char (point-min)) (forward-char 2) (org-return)
       (looking-at "b"))
     ;; On tags: add newline below.
     (with-temp-buffer (org-mode) (insert "* H :tag:")
       (goto-char (point-min)) (search-forward ":tag") (org-return)
       (buffer-string))
     ;; Before headline text.
     (with-temp-buffer (org-mode) (insert "* TODO H :tag:")
       (goto-char (point-min)) (forward-char 2) (org-return)
       (buffer-string))
     ;; At bol of headline.
     (with-temp-buffer (org-mode) (insert "* h")
       (goto-char (point-min)) (org-return) (buffer-string)))))"##,
        expect,
    );
}

// ── Meta-return: org-meta-return ─────────────────────────────────────

#[test]
fn upstream_org_meta_return() {
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
       (goto-char (point-min))
       (org-meta-return) (buffer-string))
     ;; In item: insert item above.
     (with-temp-buffer (org-mode) (insert "- a")
       (goto-char (point-min))
       (org-meta-return) (buffer-string))
     ;; In table: insert row above.
     (with-temp-buffer (org-mode) (insert "| a |")
       (goto-char (point-min)) (forward-char 2)
       (org-meta-return) (buffer-string)))))"##,
        expect,
    );
}

// ── Entry-blocked-p ──────────────────────────────────────────────────

#[test]
fn upstream_org_entry_blocked_p() {
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

// ── Find-olp ─────────────────────────────────────────────────────────

#[test]
fn upstream_org_find_olp() {
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

// ── Map-entries: basic ───────────────────────────────────────────────

#[test]
fn upstream_org_map_entries_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 11) (1) (6) (11) (1) (1))""#]];
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
       (goto-char (point-min))
       (let (org-odd-levels-only) (org-map-entries #'point "LEVEL=1")))
     ;; TODO match.
     (with-temp-buffer (org-mode) (insert "* H1\n* TODO H2\n* DONE H3")
       (goto-char (point-min))
       (org-map-entries #'point "TODO=\"TODO\""))
     ;; Tag match.
     (with-temp-buffer (org-mode) (insert "* H1 :no:\n* H2 :yes:")
       (goto-char (point-min))
       (org-map-entries #'point "yes"))
     ;; Priority match.
     (with-temp-buffer (org-mode) (insert "* [#A] H1\n* [#B] H2")
       (goto-char (point-min))
       (org-map-entries #'point "PRIORITY=\"A\""))
     ;; Property match.
     (with-temp-buffer (org-mode)
       (insert "* H1\n:PROPERTIES:\n:TEST: 1\n:END:\n* H2\n:PROPERTIES:\n:TEST: 2\n:END:")
       (goto-char (point-min))
       (org-map-entries #'point "TEST=1")))))"##,
        expect,
    );
}

// ── Map-entries: compound ────────────────────────────────────────────

#[test]
fn upstream_org_map_entries_compound() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((23) (1 12) (22))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Multiple criteria (and).
     (with-temp-buffer (org-mode) (insert "* H1 :no:\n** H2 :yes:\n* H3 :yes:")
       (goto-char (point-min))
       (let (org-odd-levels-only (org-use-tag-inheritance nil))
         (org-map-entries #'point "yes+LEVEL=1")))
     ;; Or criteria.
     (with-temp-buffer (org-mode) (insert "* H1 :yes:\n* H2 :no:\n* H3 :maybe:")
       (goto-char (point-min))
       (let (org-odd-levels-only)
         (org-map-entries #'point "yes|no")))
     ;; And criteria.
     (with-temp-buffer (org-mode) (insert "* H1 :yes:\n* H2 :no:\n* H3 :yes:no:")
       (goto-char (point-min))
       (let (org-odd-levels-only)
         (org-map-entries #'point "yes&no"))))))"##,
        expect,
    );
}

// ── Map-entries: property negative ───────────────────────────────────

#[test]
fn upstream_org_map_entries_property_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((34 67) (11))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Negative property match.
     (with-temp-buffer (org-mode)
       (insert "* H1\n:PROPERTIES:\n:TEST: 1\n:END:\n* H2\n:PROPERTIES:\n:TEST: 2\n:END:\n* H3")
       (goto-char (point-min))
       (org-map-entries #'point "TEST!=1"))
     ;; Negative priority match.
     (with-temp-buffer (org-mode) (insert "* [#A] H1\n* [#B] H2")
       (goto-char (point-min))
       (org-map-entries #'point "PRIORITY/=\"A\"")))))"##,
        expect,
    );
}

// ── Coderef ──────────────────────────────────────────────────────────

#[test]
fn upstream_org_coderef() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Standard coderef.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 1)                  (ref:sc)\n#+END_SRC\n[[(sc)]]")
       (goto-char (point-min))
       (search-forward "[[(sc")
       (org-open-at-point)
       (looking-at "(ref:sc)"))
     ;; Alternate label format.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_SRC emacs-lisp -l \"{ref:%s}\"\n(+ 1 1)                  {ref:sc}\n#+END_SRC\n[[(sc)]]")
       (goto-char (point-min))
       (search-forward "[[(sc")
       (org-open-at-point)
       (looking-at "{ref:sc}")))))"##,
        expect,
    );
}

// ── Custom-id ────────────────────────────────────────────────────────

#[test]
fn upstream_org_custom_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n:PROPERTIES:\n:CUSTOM_ID: custom\n:END:\n* H2\n[[#custom]]")
      (goto-char (point-min))
      (search-forward "[[#custom")
      (org-open-at-point)
      (looking-at-p "\\* H1"))))"##,
        expect,
    );
}

// ── Fuzzy-links ──────────────────────────────────────────────────────

#[test]
fn upstream_org_fuzzy_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"[[*Head2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-link-search-must-match-exact-headline nil))
    (list
     ;; Target match.
     (with-temp-buffer (org-mode)
       (insert "* Head1\n* Head2\n* Head3")
       (goto-char (point-min))
       (search-forward "Head2")
       (beginning-of-line)
       (insert "* Head2\nFoo Bar\n")
       (goto-char (point-min))
       (search-forward "[[*Head2")
       (org-open-at-point)
       (looking-at "\\* Head2"))
     ;; Leading star enforces heading match.
     (with-temp-buffer (org-mode)
       (insert "* Test\n<<Test>>\n[[*Test]]")
       (goto-char (point-min))
       (search-forward "[[*Test")
       (org-open-at-point)
       (looking-at "\\* Test")))))"##,
        expect,
    );
}

// ── Beginning-of-line ────────────────────────────────────────────────

#[test]
fn upstream_org_beginning_of_line() {
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

// ── End-of-line ──────────────────────────────────────────────────────

#[test]
fn upstream_org_end_of_line() {
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

// ── Shiftright-heading ───────────────────────────────────────────────

#[test]
fn upstream_org_shiftright_heading() {
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
       (goto-char (point-min))
       (org-shiftright) (buffer-string))
     ;; Shift with region.
     (with-temp-buffer (org-mode) (insert "* a1\n** a2\n* DONE b1\n")
       (goto-char (point-min))
       (let ((org-loop-over-headlines-in-active-region 'start-level))
         (transient-mark-mode 1)
         (push-mark (point) t t)
         (search-forward "* DONE b1")
         (org-shiftright))
       (buffer-string)))))"##,
        expect,
    );
}

// ── Combo: headline + properties + planning ──────────────────────────

#[test]
fn upstream_org_combo_headline_properties_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (6 \"TODO\" 65 (\"work\") \"<2024-01-15 Mon>\" (1 2 2 3 3 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] Project :work:\nDEADLINE: <2024-01-15 Mon>\n:PROPERTIES:\n:CUSTOM_ID: proj1\n:EFFORT: 2h\n:END:\n** DONE Design\n** TODO Implementation\n*** TODO Backend\n*** TODO Frontend\n** TODO Testing")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         ;; Number of headlines.
         (length headlines)
         ;; Top-level properties.
         (org-element-property :todo-keyword (nth 0 headlines))
         (org-element-property :priority (nth 0 headlines))
         (org-element-property :tags (nth 0 headlines))
         ;; Planning.
         (let ((planning (org-element-map tree 'planning #'identity nil t)))
           (org-element-property :raw-value (org-element-property :deadline planning)))
         ;; Hierarchy.
         (mapcar (lambda (h) (org-element-property :level h)) headlines))))))"##,
        expect,
    );
}

// ── Combo: table + formulas + export ─────────────────────────────────

#[test]
fn upstream_org_combo_table_formula_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| Item | Qty | Price | Total |\n|------|-----|-------|-------|\n| A    | 3   | 10    |       |\n| B    | 2   | 15    |       |\n|------|-----|-------|-------|\n|      |     |       |       |\n#+TBLFM: $4=$2*$3\n#+TBLFM: @>$4=vsum(@I..@-1)")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (table (org-element-map tree 'table #'identity nil t)))
        (list
         ;; Table type.
         (org-element-property :type table)
         ;; Number of rows.
         (length (org-element-map tree 'table-row #'identity))
         ;; Number of cells in first data row.
         (length (org-element-map
                 (nth 1 (org-element-map tree 'table-row #'identity))
                 'table-cell #'identity))
         ;; Has TBLFM.
         (org-element-map tree 'keyword
           (lambda (k) (when (equal (org-element-property :key k) "TBLFM")
                     (org-element-property :value k))))))))"##,
        expect,
    );
}

// ── Combo: links + citations + footnotes ─────────────────────────────

#[test]
fn upstream_org_combo_links_citations_footnotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "See [[https://orgmode.org][Org mode]] and [cite:@key1;@key2].\n\nAlso [fn:1] and [fn:2:inline footnote].\n\n[fn:1] Definition with *bold*.\n[fn:2] Not used.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Links.
         (length (org-element-map tree 'link #'identity))
         ;; Citations.
         (length (org-element-map tree 'citation #'identity))
         ;; Citation references.
         (length (org-element-map tree 'citation-reference #'identity))
         ;; Footnote references.
         (length (org-element-map tree 'footnote-reference #'identity))
         ;; Footnote definitions.
         (length (org-element-map tree 'footnote-definition #'identity))
         ;; Link type.
         (org-element-property :type
           (org-element-map tree 'link #'identity nil t))
         ;; Link path.
         (org-element-property :path
           (org-element-map tree 'link #'identity nil t))))))"##,
        expect,
    );
}

// ── Combo: blocks + drawers + properties ─────────────────────────────

#[test]
fn upstream_org_combo_blocks_drawers_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H\n:PROPERTIES:\n:KEY: val\n:END:\n:LOGBOOK:\nCLOCK: [2023-10-13 Fri 10:00]--[2023-10-13 Fri 11:00] =>  1:00\n:END:\n#+BEGIN_QUOTE\nQuoted text\n#+END_QUOTE\n#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\nBody paragraph.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Element types present.
         (delete-dups (mapcar #'org-element-type (org-element-map tree t #'identity)))
         ;; Property drawer.
         (length (org-element-map tree 'property-drawer #'identity))
         ;; Drawers.
         (length (org-element-map tree 'drawer #'identity))
         ;; Blocks.
         (length (org-element-map tree '(quote-block src-block) #'identity))
         ;; Keywords.
         (length (org-element-map tree 'keyword #'identity))
         ;; Paragraphs.
         (length (org-element-map tree 'paragraph #'identity))))))"##,
        expect,
    );
}

// ── Combo: todo + tags + priorities ──────────────────────────────────

#[test]
fn upstream_org_combo_todo_tags_priorities() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] Urgent :work:urgent:\n* DONE [#B] Completed :home:\n* WAIT [#C] Blocked :work:waiting:\n* Normal task\n* TODO [#A] Another urgent :work:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         ;; All TODO keywords.
         (mapcar (lambda (h) (org-element-property :todo-keyword h)) headlines)
         ;; All priorities.
         (mapcar (lambda (h) (org-element-property :priority h)) headlines)
         ;; All tags.
         (mapcar (lambda (h) (org-element-property :tags h)) headlines)
         ;; Only TODO items.
         (length (org-element-map tree 'headline
                   (lambda (h) (when (equal (org-element-property :todo-keyword h) "TODO") h))))
         ;; Only high priority.
         (length (org-element-map tree 'headline
                   (lambda (h) (when (equal (org-element-property :priority h) ?A) h))))))))"##,
        expect,
    );
}

// ── Combo: timestamps + planning + repeaters ─────────────────────────

#[test]
fn upstream_org_combo_timestamps_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (2 3 (cumulate 1 week) (nil nil nil) (active inactive active-range))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Weekly review\nSCHEDULED: <2024-01-15 Mon +1w>\nDEADLINE: <2024-01-19 Fri -3d>\n:PROPERTIES:\n:LAST_REPEAT: [2024-01-08 Mon]\n:END:\n* Meeting\n<2024-01-20 Sat 14:00-15:30>\n* Deadline only\nDEADLINE: <2024-01-22 Mon>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (planning (org-element-map tree 'planning #'identity))
             (timestamps (org-element-map tree 'timestamp #'identity)))
        (list
         ;; Number of planning lines.
         (length planning)
         ;; Number of timestamps.
         (length timestamps)
         ;; Scheduled repeater.
         (let ((sched (org-element-map tree 'planning
                        (lambda (p) (org-element-property :scheduled p)) nil t)))
           (when sched
             (list (org-element-property :repeater-type sched)
                   (org-element-property :repeater-value sched)
                   (org-element-property :repeater-unit sched))))
         ;; Deadline warning.
         (let ((dl (org-element-map tree 'planning
                     (lambda (p) (org-element-property :deadline p)) nil t)))
           (when dl
             (list (org-element-property :warning-type dl)
                   (org-element-property :warning-value dl)
                   (org-element-property :warning-unit dl))))
         ;; Timestamp types.
         (mapcar (lambda (ts) (org-element-property :type ts)) timestamps))))))"##,
        expect,
    );
}

// ── Combo: export options + headlines + sections ─────────────────────

#[test]
fn upstream_org_combo_export_headlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Test Document\n#+AUTHOR: Test\n#+OPTIONS: num:t toc:nil\n* Chapter 1\n** Section 1.1\nContent 1.1\n** Section 1.2\nContent 1.2\n* Chapter 2\n** Section 2.1\n*** Subsection 2.1.1\nContent 2.1.1")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         ;; Title.
         (plist-get info :title)
         ;; Author.
         (plist-get info :author)
         ;; Headline numbers.
         (mapcar (lambda (h)
                   (org-export-get-headline-number h info))
                 (org-element-map tree 'headline #'identity))
         ;; Relative levels.
         (mapcar (lambda (h)
                   (org-export-get-relative-level h info))
                 (org-element-map tree 'headline #'identity))
         ;; Numbered?
         (mapcar (lambda (h)
                   (org-export-numbered-headline-p h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}
