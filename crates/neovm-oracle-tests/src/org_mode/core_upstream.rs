//! Ported upstream ERT tests from org-mode's test-org.el (9.7.11).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ── Time functions ───────────────────────────────────────────────────

#[test]
fn upstream_org_parse_time_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((0 40 16 29 3 2012 nil -1 nil) (0 40 16 29 3 2012 nil -1 nil) (0 40 16 29 3 2012 nil -1 nil) (0 0 0 29 3 2012 nil -1 nil) (0 nil nil 29 3 2012 nil -1 nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   (org-parse-time-string "2012-03-29 16:40")
   (org-parse-time-string "[2012-03-29 16:40]")
   (org-parse-time-string "<2012-03-29 16:40>")
   (org-parse-time-string "<2012-03-29>")
   (org-parse-time-string "<2012-03-29>" t)))"##,
        expect,
    );
}

// ── Comments ─────────────────────────────────────────────────────────

#[test]
fn upstream_org_toggle_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* Test\" \"* COMMENT Test\" \"* TODO Test\" \"* TODO COMMENT Test\" \"* \" \"* COMMENT\" \"* TODO [#A] Headline\" \"* TODO [#A] COMMENT Headline\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Simple headline.
     (with-temp-buffer
       (org-mode)
       (insert "* COMMENT Test")
       (goto-char (point-min))
       (org-toggle-comment)
       (buffer-string))
     (with-temp-buffer
       (org-mode)
       (insert "* Test")
       (goto-char (point-min))
       (org-toggle-comment)
       (buffer-string))
     ;; With TODO keyword.
     (with-temp-buffer
       (org-mode)
       (insert "* TODO COMMENT Test")
       (goto-char (point-min))
       (org-toggle-comment)
       (buffer-string))
     (with-temp-buffer
       (org-mode)
       (insert "* TODO Test")
       (goto-char (point-min))
       (org-toggle-comment)
       (buffer-string))
     ;; Empty headline.
     (with-temp-buffer
       (org-mode)
       (insert "* COMMENT")
       (goto-char (point-min))
       (org-toggle-comment)
       (buffer-string))
     (with-temp-buffer
       (org-mode)
       (insert "* ")
       (goto-char (point-min))
       (org-toggle-comment)
       (buffer-string))
     ;; With priority.
     (with-temp-buffer
       (org-mode)
       (insert "* TODO [#A] COMMENT Headline")
       (goto-char (point-min))
       (org-toggle-comment)
       (buffer-string))
     (with-temp-buffer
       (org-mode)
       (insert "* TODO [#A] Headline")
       (goto-char (point-min))
       (org-toggle-comment)
       (buffer-string)))))"##,
        expect,
    );
}

// ── Toggle fixed-width ───────────────────────────────────────────────

#[test]
fn upstream_org_toggle_fixed_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\": A\" \"A\" \"* H\\n: \" \": * Headline\" \": #+KEYWORD: value\" \"- A\\n  : B\" \"A\\n\\nB\" \": A\\n: \\n: B\\n: \\n: C\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Toggle on in paragraph.
     (with-temp-buffer
       (org-mode)
       (insert "A")
       (goto-char (point-min))
       (org-toggle-fixed-width)
       (buffer-string))
     ;; Toggle off in fixed-width.
     (with-temp-buffer
       (org-mode)
       (insert ": A")
       (goto-char (point-min))
       (org-toggle-fixed-width)
       (buffer-string))
     ;; Toggle on after headline.
     (with-temp-buffer
       (org-mode)
       (insert "* H\n")
       (goto-char (point-min))
       (forward-line)
       (org-toggle-fixed-width)
       (buffer-string))
     ;; Toggle on for headline.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline")
       (goto-char (point-min))
       (org-toggle-fixed-width)
       (buffer-string))
     ;; Toggle on for keyword.
     (with-temp-buffer
       (org-mode)
       (insert "#+KEYWORD: value")
       (goto-char (point-min))
       (org-toggle-fixed-width)
       (buffer-string))
     ;; Preserve indentation.
     (with-temp-buffer
       (org-mode)
       (insert "- A\n  B")
       (goto-char (point-min))
       (forward-line)
       (org-toggle-fixed-width)
       (buffer-string))
     ;; Region: toggle off fixed-width.
     (with-temp-buffer
       (org-mode)
       (insert ": A\n\n: B")
       (goto-char (point-min))
       (transient-mark-mode 1)
       (push-mark (point) t t)
       (goto-char (point-max))
       (org-toggle-fixed-width)
       (buffer-string))
     ;; Region: toggle on.
     (with-temp-buffer
       (org-mode)
       (insert "A\n\n: B\n\nC")
       (goto-char (point-min))
       (transient-mark-mode 1)
       (push-mark (point) t t)
       (goto-char (point-max))
       (org-toggle-fixed-width)
       (buffer-string)))))"##,
        expect,
    );
}

