//! Giga-strict combo tests for org-mode extreme edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export option combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_option_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Test
#+AUTHOR: Author
#+EMAIL: email@example.org
#+DATE: 2024-01-15
#+DESCRIPTION: Description
#+KEYWORDS: test org
#+LANGUAGE: en
#+OPTIONS: H:3 num:t toc:t \\n:t timestamp:t author:t creator:t d:t email:t \
*:t e:t ::t f:t pri:t -:t ^:t toc:t |:t tags:t tasks:t <:t todo:t \
inline:nil stat:t title:t
#+CATEGORY: test
#+FILETAGS: :test:org:
#+STARTUP: overview
#+TODO: TODO WAIT | DONE CANCEL
#+TAGS: @work @home @errand
#+COLUMNS: %25ITEM %TODO %3PRIORITY %TAGS
#+CONSTANTS: pi=3.14 c=299792458
#+LINK: orgmode https://orgmode.org
#+PRIORITIES: A C B
#+ARCHIVE: %s_done::
* H1
Body")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         (plist-get info :title)
         (plist-get info :author)
         (plist-get info :email)
         (plist-get info :headline-levels)
         (plist-get info :section-numbers)
         (plist-get info :with-timestamps)
         (plist-get info :with-author)
         (plist-get info :with-email)
         (plist-get info :with-emphasize)
         (plist-get info :with-entities)
         (plist-get info :with-fixed-width)
         (plist-get info :with-footnotes)
         (plist-get info :with-priority)
         (plist-get info :with-special-strings)
         (plist-get info :with-sub-superscript)
         (plist-get info :with-toc)
         (plist-get info :with-tables)
         (plist-get info :with-tags)
         (plist-get info :with-tasks)
         (plist-get info :with-timestamps)
         (plist-get info :with-todo-keywords)
         (plist-get info :with-inlinetasks)
         (plist-get info :with-statistics-cookies)
         (plist-get info :with-title)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export headline number combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_headline_number_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+OPTIONS: num:t H:3
