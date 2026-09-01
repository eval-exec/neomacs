//! More strict combo tests for org-mode edge cases and interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-parse-buffer granularity levels
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_parse_granularity_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\nPara *bold* /italic/.\n** H2\nMore text.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer 'object)))
        (list
         ;; All types found at object granularity.
         (delete-dups (mapcar #'org-element-type (org-element-map tree t #'identity)))
         ;; Counts.
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))
         (length (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

#[test]
fn strict_parse_granularity_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\nPara *bold* /italic/.\n** H2\nMore text.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer 'element)))
        (list
         ;; Types at element granularity.
         (delete-dups (mapcar #'org-element-type (org-element-map tree t #'identity)))
         ;; No objects found.
         (length (org-element-map tree 'bold #'identity))
         ;; Paragraphs found.
         (length (org-element-map tree 'paragraph #'identity))))))"##,
        expect,
    );
}

#[test]
fn strict_parse_granularity_greater_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n#+BEGIN_CENTER\nPara *bold*.\n#+END_CENTER\n** H2\nMore text.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer 'greater-element)))
        (list
         ;; Types at greater-element granularity.
         (delete-dups (mapcar #'org-element-type (org-element-map tree t #'identity)))
         ;; Center block found but not its contents.
         (length (org-element-map tree 'center-block #'identity))
         ;; No paragraphs inside center.
         (length (org-element-map tree 'paragraph #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-context at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_context_at_various_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"Para\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Para with *bold* and /italic/ and _underline_ and =code=.\n")
      (goto-char (point-min))
      (list
       ;; On bold.
       (search-forward "bold")
       (org-element-type (org-element-context))
       ;; On italic.
       (search-forward "italic")
       (org-element-type (org-element-context))
       ;; On underline.
       (search-forward "underline")
       (org-element-type (org-element-context))
       ;; On code.
       (search-forward "code")
       (org-element-type (org-element-context))
       ;; On plain text.
       (search-forward "Para")
       (org-element-type (org-element-context))))))"##,
        expect,
    );
}

#[test]
fn strict_context_in_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 bold 19 italic 51 table-cell)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| *bold* | /italic/ |\n|--------+----------|\n| code   | plain    |\n")
      (goto-char (point-min))
      (list
       ;; In bold cell.
       (search-forward "bold")
       (org-element-type (org-element-context))
       ;; In italic cell.
       (search-forward "italic")
       (org-element-type (org-element-context))
       ;; In code cell.
       (search-forward "code")
       (org-element-type (org-element-context))))))"##,
        expect,
    );
}