// ── Navigation: org-back-to-heading ──────────────────────────────────

#[test]
fn upstream_org_back_to_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 11 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (let ((org-mode-hook nil)
        (org-inlinetask-min-level 3))
    (list
     ;; On heading.
     (with-temp-buffer
       (org-mode)
       (insert "* Heading")
       (goto-char (point-min))
       (org-back-to-heading)
       (bobp))
     ;; Below heading.
     (with-temp-buffer
       (org-mode)
       (insert "* Heading\nText")
       (goto-char (point-max))
       (org-back-to-heading)
       (bobp))
     ;; At inlinetask.
     (with-temp-buffer
       (org-mode)
       (insert "* Heading\n*** Inlinetask")
       (goto-char (point-max))
       (org-back-to-heading)
       (point))
     ;; Inside inlinetask.
     (with-temp-buffer
       (org-mode)
       (insert "* Heading\n*** Inlinetask\nTest\n*** END")
       (goto-char (point-min))
       (search-forward "Test")
       (org-back-to-heading)
       (point)))))"##,
        expect,
    );
}

// ── Navigation: org-up-heading-safe ──────────────────────────────────

#[test]
fn upstream_org_up_heading_safe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 t) (nil t) (1 t) (2 t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (let ((org-mode-hook nil))
    (list
     ;; Jump to parent.
     (with-temp-buffer
       (org-mode)
       (insert "\n* H1\n** H2")
       (goto-char (point-max))
       (list (org-up-heading-safe)
             (looking-at-p "^\\* H1")))
     ;; Do not jump beyond level 1.
     (with-temp-buffer
       (org-mode)
       (insert "\nText.\n* Heading")
       (goto-char (point-max))
       (list (org-up-heading-safe)
             (looking-at-p "^\\* Heading")))
     ;; From inside heading.
     (with-temp-buffer
       (org-mode)
       (insert "\n* H1\n** H2\nText")
       (goto-char (point-max))
       (list (org-up-heading-safe)
             (looking-at-p "^\\* H1")))
     ;; With inlinetask.
     (let ((org-inlinetask-min-level 3))
       (with-temp-buffer
         (org-mode)
         (insert "\n** Heading\nText.\n*** Inlinetask\nText\n*** END")
         (goto-char (point-max))
         (forward-line -1)
         (list (org-up-heading-safe)
               (looking-at-p "^\\*\\{2\\} Heading")))))))"##,
        expect,
    );
}

// ── Navigation: org-goto-sibling ─────────────────────────────────────

#[test]
fn upstream_org_goto_sibling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Forward siblings.
     (with-temp-buffer
       (org-mode)
       (insert "* Parent\n** Heading 1\n** Heading 2\n** Heading 3")
       (goto-char (point-min))
       (forward-line 2)
       (list (org-goto-sibling)
             (looking-at-p "^\\*\\* Heading 3")
             (org-goto-sibling)
             (org-goto-sibling 'previous)
             (looking-at-p "^\\*\\* Heading 2")))
     ;; From inside heading.
     (with-temp-buffer
       (org-mode)
       (insert "* Parent\n** Heading 1\n** Heading 2\nSome text.\n** Heading 3")
       (goto-char (point-min))
       (search-forward "Some text")
       (list (org-goto-sibling)
             (looking-at-p "^\\*\\* Heading 3")))
     ;; Previous from inside heading.
     (with-temp-buffer
       (org-mode)
       (insert "* Parent\n** Heading 1\n** Heading 2\nSome text.\n** Heading 3")
       (goto-char (point-min))
       (search-forward "Some text")
       (list (org-goto-sibling 'previous)
             (looking-at-p "^\\*\\* Heading 1")))))"##,
        expect,
    );
}

