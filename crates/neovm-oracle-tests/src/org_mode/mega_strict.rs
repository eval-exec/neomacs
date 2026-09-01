//! Mega-strict combo tests for org-mode extreme edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all inline markup combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_inline_markup_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "*bold* /italic/ _underline_ =verbatim= ~code~ +strike+
*bold with /italic/ inside*
/italic with *bold* inside/
_underline with *bold* inside*
=verbatim with *bold* inside=
~code with *bold* inside~
+strike with *bold* inside+
*bold with _underline_ and /italic/ and =verbatim= and ~code~ and +strike+")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (length (org-element-map tree 'underline #'identity))
         (length (org-element-map tree 'verbatim #'identity))
         (length (org-element-map tree 'code #'identity))
         (length (org-element-map tree 'strike-through #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all link type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_link_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "https://example.org
http://example.org
ftp://example.org
mailto:user@example.org
<https://angular.org>
[[https://example.org]]
[[https://example.org][description]]
[[file:path.org]]
[[file:path.org::*heading]]
[[file:path.org::#custom-id]]
[[id:uuid]]
[[#custom-id]]
[[*heading]]
[[(coderef)]]
[[attachment:file.txt]]
[[elisp:(+ 1 2)]]
[[shell:ls -la]]")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (links (org-element-map tree 'link #'identity)))
        (list
         (length links)
         (mapcar (lambda (l) (org-element-property :type l)) links)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all timestamp type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_timestamp_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<2024-01-15 Mon>
<2024-01-15 Mon 14:30>
<2024-01-15 Mon 14:30-15:30>
<2024-01-15 Mon>--<2024-01-16 Tue>
<2024-01-15 Mon 14:30>--<2024-01-16 Tue 15:30>
<2024-01-15 Mon +1w>
<2024-01-15 Mon +1w -3d>
<2024-01-15 Mon 14:30 +1w -3d>
[2024-01-15 Mon]
[2024-01-15 Mon 14:30]
[2024-01-15 Mon 14:30-15:30]
[2024-01-15 Mon]--[2024-01-16 Tue]
<%%(diary-float t 4 2)>
<%%(diary-float t 4 2) 14:30>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (timestamps (org-element-map tree 'timestamp #'identity)))
        (list
         (length timestamps)
         (mapcar (lambda (ts) (org-element-property :type ts)) timestamps)
         (mapcar (lambda (ts) (org-element-property :range-type ts)) timestamps)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all block type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_block_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_CENTER
Centered
#+END_CENTER

#+BEGIN_QUOTE
Quoted
#+END_QUOTE

#+BEGIN_EXAMPLE
Example
#+END_EXAMPLE

#+BEGIN_EXPORT html
HTML
#+END_EXPORT

#+BEGIN_EXPORT latex
LaTeX
#+END_EXPORT

#+BEGIN_VERSE
Verse
#+END_VERSE

#+BEGIN_COMMENT
Comment
#+END_COMMENT

#+BEGIN_SRC emacs-lisp
(+ 1 2)
#+END_SRC

#+BEGIN_SRC python
print('hello')
#+END_SRC

#+BEGIN_SOME_SPECIAL
Special
#+END_SOME_SPECIAL")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'center-block #'identity))
         (length (org-element-map tree 'quote-block #'identity))
         (length (org-element-map tree 'example-block #'identity))
         (length (org-element-map tree 'export-block #'identity))
         (length (org-element-map tree 'verse-block #'identity))
         (length (org-element-map tree 'comment-block #'identity))
         (length (org-element-map tree 'src-block #'identity))
         (length (org-element-map tree 'special-block #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all list type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_list_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- Unordered item 1
- Unordered item 2
  - Nested unordered
  - Another nested
- Unordered item 3

1. Ordered item 1
2. Ordered item 2
   1. Nested ordered
   2. Another nested
3. Ordered item 3

1) Ordered with paren
2) Another paren

- tag1 :: Description 1
- tag2 :: Description 2
  - Nested in description

- [ ] Unchecked
- [X] Checked
- [-] Partial
- No checkbox")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (lists (org-element-map tree 'plain-list #'identity))
             (items (org-element-map tree 'item #'identity)))
        (list
         (length lists)
         (length items)
         (mapcar (lambda (l) (org-element-property :type l)) lists)
         (mapcar (lambda (i) (org-element-property :checkbox i)) items)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all headline feature combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_headline_feature_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Simple headline
* TODO Headline with todo
* DONE Headline with done
* [#A] Headline with priority
* TODO [#B] Headline with todo and priority
* Headline :tag:
* Headline :tag1:tag2:
* TODO [#C] Headline with all :tag1:tag2:
* COMMENT Commented headline
* TODO [#A] COMMENT Commented with todo :tag:
* [/] Headline with statistics
* [50%] Headline with percent statistics
* TODO [#B] Full featured :work:urgent: [/]")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         (length headlines)
         (mapcar (lambda (h) (org-element-property :todo-keyword h)) headlines)
         (mapcar (lambda (h) (org-element-property :priority h)) headlines)
         (mapcar (lambda (h) (org-element-property :tags h)) headlines)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all planning combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_planning_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Task 1
DEADLINE: <2024-01-15 Mon>

* Task 2
SCHEDULED: <2024-01-15 Mon>

* Task 3
CLOSED: [2024-01-14 Sun]

* Task 4
DEADLINE: <2024-01-15 Mon> SCHEDULED: <2024-01-14 Sun>

* Task 5
SCHEDULED: <2024-01-14 Sun> DEADLINE: <2024-01-15 Mon> CLOSED: [2024-01-13 Sat]

* Task 6
DEADLINE: <2024-01-15 Mon +1w -3d>

* Task 7
SCHEDULED: <2024-01-14 Sun +1w>

* Task 8
DEADLINE: <2024-01-15 Mon 14:30>

* Task 9
SCHEDULED: <2024-01-14 Sun 09:00-10:00>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (planning (org-element-map tree 'planning #'identity)))
        (list
         (length planning)
         (mapcar (lambda (p) (list (org-element-property :scheduled p)
                             (org-element-property :deadline p)
                             (org-element-property :closed p)))
                 planning)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all property drawer combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_property_drawer_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1
:PROPERTIES:
:EMPTY:
:KEY: value
:KEY_NUM: 42
:KEY_BOOL: t
:CUSTOM_ID: myid
:EFFORT: 2:30
:CATEGORY: work
:END:

* H2
:PROPERTIES:
:KEY: overwritten
:KEY+: extended
:END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (drawers (org-element-map tree 'property-drawer #'identity)))
        (list
         (length drawers)
         ;; Node properties.
         (length (org-element-map tree 'node-property #'identity))
         ;; Property keys.
         (mapcar (lambda (p) (org-element-property :key p))
                 (org-element-map tree 'node-property #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all drawer combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_drawer_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H
:PROPERTIES:
:KEY: val
:END:
:LOGBOOK:
CLOCK: [2024-01-15 Mon 09:00]--[2024-01-15 Mon 10:00] =>  1:00
- Note taken on [2024-01-15 Mon 10:00] \\
  Some note
:END:
:CUSTOM_DRAWER:
Custom content
:END:
Body text.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'property-drawer #'identity))
         (length (org-element-map tree 'drawer #'identity))
         (mapcar (lambda (d) (org-element-property :drawer-name d))
                 (org-element-map tree 'drawer #'identity))
         (length (org-element-map tree 'clock #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all dynamic block combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_dynamic_block_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN: clocktable :scope file :maxlevel 2
#+END:

#+BEGIN: myblock :param1 val1 :param2 val2
Content
#+END:

#+BEGIN: another-block
More content
#+END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (blocks (org-element-map tree 'dynamic-block #'identity)))
        (list
         (length blocks)
         (mapcar (lambda (b) (org-element-property :block-name b)) blocks)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all footnote type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_footnote_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Standard[fn:1] inline[fn:2:def] anonymous[fn::anon] named[fn:label].
* H
Section[fn:3].

[fn:1] Standard definition.
[fn:3] Section definition with *bold*.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'footnote-reference #'identity))
         (length (org-element-map tree 'footnote-definition #'identity))
         (mapcar (lambda (ref) (org-element-property :type ref))
                 (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all entity type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_entity_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-entities)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\alpha \\beta \\gamma \\delta \\epsilon
\\Alpha \\Beta \\Gamma \\Delta \\Epsilon
\\Agrave \\Aacute \\Acirc \\Atilde \\Auml \\Aring
\\agrave \\aacute \\acirc \\atilde \\auml \\aring
\\uuml \\Uuml \\oelig \\OElig \\aelig \\AElig
\\ss \\copyright \\pounds \\yen \\deg \\micro
\\times \\div \\pm \\not \\le \\ge \\approx \\equiv
\\rightarrow \\leftarrow \\Rightarrow \\Leftarrow
\\infty \\nabla \\partial \\sum \\prod \\int
\\forall \\exists \\in \\notin \\subset \\supset
\\cup \\cap \\emptyset \\neg \\land \\lor \\oplus")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'entity #'identity))
         ;; First 10 names.
         (mapcar (lambda (e) (org-element-property :name e))
                 (take 10 (org-element-map tree 'entity #'identity)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all LaTeX fragment type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_latex_fragment_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Inline: $x^2$ and $E=mc^2$ and $\\alpha + \\beta$.
Display: $$\\int_0^1 f(x)dx$$ and $$\\sum_{i=1}^n i$$.
Paren: \\(x^2\\) and \\[E=mc^2\\].
Command: \\command{} and \\emph{text}.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'latex-fragment #'identity))
         ;; Fragment values.
         (mapcar (lambda (f) (org-element-property :value f))
                 (org-element-map tree 'latex-fragment #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all LaTeX environment type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_latex_environment_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\\begin{equation}
x^2 + y^2 = z^2
\\end{equation}

\\begin{align}
x &= 1 \\\\
y &= 2
\\end{align}

\\begin{eqnarray}
a &=& b \\\\
c &=& d
\\end{eqnarray}

\\begin{theorem}
Important theorem.
\\end{theorem}")

      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'latex-environment #'identity))
         ;; Environment values.
         (mapcar (lambda (e) (org-element-property :value e))
                 (org-element-map tree 'latex-environment #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all macro type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_macro_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+MACRO: name World
#+MACRO: greet Hello
#+MACRO: greeting {{{greet}}} {{{name}}}

{{{name}}}.
{{{greet}}}.
{{{greeting}}}.
{{{greet(Beautiful)}}}.
{{{greet}}} {{{name}}}!")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'macro #'identity))
         (mapcar (lambda (m) (org-element-property :value m))
                 (org-element-map tree 'macro #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all export snippet type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_export_snippet_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "@@html:<b>bold</b>@@
@@html:<i>italic</i>@@
@@html:<p>paragraph</p>@@
@@latex:\\textbf{bold}@@
@@latex:\\textit{italic}@@
@@latex:\\emph{emphasis}@@
@@ascii:plain text@@
@@texinfo:@code{code}@@
@@mybackend:custom content@@")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'export-snippet #'identity))
         (mapcar (lambda (s) (org-element-property :back-end s))
                 (org-element-map tree 'export-snippet #'identity))
         (mapcar (lambda (s) (org-element-property :value s))
                 (org-element-map tree 'export-snippet #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all radio target type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_radio_target_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<<<simple>>>
<<<with spaces>>>
<<<with \\alpha entity>>>
<<<with *bold*>>>
<<<with /italic/>>>
<<<multi word target>>>
<<<CamelCaseTarget>>>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'radio-target #'identity))
         (mapcar #'org-element-type
                 (org-element-map tree 'radio-target #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all target type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_target_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "<<simple>>
<<with spaces>>
<<multi word target>>
<<CamelCaseTarget>>
<<target-with-dashes>>
<<target_with_underscores>>")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'target #'identity))
         (mapcar #'org-element-type
                 (org-element-map tree 'target #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all statistics cookie type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_statistics_cookie_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H [0/0]
* H [1/2]
* H [5/10]
* H [0%]
* H [50%]
* H [100%]
* H [/]
* H [%]")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'statistics-cookie #'identity))
         (mapcar (lambda (c) (org-element-property :value c))
                 (org-element-map tree 'statistics-cookie #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all inlinetask type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_inlinetask_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-inlinetask)
  (let ((org-mode-hook nil)
        (org-inlinetask-min-level 15))
    (with-temp-buffer (org-mode)
      (insert "* Regular
*************** Simple inline
Body
*************** END

*************** TODO Inline with todo
Body
*************** END

*************** DONE Inline with done
Body
*************** END

*************** [#A] Inline with priority
Body
*************** END

*************** TODO [#B] Inline with todo and priority :tag:
Body
*************** END")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'inlinetask #'identity))
         (mapcar (lambda (t) (list (org-element-property :todo-keyword t)
                             (org-element-property :priority t)
                             (org-element-property :tags t)))
                 (org-element-map tree 'inlinetask #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all clock type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_clock_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Task
:LOGBOOK:
CLOCK: [2024-01-15 Mon 09:00]--[2024-01-15 Mon 10:00] =>  1:00
CLOCK: [2024-01-15 Mon 11:00]--[2024-01-15 Mon 12:30] =>  1:30
CLOCK: [2024-01-14 Sun 14:00]--[2024-01-14 Sun 16:00] =>  2:00
CLOCK: [2024-01-13 Sat 10:00]--[2024-01-13 Sat 11:00] =>  1:00
:END:

* Running
CLOCK: [2024-01-15 Mon 13:00]")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (clocks (org-element-map tree 'clock #'identity)))
        (list
         (length clocks)
         (mapcar (lambda (c) (org-element-property :status c)) clocks)
         (mapcar (lambda (c) (org-element-property :duration c)) clocks)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all diary sexp type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_diary_sexp_type_combinations() {
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
%%(diary-anniversary 1956 5 14) Birthday
%%(diary-block 1 1 2024 12 31 2024) Year block
%%(diary-float 0 1 1) First Monday")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'diary-sexp #'identity))
         (mapcar #'org-element-type
                 (org-element-map tree 'diary-sexp #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all horizontal rule type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_horizontal_rule_type_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Above

-----

Below

--------

End

-----------

Final")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'horizontal-rule #'identity))
         (mapcar #'org-element-type
                 (org-element-map tree 'horizontal-rule #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mega: org-element with all line break type combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn mega_all_line_break_type_combinations() {
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

Normal paragraph.

Line 4\\\\
Line 5\\\\
Line 6\\\\
Line 7")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'line-break #'identity))
         (mapcar #'org-element-type
                 (org-element-map tree 'line-break #'identity))))))"##,
        expect,
    );
}
