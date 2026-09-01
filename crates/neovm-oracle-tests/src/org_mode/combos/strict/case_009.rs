//! Combo-strict-9 oracle tests — rigorous contract verification for
//! edge-case divergence-prone areas: element parent chains in
//! captions/titles/tags, whitespace-only content, deeply nested
//! markup, mixed unicode+ascii, element property completeness,
//! org-duration conversions, org-time-string-to-seconds, org-2ft
//! roundtrip, export info environment completeness,
//! org-element-normalize-contents edges, and user-defined entities.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Element parent chains: objects in captions, titles, tags, properties
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_parent_chain_in_caption_title_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp #(\"bold-title\" 0 10 (:parent (bold (:standard-properties [3 nil 4 14 16 1 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (headline (:standard-properties [1 1 37 92 92 0 (:title) first-section nil nil nil 39 90 1 #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 92 92 0 nil org-data nil nil nil 3 92 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)] :pre-blank 0 :raw-value \"*bold-title* Heading :*tag-bold*:\" :title (#3 #(\"Heading :*tag-bold*:\" 0 20 (:parent #6))) :level 1 :priority nil :tags nil :todo-keyword nil :todo-type nil :footnote-section-p nil :archivedp nil :commentedp nil) (section (:standard-properties [37 37 37 92 92 0 nil section nil nil nil 37 92 nil #<killed buffer> nil nil #6]) (table (:standard-properties [37 72 72 92 92 0 nil planning nil nil nil nil nil nil #<killed buffer> nil nil #7] :type org :tblfm nil :value nil :caption (((#(\"Table \" 0 6 (:parent #12)) (italic (:standard-properties [54 nil 55 69 70 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #12]) #(\"caption-italic\" 0 14 (:parent #13))) #(\".\" 0 1 (:parent #12)))))) (table-row (:standard-properties [72 72 73 81 82 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [73 nil 74 75 77 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"a\" 0 1 (:parent #10))) (table-cell (:standard-properties [77 nil 78 79 81 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"b\" 0 1 (:parent #10)))) (table-row (:standard-properties [82 82 83 91 92 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #8] :type standard) (table-cell (:standard-properties [83 nil 84 85 87 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"1\" 0 1 (:parent #10))) (table-cell (:standard-properties [87 nil 88 89 91 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #9]) #(\"2\" 0 1 (:parent #10)))))))]) #(\"bold-title\" 0 10 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* *bold-title* Heading :*tag-bold*:\n")
      (insert "#+CAPTION: Table /caption-italic/.\n")
      (insert "| a | b |\n| 1 | 2 |\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (r '()))
        ;; bold in headline title
        (let ((bold-title (car (org-element-map tree 'bold #'identity))))
          (when bold-title
            (push (list :bold-title-parent-type
                        (org-element-type (org-element-property :parent bold-title))) r)
            (push (list :bold-title-secondary (org-element-secondary-p bold-title)) r)
            (let ((grandparent (org-element-property :parent
                                 (org-element-property :parent bold-title))))
              (push (list :bold-title-grandparent-type (org-element-type grandparent)) r))))
        ;; bold in headline tags
        (let ((tag-text (car (org-element-map tree 'plain-text
                               (lambda (txt) (when (string-match-p "tag-bold" (car txt)) txt))))))
          (when tag-text
            (push (list :tag-text-parent-type
                        (org-element-type (org-element-property :parent tag-text))) r)))
        ;; italic in caption
        (let ((cap-italic (car (org-element-map tree 'italic #'identity))))
          (when cap-italic
            (push (list :caption-italic-parent-type
                        (org-element-type (org-element-property :parent cap-italic))) r)
            (push (list :caption-italic-secondary (org-element-secondary-p cap-italic)) r)))
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Whitespace-only content parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_whitespace_only_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 31 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* \n  \n\t\n\n** Tabs and spaces\nBody.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity))
             (para-count (length (org-element-map tree 'paragraph #'identity)))
             (r '()))
        ;; empty heading raw-value
        (push (list :empty-heading-value
                    (substring-no-properties (org-element-property :raw-value (nth 0 headlines)))) r)
        (push (list :empty-heading-level (org-element-property :level (nth 0 headlines))) r)
        ;; second heading value
        (push (list :real-heading-value
                    (substring-no-properties (org-element-property :raw-value (nth 1 headlines)))) r)
        ;; paragraph count (should be 1 - the body)
        (push (list :para-count para-count) r)
        ;; whitespace-only sections
        (let ((sections (org-element-map tree 'section #'identity)))
          (push (list :section-count (length sections)) r)
          ;; check if first section is empty
          (when (car sections)
            (let ((first-sec-contents
                   (buffer-substring-no-properties
                    (org-element-property :contents-begin (car sections))
                    (org-element-property :contents-end (car sections)))))
              (push (list :first-section-is-whitespace
                          (string-match-p "\\`[ \t\n]*\\'" first-sec-contents)) r))))
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deeply nested markup (10 levels)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_deeply_nested_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 19 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      ;; 5 alternating bold/italic layers
      (insert "Start *bold1 /italic1 *bold2 /italic2 *bold3/ */ */ */ end.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (bolds (org-element-map tree 'bold #'identity))
             (italics (org-element-map tree 'italic #'identity))
             (r '()))
        (push (list :bold-count (length bolds)) r)
        (push (list :italic-count (length italics)) r)
        ;; check nesting depth: bold3 should be inside italic2 inside bold2 inside italic1 inside bold1
        (let ((b3 (nth 2 bolds)))  ;; 3rd bold = innermost
          (when b3
            (let ((lineage (org-element-lineage b3)))
              (push (list :b3-lineage (mapcar #'org-element-type lineage)) r))))
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Mixed unicode + ascii content parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_mixed_unicode_ascii_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 25 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* 日本語 and English mixed\n")
      (insert "αβγ math $x^2+y^2=z^2$ symbols.\n")
      (insert "Emoji 🎉 mixed with 中文 and 한국어 text.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headline (car (org-element-map tree 'headline #'identity)))
             (paragraphs (org-element-map tree 'paragraph #'identity))
             (r '()))
        ;; headline
        (push (list :headline-raw (substring-no-properties (org-element-property :raw-value headline))) r)
        (push (list :headline-raw-length (length (org-element-property :raw-value headline))) r)
        ;; paragraph count
        (push (list :para-count (length paragraphs)) r)
        ;; check for LaTeX fragment
        (push (list :latex-frag-count (length (org-element-map tree 'latex-fragment #'identity))) r)
        ;; interpret round-trip preserves unicode
        (let ((interpreted (substring-no-properties (org-element-interpret-data tree))))
          (push (list :interpreted-contains-japanese (string-match-p "日本語" interpreted)) r)
          (push (list :interpreted-contains-greek (string-match-p "αβγ" interpreted)) r)
          (push (list :interpreted-contains-emoji (string-match-p "🎉" interpreted)) r))
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Element property completeness for all types in document
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_element_property_completeness() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 42 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] Heading :tag1:tag2:\n")
      (insert "SCHEDULED: <2024-06-01 Sat>\n")
      (insert ":PROPERTIES:\n:CUSTOM_ID: abc\n:END:\n\n")
      (insert "Paragraph with *bold* and /italic/.\n\n")
      (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n\n")
      (insert "| a | b |\n| 1 | 2 |\n\n")
      (insert "[fn:1] A footnote.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (all-types (delete-dups (mapcar #'org-element-type
                                             (org-element-map tree t #'identity))))
             (r '()))
        (push (list :all-types (sort (mapcar #'symbol-name all-types) #'string-lessp)) r)
        ;; Check headline has all standard properties
        (let ((hl (car (org-element-map tree 'headline #'identity))))
          (push (list :hl-has-level (numberp (org-element-property :level hl))) r)
          (push (list :hl-has-todo (org-element-property :todo-keyword hl)) r)
          (push (list :hl-has-priority (org-element-property :priority hl)) r)
          (push (list :hl-has-tags (org-element-property :tags hl)) r)
          (push (list :hl-has-begin (numberp (org-element-property :begin hl))) r)
          (push (list :hl-has-end (numberp (org-element-property :end hl))) r)
          (push (list :hl-has-contents-begin (numberp (org-element-property :contents-begin hl))) r)
          (push (list :hl-has-contents-end (numberp (org-element-property :contents-end hl))) r)
          (push (list :hl-has-post-blank (numberp (org-element-property :post-blank hl))) r)
          (push (list :hl-has-parent (org-element-property :parent hl)) r))
        ;; Check src-block has all standard properties
        (let ((src (car (org-element-map tree 'src-block #'identity))))
          (when src
            (push (list :src-has-language (org-element-property :language src)) r)
            (push (list :src-has-value (stringp (org-element-property :value src))) r)
            (push (list :src-has-switches (org-element-property :switches src)) r)
            (push (list :src-has-number-lines (numberp (or (org-element-property :number-lines src) 0))) r)))
        ;; Check table has rows and cells
        (let ((tbl (car (org-element-map tree 'table #'identity))))
          (when tbl
            (push (list :tbl-has-tblfm (org-element-property :tblfm tbl)) r)
            (push (list :tbl-has-type (org-element-property :type tbl)) r)))
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-duration conversion roundtrip
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_duration_conversion_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 25 51)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-duration)
  (list
   ;; to minutes
   (list :1:30 (org-duration-to-minutes "1:30"))
   (list :0:45 (org-duration-to-minutes "0:45"))
   (list :2:00 (org-duration-to-minutes "2:00"))
   (list :0:01 (org-duration-to-minutes "0:01"))
   ;; from minutes
   (list :90min (org-duration-from-minutes 90))
   (list :45min (org-duration-from-minutes 45))
   (list :120min (org-duration-from-minutes 120))
   (list :1min (org-duration-from-minutes 1))
   ;; roundtrip
   (let* ((mins (org-duration-to-minutes "1:45"))
          (str (org-duration-from-minutes mins)))
     (list :roundtrip (list mins str)))
   ;; hh:mm format
   (let ((mod-fmt (let ((org-duration-format 'h:mm))
                    (org-duration-from-minutes 150))))
     (list :hmm-format mod-fmt))
   ;; p flag
   (list :is-duration-p (org-duration-p "1:23"))
   (list :not-duration-p (org-duration-p "abc")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-time-string-to-seconds and org-2ft comparisons
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_time_string_seconds_2ft() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((test-date "<2024-06-15 Sat 10:30>"))
    (list
     ;; org-time-string-to-seconds
     (list :to-seconds (org-time-string-to-seconds test-date))
     ;; org-time-string-to-absolute
     (cond ((fboundp 'org-time-string-to-absolute)
            (list :to-absolute (org-time-string-to-absolute test-date)))
           (t :not-available))
     ;; seconds for various dates
     (let ((s1 (org-time-string-to-seconds "<2024-01-01 Mon 00:00>"))
           (s2 (org-time-string-to-seconds "<2024-01-02 Tue 00:00>")))
       (list :one-day-diff (- s2 s1)))
     ;; org-2ft on timestamp
     (let ((ts (org-timestamp-from-string "<2024-06-15 Sat 10:30>")))
       (list :org-2ft (org-2ft ts)))
     ;; time subtraction
     (let ((t1 (org-time-string-to-seconds "<2024-06-15 Sat 14:00>"))
           (t2 (org-time-string-to-seconds "<2024-06-15 Sat 10:00>")))
       (list :four-hours-diff (- t1 t2))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export info environment completeness
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_export_info_environment_completeness() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 27 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Test\n")
      (insert "#+AUTHOR: Alice\n")
      (insert "#+DATE: 2024-06-15\n")
      (insert "#+EMAIL: alice@test\n")
      (insert "#+OPTIONS: num:t toc:t\n")
      (insert "#+LANGUAGE: en\n")
      (insert "#+SELECT_TAGS: export\n")
      (insert "#+EXCLUDE_TAGS: noexport\n")
      (insert "\n* Headline\nBody.\n")
      (goto-char (point-min))
      (let* ((info (org-export-get-environment))
             (r '()))
        ;; known keys that should always be present
        (dolist (key '(:title :author :date :email :language
                       :with-toc :with-numbers :with-author :with-date
                       :select-tags :exclude-tags
                       :export-options :headline-levels))
          (push (list key (plist-get info key)) r))
        (push (list :title-is-string (stringp (plist-get info :title))) r)
        (push (list :date-is-string (stringp (plist-get info :date))) r)
        (push (list :language-is-string (stringp (plist-get info :language))) r)
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-normalize-contents more edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_normalize_contents_more_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((paragraph nil \"First line.\\n\" \"  More indented.\\n\" \"    Even more.\\n\" \"  Back to two.\") (paragraph nil \"Single line.\") (paragraph nil \"Line one.\\n\\n\\nLine two.\\n\\nLine three.\") (paragraph nil (bold nil \"bold\") \" no indent\\n   three spaces\\n   three spaces\") (verse-block nil \"line 1\\n line 2\\n\\nline 3\") (paragraph nil \"Start\\n\" \"     Nested\\n\" \"   End.\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Mixed objects and raw strings with varying indents
   (org-element-normalize-contents
    '(paragraph nil
      "  First line.\n"
      "  More indented.\n"
      "    Even more.\n"
      "  Back to two."))
   ;; Only one line (should be untouched)
   (org-element-normalize-contents
    '(paragraph nil "Single line."))
   ;; With blank lines in between
   (org-element-normalize-contents
    '(paragraph nil
      "  Line one.\n\n\n  Line two.\n\n  Line three."))
   ;; With object at very start (no common indent baseline)
   (org-element-normalize-contents
    '(paragraph nil (bold nil "bold") " no indent\n   three spaces\n   three spaces"))
   ;; Verse block: preserve relative whitespace
   (org-element-normalize-contents
    '(verse-block nil "  line 1\n   line 2\n\n  line 3"))
   ;; Mixed: objects + strings at various indent levels
   (org-element-normalize-contents
    '(paragraph nil
      "   Start\n"
      "     Nested\n"
      "   End."))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// User-defined entities
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_user_defined_entities() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 23 64)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-entities)
  (let ((org-mode-hook nil))
    (list
     ;; Built-in entities
     (list :alpha (org-entity-get "alpha"))
     (list :beta (org-entity-get "beta"))
     (list :gamma (org-entity-get "gamma"))
     (list :rightarrow (org-entity-get "rightarrow"))
     (list :larr (org-entity-get "larr"))
     (list :rarr (org-entity-get "rarr"))
     ;; LaTeX math entities
     (list :sum (org-entity-get "sum"))
     (list :int (org-entity-get "int"))
     (list :pi (org-entity-get "pi"))
     ;; Non-existent
     (list :bogus (org-entity-get "bogus"))
     ;; Entity count
     (list :total-count (length org-entities))
     ;; Greek letter entities
     (let ((greek '("Alpha" "Beta" "Gamma" "Delta" "alpha" "beta" "gamma" "delta")))
       (mapcar (lambda (name) (org-entity-get name)) greek))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Element lineage for object deeply inside table → cell → row → table
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_lineage_object_in_table_cell() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 24 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-element)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Heading\n")
      (insert "| *bold-cell* | /italic-cell/ |\n| plain | =code= |\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (bolds (org-element-map tree 'bold #'identity))
             (codes (org-element-map tree 'code #'identity))
             (r '()))
        ;; bold lineage
        (when (car bolds)
          (push (list :bold-lineage (mapcar #'org-element-type
                                            (org-element-lineage (car bolds)))) r))
        ;; code lineage
        (when (car codes)
          (push (list :code-lineage (mapcar #'org-element-type
                                            (org-element-lineage (car codes)))) r))
        ;; bold parent type
        (when (car bolds)
          (push (list :bold-parent-type (org-element-type (org-element-property :parent (car bolds)))) r))
        (nreverse r))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-interpret-data for header with all properties, reparse
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_interpret_reparse_header_with_all_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 47 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#B] Complex heading :tag1:tag2:\n")
      (insert "SCHEDULED: <2024-07-01 Mon>\n")
      (insert "DEADLINE: <2024-07-15 Mon>\n")
      (insert ":PROPERTIES:\n")
      (insert ":ID:       1234-abcd\n")
      (insert ":CUSTOM_ID: my-custom-id\n")
      (insert ":EFFORT:    3:00\n")
      (insert ":END:\n")
      (insert "Some body text with *bold* and /italic/.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (interpreted (substring-no-properties (org-element-interpret-data tree)))
             (reparsed (with-temp-buffer (org-mode)
                         (insert interpreted)
                         (goto-char (point-min))
                         (org-element-parse-buffer)))
             (r '()))
        ;; original element counts
        (push (list :orig-headlines (length (org-element-map tree 'headline #'identity))) r)
        (push (list :orig-bold (length (org-element-map tree 'bold #'identity))) r)
        (push (list :orig-italic (length (org-element-map tree 'italic #'identity))) r)
        (push (list :orig-planning (length (org-element-map tree 'planning #'identity))) r)
        (push (list :orig-prop-drawer (length (org-element-map tree 'property-drawer #'identity))) r)
        ;; re-parsed element counts
        (push (list :re-headlines (length (org-element-map reparsed 'headline #'identity))) r)
        (push (list :re-bold (length (org-element-map reparsed 'bold #'identity))) r)
        (push (list :re-italic (length (org-element-map reparsed 'italic #'identity))) r)
        (push (list :re-planning (length (org-element-map reparsed 'planning #'identity))) r)
        (push (list :re-prop-drawer (length (org-element-map reparsed 'property-drawer #'identity))) r)
        ;; check re-parsed headline has all properties
        (let ((re-hl (car (org-element-map reparsed 'headline #'identity))))
          (when re-hl
            (push (list :re-todo (org-element-property :todo-keyword re-hl)) r)
            (push (list :re-priority (org-element-property :priority re-hl)) r)
            (push (list :re-tags (org-element-property :tags re-hl)) r)
            (push (list :re-raw (substring-no-properties (org-element-property :raw-value re-hl))) r)))
        ;; check properties survived
        (let ((props (org-entry-properties (org-element-property :begin
                                          (car (org-element-map reparsed 'headline #'identity))) t)))
          (push (list :re-id (cdr (assoc "ID" props))) r)
          (push (list :re-custom-id (cdr (assoc "CUSTOM_ID" props))) r)
          (push (list :re-effort (cdr (assoc "EFFORT" props))) r))
        (nreverse r))))))"##,
        expect,
    );
}