// ── Navigation: org-get-heading ──────────────────────────────────────

#[test]
fn upstream_org_get_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"H\" \"H\" \"TODO H\" \"[#A] H\" \"COMMENT H\" \"H :tag:\" \"H\" \"H\" \"H\" \"H\" \"\" \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Basic.
     (with-temp-buffer (org-mode) (insert "* H") (goto-char (point-min)) (org-get-heading))
     ;; From body.
     (with-temp-buffer (org-mode) (insert "* H\nText") (goto-char (point-max)) (org-get-heading))
     ;; With TODO.
     (with-temp-buffer (org-mode) (insert "* TODO H") (goto-char (point-min)) (org-get-heading))
     ;; With priority.
     (with-temp-buffer (org-mode) (insert "* [#A] H") (goto-char (point-min)) (org-get-heading))
     ;; With COMMENT.
     (with-temp-buffer (org-mode) (insert "* COMMENT H") (goto-char (point-min)) (org-get-heading))
     ;; With tags.
     (with-temp-buffer (org-mode) (insert "* H :tag:") (goto-char (point-min)) (org-get-heading))
     ;; NO-TAGS.
     (with-temp-buffer (org-mode) (insert "* H :tag:") (goto-char (point-min)) (org-get-heading t))
     ;; NO-TODO.
     (with-temp-buffer (org-mode) (insert "* TODO H") (goto-char (point-min)) (org-get-heading nil t))
     ;; NO-PRIORITY.
     (with-temp-buffer (org-mode) (insert "* [#A] H") (goto-char (point-min)) (org-get-heading nil nil t))
     ;; NO-COMMENT.
     (with-temp-buffer (org-mode) (insert "* COMMENT H") (goto-char (point-min)) (org-get-heading nil nil nil t))
     ;; Empty headline.
     (with-temp-buffer (org-mode) (insert "* ") (goto-char (point-min)) (org-get-heading))
     (with-temp-buffer (org-mode) (insert "* ") (goto-char (point-min)) (org-get-heading t)))))"##,
        expect,
    );
}

// ── Navigation: org-in-commented-heading-p ───────────────────────────

#[test]
fn upstream_org_in_commented_heading_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Commented headline.
     (with-temp-buffer
       (org-mode)
       (insert "* COMMENT Headline\nBody")
       (goto-char (point-max))
       (org-in-commented-heading-p))
     ;; Commented ancestor.
     (with-temp-buffer
       (org-mode)
       (insert "* COMMENT Headline\n** Level 2\nBody")
       (goto-char (point-max))
       (org-in-commented-heading-p))
     ;; Case-sensitive.
     (with-temp-buffer
       (org-mode)
       (insert "* Comment Headline\nBody")
       (goto-char (point-max))
       (org-in-commented-heading-p))
     ;; Standalone keyword.
     (with-temp-buffer
       (org-mode)
       (insert "* COMMENTHeadline\nBody")
       (goto-char (point-max))
       (org-in-commented-heading-p))
     ;; Optional argument.
     (with-temp-buffer
       (org-mode)
       (insert "* COMMENT Headline\n** Level 2\nBody")
       (goto-char (point-max))
       (org-in-commented-heading-p t)))))"##,
        expect,
    );
}

// ── Navigation: org-in-archived-heading-p ────────────────────────────

#[test]
fn upstream_org_in_archived_heading_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Archived headline.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline :ARCHIVE:\nBody")
       (goto-char (point-max))
       (org-in-archived-heading-p))
     ;; Archived ancestor.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline :ARCHIVE:\n** Level 2\nBody")
       (goto-char (point-max))
       (org-in-archived-heading-p))
     ;; Optional argument.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline :ARCHIVE:\n** Level 2\nBody")
       (goto-char (point-max))
       (org-in-archived-heading-p t))
     ;; Not ARCHIVE.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline :NOARCHIVE:\n** Level 2\nBody")
       (goto-char (point-max))
       (org-in-archived-heading-p)))))"##,
        expect,
    );
}

// ── Navigation: org-get-outline-path ─────────────────────────────────