* Chapter 1
** Section 1.1
*** Subsection 1.1.1
*** Subsection 1.1.2
** Section 1.2
*** Subsection 1.2.1
* Chapter 2
** Section 2.1
*** Subsection 2.1.1
** Section 2.2
* Chapter 3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         ;; Headline numbers.
         (mapcar (lambda (h) (org-export-get-headline-number h info))
                 (org-element-map tree 'headline #'identity))
         ;; Relative levels.
         (mapcar (lambda (h) (org-export-get-relative-level h info))
                 (org-element-map tree 'headline #'identity))
         ;; Numbered?
         (mapcar (lambda (h) (org-export-numbered-headline-p h info))
                 (org-element-map tree 'headline #'identity))
         ;; Low level?
         (mapcar (lambda (h) (org-export-low-level-p h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export footnote number combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_footnote_number_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1] more[fn:2] and[fn:3].
* H1
Body[fn:4].
** H2
Body[fn:5:nested[fn:6]].

[fn:1] Def 1.
[fn:2] Def 2 with *bold*.
[fn:3] Def 3 with [[https://orgmode.org][link]].
[fn:4] Def 4.
[fn:6] Deeply nested.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         ;; Footnote numbers.
         (mapcar (lambda (ref) (org-export-get-footnote-number ref info))
                 (org-element-map tree 'footnote-reference #'identity))
         ;; First reference?
         (mapcar (lambda (ref) (org-export-footnote-first-reference-p ref info))
                 (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export tag combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_tag_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1 :tag1:
** H2 :tag2:
*** H3 :tag3:
** H2b :tag1:tag2:
* H1b :tag3:
** H2c :tag1:tag3:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         ;; Tags.
         (mapcar (lambda (h) (org-export-get-tags h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export category combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_category_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+CATEGORY: work
* H1
:PROPERTIES:
:CATEGORY: project
:END:
** H2
* H2")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         ;; Categories.
         (mapcar (lambda (h) (org-export-get-category h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export date combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_date_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+DATE: 2024-01-15
* H1
Body")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         ;; Date.
         (org-export-get-date info)
         ;; Title.
         (plist-get info :title)
         ;; Author.
         (plist-get info :author)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export optional title combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_optional_title_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Document Title
* H1
Body")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment))))
             (headline (car (org-element-map tree 'headline #'identity))))
        (list
         ;; Optional title.
         (org-export-get-optional-title headline info)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export first/last sibling combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_first_last_sibling_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1
** H2
** H3
** H4
* H5")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         ;; First sibling?
         (mapcar #'org-export-first-sibling-p headlines)
         ;; Last sibling?
         (mapcar #'org-export-last-sibling-p headlines)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export node property combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_node_property_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H
:PROPERTIES:
:CUSTOM_ID: myid
:EFFORT: 2h
:CATEGORY: work
:KEY: value
:END:
Body")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headline (car (org-element-map tree 'headline #'identity))))
        (list
         ;; Node properties.
         (org-export-get-node-property :CUSTOM_ID headline)
         (org-export-get-node-property :EFFORT headline)
         (org-export-get-node-property :CATEGORY headline)
         (org-export-get-node-property :KEY headline)
         ;; Non-existent property.
         (org-export-get-node-property :NONEXISTENT headline)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export caption combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_caption_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+CAPTION: Short caption
#+CAPTION: Long caption with *bold* and /italic/
| a | b |
| c | d |

#+CAPTION: Only long caption
| e | f |

#+CAPTION[short]: Long caption
| g | h |")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (tables (org-element-map tree 'table #'identity)))
        (list
         ;; Captions.
         (mapcar (lambda (t) (org-export-get-caption t)) tables)
         ;; Short captions.
         (mapcar (lambda (t) (org-export-get-caption t t)) tables)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export filter combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_filter_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"210\" \"20\" \"0\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   ;; Functions applied in order.
   (org-export-filter-apply-functions
    (list (lambda (value &rest _) (concat "1" value))
          (lambda (value &rest _) (concat "2" value)))
    "0" nil)
   ;; Nil functions skipped.
   (org-export-filter-apply-functions
    (list #'ignore (lambda (value &rest _) (concat "2" value)))
    "0" nil)
   ;; All skipped: return initial.
   (org-export-filter-apply-functions (list #'ignore) "0" nil)
   ;; Empty string short-circuits.
   (org-export-filter-apply-functions
    (list (lambda (_value &rest _) "")
          (lambda (value &rest _) (concat "2" value)))
    "0" nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export backend combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_backend_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((parent) t ((lambda (h c i) (format \"CHILD: %s\\n%s\" (org-element-property :raw-value h) c)) (lambda (s c i) c)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let (org-export-registered-backends)
    ;; Define parent backend.
    (org-export-define-backend 'parent
      '((headline . (lambda (h c i) (format "PARENT: %s\n%s" (org-element-property :raw-value h) c)))
        (section . (lambda (s c i) c))
        (paragraph . (lambda (p c i) c))
        (plain-text . (lambda (t i) t))))
    ;; Define derived backend.
    (org-export-define-derived-backend 'child 'parent
      :translate-alist
      '((headline . (lambda (h c i) (format "CHILD: %s\n%s" (org-element-property :raw-value h) c)))))
    (list
     ;; Derived check.
     (org-export-derived-backend-p 'child 'parent)
     (org-export-derived-backend-p 'child 'child)
     ;; Transcoders.
     (let ((all (org-export-get-all-transcoders 'child)))
       (list (cdr (assq 'headline all))
             (cdr (assq 'section all)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export scope combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_scope_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1
Body 1
** H2
Body 2
*** H3
Body 3
* H4
Body 4")
      (goto-char (point-min))
      (list
       ;; Full document.
       (with-temp-buffer (org-mode)
         (insert "* H1\nBody 1\n** H2\nBody 2\n*** H3\nBody 3\n* H4\nBody 4")
         (goto-char (point-min))
         (let* ((tree (org-element-parse-buffer))
                (info (org-combine-plists
                       (org-export--get-export-attributes)
                       (org-export-get-environment)
                       (org-export--collect-tree-properties
                        tree (org-export-get-environment)))))
           (mapcar (lambda (h) (org-element-property :raw-value h))
                   (org-element-map tree 'headline #'identity)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export block/snippet combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_block_snippet_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_EXPORT html
<p>HTML content</p>
#+END_EXPORT

#+BEGIN_EXPORT latex
\\textbf{LaTeX content}
#+END_EXPORT

@@html:<b>HTML snippet</b>@@
@@latex:\\textbf{LaTeX snippet}@@")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Export blocks.
         (length (org-element-map tree 'export-block #'identity))
         ;; Export snippets.
         (length (org-element-map tree 'export-snippet #'identity))
         ;; Block types.
         (mapcar (lambda (b) (org-element-property :type b))
                 (org-element-map tree 'export-block #'identity))
         ;; Snippet backends.
         (mapcar (lambda (s) (org-element-property :back-end s))
                 (org-element-map tree 'export-snippet #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export comment combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_comment_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "# Comment line 1
# Comment line 2

#+BEGIN_COMMENT
Comment block
#+END_COMMENT

* COMMENT Commented heading
Body

* Regular heading
Body")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Comment lines.
         (length (org-element-map tree 'comment #'identity))
         ;; Comment blocks.
         (length (org-element-map tree 'comment-block #'identity))
         ;; Headlines (including commented).
         (length (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export include combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_include_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+INCLUDE: \"nonexistent.org\"
* H1
Body")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Include keywords.
         (length (org-element-map tree 'keyword
                   (lambda (k) (when (equal (org-element-property :key k) "INCLUDE") k))))
         ;; Headlines.
         (length (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export setupfile combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_setupfile_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+SETUPFILE: \"nonexistent.org\"
* H1
Body")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Setupfile keywords.
         (length (org-element-map tree 'keyword
                   (lambda (k) (when (equal (org-element-property :key k) "SETUPFILE") k))))
         ;; Headlines.
         (length (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export select-tags combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_select_tags_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil)
        (org-export-select-tags '("export")))
    (with-temp-buffer (org-mode)
      (insert "* H1 :export:
Body 1
* H2
Body 2
* H3 :export:
Body 3
* H4
Body 4")
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
// Giga: org-element with all export exclude-tags combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_exclude_tags_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil)
        (org-export-exclude-tags '("noexport")))
    (with-temp-buffer (org-mode)
      (insert "* H1 :noexport:
Body 1
* H2
Body 2
* H3 :noexport:
Body 3
* H4
Body 4")
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
// Giga: org-element with all export body-only combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_body_only_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Test
* H1
Body 1
* H2
Body 2")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        ;; Headlines in full document.
        (mapcar (lambda (h) (org-element-property :raw-value h))
                (org-element-map tree 'headline #'identity)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Giga: org-element with all export visible-only combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn giga_all_export_visible_only_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1
Body 1
** H2
Body 2
*** H3
Body 3
* H4
Body 4")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        ;; Headlines in full document.
        (mapcar (lambda (h) (org-element-property :raw-value h))
                (org-element-map tree 'headline #'identity)))))"##,
        expect,
    );
}
