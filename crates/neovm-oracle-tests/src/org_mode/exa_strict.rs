//! Exa-strict combo tests for org-mode extreme edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-get-outline-path combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_get_outline_path_combinations() {
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

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-format-outline-path combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_format_outline_path_combinations() {
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

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-end-of-meta-data combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_end_of_meta_data_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Skip planning.
     (with-temp-buffer (org-mode) (insert "* Headline\nSCHEDULED: <2014-03-04 tue.>")
       (goto-char (point-min)) (org-end-of-meta-data) (eobp))
     ;; Skip properties.
     (with-temp-buffer (org-mode) (insert "* Headline\nSCHEDULED: <2014-03-04 tue.>\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-end-of-meta-data) (eobp))
     ;; Skip both.
     (with-temp-buffer (org-mode) (insert "* Headline\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-end-of-meta-data) (eobp))
     ;; Nothing to skip.
     (with-temp-buffer (org-mode) (insert "* Headline\nContents")
       (goto-char (point-min)) (org-end-of-meta-data) (looking-at "Contents"))
     ;; With argument: skip empty lines.
     (with-temp-buffer (org-mode) (insert "* Headline\n\nContents")
       (goto-char (point-min)) (org-end-of-meta-data t) (looking-at "Contents"))
     ;; With argument: skip LOGBOOK.
     (with-temp-buffer (org-mode) (insert "* Headline\n:LOGBOOK:\nlogging\n:END:\nContents")
       (goto-char (point-min)) (org-end-of-meta-data t) (looking-at "Contents"))
     ;; Incomplete drawer not skipped.
     (with-temp-buffer (org-mode) (insert "* Headline\n:LOGBOOK:\nlogging\nContents")
       (goto-char (point-min)) (org-end-of-meta-data t) (looking-at ":LOGBOOK:")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-end-of-subtree combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_end_of_subtree_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (let ((org-mode-hook nil))
    (list
     ;; Simple call.
     (with-temp-buffer (org-mode)
       (insert "\n* Heading\n** Sub1\n** Sub 2\nasd\n* Heading 2")
       (goto-char (point-min)) (forward-line 1) (org-end-of-subtree)
       (forward-char) (looking-at-p "^\\* Heading 2"))
     ;; TO-HEADING.
     (with-temp-buffer (org-mode)
       (insert "\n* Heading\n** Sub1\n** Sub 2\nasd\n* Heading 2")
       (goto-char (point-min)) (forward-line 1) (org-end-of-subtree nil t)
       (looking-at-p "^\\* Heading 2"))
     ;; Before first heading.
     (with-temp-buffer (org-mode)
       (insert "\nSome text.\n* Heading\n** Sub1\n** Sub 2\nasd\n* Heading 2")
       (goto-char (point-min)) (org-end-of-subtree) (eobp))
     ;; With inlinetask.
     (let ((org-inlinetask-min-level 3))
       (with-temp-buffer (org-mode)
         (insert "\n* Heading\nsome text\n*** Inlinetask\nt\n*** END\n** Sub1\n** Sub 2\nasd\n* Heading 2")
         (goto-char (point-min)) (search-forward "some text") (org-end-of-subtree)
         (forward-line 0) (looking-at-p "^asd"))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-forward-element combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_forward_element_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Standard move.
     (with-temp-buffer (org-mode)
       (insert "First paragraph.\n\n\nSecond paragraph.")
       (goto-char (point-min)) (org-forward-element) (looking-at "Second paragraph."))
     ;; Greater element: skip contents.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_CENTER\nInside.\n#+END_CENTER\n\nOutside.")
       (goto-char (point-min)) (org-forward-element) (looking-at "Outside."))
     ;; Headline move.
     (with-temp-buffer (org-mode)
       (insert "\n* Head 1\n** Head 1.1\n*** Head 1.1.1\n** Head 1.2")
       (goto-line 3) (org-forward-element) (looking-at "** Head 1.2"))
     ;; List: move past list.
     (with-temp-buffer (org-mode)
       (insert "\n- item1\n\n  - sub1\n\n  - sub2\n\n- item2\n\nOutside.")
       (goto-char (point-min)) (forward-line 1) (org-forward-element) (looking-at "Outside.")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-backward-element combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_backward_element_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Not at beginning: move to beginning.
     (with-temp-buffer (org-mode)
       (insert "Paragraph1.\n\nParagraph2.")
       (goto-char (point-max)) (org-backward-element) (looking-at "Paragraph2."))
     ;; Headline: previous same level.
     (with-temp-buffer (org-mode)
       (insert "\n* Head 1\n** Head 1.1\n*** Head 1.1.1\n** Head 1.2")
       (goto-line 5) (org-backward-element) (looking-at "** Head 1.1"))
     ;; Headline: parent if no same level.
     (with-temp-buffer (org-mode)
       (insert "\n* Head 1\n** Head 1.1\n*** Head 1.1.1\n** Head 1.2")
       (goto-line 3) (org-backward-element) (looking-at "* Head 1"))
     ;; Greater element.
     (with-temp-buffer (org-mode)
       (insert "Before.\n#+BEGIN_CENTER\nInside.\n#+END_CENTER")
       (goto-line 3) (org-backward-element) (looking-at "#+BEGIN_CENTER"))
     ;; List backward.
     (with-temp-buffer (org-mode)
       (insert "\n- item1\n\n  - sub1\n\n  - sub2\n\n- item2\n\nOutside.")
       (goto-line 8) (org-backward-element) (looking-at "  - sub2")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-up-element combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_up_element_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Headline: move to parent.
     (with-temp-buffer (org-mode)
       (insert "* Head1\n** Sub-Head1\n** Sub-Head2")
       (goto-char (point-min)) (forward-line 2) (org-up-element) (looking-at "\\* Head1"))
     ;; Greater element.
     (with-temp-buffer (org-mode)
       (insert "Before.\n#+BEGIN_CENTER\nParagraph1\nParagraph2\n#+END_CENTER")
       (goto-line 3) (org-up-element) (looking-at "#+BEGIN_CENTER"))
     ;; List: item to parent item.
     (with-temp-buffer (org-mode)
       (insert "* Top\n- item1\n\n  - sub1\n\n  - sub2\n\n    Paragraph.\n\n- item2")
       (goto-line 8) (org-up-element) (looking-at "  - sub2"))
     ;; Sub-list item to parent item.
     (with-temp-buffer (org-mode)
       (insert "* Top\n- item1\n\n  - sub1\n\n  - sub2\n\n- item2")
       (goto-line 4) (org-up-element) (looking-at "- item1"))
     ;; Top item to list beginning.
     (with-temp-buffer (org-mode)
       (insert "* Top\n- item1\n\n- item2")
       (goto-line 4) (org-up-element) (looking-at "- item1")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-down-element combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_down_element_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Plain list: move to first item.
     (with-temp-buffer (org-mode)
       (insert "- Item 1\n  - Item 1.1\n  - Item 2.2")
       (goto-char (point-min)) (forward-line 1) (org-down-element) (looking-at "- Item 1.1"))
     ;; Table: move to first row.
     (with-temp-buffer (org-mode)
       (insert "| a | b |")
       (goto-char (point-min)) (org-down-element) (looking-at "a | b |"))
     ;; Greater element: move inside.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_CENTER\nParagraph.\n#+END_CENTER")
       (goto-char (point-min)) (org-down-element) (looking-at "Paragraph.")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-next/previous-visible-heading combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_next_previous_visible_heading_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Forward.
     (with-temp-buffer (org-mode)
       (insert "Text\n* H1\n* H2\n* H3")
       (goto-char (point-min)) (org-next-visible-heading 1) (looking-at "\\* H1"))
     ;; Multiple.
     (with-temp-buffer (org-mode)
       (insert "Text\n* H1\n* H2\n* H3")
       (goto-char (point-min)) (org-next-visible-heading 2) (looking-at "\\* H2"))
     ;; Backward.
     (with-temp-buffer (org-mode)
       (insert "* H1\n* H2\n* H3\nText")
       (goto-char (point-max)) (org-previous-visible-heading 1) (looking-at "\\* H3"))
     ;; Multiple backward.
     (with-temp-buffer (org-mode)
       (insert "* H1\n* H2\n* H3\nText")
       (goto-char (point-max)) (org-previous-visible-heading 2) (looking-at "\\* H2")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-forward-heading-same-level combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_forward_heading_same_level_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Forward same level.
     (with-temp-buffer (org-mode)
       (insert "* H1\n** S1\n** S2\n** S3\n* H2")
       (goto-char (point-min)) (forward-line 1) (org-forward-heading-same-level 1) (looking-at "\\*\\* S2"))
     ;; Forward past all.
     (with-temp-buffer (org-mode)
       (insert "* H1\n** S1\n** S2\n* H2")
       (goto-char (point-min)) (forward-line 2) (org-forward-heading-same-level 1) (looking-at "\\* H2"))
     ;; Backward same level.
     (with-temp-buffer (org-mode)
       (insert "* H1\n** S1\n** S2\n** S3\n* H2")
       (goto-char (point-min)) (forward-line 3) (org-forward-heading-same-level -1) (looking-at "\\*\\* S2")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-move-subtree combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_move_subtree_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-move-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Move down.
     (with-temp-buffer (org-mode)
       (insert "* A\nBody A\n* B\nBody B\n* C\nBody C")
       (goto-char (point-min)) (org-move-subtree 1)
       (buffer-substring-no-properties (point-min) (point-max)))
     ;; Move up.
     (with-temp-buffer (org-mode)
       (insert "* A\nBody A\n* B\nBody B\n* C\nBody C")
       (goto-char (point-min)) (forward-line 2) (org-move-subtree -1)
       (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-promote/org-demote combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_promote_demote_combinations() {
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
     (with-temp-buffer (org-mode) (insert "** Heading")
       (goto-char (point-min)) (org-promote) (buffer-string))
     ;; Demote.
     (with-temp-buffer (org-mode) (insert "* Heading")
       (goto-char (point-min)) (org-demote) (buffer-string))
     ;; Promote subtree.
     (with-temp-buffer (org-mode) (insert "** H1\n*** S1\n*** S2")
       (goto-char (point-min)) (org-promote-subtree) (buffer-string))
     ;; Demote subtree.
     (with-temp-buffer (org-mode) (insert "* H1\n** S1\n** S2")
       (goto-char (point-min)) (org-demote-subtree) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-toggle-heading combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_toggle_heading_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* Item\" \"Heading\" \"* Item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Toggle on.
     (with-temp-buffer (org-mode) (insert "Item")
       (goto-char (point-min)) (org-toggle-heading) (buffer-string))
     ;; Toggle off.
     (with-temp-buffer (org-mode) (insert "* Heading")
       (goto-char (point-min)) (org-toggle-heading) (buffer-string))
     ;; Toggle on numbered.
     (with-temp-buffer (org-mode) (insert "Item")
       (goto-char (point-min)) (org-toggle-heading 1) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-get-valid-level combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_get_valid_level_combinations() {
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

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-at-planning-p combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_at_planning_p_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H\nDEADLINE: <2023-10-13 Fri>")
       (goto-char (point-min)) (forward-line 1) (org-at-planning-p))
     ;; Not planning: after comment.
     (with-temp-buffer (org-mode) (insert "* H\n# Comment\nDEADLINE: <2023-10-13 Fri>")
       (goto-char (point-min)) (forward-line 2) (org-at-planning-p))
     ;; Not planning: standalone.
     (with-temp-buffer (org-mode) (insert "DEADLINE: <2023-10-13 Fri>")
       (goto-char (point-min)) (org-at-planning-p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-match-sparse-tree combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_match_sparse_tree_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"B\" \"C\" \"D\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
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

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-toggle-tag combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_toggle_tag_combinations() {
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
     (with-temp-buffer (org-mode) (insert "* Heading")
       (goto-char (point-min)) (org-toggle-tag "test") (buffer-string))
     ;; Toggle tag off.
     (with-temp-buffer (org-mode) (insert "* Heading :test:")
       (goto-char (point-min)) (org-toggle-tag "test") (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-todo combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_todo_combinations() {
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
       (goto-char (point-min)) (org-todo 'todo) (buffer-string))
     ;; Cycle to DONE.
     (with-temp-buffer (org-mode) (insert "* TODO Heading")
       (goto-char (point-min)) (org-todo 'done) (buffer-string))
     ;; Cycle DONE -> empty.
     (with-temp-buffer (org-mode) (insert "* DONE Heading")
       (goto-char (point-min)) (org-todo nil) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-set-tags combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_set_tags_combinations() {
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
       (goto-char (point-min)) (org-set-tags '("tag1")) (buffer-string))
     ;; Replace tag.
     (with-temp-buffer (org-mode) (insert "* Heading :old:")
       (goto-char (point-min)) (org-set-tags '("new")) (buffer-string))
     ;; Multiple tags.
     (with-temp-buffer (org-mode) (insert "* Heading")
       (goto-char (point-min)) (org-set-tags '("a" "b")) (buffer-string))
     ;; Remove tags.
     (with-temp-buffer (org-mode) (insert "* Heading :tag:")
       (goto-char (point-min)) (org-set-tags nil) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-get-repeat combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_get_repeat_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"+1w\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; With repeater.
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2023-10-13 Fri +1w>")
       (goto-char (point-min)) (forward-line 1) (org-get-repeat))
     ;; No repeater.
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2023-10-13 Fri>")
       (goto-char (point-min)) (forward-line 1) (org-get-repeat)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-timestamp-has-time-p combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_timestamp_has_time_p_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; With time.
     (with-temp-buffer (org-mode) (insert "<2023-10-13 Fri 14:30>")
       (goto-char (point-min)) (org-at-timestamp-p 'lax) (org-timestamp-has-time-p))
     ;; Without time.
     (with-temp-buffer (org-mode) (insert "<2023-10-13 Fri>")
       (goto-char (point-min)) (org-at-timestamp-p 'lax) (org-timestamp-has-time-p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-at-timestamp-p combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_at_timestamp_p_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bracket bracket nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Active timestamp.
     (with-temp-buffer (org-mode) (insert "<2023-10-13 Fri>")
       (goto-char (point-min)) (org-at-timestamp-p 'lax))
     ;; Inactive timestamp.
     (with-temp-buffer (org-mode) (insert "[2023-10-13 Fri]")
       (goto-char (point-min)) (org-at-timestamp-p 'lax))
     ;; Not at timestamp.
     (with-temp-buffer (org-mode) (insert "Not a timestamp")
       (goto-char (point-min)) (org-at-timestamp-p 'lax)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Exa: org-element with all org-get-category combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn exa_all_get_category_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Work\" \"???\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; From keyword.
     (with-temp-buffer (org-mode) (insert "#+CATEGORY: Work\n* Heading")
       (goto-char (point-min)) (org-get-category))
     ;; Default.
     (with-temp-buffer (org-mode) (insert "* Heading")
       (goto-char (point-min)) (org-get-category)))))"##,
        expect,
    );
}