#[test]
fn upstream_org_get_outline_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil (\"H\") (\"H\") (\"H\") (\"Org\") (\"H\") (\"H\" \"S\") (\"H\") (\"This\" \"is\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Top-level: no path.
     (with-temp-buffer (org-mode) (insert "* H") (goto-char (point-min)) (org-get-outline-path))
     ;; Nested.
     (with-temp-buffer (org-mode) (insert "* H\n** S") (goto-char (point-max)) (org-get-outline-path))
     ;; From body.
     (with-temp-buffer (org-mode) (insert "* H\n** S\nText") (goto-char (point-max)) (org-get-outline-path))
     ;; TODO/tags ignored.
     (with-temp-buffer (org-mode) (insert "* TODO H [0/1] :tag:\n** S") (goto-char (point-max)) (org-get-outline-path))
     ;; Links replaced.
     (with-temp-buffer (org-mode) (insert "* [[https://orgmode.org][Org]]\n** S") (goto-char (point-max)) (org-get-outline-path))
     ;; With self.
     (with-temp-buffer (org-mode) (insert "* H") (goto-char (point-min)) (org-get-outline-path t))
     (with-temp-buffer (org-mode) (insert "* H\n** S\nText") (goto-char (point-max)) (org-get-outline-path t))
     ;; Empty headlines.
     (with-temp-buffer (org-mode) (insert "* H\n** ") (goto-char (point-max)) (org-get-outline-path))
     ;; COMMENT removed.
     (with-temp-buffer (org-mode) (insert "* COMMENT This\n** COMMENT is\n*** test") (goto-char (point-max)) (org-get-outline-path)))))"##,
        expect,
    );
}

// ── Navigation: org-format-outline-path ──────────────────────────────

#[test]
fn upstream_org_format_outline_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"one/two/three\" 0 3 (face org-level-1) 4 7 (face org-level-2) 8 13 (face org-level-3)) \"\" \"\" \">>\" #(\"one/tw o/three\" 0 3 (face org-level-1) 4 8 (face org-level-2) 9 14 (face org-level-3)) #(\">>|one|two|three\" 3 6 (face org-level-1) 7 10 (face org-level-2) 11 16 (face org-level-3)) #(\"one/two/..\" 0 3 (face org-level-1) 4 7 (face org-level-2)) #(\"on\" 0 2 (face org-level-1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   (org-format-outline-path (list "one" "two" "three"))
   ;; Empty path.
   (org-format-outline-path '())
   (org-format-outline-path '(nil))
   ;; With prefix.
   (org-format-outline-path '() nil ">>")
   ;; Trailing whitespace.
   (org-format-outline-path (list "one\t" "tw o " "three  "))
   ;; Custom separator.
   (org-format-outline-path (list "one" "two" "three") nil ">>" "|")
   ;; Truncate.
   (org-format-outline-path (list "one" "two" "three" "four") 10)
   ;; Narrow width.
   (org-format-outline-path (list "one" "two" "three" "four") 2)))"##,
        expect,
    );
}

// ── Navigation: org-end-of-meta-data ─────────────────────────────────

#[test]
fn upstream_org_end_of_meta_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Skip planning.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline\nSCHEDULED: <2014-03-04 tue.>")
       (goto-char (point-min))
       (org-end-of-meta-data)
       (eobp))
     ;; Skip properties.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline\nSCHEDULED: <2014-03-04 tue.>\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min))
       (org-end-of-meta-data)
       (eobp))
     ;; Skip both.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min))
       (org-end-of-meta-data)
       (eobp))
     ;; Nothing to skip.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline\nContents")
       (goto-char (point-min))
       (org-end-of-meta-data)
       (looking-at "Contents"))
     ;; With argument: skip empty lines.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline\n\nContents")
       (goto-char (point-min))
       (org-end-of-meta-data t)
       (looking-at "Contents"))
     ;; With argument: skip LOGBOOK.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline\n:LOGBOOK:\nlogging\n:END:\nContents")
       (goto-char (point-min))
       (org-end-of-meta-data t)
       (looking-at "Contents"))
     ;; Incomplete drawer not skipped.
     (with-temp-buffer
       (org-mode)
       (insert "* Headline\n:LOGBOOK:\nlogging\nContents")
       (goto-char (point-min))
       (org-end-of-meta-data t)
       (looking-at ":LOGBOOK:")))))"##,
        expect,
    );
}

