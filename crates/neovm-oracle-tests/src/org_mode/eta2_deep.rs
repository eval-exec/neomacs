//! Eta-2 strict combo tests for org-mode extreme edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex document parsing (all element types)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_full_document_all_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (5 6 13 2 1 1 1 1 1 3 1 1 1 1 1 1 1 1 4 6 2 5 2 1 0 2 4 1 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Full Document
#+AUTHOR: Test
#+OPTIONS: H:3 num:t toc:t
#+FILETAGS: :test:

* TODO [#A] Chapter 1 :ch1:
SCHEDULED: <2024-01-15 Mon +1w>
DEADLINE: <2024-01-19 Fri -3d>
:PROPERTIES:
:CUSTOM_ID: ch1
:EFFORT: 2h
:END:
:LOGBOOK:
CLOCK: [2024-01-15 Mon 09:00]--[2024-01-15 Mon 10:00] =>  1:00
:END:

Paragraph with *bold*, /italic/, _underline_, =verbatim=, ~code~, +strike+.

Also [[https://orgmode.org][link]], [cite:@key1;@key2], [fn:1], and \\alpha.

| Name | Value |
|------+-------|
| A    |     1 |
| B    |     2 |
#+TBLFM: @3$2=vsum(@1$2..@2$2)

#+BEGIN_QUOTE
Quoted text.
#+END_QUOTE

#+BEGIN_SRC emacs-lisp
(+ 1 2)
#+END_SRC

** DONE Section 1.1 :s11:
CLOSED: [2024-01-16 Wed 10:00]

- [ ] Task 1
- [X] Task 2
  - [ ] Sub-task 2.1
  - [X] Sub-task 2.2
- [ ] Task 3

** TODO Section 1.2
<<target>> See [[#ch1][Chapter 1]].

*** Subsection 1.2.1
#+BEGIN_CENTER
Centered text.
#+END_CENTER

* WAIT Chapter 2 :ch2:
#+BEGIN_COMMENT
Under development.
#+END_COMMENT

[fn:1] Footnote with *bold* and [[https://orgmode.org][link]].")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         (length (org-element-map tree 'headline #'identity))
         (length (org-element-map tree 'section #'identity))
         (length (org-element-map tree 'paragraph #'identity))
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (length (org-element-map tree 'underline #'identity))
         (length (org-element-map tree 'verbatim #'identity))
         (length (org-element-map tree 'code #'identity))
         (length (org-element-map tree 'strike-through #'identity))
         (length (org-element-map tree 'link #'identity))
         (length (org-element-map tree 'citation #'identity))
         (length (org-element-map tree 'footnote-reference #'identity))
         (length (org-element-map tree 'footnote-definition #'identity))
         (length (org-element-map tree 'quote-block #'identity))
         (length (org-element-map tree 'src-block #'identity))
         (length (org-element-map tree 'center-block #'identity))
         (length (org-element-map tree 'comment-block #'identity))
         (length (org-element-map tree 'table #'identity))
         (length (org-element-map tree 'table-row #'identity))
         (length (org-element-map tree 'table-cell #'identity))
         (length (org-element-map tree 'plain-list #'identity))
         (length (org-element-map tree 'item #'identity))
         (length (org-element-map tree 'planning #'identity))
         (length (org-element-map tree 'clock #'identity))
         (length (org-element-map tree 'property-drawer #'identity))
         (length (org-element-map tree 'drawer #'identity))
         (length (org-element-map tree 'keyword #'identity))
         (length (org-element-map tree 'entity #'identity))
         (length (org-element-map tree 'target #'identity)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex export round-trip (all features)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_roundtrip_all_features() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Round Trip Test
* H1
Paragraph with *bold* and /italic/.
** H2
| a | b |
| c | d |
* H3
- Item 1
- Item 2
#+BEGIN_SRC emacs-lisp
(+ 1 2)
#+END_SRC")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         (substring-no-properties (org-export-data tree info))
         (mapcar (lambda (h) (org-export-get-headline-number h info))
                 (org-element-map tree 'headline #'identity))
         (mapcar (lambda (h) (org-export-get-relative-level h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex property inheritance (4 levels)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_property_inheritance_4_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 3 (1 2 3 4) (\"a\") (\"b\") (\"c\") (\"d\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((level4 (org-element-create 'level4 '(:shared 4 :own4 "d")))
         (level3 (org-element-create 'level3 '(:shared 3 :own3 "c") level4))
         (level2 (org-element-create 'level2 '(:shared 2 :own2 "b") level3))
         (level1 (org-element-create 'level1 '(:shared 1 :own1 "a") level2)))
    (list
     (org-element-property-inherited :shared level4 'with-self)
     (org-element-property-inherited :shared level4)
     (org-element-property-inherited :shared level4 'with-self 'accumulate)
     (org-element-property-inherited :own1 level4 'with-self 'accumulate)
     (org-element-property-inherited :own2 level4 'with-self 'accumulate)
     (org-element-property-inherited :own3 level4 'with-self 'accumulate)
     (org-element-property-inherited :own4 level4 'with-self 'accumulate))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex element operations chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_element_operations_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((doc (org-element-create 'org-data nil))
         (h1 (org-element-create
              'headline '(:level 1 :raw-value "A" :title ("A"))
              (org-element-create 'section nil (org-element-create 'paragraph nil "P1.\n"))))
         (h2 (org-element-create
              'headline '(:level 1 :raw-value "B" :title ("B"))
              (org-element-create 'section nil (org-element-create 'paragraph nil "P2.\n"))))
         (h3 (org-element-create
              'headline '(:level 1 :raw-value "C" :title ("C"))
              (org-element-create 'section nil (org-element-create 'paragraph nil "P3.\n")))))
    (org-element-adopt doc h1 h2 h3)
    (let ((after-adopt (org-element-interpret-data doc)))
      (org-element-extract h2)
      (let ((after-extract (org-element-interpret-data doc)))
        (org-element-swap-A-B h1 h3)
        (let ((after-swap (org-element-interpret-data doc)))
          (let* ((sec (car (org-element-contents h1)))
                 (para (car (org-element-contents sec))))
            (org-element-set para (org-element-create 'paragraph nil "New.\n")))
          (list (substring-no-properties after-adopt)
                (substring-no-properties after-extract)
                (substring-no-properties after-swap)
                (substring-no-properties (org-element-interpret-data doc))
                (org-element-property :parent h2))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex deferred chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_deferred_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (let ((el (org-element-create
              'dummy
              `(:deferred ,(org-element-deferred-create
                            t (lambda (el) (org-element-put-property el :foo 'bar) nil))))))
     (list (org-element-property :foo el) (org-element-property :foo2 el)))
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create nil (lambda (_) 'bar))))))
     (org-element-property :foo el))
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create t (lambda (_) 'bar))))))
     (list (org-element-property :foo el) (org-element-property-raw :foo el)))
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create nil (lambda (_) 'bar))))))
     (list (org-element-property :foo el)
           (org-element-property-raw :foo el)
           (org-element-property :foo el nil 'force)
           (org-element-property-raw :foo el)))
   (let ((el (org-element-create
              'dummy `( :foo 1 :bar ,(org-element-deferred-create-alias :foo)))))
     (list (org-element-property :foo el) (org-element-property :bar el)))
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create-list
                              (list 1 2 (org-element-deferred-create nil (lambda _) 3)))))))
     (org-element-property :foo el))
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create
                              nil (lambda (el)
                                    (org-element-put-property el :foo 1)
                                    (throw :org-element-deferred-retry nil)))))))
     (org-element-property :foo el))
   (let ((el (org-element-create
              'dummy `(:foo ,(org-element-deferred-create
                              nil (lambda (el)
                                    (org-element-deferred-create
                                     nil (lambda (_) 1)))))))
     (org-element-property :foo el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex parse-and-interpret round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_parse_interpret_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"*text*\\n\" 1 5 (:parent (bold (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"text\" 0 4 (:parent #3))))) #(\"/text/\\n\" 1 5 (:parent (italic (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"text\" 0 4 (:parent #3))))) \"~text~\\n\" \"=text=\\n\" #(\"_text_\\n\" 1 5 (:parent (underline (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"text\" 0 4 (:parent #3))))) #(\"+target+\\n\" 1 7 (:parent (strike-through (:standard-properties [1 nil 2 8 9 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 9 9 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 9 9 0 nil first-section nil nil nil 1 9 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 9 9 0 nil org-data nil nil nil 3 9 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"target\" 0 6 (:parent #3))))) #(\"a_b\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (subscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p nil) #(\"b\" 0 1 (:parent #4))))) 2 3 (:parent (subscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p nil) #(\"b\" 0 1 (:parent #3))))) #(\"a_{b}\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (subscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p t) #(\"b\" 0 1 (:parent #4))))) 3 4 (:parent (subscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p t) #(\"b\" 0 1 (:parent #3))))) #(\"a^b\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (superscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p nil) #(\"b\" 0 1 (:parent #4))))) 2 3 (:parent (superscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p nil) #(\"b\" 0 1 (:parent #3))))) #(\"a^{b}\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (superscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p t) #(\"b\" 0 1 (:parent #4))))) 3 4 (:parent (superscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p t) #(\"b\" 0 1 (:parent #3))))) #(\"\\\\alpha text\\n\" 7 11 (:parent (paragraph (:standard-properties [1 1 1 12 12 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 12 12 0 nil first-section nil nil nil 1 12 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 12 12 0 nil org-data nil nil nil 3 12 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) (entity (:standard-properties [1 nil nil nil 8 1 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :name \"alpha\" :latex \"\\\\alpha\" :latex-math-p t :html \"&alpha;\" :ascii \"alpha\" :latin1 \"alpha\" :utf-8 \"α\" :use-brackets-p nil)) #(\"text\" 0 4 (:parent #3))))) #(\"\\\\alpha{}text\\n\" 8 12 (:parent (paragraph (:standard-properties [1 1 1 13 13 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 13 13 0 nil first-section nil nil nil 1 13 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 13 13 0 nil org-data nil nil nil 3 13 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) (entity (:standard-properties [1 nil nil nil 9 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :name \"alpha\" :latex \"\\\\alpha\" :latex-math-p t :html \"&alpha;\" :ascii \"alpha\" :latin1 \"alpha\" :utf-8 \"α\" :use-brackets-p t)) #(\"text\" 0 4 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-test-parse-and-interpret
         (lambda (text)
           (with-temp-buffer
             (org-mode) (insert text)
             (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall org-test-parse-and-interpret "*text*")
     (funcall org-test-parse-and-interpret "/text/")
     (funcall org-test-parse-and-interpret "~text~")
     (funcall org-test-parse-and-interpret "=text=")
     (funcall org-test-parse-and-interpret "_text_")
     (funcall org-test-parse-and-interpret "+target+")
     (funcall org-test-parse-and-interpret "a_b")
     (funcall org-test-parse-and-interpret "a_{b}")
     (funcall org-test-parse-and-interpret "a^b")
     (funcall org-test-parse-and-interpret "a^{b}")
     (funcall org-test-parse-and-interpret "\\alpha text")
     (funcall org-test-parse-and-interpret "\\alpha{}text"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex link round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_link_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[[https://orgmode.org]]\\n\" #(\"[[https://orgmode.org][Org mode]]\\n\" 23 31 (:parent (link (:standard-properties [1 nil 24 32 34 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 34 34 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 34 34 0 nil first-section nil nil nil 1 34 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 34 34 0 nil org-data nil nil nil 3 34 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)] :type \"https\" :type-explicit-p t :path \"//orgmode.org\" :format bracket :raw-link \"https://orgmode.org\" :application nil :search-option nil) #(\"Org mode\" 0 8 (:parent #3))))) \"[[file:todo.org::*task]]\\n\" \"[[id:aaaa]]\\n\" \"[[#id]]\\n\" \"https://orgmode.org\\n\" \"<https://orgmode.org>\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-test-parse-and-interpret
         (lambda (text)
           (with-temp-buffer
             (org-mode) (insert text)
             (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall org-test-parse-and-interpret "[[https://orgmode.org]]")
     (funcall org-test-parse-and-interpret "[[https://orgmode.org][Org mode]]")
     (funcall org-test-parse-and-interpret "[[file:todo.org::*task]]")
     (funcall org-test-parse-and-interpret "[[id:aaaa]]")
     (funcall org-test-parse-and-interpret "[[#id]]")
     (funcall org-test-parse-and-interpret "https://orgmode.org")
     (funcall org-test-parse-and-interpret "<https://orgmode.org>"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex footnote round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_footnote_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"Text[fn:1]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 11 11 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 11 11 0 nil first-section nil nil nil 1 11 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 11 11 0 nil org-data nil nil nil 3 11 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil nil nil 11 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label \"1\" :type standard))))) #(\"Text[fn:label]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 15 15 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 15 15 0 nil first-section nil nil nil 1 15 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 15 15 0 nil org-data nil nil nil 3 15 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil nil nil 15 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label \"label\" :type standard))))) #(\"Text[fn:label:def]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 19 19 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 19 19 0 nil first-section nil nil nil 1 19 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 19 19 0 nil org-data nil nil nil 3 19 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil 15 18 19 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label \"label\" :type inline) #(\"def\" 0 3 (:parent #4))))) 14 17 (:parent (footnote-reference (:standard-properties [5 nil 15 18 19 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 19 19 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 19 19 0 nil first-section nil nil nil 1 19 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 19 19 0 nil org-data nil nil nil 3 19 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"Text\" 0 4 (:parent #6)) #3)] :label \"label\" :type inline) #(\"def\" 0 3 (:parent #3))))) #(\"Text[fn::def]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 14 14 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 14 14 0 nil first-section nil nil nil 1 14 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 14 14 0 nil org-data nil nil nil 3 14 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil 10 13 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label nil :type inline) #(\"def\" 0 3 (:parent #4))))) 9 12 (:parent (footnote-reference (:standard-properties [5 nil 10 13 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 14 14 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 14 14 0 nil first-section nil nil nil 1 14 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 14 14 0 nil org-data nil nil nil 3 14 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"Text\" 0 4 (:parent #6)) #3)] :label nil :type inline) #(\"def\" 0 3 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-test-parse-and-interpret
         (lambda (text)
           (with-temp-buffer
             (org-mode) (insert text)
             (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall org-test-parse-and-interpret "Text[fn:1]")
     (funcall org-test-parse-and-interpret "Text[fn:label]")
     (funcall org-test-parse-and-interpret "Text[fn:label:def]")
     (funcall org-test-parse-and-interpret "Text[fn::def]"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex block round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_block_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (#(\"#+begin_center\\nText\\n#+end_center\\n\" 15 20 (:parent (paragraph (:standard-properties [16 16 16 21 21 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (center-block (:standard-properties [1 1 16 21 33 0 nil top-comment nil nil nil 16 21 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 33 33 0 nil first-section nil nil nil 1 33 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 33 33 0 nil org-data nil nil nil 3 33 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"Text\\n\" 0 5 (:parent #3))))) #(\"#+begin_quote\\nText\\n#+end_quote\\n\" 14 19 (:parent (paragraph (:standard-properties [15 15 15 20 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (quote-block (:standard-properties [1 1 15 20 31 0 nil top-comment nil nil nil 15 20 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 31 31 0 nil first-section nil nil nil 1 31 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 31 31 0 nil org-data nil nil nil 3 31 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"Text\\n\" 0 5 (:parent #3))))) \"#+begin_example\\nTest\\n#+end_example\\n\" \"#+begin_export HTML\\n<p>Text</p>\\n#+end_export\\n\" #(\"#+begin_verse\\nTest\\n#+end_verse\\n\" 14 19 (:parent (verse-block (:standard-properties [1 1 15 20 31 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 31 31 0 nil first-section nil nil nil 1 31 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 31 31 0 nil org-data nil nil nil 3 31 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Test\\n\" 0 5 (:parent #3))))))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-src-preserve-indentation t)
        (org-test-parse-and-interpret
         (lambda (text)
           (with-temp-buffer
             (org-mode) (insert text)
             (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall org-test-parse-and-interpret "#+BEGIN_CENTER\nText\n#+END_CENTER")
     (funcall org-test-parse-and-interpret "#+BEGIN_QUOTE\nText\n#+END_QUOTE")
     (funcall org-test-parse-and-interpret "#+BEGIN_EXAMPLE\nTest\n#+END_EXAMPLE")
     (funcall org-test-parse-and-interpret "#+BEGIN_EXPORT HTML\n<p>Text</p>\n#+END_EXPORT")
     (funcall org-test-parse-and-interpret "#+BEGIN_VERSE\nTest\n#+END_VERSE"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex inline round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_inline_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"call_test()\\n\" \"call_test(x=2)\\n\" \"src_emacs-lisp{(+ 1 1)}\\n\" \"@@backend:contents@@\\n\" \"\\\\command{}\\n\" \"$x$\\n\" \"$$x+y$$\\n\" \"\\\\(x+y\\\\)\\n\" \"\\\\[x+y\\\\]\\n\" \"[0/1]\\n\" \"[66%]\\n\" #(\"First line \\\\\\\\\\nSecond line\\n\" 0 11 (:parent (paragraph (:standard-properties [1 1 1 26 26 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 26 26 0 nil first-section nil nil nil 1 26 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 26 26 0 nil org-data nil nil nil 3 26 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"First line \" 0 11 (:parent #3)) (line-break (:standard-properties [12 nil nil nil 15 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3])) #(\"Second line\" 0 11 (:parent #3)))) 14 25 (:parent (paragraph (:standard-properties [1 1 1 26 26 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 26 26 0 nil first-section nil nil nil 1 26 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 26 26 0 nil org-data nil nil nil 3 26 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"First line \" 0 11 (:parent #3)) (line-break (:standard-properties [12 nil nil nil 15 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3])) #(\"Second line\" 0 11 (:parent #3))))) \"<<target>>\\n\" #(\"<<<some text>>>\\n\" 3 12 (:parent (radio-target (:standard-properties [1 nil 4 13 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 16 16 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 16 16 0 nil first-section nil nil nil 1 16 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 16 16 0 nil org-data nil nil nil 3 16 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)] :value \"some text\") #(\"some text\" 0 9 (:parent #3))))) \"{{{test}}}\\n\" \"{{{test(arg1,arg2)}}}\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-test-parse-and-interpret
         (lambda (text)
           (with-temp-buffer
             (org-mode) (insert text)
             (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall org-test-parse-and-interpret "call_test()")
     (funcall org-test-parse-and-interpret "call_test(x=2)")
     (funcall org-test-parse-and-interpret "src_emacs-lisp{(+ 1 1)}")
     (funcall org-test-parse-and-interpret "@@backend:contents@@")
     (funcall org-test-parse-and-interpret "\\command{}")
     (funcall org-test-parse-and-interpret "$x$")
     (funcall org-test-parse-and-interpret "$$x+y$$")
     (funcall org-test-parse-and-interpret "\\(x+y\\)")
     (funcall org-test-parse-and-interpret "\\[x+y\\]")
     (funcall org-test-parse-and-interpret "[0/1]")
     (funcall org-test-parse-and-interpret "[66%]")
     (funcall org-test-parse-and-interpret "First line \\\\\nSecond line")
     (funcall org-test-parse-and-interpret "<<target>>")
     (funcall org-test-parse-and-interpret "<<<some text>>>")
     (funcall org-test-parse-and-interpret "{{{test}}}")
     (funcall org-test-parse-and-interpret "{{{test(arg1,arg2)}}}"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex table round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_table_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"| a | b |\\n| c | d |\\n\" 2 3 (:parent (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) #3 (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"b\" 0 1 (:parent #7))))]) #(\"a\" 0 1 (:parent #3)))) 6 7 (:parent (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"a\" 0 1 (:parent #7))) #3)]) #(\"b\" 0 1 (:parent #3)))) 12 13 (:parent (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) #6)] :type standard) #3 (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"d\" 0 1 (:parent #7))))]) #(\"c\" 0 1 (:parent #3)))) 16 17 (:parent (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) #6)] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"c\" 0 1 (:parent #7))) #3)]) #(\"d\" 0 1 (:parent #3))))) #(\"| a | b |\\n|---+---|\\n| c | d |\\n\" 2 3 (:parent (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) #3 (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"b\" 0 1 (:parent #7))))]) #(\"a\" 0 1 (:parent #3)))) 6 7 (:parent (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"a\" 0 1 (:parent #7))) #3)]) #(\"b\" 0 1 (:parent #3)))) 22 23 (:parent (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) #6)] :type standard) #3 (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"d\" 0 1 (:parent #7))))]) #(\"c\" 0 1 (:parent #3)))) 26 27 (:parent (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) #6)] :type standard) (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"c\" 0 1 (:parent #7))) #3)]) #(\"d\" 0 1 (:parent #3))))) #(\"| 2 |\\n| 4 |\\n| 3 |\\n#+TBLFM: @3=vmean(@1..@2)\\n\" 2 3 (:parent (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 6 7 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 19 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 44 44 0 nil first-section nil nil nil 1 44 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 44 44 0 nil org-data nil nil nil 3 44 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm (\"@3=vmean(@1..@2)\") :value nil) #6 (table-row (:standard-properties [7 7 8 12 13 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [8 nil 9 10 12 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"4\" 0 1 (:parent #11)))) (table-row (:standard-properties [13 13 14 18 19 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [14 nil 15 16 18 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"3\" 0 1 (:parent #11)))))] :type standard) #3)]) #(\"2\" 0 1 (:parent #3)))) 8 9 (:parent (table-cell (:standard-properties [8 nil 9 10 12 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [7 7 8 12 13 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 19 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 44 44 0 nil first-section nil nil nil 1 44 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 44 44 0 nil org-data nil nil nil 3 44 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm (\"@3=vmean(@1..@2)\") :value nil) (table-row (:standard-properties [1 1 2 6 7 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"2\" 0 1 (:parent #11)))) #6 (table-row (:standard-properties [13 13 14 18 19 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [14 nil 15 16 18 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"3\" 0 1 (:parent #11)))))] :type standard) #3)]) #(\"4\" 0 1 (:parent #3)))) 14 15 (:parent (table-cell (:standard-properties [14 nil 15 16 18 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [13 13 14 18 19 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 19 44 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 44 44 0 nil first-section nil nil nil 1 44 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 44 44 0 nil org-data nil nil nil 3 44 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm (\"@3=vmean(@1..@2)\") :value nil) (table-row (:standard-properties [1 1 2 6 7 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"2\" 0 1 (:parent #11)))) (table-row (:standard-properties [7 7 8 12 13 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [8 nil 9 10 12 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"4\" 0 1 (:parent #11)))) #6)] :type standard) #3)]) #(\"3\" 0 1 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-test-parse-and-interpret
         (lambda (text)
           (with-temp-buffer
             (org-mode) (insert text)
             (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall org-test-parse-and-interpret "| a | b |\n| c | d |")
     (funcall org-test-parse-and-interpret "| a | b |\n|---+---|\n| c | d |")
     (funcall org-test-parse-and-interpret
              "| 2 |\n| 4 |\n| 3 |\n#+TBLFM: @3=vmean(@1..@2)"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex timestamp round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_timestamp_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 0 0 0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-test-parse-and-interpret
         (lambda (text)
           (with-temp-buffer
             (org-mode) (insert text)
             (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (string-match "<2012-03-29 .* 16:40>"
                   (funcall org-test-parse-and-interpret "<2012-03-29 thu. 16:40>"))
     (string-match "\\[2012-03-29 .* 16:40\\]"
                   (funcall org-test-parse-and-interpret "[2012-03-29 thu. 16:40]"))
     (string-match "<2012-03-29 .* 16:40>--<2012-03-29 .* 16:41>"
                   (funcall org-test-parse-and-interpret
                            "<2012-03-29 thu. 16:40>--<2012-03-29 thu. 16:41>"))
     (string-match "<2012-03-29 .* 16:40-16:41>"
                   (funcall org-test-parse-and-interpret
                            "<2012-03-29 thu. 16:40-16:41>"))
     (string-match "<2012-03-29 .* \\+1y>"
                   (funcall org-test-parse-and-interpret "<2012-03-29 thu. +1y>"))
     (equal "<%%(diary-float t 4 2)>\n"
            (funcall org-test-parse-and-interpret "<%%(diary-float t 4 2)>")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex keyword/comment round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_keyword_comment_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#+keyword: value\\n\" \"# Comment\\n\" \"#+begin_comment\\nTest\\n#+end_comment\\n\" \": Test\\n\" \"-----\\n\" \"%%(org-anniversary 1956  5 14)(2) Arthur Dent is %d years old\\n\" \"\\\\begin{equation}\\n1+1=2\\n\\\\end{equation}\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-test-parse-and-interpret
         (lambda (text)
           (with-temp-buffer
             (org-mode) (insert text)
             (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall org-test-parse-and-interpret "#+KEYWORD: value")
     (funcall org-test-parse-and-interpret "# Comment")
     (funcall org-test-parse-and-interpret "#+BEGIN_COMMENT\nTest\n#+END_COMMENT")
     (funcall org-test-parse-and-interpret ": Test")
     (funcall org-test-parse-and-interpret "-------")
     (funcall org-test-parse-and-interpret
              "%%(org-anniversary 1956  5 14)(2) Arthur Dent is %d years old")
     (funcall org-test-parse-and-interpret
              "\\begin{equation}\n1+1=2\n\\end{equation}"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex citation round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_citation_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[cite:@key]\\n\" \"[cite/style:@key]\\n\" #(\"[cite:pre @key]\\n\" 6 10 (:parent (citation-reference (:standard-properties [7 nil nil nil 15 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (citation (:standard-properties [1 nil 7 15 16 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 16 16 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 16 16 0 nil first-section nil nil nil 1 16 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 16 16 0 nil org-data nil nil nil 3 16 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)]) #6)] :style nil) #3)] :key \"key\" :prefix (#(\"pre \" 0 4 (:parent #3))))))) #(\"[cite:@key post]\\n\" 10 15 (:parent (citation-reference (:standard-properties [7 nil nil nil 16 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (citation (:standard-properties [1 nil 7 16 17 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 17 17 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 17 17 0 nil first-section nil nil nil 1 17 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 17 17 0 nil org-data nil nil nil 3 17 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)]) #6)] :style nil) #3)] :key \"key\" :suffix (#(\" post\" 0 5 (:parent #3))))))) \"[cite:@a;@b;@c]\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil)
        (org-test-parse-and-interpret
         (lambda (text)
           (with-temp-buffer
             (org-mode) (insert text)
             (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall org-test-parse-and-interpret "[cite:@key]")
     (funcall org-test-parse-and-interpret "[cite/style:@key]")
     (funcall org-test-parse-and-interpret "[cite:pre @key]")
     (funcall org-test-parse-and-interpret "[cite:@key post]")
     (funcall org-test-parse-and-interpret "[cite:@a;@b;@c]"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex export options (all 24+)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_all_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Options Test\" 0 12 (:parent (#(\"Options Test\" 0 12 (:parent #4)))))) (#(\"Author\" 0 6 (:parent (#(\"Author\" 0 6 (:parent #4)))))) \"email@example.org\" 3 t t t t t t t t t t t t t t t t t nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Options Test
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
         (plist-get info :with-title))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex export headline numbers (all levels)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_headline_numbers_all_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+OPTIONS: num:t H:3
* Ch1
** S1
*** SS1
*** SS2
** S2
*** SS3
* Ch2
** S3
*** SS4
** S4")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         (mapcar (lambda (h) (org-export-get-headline-number h info))
                 (org-element-map tree 'headline #'identity))
         (mapcar (lambda (h) (org-export-get-relative-level h info))
                 (org-element-map tree 'headline #'identity))
         (mapcar (lambda (h) (org-export-numbered-headline-p h info))
                 (org-element-map tree 'headline #'identity))
         (mapcar (lambda (h) (org-export-low-level-p h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex export footnote numbers (all types)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_footnote_numbers_all_types() {
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
         (mapcar (lambda (ref) (org-export-get-footnote-number ref info))
                 (org-element-map tree 'footnote-reference #'identity))
         (mapcar (lambda (ref) (org-export-footnote-first-reference-p ref info))
                 (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex export tags/categories
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_tags_categories() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+CATEGORY: work
* H1 :tag1:
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
         (mapcar (lambda (h) (org-export-get-tags h info))
                 (org-element-map tree 'headline #'identity))
         (mapcar (lambda (h) (org-export-get-category h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex export first/last sibling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_first_last_sibling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n** H3\n** H4\n* H5")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         (mapcar #'org-export-first-sibling-p headlines)
         (mapcar #'org-export-last-sibling-p headlines)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex export filter apply
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_filter_apply() {
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

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex export backend chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_backend_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((parent) t ((lambda (h c i) (format \"CHILD: %s\\n%s\" (org-element-property :raw-value h) c)) (lambda (s c i) c)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let (org-export-registered-backends)
    (org-export-define-backend 'parent
      '((headline . (lambda (h c i) (format "PARENT: %s\n%s" (org-element-property :raw-value h) c)))
        (section . (lambda (s c i) c))
        (paragraph . (lambda (p c i) c))
        (plain-text . (lambda (t i) t))))
    (org-export-define-derived-backend 'child 'parent
      :translate-alist
      '((headline . (lambda (h c i) (format "CHILD: %s\n%s" (org-element-property :raw-value h) c)))))
    (list
     (org-export-derived-backend-p 'child 'parent)
     (org-export-derived-backend-p 'child 'child)
     (let ((all (org-export-get-all-transcoders 'child)))
       (list (cdr (assq 'headline all))
             (cdr (assq 'section all)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex export read-attribute
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_read_attribute() {
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

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex export caption
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_caption() {
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
// Eta-2: org-element with complex export optional title
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_optional_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-optional-title)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Document Title\n* H\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment))))
             (headline (car (org-element-map tree 'headline #'identity))))
        (org-export-get-optional-title headline info)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta-2: org-element with complex export node property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta2_export_node_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H\n:PROPERTIES:\n:CUSTOM_ID: myid\n:END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headline (car (org-element-map tree 'headline #'identity))))
        (org-export-get-node-property :CUSTOM_ID headline))))"##,
        expect,
    );
}