#[test]
fn strict_context_in_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"Headline\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Headline with *bold* and /italic/\n")
      (goto-char (point-min))
      (list
       ;; On bold in headline.
       (search-forward "bold")
       (org-element-type (org-element-context))
       ;; On italic in headline.
       (search-forward "italic")
       (org-element-type (org-element-context))
       ;; On plain text in headline.
       (search-forward "Headline")
       (org-element-type (org-element-context))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element lineage with various types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_lineage_in_nested_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n#+BEGIN_CENTER\nPara with *bold*.\n#+END_CENTER\n")
      (goto-char (point-min))
      (search-forward "bold")
      (let* ((tree (org-element-parse-buffer))
             (bold (car (org-element-map tree 'bold #'identity))))
        (list
         ;; Full lineage.
         (mapcar #'org-element-type (org-element-lineage bold))
         ;; With self.
         (mapcar #'org-element-type (org-element-lineage bold nil t))
         ;; Filtered: headlines only.
         (mapcar #'org-element-type (org-element-lineage bold 'headline))
         ;; Filtered: blocks only.
         (mapcar #'org-element-type (org-element-lineage bold 'center-block))
         ;; Filtered: with self.
         (mapcar #'org-element-type (org-element-lineage bold '(bold paragraph) t))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-map with type lists
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_map_with_type_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "*bold* /italic/ _underline_ =verbatim= +strike+ ~code~\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer)))
        (list
         ;; Map with list of types.
         (mapcar #'org-element-type
                 (org-element-map tree '(bold italic underline) #'identity))
         ;; Map with t (all types).
         (length (org-element-map tree t #'identity))
         ;; Map single type.
         (length (org-element-map tree 'bold #'identity))
         (length (org-element-map tree 'italic #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: export with subtree scope
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_export_subtree_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-test-default-backend)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\nBody 1\n** H2\nBody 2\n*** H3\nBody 3\n* H4\nBody 4")
      (goto-char (point-min))
      (forward-line 2)
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        ;; Export subtree.
        (org-export-as (org-test-default-backend) 'subtree)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-copy deep vs shallow
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_copy_deep_vs_shallow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((original (org-element-create
                    'headline '(:level 1 :raw-value "Test")
                    (org-element-create
                     'section nil
                     (org-element-create 'paragraph nil "Body.\n"))))
         (copy (org-element-copy original)))
    (list
     ;; Not eq.
     (eq original copy)
     ;; Properties preserved.
     (equal (org-element-property :level original)
            (org-element-property :level copy))
     ;; Contents are new.
     (eq (org-element-contents original)
         (org-element-contents copy))
     ;; But contents are equal.
     (equal (org-element-contents original)
            (org-element-contents copy))
     ;; No parent on copy.
     (org-element-property :parent copy)
     ;; Original has no parent either (was root).
     (org-element-property :parent original))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-swap-A-B
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_swap_preserves_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"A\" \"A1\" \"B\" \"B1\" \"C\") \"** A1\\nBody A1 A1\\nBody A1\\n* B\\nBody B\\n** B1\\nBody B1\\n* C\\nBody C\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\nBody A\n** A1\nBody A1\n* B\nBody B\n** B1\nBody B1\n* C\nBody C")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headlines (org-element-map tree 'headline #'identity)))
        ;; Swap A and B.
        (org-element-swap-A-B (nth 0 headlines) (nth 1 headlines))
        (list
         ;; Order after swap.
         (mapcar (lambda (h) (org-element-property :raw-value h))
                 (org-element-map tree 'headline #'identity))
         ;; Buffer content.
         (buffer-substring-no-properties (point-min) (point-max)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-uniq
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_uniq_with_mixed_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-uniq)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((p1 (org-element-create 'paragraph nil "p1"))
         (p2 (org-element-create 'paragraph nil "p2"))
         (h1 (org-element-create 'headline '(:level 1)))
         (list (list p1 p2 h1 p1 p2 h1 p1)))
    (list
     ;; Length before uniq.
     (length list)
     ;; Length after uniq.
     (length (org-element-uniq list))
     ;; Order preserved.
     (mapcar #'org-element-type (org-element-uniq list)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-deferred operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_deferred_create_and_resolve() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-deferred-force-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Deferred-p checks.
   (org-element-deferred-p (org-element-deferred-create t (lambda (_) 1)))
   (org-element-deferred-p (org-element-deferred-create nil (lambda (_) 1)))
   (org-element-deferred-p '(not-deferred))
   ;; Force-p checks.
   (org-element-deferred-force-p (org-element-deferred-create t (lambda (_) 1)))
   (org-element-deferred-force-p (org-element-deferred-create nil (lambda (_) 1)))
   ;; Function access.
   (functionp (org-element-deferred-get-function
               (org-element-deferred-create nil (lambda (_) 1))))
   ;; Create alias.
   (let ((el (org-element-create 'dummy '(:foo 1 :bar ,(org-element-deferred-create-alias :foo)))))
     (list (org-element-property :foo el)
           (org-element-property :bar el)))
   ;; Create list.
   (let ((el (org-element-create 'dummy '(:foo ,(org-element-deferred-create-list
                                                  (list 1 2 (org-element-deferred-create nil (lambda (_) 3))))))))
     (org-element-property :foo el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-set with keep-props
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_set_with_keep_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bar (1 2 10))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   ;; Keep :foo when setting.
   (org-element-property
    :foo
    (org-element-set
     (org-element-create 'dummy '(:foo bar))
     (org-element-create 'dummy '(:foo2 bar2))
     '(:foo)))
   ;; Keep multiple props.
   (let ((result (org-element-set
                  (org-element-create 'dummy '(:a 1 :b 2 :c 3))
                  (org-element-create 'dummy '(:x 10 :y 20))
                  '(:a :b))))
     (list (org-element-property :a result)
           (org-element-property :b result)
           (org-element-property :x result)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-insert-before in secondary strings
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_insert_before_secondary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (entity italic)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* /A/\n  Paragraph.")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headline (car (org-element-map tree 'headline #'identity)))
             (italic (car (org-element-map tree 'italic #'identity))))
        ;; Insert entity before italic in title.
        (org-element-insert-before '(entity (:name "\\alpha")) italic)
        ;; Check title now has entity and italic.
        (org-element-map (org-element-property :title headline) '(entity italic)
                         #'org-element-type)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-extract from various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_extract_from_various_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil (\"H1\" \"H3\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Extract paragraph from center block.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_CENTER\nPara\n#+END_CENTER")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (para (car (org-element-map tree 'paragraph #'identity))))
         (org-element-extract para)
         (org-element-map tree 'paragraph #'identity)))
     ;; Extract bold from paragraph.
     (with-temp-buffer (org-mode)
       (insert "Text *bold* more.")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (bold (car (org-element-map tree 'bold #'identity))))
         (org-element-extract bold)
         (org-element-map tree 'bold #'identity)))
     ;; Extract headline from document.
     (with-temp-buffer (org-mode)
       (insert "* H1\n* H2\n* H3")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (h2 (nth 1 (org-element-map tree 'headline #'identity))))
         (org-element-extract h2)
         (mapcar (lambda (h) (org-element-property :raw-value h))
                 (org-element-map tree 'headline #'identity)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-adopt with various child types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_adopt_various_children() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"text1\" 0 5 (:parent (section nil #(\"text1\" 0 5 (:parent #4)) (paragraph (:standard-properties [nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil #4]) #(\"para\" 0 4 (:parent #5))) #(\"text2\" 0 5 (:parent #4))))) paragraph #(\"text2\" 0 5 (:parent (section nil #(\"text1\" 0 5 (:parent #4)) (paragraph (:standard-properties [nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil #4]) #(\"para\" 0 4 (:parent #5))) #(\"text2\" 0 5 (:parent #4)))))) section)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((parent (org-element-create 'section nil)))
    ;; Adopt strings and elements.
    (org-element-adopt parent "text1" (org-element-create 'paragraph nil "para") "text2")
    (list
     ;; Contents.
     (mapcar (lambda (c) (if (stringp c) c (org-element-type c)))
             (org-element-contents parent))
     ;; Parent set on children.
     (org-element-type (org-element-property :parent (nth 1 (org-element-contents parent)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element secondary string handling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_secondary_string_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (t (plain-text bold plain-text italic) headline headline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Title with *bold* and /italic/")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (headline (car (org-element-map tree 'headline #'identity)))
             (title (org-element-property :title headline)))
        (list
         ;; Title is a list (secondary string).
         (listp title)
         ;; Contains objects.
         (mapcar #'org-element-type title)
         ;; Bold in title has correct parent.
         (let ((bold (car (org-element-map title 'bold #'identity))))
           (org-element-type (org-element-property :parent bold)))
         ;; Italic in title has correct parent.
         (let ((italic (car (org-element-map title 'italic #'identity))))
           (org-element-type (org-element-property :parent italic))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-property-raw setter
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_property_raw_setter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 baz 3.14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((el (org-element-create 'dummy '(:foo 1))))
    ;; Set via setf.
    (setf (org-element-property-raw :foo el) 42)
    (setf (org-element-property-raw :bar el) 'baz)
    (setf (org-element-property-raw :num el) 3.14)
    (list (org-element-property-raw :foo el)
          (org-element-property-raw :bar el)
          (org-element-property-raw :num el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-properties-mapc with deferred
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_properties_mapc_with_deferred() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((el (org-element-create
             'dummy
             `( :foo ,(org-element-deferred-create t (lambda (_) 1))
                :bar 2))))
    (list
     ;; Find deferred property.
     (catch :found
       (org-element-properties-mapc
        (lambda (_ val _)
          (when (org-element-deferred-p val)
            (throw :found t)))
        el))
     ;; After undefer, find resolved value.
     (catch :found
       (org-element-properties-mapc
        (lambda (prop val _)
          (when (and (eq prop :foo) (eq 1 val))
            (throw :found t)))
        el 'undefer)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element-properties-map with different arities
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_properties_map_arities() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2 3) (1 2 nil) (1 2 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let ((el (org-element-create 'dummy '(:foo 1 :bar 2 :baz 3))))
    (list
     ;; Single argument.
     (org-element-properties-map #'identity el)
     ;; Two arguments.
     (org-element-properties-map
      (lambda (prop val) (unless (eq prop :baz) val)) el)
     ;; Three arguments.
     (org-element-properties-map
      (lambda (prop val node)
        (if (eq prop :baz)
            (1+ (org-element-property-raw :baz node))
          val))
      el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: org-element at point edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_at_point_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (paragraph paragraph paragraph headline drawer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; At beginning of buffer.
     (with-temp-buffer (org-mode) (insert "Para")
       (goto-char (point-min))
       (org-element-type (org-element-at-point)))
     ;; At end of buffer.
     (with-temp-buffer (org-mode) (insert "Para")
       (goto-char (point-max))
       (org-element-type (org-element-at-point)))
     ;; Between two paragraphs.
     (with-temp-buffer (org-mode) (insert "P1\n\nP2")
       (goto-char (point-min)) (forward-line 2)
       (org-element-type (org-element-at-point)))
     ;; At blank line after headline.
     (with-temp-buffer (org-mode) (insert "* H\n\nP")
       (goto-char (point-min)) (forward-line 1)
       (org-element-type (org-element-at-point)))
     ;; At drawer boundary.
     (with-temp-buffer (org-mode) (insert ":DRAWER:\ncontents\n:END:")
       (goto-char (point-min)) (forward-line 2)
       (org-element-type (org-element-at-point))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: export with all headline features
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_export_headline_features() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TODO\" \"DONE\" nil nil nil) (65 66 nil nil 67) ((\"tag1\" \"tag2\") nil nil nil (\"important\")) ((1) (2) (3) (4) (5)) (t t t t t) (1 1 1 1 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] H1 :tag1:tag2:\n* DONE [#B] H2\n* COMMENT H3\n* Normal H4\n* [#C] H5 :important:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment))))
             (headlines (org-element-map tree 'headline #'identity)))
        (list
         ;; TODO keywords.
         (mapcar (lambda (h) (org-element-property :todo-keyword h)) headlines)
         ;; Priorities.
         (mapcar (lambda (h) (org-element-property :priority h)) headlines)
         ;; Tags.
         (mapcar (lambda (h) (org-element-property :tags h)) headlines)
         ;; Headline numbers.
         (mapcar (lambda (h) (org-export-get-headline-number h info)) headlines)
         ;; Numbered?
         (mapcar (lambda (h) (org-export-numbered-headline-p h info)) headlines)
         ;; Relative levels.
         (mapcar (lambda (h) (org-export-get-relative-level h info)) headlines))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Strict: export footnote edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strict_export_footnote_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1] more[fn:2:inline] and[fn::anon].\n\n[fn:1] Standard def.\n")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties
                     tree (org-export-get-environment)))))
        (list
         ;; Reference types.
         (mapcar (lambda (ref) (org-element-property :type ref))
                 (org-element-map tree 'footnote-reference #'identity))
         ;; Footnote numbers.
         (mapcar (lambda (ref) (org-export-get-footnote-number ref info))
                 (org-element-map tree 'footnote-reference #'identity))
         ;; First reference?
         (mapcar (lambda (ref) (org-export-footnote-first-reference-p ref info))
                 (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}