// ── Navigation: org-end-of-subtree ───────────────────────────────────

#[test]
fn upstream_org_end_of_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (let ((org-mode-hook nil))
    (list
     ;; Simple call.
     (with-temp-buffer
       (org-mode)
       (insert "\n* Heading\n** Sub1\n** Sub 2\nasd\n* Heading 2")
       (goto-char (point-min))
       (forward-line 1)
       (org-end-of-subtree)
       (forward-char)
       (looking-at-p "^\\* Heading 2"))
     ;; TO-HEADING.
     (with-temp-buffer
       (org-mode)
       (insert "\n* Heading\n** Sub1\n** Sub 2\nasd\n* Heading 2")
       (goto-char (point-min))
       (forward-line 1)
       (org-end-of-subtree nil t)
       (looking-at-p "^\\* Heading 2"))
     ;; Before first heading.
     (with-temp-buffer
       (org-mode)
       (insert "\nSome text.\n* Heading\n** Sub1\n** Sub 2\nasd\n* Heading 2")
       (goto-char (point-min))
       (org-end-of-subtree)
       (eobp))
     ;; With inlinetask.
     (let ((org-inlinetask-min-level 3))
       (with-temp-buffer
         (org-mode)
         (insert "\n* Heading\nsome text\n*** Inlinetask\nt\n*** END\n** Sub1\n** Sub 2\nasd\n* Heading 2")
         (goto-char (point-min))
         (search-forward "some text")
         (org-end-of-subtree)
         (forward-line 0)
         (looking-at-p "^asd"))))))"##,
        expect,
    );
}

// ── Navigation: forward-element ──────────────────────────────────────

#[test]
fn upstream_org_forward_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Standard move.
     (with-temp-buffer
       (org-mode)
       (insert "First paragraph.\n\n\nSecond paragraph.")
       (goto-char (point-min))
       (org-forward-element)
       (looking-at "Second paragraph."))
     ;; Greater element: skip contents.
     (with-temp-buffer
       (org-mode)
       (insert "#+BEGIN_CENTER\nInside.\n#+END_CENTER\n\nOutside.")
       (goto-char (point-min))
       (org-forward-element)
       (looking-at "Outside."))
     ;; At end of greater element contents.
     (with-temp-buffer
       (org-mode)
       (insert "#+BEGIN_CENTER\nInside.\n#+END_CENTER\n\nOutside.")
       (goto-char (point-min))
       (forward-line 1)
       (org-forward-element)
       (looking-at "Outside."))
     ;; Headline move.
     (with-temp-buffer
       (org-mode)
       (insert "\n* Head 1\n** Head 1.1\n*** Head 1.1.1\n** Head 1.2")
       (goto-line 3)
       (org-forward-element)
       (looking-at "** Head 1.2"))
     ;; List: move past list.
     (with-temp-buffer
       (org-mode)
       (insert "\n- item1\n\n  - sub1\n\n  - sub2\n\n- item2\n\nOutside.")
       (goto-char (point-min))
       (forward-line 1)
       (org-forward-element)
       (looking-at "Outside.")))))"##,
        expect,
    );
}

// ── Navigation: backward-element ─────────────────────────────────────

#[test]
fn upstream_org_backward_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Not at beginning: move to beginning.
     (with-temp-buffer
       (org-mode)
       (insert "Paragraph1.\n\nParagraph2.")
       (goto-char (point-max))
       (org-backward-element)
       (looking-at "Paragraph2."))
     ;; Headline: previous same level.
     (with-temp-buffer
       (org-mode)
       (insert "\n* Head 1\n** Head 1.1\n*** Head 1.1.1\n** Head 1.2")
       (goto-line 5)
       (org-backward-element)
       (looking-at "** Head 1.1"))
     ;; Headline: parent if no same level.
     (with-temp-buffer
       (org-mode)
       (insert "\n* Head 1\n** Head 1.1\n*** Head 1.1.1\n** Head 1.2")
       (goto-line 3)
       (org-backward-element)
       (looking-at "* Head 1"))
     ;; Greater element.
     (with-temp-buffer
       (org-mode)
       (insert "Before.\n#+BEGIN_CENTER\nInside.\n#+END_CENTER")
       (goto-line 3)
       (org-backward-element)
       (looking-at "#+BEGIN_CENTER"))
     ;; List backward.
     (with-temp-buffer
       (org-mode)
       (insert "\n- item1\n\n  - sub1\n\n  - sub2\n\n- item2\n\nOutside.")
       (goto-line 8)
       (org-backward-element)
       (looking-at "  - sub2")))))"##,
        expect,
    );
}

