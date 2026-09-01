//! More strict combo tests covering remaining org subsystems.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-parse-buffer with visible-only
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_parse_visible_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n** H3 :visible:\n** H4\n** H5 :visible:")
      (goto-char (point-min))
      (org-occur ":visible:")
      (org-element-map (org-element-parse-buffer nil t) 'headline
        (lambda (hl) (org-element-property :raw-value hl)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-normalize-contents edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_normalize_contents_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((paragraph nil \"One \" (emphasis nil \"space\") \"\\n Two spaces\") (paragraph nil (verbatim nil \"V\") \"No space\\n  Two\\n   Three\") (paragraph nil \"Two spaces\\n\\n\\nTwo spaces\") (paragraph nil \"1 space\" (line-break) \" 2 spaces\") (verse-block nil \"line 1\\n\\nline 2\") (paragraph nil \"No space\\nTwo spaces\\n Three spaces\") (paragraph nil \" Two spaces \" (bold nil \" and\\nOne space\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; With objects.
   (org-element-normalize-contents
    '(paragraph nil " One " (emphasis nil "space") "\n  Two spaces"))
   ;; Object at start: no common indent.
   (org-element-normalize-contents
    '(paragraph nil (verbatim nil "V") "No space\n  Two\n   Three"))
   ;; Blank lines.
   (org-element-normalize-contents
    '(paragraph nil "  Two spaces\n\n \n  Two spaces"))
   ;; With line break.
   (org-element-normalize-contents
    '(paragraph nil " 1 space" (line-break) "  2 spaces"))
   ;; Verse block.
   (org-element-normalize-contents
    '(verse-block nil "  line 1\n\n  line 2"))
   ;; With argument: ignore first line indent.
   (org-element-normalize-contents
    '(paragraph nil "No space\n  Two spaces\n   Three spaces") t)
   ;; Recursive objects.
   (org-element-normalize-contents
    '(paragraph nil "  Two spaces " (bold nil " and\n One space")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-secondary-p edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_secondary_p_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:title :foo nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let ((org-mode-hook nil))
    (list
     ;; In title secondary string.
     (with-temp-buffer (org-mode) (insert "* Headline *object*")
       (goto-char (point-min))
       (org-element-map (org-element-parse-buffer) 'bold
         (lambda (object) (org-element-secondary-p object))
         nil t))
     ;; Manual construction.
     (org-element-secondary-p
      (let* ((el (org-element-create 'dummy '(:secondary (:foo))))
             (child (org-element-create "string" `(:parent ,el))))
        (org-element-put-property el :foo (list child))
        child))
     ;; Outside secondary string.
     (with-temp-buffer (org-mode) (insert "Paragraph *object*")
       (goto-char (point-min))
       (org-element-map (org-element-parse-buffer) 'bold
         (lambda (object) (org-element-type (org-element-secondary-p object)))
         nil t))
     ;; Wrong property.
     (eq :foo
         (org-element-secondary-p
          (let* ((el (org-element-create 'dummy '(:secondary (:foo))))
                 (child (org-element-create "string" `(:parent ,el))))
            (org-element-put-property el :bar (list child))
            child))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-at-point with inlinetasks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_at_point_with_inlinetasks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (headline inlinetask paragraph inlinetask headline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (let ((org-mode-hook nil)
        (org-inlinetask-min-level 15))
    (with-temp-buffer (org-mode)
      (insert "* Regular heading\n*************** Inline task\nInline body\n*************** END\n* Another heading")
      (goto-char (point-min))
      (list
       ;; On regular heading.
       (org-element-type (org-element-at-point))
       ;; On inlinetask.
       (progn (forward-line 1) (org-element-type (org-element-at-point)))
       ;; Inside inlinetask body.
       (progn (forward-line 1) (org-element-type (org-element-at-point)))
       ;; On END.
       (progn (forward-line 1) (org-element-type (org-element-at-point)))
       ;; On next regular heading.
       (progn (forward-line 1) (org-element-type (org-element-at-point)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with folded content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_with_folding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"H1\" \"H2\" \"H3\" \"H4\") 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\nBody 1\n** H2\nBody 2\n*** H3\nBody 3\n* H4\nBody 4")
      (goto-char (point-min))
      (org-overview)
      (list
       ;; All headlines still found when folded.
       (mapcar (lambda (h) (org-element-property :raw-value h))
               (org-element-map (org-element-parse-buffer) 'headline #'identity))
       ;; Sections still found.
       (length (org-element-map (org-element-parse-buffer) 'section #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with narrow buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"H2\") 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\nBody 1\n* H2\nBody 2\n* H3\nBody 3")
      (goto-char (point-min))
      (forward-line 2)
      (narrow-to-region (point) (progn (forward-line 2) (point)))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Only visible headlines.
         (mapcar (lambda (h) (org-element-property :raw-value h))
                 (org-element-map tree 'headline #'identity))
         ;; Paragraphs in narrowed region.
         (length (org-element-map tree 'paragraph #'identity)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: export with exclude tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_export_exclude_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil)
        (org-export-exclude-tags '("noexport")))
    (with-temp-buffer (org-mode)
      (insert "* H1 :noexport:\nBody 1\n* H2\nBody 2\n* H3 :noexport:\nBody 3\n* H4\nBody 4")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        ;; Only non-excluded headlines.
        (mapcar (lambda (h) (org-element-property :raw-value h))
                (org-element-map tree 'headline #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: export with select tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_export_select_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil)
        (org-export-select-tags '("export")))
    (with-temp-buffer (org-mode)
      (insert "* H1 :export:\nBody 1\n* H2\nBody 2\n* H3 :export:\nBody 3\n* H4\nBody 4")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        ;; Only selected headlines.
        (mapcar (lambda (h) (org-element-property :raw-value h))
                (org-element-map tree 'headline #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with CJK content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_with_cjk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* 日本語見出し\n本文の段落です。\n** 中文标题\n这是一个段落。\n* 한국어 제목\n한국어 단락입니다.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Headlines with CJK.
         (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                 (org-element-map tree 'headline #'identity))
         ;; Paragraphs with CJK.
         (length (org-element-map tree 'paragraph #'identity))
         ;; Interpret round-trip.
         (substring-no-properties (org-element-interpret-data tree))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with special characters
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Title with \\alpha and \\beta entities\nPara with $x^2$ and \\[E=mc^2\\].\n\n#+BEGIN_SRC emacs-lisp\n;; Comment with special chars: <>&\"'\n(+ 1 2)\n#+END_SRC")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Entities.
         (length (org-element-map tree 'entity #'identity))
         ;; LaTeX fragments.
         (length (org-element-map tree 'latex-fragment #'identity))
         ;; LaTeX environments.
         (length (org-element-map tree 'latex-environment #'identity))
         ;; Source block value preserved.
         (org-element-property :value
           (car (org-element-map tree 'src-block #'identity)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with mixed list types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_mixed_list_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- Unordered 1\n- Unordered 2\n\n1. Ordered 1\n2. Ordered 2\n\n- tag :: description\n- tag2 :: desc2")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (lists (org-element-map tree 'plain-list #'identity)))
        (list
         ;; Number of lists.
         (length lists)
         ;; List types.
         (mapcar (lambda (l) (org-element-property :type l)) lists)
         ;; Total items.
         (length (org-element-map tree 'item #'identity))
         ;; Description items have tags.
         (mapcar (lambda (item)
                   (when (org-element-property :tag item)
                     (substring-no-properties
                      (org-element-interpret-data (org-element-property :tag item)))))
                 (org-element-map tree 'item #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with checkboxes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_checkboxes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- [ ] Unchecked\n- [X] Checked\n- [-] Partial\n- No checkbox")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (items (org-element-map tree 'item #'identity)))
        (mapcar (lambda (item) (org-element-property :checkbox item)) items))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with statistics cookies
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_statistics_cookies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H [1/2]\n** S1\n** S2\n* H2 [33%]\n** A\n** B\n** C")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Statistics cookies found.
         (length (org-element-map tree 'statistics-cookie #'identity))
         ;; Cookie types.
         (mapcar (lambda (c) (org-element-property :value c))
                 (org-element-map tree 'statistics-cookie #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with radio targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_radio_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<<<radio>>> and <<<radio \\alpha>>>.\n\nSee radio.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Radio targets found.
         (length (org-element-map tree 'radio-target #'identity))
         ;; Target types.
         (mapcar #'org-element-type
                 (org-element-map tree 'radio-target #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with macros
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+MACRO: greet Hello\n\n{{{greet}}} World.\n\n{{{greet(Beautiful)}}}.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Macros found.
         (length (org-element-map tree 'macro #'identity))
         ;; Macro names.
         (mapcar (lambda (m) (org-element-property :value m))
                 (org-element-map tree 'macro #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with export snippets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_export_snippets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text @@html:<b>bold</b>@@ and @@latex:\\textbf{bold}@@.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Export snippets found.
         (length (org-element-map tree 'export-snippet #'identity))
         ;; Backends.
         (mapcar (lambda (s) (org-element-property :back-end s))
                 (org-element-map tree 'export-snippet #'identity))
         ;; Values.
         (mapcar (lambda (s) (org-element-property :value s))
                 (org-element-map tree 'export-snippet #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with inline tasks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_inlinetasks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (let ((org-mode-hook nil)
        (org-inlinetask-min-level 15))
    (with-temp-buffer (org-mode)
      (insert "* Regular\n*************** TODO Inline task :tag:\nBody\n*************** END\n* Another")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Headlines (including inlinetask).
         (length (org-element-map tree 'headline #'identity))
         ;; Inlinetasks specifically.
         (length (org-element-map tree 'inlinetask #'identity))
         ;; Inlinetask properties.
         (let ((task (car (org-element-map tree 'inlinetask #'identity))))
           (list (org-element-property :todo-keyword task)
                 (org-element-property :tags task)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with drawers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_drawers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H\n:PROPERTIES:\n:KEY: val\n:END:\n:LOGBOOK:\nNote\n:END:\n:MYDRAWER:\nContent\n:END:\nBody.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Property drawers.
         (length (org-element-map tree 'property-drawer #'identity))
         ;; Regular drawers.
         (length (org-element-map tree 'drawer #'identity))
         ;; Drawer names.
         (mapcar (lambda (d) (org-element-property :drawer-name d))
                 (org-element-map tree 'drawer #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with dynamic blocks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_dynamic_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN: clocktable :scope file :maxlevel 2\n#+END:\n\n#+BEGIN: myblock :param val\nContent\n#+END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Dynamic blocks found.
         (length (org-element-map tree 'dynamic-block #'identity))
         ;; Block names.
         (mapcar (lambda (b) (org-element-property :block-name b))
                 (org-element-map tree 'dynamic-block #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with footnotes in various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_footnotes_various_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1] and [fn:2:inline def].\n\n[fn:1] Standard.\n\n* H\nBody[fn:3].\n\n[fn:3] In section.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; References.
         (length (org-element-map tree 'footnote-reference #'identity))
         ;; Definitions.
         (length (org-element-map tree 'footnote-definition #'identity))
         ;; Reference types.
         (mapcar (lambda (ref) (org-element-property :type ref))
                 (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with timestamps of all types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_timestamps_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2024-01-15 Mon>\n[2024-01-15 Mon]\n<2024-01-15 Mon 14:30>\n<2024-01-15 Mon>--<2024-01-16 Tue>\n<2024-01-15 Mon 14:30-15:30>\n<2024-01-15 Mon +1w>\n<2024-01-15 Mon -3d>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (timestamps (org-element-map tree 'timestamp #'identity)))
        (list
         ;; Count.
         (length timestamps)
         ;; Types.
         (mapcar (lambda (ts) (org-element-property :type ts)) timestamps)
         ;; Range types.
         (mapcar (lambda (ts) (org-element-property :range-type ts)) timestamps)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with diary sexps
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_diary_sexps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "%%(org-anniversary 1956 5 14) Arthur Dent is %d years old\n%%(diary-float t 4 2)")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Diary sexps found.
         (length (org-element-map tree 'diary-sexp #'identity))
         ;; Types.
         (mapcar #'org-element-type
                 (org-element-map tree 'diary-sexp #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with horizontal rules
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_horizontal_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Above\n\n-----\n\nBelow\n\n--------\n\nEnd")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Horizontal rules found.
         (length (org-element-map tree 'horizontal-rule #'identity))
         ;; Types.
         (mapcar #'org-element-type
                 (org-element-map tree 'horizontal-rule #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element with line breaks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_line_breaks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Line 1\\\\\nLine 2\\\\\nLine 3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Line breaks found.
         (length (org-element-map tree 'line-break #'identity))
         ;; Types.
         (mapcar #'org-element-type
                 (org-element-map tree 'line-break #'identity))))))"##,
        expect,
    );
}
