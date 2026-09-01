//! More strict combo tests covering advanced org-mode scenarios.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex table structures
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_table_with_formulas_and_alignment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| Name   | Q1 | Q2 | Q3 | Q4 | Total |
|--------+----+----+----+----+-------|
| Alice  | 10 | 20 | 30 | 40 |       |
| Bob    | 15 | 25 | 35 | 45 |       |
|--------+----+----+----+----+-------|
| Sum    |    |    |    |    |       |
#+TBLFM: $6=$2+$3+$4+$5
#+TBLFM: @4$2=vsum(@I..@-1)
#+TBLFM: @4$3=vsum(@I..@-1)
#+TBLFM: @4$4=vsum(@I..@-1)
#+TBLFM: @4$5=vsum(@I..@-1)
#+TBLFM: @4$6=vsum(@I..@-1)")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (table (org-element-map tree 'table #'identity nil t)))
        (list
         ;; Table type.
         (org-element-property :type table)
         ;; Rows.
         (length (org-element-map tree 'table-row #'identity))
         ;; Cells in first data row.
         (length (org-element-map
                 (nth 1 (org-element-map tree 'table-row #'identity))
                 'table-cell #'identity))
         ;; TBLFM lines.
         (length (org-element-map tree 'keyword
                   (lambda (k) (when (equal (org-element-property :key k) "TBLFM") k))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex list nesting
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_list_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- Top 1
  - Sub 1.1
    - Sub-sub 1.1.1
    - Sub-sub 1.1.2
  - Sub 1.2
    - Sub-sub 1.2.1
- Top 2
  1. Ordered 2.1
  2. Ordered 2.2
  3. Ordered 2.3
- Top 3
  - tag :: description
  - tag2 :: description2
- Top 4
  - [ ] Task 1
  - [X] Task 2
  - [-] Task 3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Total items.
         (length (org-element-map tree 'item #'identity))
         ;; Total lists.
         (length (org-element-map tree 'plain-list #'identity))
         ;; List types.
         (mapcar (lambda (l) (org-element-property :type l))
                 (org-element-map tree 'plain-list #'identity))
         ;; Checkbox states.
         (mapcar (lambda (i) (org-element-property :checkbox i))
                 (org-element-map tree 'item #'identity))
         ;; Description tags.
         (mapcar (lambda (i)
                   (when (org-element-property :tag i)
                     (substring-no-properties
                      (org-element-interpret-data (org-element-property :tag i)))))
                 (org-element-map tree 'item #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex link types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_link_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "https://example.org plain link
[[https://example.org][explicit link]]
[[file:path/to/file.org][file link]]
[[file:path/to/file.org::*heading][file heading link]]
[[id:uuid-1234][id link]]
[[#custom-id][custom-id link]]
[[*heading][star link]]
[[(code-ref)][coderef link]]
<https://angular.org>
mailto:user@example.org
[[attachment:file.txt][attachment link]]")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (links (org-element-map tree 'link #'identity)))
        (list
         ;; Number of links.
         (length links)
         ;; Link types.
         (mapcar (lambda (l) (org-element-property :type l)) links)
         ;; Link paths (first 5).
         (mapcar (lambda (l) (org-element-property :path l))
                 (take 5 links))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex citation formats
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_citation_formats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Simple [cite:@key1].
Multiple [cite:@a;@b;@c].
With style [cite/style:@key].
With prefix [cite:common-prefix;@key].
With suffix [cite:@key;common-suffix].
Complex [cite:pre @a;@b; post].
Nested [cite:@outer; inner @ref].")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Citations.
         (length (org-element-map tree 'citation #'identity))
         ;; References.
         (length (org-element-map tree 'citation-reference #'identity))
         ;; Styles.
         (mapcar (lambda (c) (org-element-property :style c))
                 (org-element-map tree 'citation #'identity))
         ;; Keys.
         (mapcar (lambda (r) (org-element-property :key r))
                 (org-element-map tree 'citation-reference #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex footnote nesting
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_footnote_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1] more[fn:2:inline] and[fn::anon].
* H1
Body[fn:3].
** H2
Body[fn:4:nested[fn:5]].

[fn:1] Standard def with *bold*.
[fn:3] In section def with [[https://orgmode.org][link]].
[fn:5] Deeply nested.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         ;; References.
         (length (org-element-map tree 'footnote-reference #'identity))
         ;; Definitions.
         (length (org-element-map tree 'footnote-definition #'identity))
         ;; Reference types.
         (mapcar (lambda (ref) (org-element-property :type ref))
                 (org-element-map tree 'footnote-reference #'identity))
         ;; Numbers.
         (mapcar (lambda (ref) (org-export-get-footnote-number ref info))
                 (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex timestamp scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_timestamp_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (7 (active inactive inactive active-range active-range inactive diary) (nil nil nil timerange daterange nil nil) ((nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil)) ((all 3 day) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO Weekly review
SCHEDULED: <2024-01-15 Mon +1w>
DEADLINE: <2024-01-19 Fri -3d>
CLOSED: [2024-01-14 Sun 10:30]
:PROPERTIES:
:LAST_REPEAT: [2024-01-08 Mon]
:END:

* Meeting
<2024-01-20 Sat 14:00-15:30>

* Deadline only
DEADLINE: <2024-01-22 Mon>

* Date range
<2024-01-23 Tue>--<2024-01-25 Thu>

* Inactive
[2024-01-26 Fri 09:00]

* Diary
<%%(diary-float t 4 2)>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (timestamps (org-element-map tree 'timestamp #'identity)))
        (list
         ;; Count.
         (length timestamps)
         ;; Types.
         (mapcar (lambda (ts) (org-element-property :type ts)) timestamps)
         ;; Range types.
         (mapcar (lambda (ts) (org-element-property :range-type ts)) timestamps)
         ;; Repeaters.
         (mapcar (lambda (ts) (list (org-element-property :repeater-type ts)
                              (org-element-property :repeater-value ts)
                              (org-element-property :repeater-unit ts)))
                 timestamps)
         ;; Warnings.
         (mapcar (lambda (ts) (list (org-element-property :warning-type ts)
                              (org-element-property :warning-value ts)
                              (org-element-property :warning-unit ts)))
                 timestamps))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex block nesting
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_block_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_CENTER
#+BEGIN_QUOTE
#+BEGIN_SRC emacs-lisp
(+ 1 2)
#+END_SRC
#+END_QUOTE
#+END_CENTER

#+BEGIN_EXAMPLE
Example text
#+END_EXAMPLE

#+BEGIN_EXPORT html
<p>HTML content</p>
#+END_EXPORT

#+BEGIN_VERSE
Verse line 1
Verse line 2
#+END_VERSE

#+BEGIN_COMMENT
Comment block
#+END_COMMENT")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Block types.
         (mapcar #'org-element-type
                 (org-element-map tree
                   '(center-block quote-block src-block example-block
                     export-block verse-block comment-block)
                   #'identity))
         ;; Nested depth: source block inside center+quote.
         (let ((src (car (org-element-map tree 'src-block #'identity))))
           (mapcar #'org-element-type
                   (org-element-lineage src nil t)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex property inheritance
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_property_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (4 3 (1 2 3 4) (\"p\") (\"c\") (\"gc\") (\"ggc\") (1 \"p\" 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  ;; Build a 4-level hierarchy with properties at each level.
  (let* ((great-grandchild (org-element-create 'great-grandchild '(:shared 4 :own-ggc "ggc")))
         (grandchild (org-element-create 'grandchild '(:shared 3 :own-gc "gc") great-grandchild))
         (child (org-element-create 'child '(:shared 2 :own-c "c") grandchild))
         (parent (org-element-create 'parent '(:shared 1 :own-p "p") child)))
    (list
     ;; At great-grandchild: own value wins.
     (org-element-property-inherited :shared great-grandchild 'with-self)
     ;; Without self: get parent's.
     (org-element-property-inherited :shared great-grandchild)
     ;; Accumulate all.
     (org-element-property-inherited :shared great-grandchild 'with-self 'accumulate)
     ;; Only parent has :own-p.
     (org-element-property-inherited :own-p great-grandchild 'with-self 'accumulate)
     ;; Only child has :own-c.
     (org-element-property-inherited :own-c great-grandchild 'with-self 'accumulate)
     ;; Only grandchild has :own-gc.
     (org-element-property-inherited :own-gc great-grandchild 'with-self 'accumulate)
     ;; Only great-grandchild has :own-ggc.
     (org-element-property-inherited :own-ggc great-grandchild 'with-self 'accumulate)
     ;; PROPERTY as list.
     (org-element-property-inherited
      '(:shared :own-p) great-grandchild nil 'accumulate))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex export scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_export_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Complex Document
#+AUTHOR: Test Author
#+OPTIONS: H:3 num:t toc:t
#+FILETAGS: :test:

* Chapter 1 :ch1:
** Section 1.1 :s11:
Content with *bold* and /italic/.
*** Subsection 1.1.1
More content.
** Section 1.2 :s12:
Content with [[https://orgmode.org][link]].

* Chapter 2 :ch2:
** Section 2.1
Content with [fn:1].

[fn:1] Footnote definition.")
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
         (mapcar (lambda (h) (org-export-get-headline-number h info))
                 (org-element-map tree 'headline #'identity))
         ;; Relative levels.
         (mapcar (lambda (h) (org-export-get-relative-level h info))
                 (org-element-map tree 'headline #'identity))
         ;; Numbered?
         (mapcar (lambda (h) (org-export-numbered-headline-p h info))
                 (org-element-map tree 'headline #'identity))
         ;; Tags.
         (mapcar (lambda (h) (org-export-get-tags h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex agenda scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_agenda_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TODO\" \"DONE\" nil \"TODO\" nil \"TODO\") (65 66 67 65 nil 66) (((timestamp (:standard-properties [36 nil nil nil 52 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2024-01-15 Mon>\" :year-start 2024 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2024 :month-end 1 :day-end 15 :hour-end nil :minute-end nil)) nil) (nil nil) ((timestamp (:standard-properties [170 nil nil nil 186 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2024-01-20 Sat>\" :year-start 2024 :month-start 1 :day-start 20 :hour-start nil :minute-start nil :year-end 2024 :month-end 1 :day-end 20 :hour-end nil :minute-end nil)) nil) (nil (timestamp (:standard-properties [225 nil nil nil 241 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2024-01-22 Mon>\" :year-start 2024 :month-start 1 :day-start 22 :hour-start nil :minute-start nil :year-end 2024 :month-end 1 :day-end 22 :hour-end nil :minute-end nil))) nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-agenda)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] Urgent task
SCHEDULED: <2024-01-15 Mon>
DEADLINE: <2024-01-19 Fri>

* DONE [#B] Completed task
CLOSED: [2024-01-14 Sun]

* WAIT [#C] Waiting task
SCHEDULED: <2024-01-20 Sat>

* TODO [#A] Another urgent
DEADLINE: <2024-01-22 Mon>

* Normal task without planning
* TODO [#B] Low priority todo")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         ;; All TODO keywords.
         (mapcar (lambda (h) (org-element-property :todo-keyword h)) headlines)
         ;; All priorities.
         (mapcar (lambda (h) (org-element-property :priority h)) headlines)
         ;; Planning info.
         (mapcar (lambda (h)
                   (let ((planning (org-element-map h 'planning #'identity nil t)))
                     (when planning
                       (list (org-element-property :scheduled planning)
                             (org-element-property :deadline planning)))))
                 headlines))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex clock scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_clock_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (5 (closed closed closed closed running) (\"1:30\" \"1:00\" \"2:00\" \"1:30\" nil) ((2024 1 15 9 0) (2024 1 15 11 0) (2024 1 14 14 0) (2024 1 13 10 0) (2024 1 15 13 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Task 1
:LOGBOOK:
CLOCK: [2024-01-15 Mon 09:00]--[2024-01-15 Mon 10:30] =>  1:30
CLOCK: [2024-01-15 Mon 11:00]--[2024-01-15 Mon 12:00] =>  1:00
CLOCK: [2024-01-14 Sun 14:00]--[2024-01-14 Sun 16:00] =>  2:00
:END:

* Task 2
:LOGBOOK:
CLOCK: [2024-01-13 Sat 10:00]--[2024-01-13 Sat 11:30] =>  1:30
:END:

* Running clock
CLOCK: [2024-01-15 Mon 13:00]")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (clocks (org-element-map tree 'clock #'identity)))
        (list
         ;; Total clocks.
         (length clocks)
         ;; Clock statuses.
         (mapcar (lambda (c) (org-element-property :status c)) clocks)
         ;; Clock durations.
         (mapcar (lambda (c) (org-element-property :duration c)) clocks)
         ;; Clock timestamps.
         (mapcar (lambda (c)
                   (let ((ts (org-element-property :value c)))
                     (list (org-element-property :year-start ts)
                           (org-element-property :month-start ts)
                           (org-element-property :day-start ts)
                           (org-element-property :hour-start ts)
                           (org-element-property :minute-start ts))))
                 clocks))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex drawer scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_drawer_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H
:PROPERTIES:
:CUSTOM_ID: myid
:EFFORT: 2h
:CATEGORY: work
:END:
:LOGBOOK:
CLOCK: [2024-01-15 Mon 09:00]--[2024-01-15 Mon 10:00] =>  1:00
- Note taken on [2024-01-15 Mon 10:00] \\
  Some note
:END:
:MYDRAWER:
Custom content
:END:
Body text.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Property drawer.
         (length (org-element-map tree 'property-drawer #'identity))
         ;; Regular drawers.
         (length (org-element-map tree 'drawer #'identity))
         ;; Drawer names.
         (mapcar (lambda (d) (org-element-property :drawer-name d))
                 (org-element-map tree 'drawer #'identity))
         ;; Clocks inside drawers.
         (length (org-element-map tree 'clock #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex dynamic block scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_dynamic_block_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN: clocktable :scope file :maxlevel 2 :block today
#+END:

* Task
:LOGBOOK:
CLOCK: [2024-01-15 Mon 09:00]--[2024-01-15 Mon 10:00] =>  1:00
:END:

#+BEGIN: myblock :param1 val1 :param2 val2
Content
#+END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Dynamic blocks.
         (length (org-element-map tree 'dynamic-block #'identity))
         ;; Block names.
         (mapcar (lambda (b) (org-element-property :block-name b))
                 (org-element-map tree 'dynamic-block #'identity))
         ;; Clocks.
         (length (org-element-map tree 'clock #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex LaTeX scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_latex_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Entity: \\alpha, \\beta, \\gamma, \\Omega.

Inline LaTeX: $x^2 + y^2 = z^2$ and $E = mc^2$.

Display LaTeX: $$\\int_0^1 f(x) dx$$ and $$\\sum_{i=1}^n i$$.

Environment:
\\begin{equation}
\\label{eq:1}
a^2 + b^2 = c^2
\\end{equation}

Environment 2:
\\begin{align}
x &= 1 \\\\
y &= 2
\\end{align}")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Entities.
         (length (org-element-map tree 'entity #'identity))
         ;; Entity names.
         (mapcar (lambda (e) (org-element-property :name e))
                 (org-element-map tree 'entity #'identity))
         ;; LaTeX fragments.
         (length (org-element-map tree 'latex-fragment #'identity))
         ;; LaTeX environments.
         (length (org-element-map tree 'latex-environment #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex macro scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_macro_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+MACRO: greet Hello
#+MACRO: name World
#+MACRO: greeting {{{greet}}} {{{name}}}

{{{greeting}}}.
{{{greet(Beautiful)}}} {{{name}}}.
{{{greet}}} {{{name}}}!")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Macros found.
         (length (org-element-map tree 'macro #'identity))
         ;; Macro values.
         (mapcar (lambda (m) (org-element-property :value m))
                 (org-element-map tree 'macro #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex entity scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_entity_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-entities)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\alpha \\beta \\gamma \\delta \\epsilon \\zeta \\eta \\theta
\\iota \\kappa \\lambda \\mu \\nu \\xi \\pi \\rho \\sigma \\tau
\\upsilon \\phi \\chi \\psi \\omega
\\Alpha \\Beta \\Gamma \\Delta \\Epsilon \\Zeta \\Eta \\Theta
\\Iota \\Kappa \\Lambda \\Mu \\Nu \\Xi \\Pi \\Rho \\Sigma \\Tau
\\Upsilon \\Phi \\Chi \\Psi \\Omega")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Entities found.
         (length (org-element-map tree 'entity #'identity))
         ;; First 5 names.
         (mapcar (lambda (e) (org-element-property :name e))
                 (take 5 (org-element-map tree 'entity #'identity)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex radio target scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_radio_target_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<<<radio1>>> and <<<radio2>>> and <<<radio3>>>.
<<<radio with \\alpha entity>>>
<<<radio with *bold*>>>

See radio1, radio2, radio3.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Radio targets.
         (length (org-element-map tree 'radio-target #'identity))
         ;; Types.
         (mapcar #'org-element-type
                 (org-element-map tree 'radio-target #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex statistics cookie scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_statistics_cookie_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Project [1/3]
** Task 1
** Task 2
** Task 3

* Progress [50%]
** Done
** Todo

* Mixed [2/4]
** A
** B
** C
** D")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Statistics cookies.
         (length (org-element-map tree 'statistics-cookie #'identity))
         ;; Cookie values.
         (mapcar (lambda (c) (org-element-property :value c))
                 (org-element-map tree 'statistics-cookie #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex inlinetask scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_inlinetask_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (let ((org-mode-hook nil)
        (org-inlinetask-min-level 15))
    (with-temp-buffer (org-mode)
      (insert "* Regular heading
*************** TODO Inline task 1 :tag1:
Body of inline task 1
*************** END

*************** DONE Inline task 2 :tag2:
Body of inline task 2
*************** END

* Another heading
*************** WAIT Inline task 3
Body
*************** END")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Headlines.
         (length (org-element-map tree 'headline #'identity))
         ;; Inlinetasks.
         (length (org-element-map tree 'inlinetask #'identity))
         ;; Inlinetask properties.
         (mapcar (lambda (task)
                   (list (org-element-property :todo-keyword task)
                         (org-element-property :tags task)))
                 (org-element-map tree 'inlinetask #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex export snippet scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_export_snippet_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "HTML: @@html:<b>bold</b>@@ and @@html:<i>italic</i>@@.
LaTeX: @@latex:\\textbf{bold}@@ and @@latex:\\textit{italic}@@.
Ascii: @@ascii:plain text@@.
Custom: @@mybackend:custom content@@.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Export snippets.
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
// Advanced: org-element with complex diary sexp scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_diary_sexp_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "%%(org-anniversary 1956 5 14) Arthur Dent is %d years old
%%(diary-float t 4 2) Pick up laundry
%%(diary-cyclic 1 1 1 2020) Daily task
%%(org-agenda-skip-entry-if 'todo 'done)")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Diary sexps.
         (length (org-element-map tree 'diary-sexp #'identity))
         ;; Types.
         (mapcar #'org-element-type
                 (org-element-map tree 'diary-sexp #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex horizontal rule scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_horizontal_rule_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Above

-----

Middle

--------

Below

-----------

End")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Horizontal rules.
         (length (org-element-map tree 'horizontal-rule #'identity))
         ;; Types.
         (mapcar #'org-element-type
                 (org-element-map tree 'horizontal-rule #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced: org-element with complex line break scenarios
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn advanced_complex_line_break_scenarios() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Line 1\\\\
Line 2\\\\
Line 3

No break here.

Line 4\\\\
Line 5\\\\
Line 6\\\\
Line 7")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Line breaks.
         (length (org-element-map tree 'line-break #'identity))
         ;; Types.
         (mapcar #'org-element-type
                 (org-element-map tree 'line-break #'identity))))))"##,
        expect,
    );
}