// ── Navigation: up-element ───────────────────────────────────────────

#[test]
fn upstream_org_up_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Headline: move to parent.
     (with-temp-buffer
       (org-mode)
       (insert "* Head1\n** Sub-Head1\n** Sub-Head2")
       (goto-char (point-min))
       (forward-line 2)
       (org-up-element)
       (looking-at "\\* Head1"))
     ;; Greater element.
     (with-temp-buffer
       (org-mode)
       (insert "Before.\n#+BEGIN_CENTER\nParagraph1\nParagraph2\n#+END_CENTER")
       (goto-line 3)
       (org-up-element)
       (looking-at "#+BEGIN_CENTER"))
     ;; List: item to parent item.
     (with-temp-buffer
       (org-mode)
       (insert "* Top\n- item1\n\n  - sub1\n\n  - sub2\n\n    Paragraph.\n\n- item2")
       (goto-line 8)
       (org-up-element)
       (looking-at "  - sub2"))
     ;; Sub-list item to parent item.
     (with-temp-buffer
       (org-mode)
       (insert "* Top\n- item1\n\n  - sub1\n\n  - sub2\n\n- item2")
       (goto-line 4)
       (org-up-element)
       (looking-at "- item1"))
     ;; Top item to list beginning.
     (with-temp-buffer
       (org-mode)
       (insert "* Top\n- item1\n\n- item2")
       (goto-line 4)
       (org-up-element)
       (looking-at "- item1")))))"##,
        expect,
    );
}

// ── Navigation: down-element ─────────────────────────────────────────

#[test]
fn upstream_org_down_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Plain list: move to first item.
     (with-temp-buffer
       (org-mode)
       (insert "- Item 1\n  - Item 1.1\n  - Item 2.2")
       (goto-char (point-min))
       (forward-line 1)
       (org-down-element)
       (looking-at "- Item 1.1"))
     ;; Table: move to first row.
     (with-temp-buffer
       (org-mode)
       (insert "| a | b |")
       (goto-char (point-min))
       (org-down-element)
       (looking-at "a | b |"))
     ;; Greater element: move inside.
     (with-temp-buffer
       (org-mode)
       (insert "#+BEGIN_CENTER\nParagraph.\n#+END_CENTER")
       (goto-char (point-min))
       (org-down-element)
       (looking-at "Paragraph.")))))"##,
        expect,
    );
}

// ── Navigation: org-next/previous-visible-heading ────────────────────

#[test]
fn upstream_org_next_visible_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Forward.
     (with-temp-buffer
       (org-mode)
       (insert "Text\n* H1\n* H2\n* H3")
       (goto-char (point-min))
       (org-next-visible-heading 1)
       (looking-at "\\* H1"))
     ;; Multiple.
     (with-temp-buffer
       (org-mode)
       (insert "Text\n* H1\n* H2\n* H3")
       (goto-char (point-min))
       (org-next-visible-heading 2)
       (looking-at "\\* H2"))
     ;; Backward.
     (with-temp-buffer
       (org-mode)
       (insert "* H1\n* H2\n* H3\nText")
       (goto-char (point-max))
       (org-previous-visible-heading 1)
       (looking-at "\\* H3"))
     ;; Multiple backward.
     (with-temp-buffer
       (org-mode)
       (insert "* H1\n* H2\n* H3\nText")
       (goto-char (point-max))
       (org-previous-visible-heading 2)
       (looking-at "\\* H2")))))"##,
        expect,
    );
}

// ── Navigation: org-forward-heading-same-level ───────────────────────

#[test]
fn upstream_org_forward_heading_same_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Forward same level.
     (with-temp-buffer
       (org-mode)
       (insert "* H1\n** S1\n** S2\n** S3\n* H2")
       (goto-char (point-min))
       (forward-line 1)
       (org-forward-heading-same-level 1)
       (looking-at "\\*\\* S2"))
     ;; Forward past all.
     (with-temp-buffer
       (org-mode)
       (insert "* H1\n** S1\n** S2\n* H2")
       (goto-char (point-min))
       (forward-line 2)
       (org-forward-heading-same-level 1)
       (looking-at "\\* H2"))
     ;; Backward same level.
     (with-temp-buffer
       (org-mode)
       (insert "* H1\n** S1\n** S2\n** S3\n* H2")
       (goto-char (point-min))
       (forward-line 3)
       (org-forward-heading-same-level -1)
       (looking-at "\\*\\* S2")))))"##,
        expect,
    );
}

// ── Structure: org-move-subtree ──────────────────────────────────────

#[test]
fn upstream_org_move_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-move-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Move down.
     (with-temp-buffer
       (org-mode)
       (insert "* A\nBody A\n* B\nBody B\n* C\nBody C")
       (goto-char (point-min))
       (org-move-subtree 1)
       (buffer-substring-no-properties (point-min) (point-max)))
     ;; Move up.
     (with-temp-buffer
       (org-mode)
       (insert "* A\nBody A\n* B\nBody B\n* C\nBody C")
       (goto-char (point-min))
       (forward-line 2)
       (org-move-subtree -1)
       (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

// ── Structure: org-promote/org-demote ────────────────────────────────

#[test]
fn upstream_org_promote_demote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* Heading\" \"** Heading\" \"* H1\\n** S1\\n** S2\" \"** H1\\n*** S1\\n*** S2\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Promote.
     (with-temp-buffer
       (org-mode)
       (insert "** Heading")
       (goto-char (point-min))
       (org-promote)
       (buffer-string))
     ;; Demote.
     (with-temp-buffer
       (org-mode)
       (insert "* Heading")
       (goto-char (point-min))
       (org-demote)
       (buffer-string))
     ;; Promote subtree.
     (with-temp-buffer
       (org-mode)
       (insert "** H1\n*** S1\n*** S2")
       (goto-char (point-min))
       (org-promote-subtree)
       (buffer-string))
     ;; Demote subtree.
     (with-temp-buffer
       (org-mode)
       (insert "* H1\n** S1\n** S2")
       (goto-char (point-min))
       (org-demote-subtree)
       (buffer-string)))))"##,
        expect,
    );
}

// ── Structure: org-toggle-heading ────────────────────────────────────

#[test]
fn upstream_org_toggle_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* Item\" \"Heading\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Toggle on.
     (with-temp-buffer
       (org-mode)
       (insert "Item")
       (goto-char (point-min))
       (org-toggle-heading)
       (buffer-string))
     ;; Toggle off.
     (with-temp-buffer
       (org-mode)
       (insert "* Heading")
       (goto-char (point-min))
       (org-toggle-heading)
       (buffer-string)))))"##,
        expect,
    );
}

// ── org-get-valid-level ──────────────────────────────────────────────

#[test]
fn upstream_org_get_valid_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 4 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   (org-get-valid-level 1 1)
   (org-get-valid-level 1 2)
   (org-get-valid-level 3 1)
   (org-get-valid-level 2 -1)))"##,
        expect,
    );
}

// ── org-at-planning-p ────────────────────────────────────────────────

#[test]
fn upstream_org_at_planning_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer
       (org-mode)
       (insert "* H\nDEADLINE: <2023-10-13 Fri>")
       (goto-char (point-min))
       (forward-line 1)
       (org-at-planning-p))
     ;; Not planning: after comment.
     (with-temp-buffer
       (org-mode)
       (insert "* H\n# Comment\nDEADLINE: <2023-10-13 Fri>")
       (goto-char (point-min))
       (forward-line 2)
       (org-at-planning-p))
     ;; Not planning: standalone.
     (with-temp-buffer
       (org-mode)
       (insert "DEADLINE: <2023-10-13 Fri>")
       (goto-char (point-min))
       (org-at-planning-p)))))"##,
        expect,
    );
}

// ── org-match-sparse-tree ────────────────────────────────────────────

#[test]
fn upstream_org_match_sparse_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"B\" \"C\" \"D\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "* TODO A\nBody\n* DONE B\nBody\n* TODO C\nBody\n* DONE D\nBody")
      (goto-char (point-min))
      (org-match-sparse-tree nil "TODO")
      (let ((visible nil))
        (org-element-map (org-element-parse-buffer) 'headline
          (lambda (h)
            (let ((title (org-element-property :raw-value h)))
              (when (org-element-property :begin h)
                (push title visible)))))
        (nreverse visible)))))"##,
        expect,
    );
}

// ── org-set-tags-command ─────────────────────────────────────────────

#[test]
fn upstream_org_toggle_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* Heading                                                              :test:\" \"* Heading\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Toggle tag on.
     (with-temp-buffer
       (org-mode)
       (insert "* Heading")
       (goto-char (point-min))
       (org-toggle-tag "test")
       (buffer-string))
     ;; Toggle tag off.
     (with-temp-buffer
       (org-mode)
       (insert "* Heading :test:")
       (goto-char (point-min))
       (org-toggle-tag "test")
       (buffer-string)))))"##,
        expect,
    );
}

// ── org-todo ─────────────────────────────────────────────────────────

#[test]
fn upstream_org_todo_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"* TODO Heading\" 0 14 (org-todo-head \"TODO\")) #(\"* DONE Heading\" 0 14 (org-todo-head \"TODO\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-todo-keywords '((sequence "TODO" "DONE"))))
    (list
     (with-temp-buffer
       (org-mode)
       (insert "* Heading")
       (goto-char (point-min))
       (org-todo 'todo)
       (buffer-string))
     (with-temp-buffer
       (org-mode)
       (insert "* TODO Heading")
       (goto-char (point-min))
       (org-todo 'done)
       (buffer-string)))))"##,
        expect,
    );
}

// ── org-get-repeat ───────────────────────────────────────────────────

#[test]
fn upstream_org_get_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"+1w\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; With repeater.
     (with-temp-buffer
       (org-mode)
       (insert "* H\nSCHEDULED: <2023-10-13 Fri +1w>")
       (goto-char (point-min))
       (forward-line 1)
       (org-get-repeat))
     ;; No repeater.
     (with-temp-buffer
       (org-mode)
       (insert "* H\nSCHEDULED: <2023-10-13 Fri>")
       (goto-char (point-min))
       (forward-line 1)
       (org-get-repeat)))))"##,
        expect,
    );
}

// ── org-timestamp-has-time-p ─────────────────────────────────────────

#[test]
fn upstream_org_timestamp_has_time_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; With time.
     (with-temp-buffer
       (org-mode)
       (insert "<2023-10-13 Fri 14:30>")
       (goto-char (point-min))
       (org-at-timestamp-p 'lax)
       (org-timestamp-has-time-p))
     ;; Without time.
     (with-temp-buffer
       (org-mode)
       (insert "<2023-10-13 Fri>")
       (goto-char (point-min))
       (org-at-timestamp-p 'lax)
       (org-timestamp-has-time-p)))))"##,
        expect,
    );
}

// ── org-at-timestamp-p ───────────────────────────────────────────────

#[test]
fn upstream_org_at_timestamp_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bracket bracket nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Active timestamp.
     (with-temp-buffer
       (org-mode)
       (insert "<2023-10-13 Fri>")
       (goto-char (point-min))
       (org-at-timestamp-p 'lax))
     ;; Inactive timestamp.
     (with-temp-buffer
       (org-mode)
       (insert "[2023-10-13 Fri]")
       (goto-char (point-min))
       (org-at-timestamp-p 'lax))
     ;; Not at timestamp.
     (with-temp-buffer
       (org-mode)
       (insert "Not a timestamp")
       (goto-char (point-min))
       (org-at-timestamp-p 'lax)))))"##,
        expect,
    );
}

// ── org-get-category ─────────────────────────────────────────────────

#[test]
fn upstream_org_get_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Work\" \"???\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; From keyword.
     (with-temp-buffer
       (org-mode)
       (insert "#+CATEGORY: Work\n* Heading")
       (goto-char (point-min))
       (org-get-category))
     ;; Default.
     (with-temp-buffer
       (org-mode)
       (insert "* Heading")
       (goto-char (point-min))
       (org-get-category)))))"##,
        expect,
    );
}
